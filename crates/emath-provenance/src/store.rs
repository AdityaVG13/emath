//! Blocking provenance store over the async frankengraphdb engine (graphdb).
//!
//! The engine (`fgdb` + asupersync) is async-native for open/create/write and
//! sync for reads, so the `Database` and the asupersync runtime live on a
//! dedicated worker thread with a large stack; `ProvenanceStore` is a channel
//! proxy over that worker (emath-store sqlite facade precedent, CUTOVER_PLAN
//! §9.10). Single-writer by design (one owner per graph directory); see
//! CONTRACT.md. All public methods are blocking.

use std::path::Path;
use std::sync::mpsc;

use asupersync::Budget;
use asupersync::runtime::{Runtime, RuntimeBuilder};
use fgdb::Database;
use fgdb_delta_types::{ElementId, LabelId, RelationId};
use fgdb_types::context::{CommitCx, PurposeContexts};
use fgdb_types::{DatabaseSecurityNamespaceId, EId, VId};

use crate::{
    Adjacency, AuthoredEdge, EdgeId, EdgeKind, Lineage, MAX_LINEAGE_DEPTH, NodeId, NodeKind,
    ProvenanceError, lineage_closure, would_create_cycle,
};

/// Worker stack: fgdb open/write futures recurse deeply while polling, so the
/// engine runs on a large-stack thread, never on the caller's thread.
const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

/// Fixed developer key material for the spike. frankengraphdb at the pinned
/// rev supplies no key management (bead fgdb-warden has not landed);
/// `DatabaseKeys` must be caller-supplied. The [0x5a]/[0x77]/[0x3c] constants
/// mirror the upstream test suites; see CONTRACT.md no-claim boundaries.
fn dev_key_material() -> ([u8; 32], DatabaseSecurityNamespaceId, [u8; 32]) {
    (
        [0x5a; 32],
        DatabaseSecurityNamespaceId([0x77; 32]),
        [0x3c; 32],
    )
}

/// Engine operation dispatched to the worker thread. Each op carries its own
/// reply channel typed to the op result.
enum Op {
    InsertNode {
        id: NodeId,
        kind: NodeKind,
        reply: mpsc::Sender<Result<NodeId, ProvenanceError>>,
    },
    InsertEdge {
        id: EdgeId,
        kind: EdgeKind,
        src: NodeId,
        dst: NodeId,
        reply: mpsc::Sender<Result<EdgeId, ProvenanceError>>,
    },
    Lineage {
        seed: NodeId,
        max_depth: usize,
        reply: mpsc::Sender<Result<Lineage, ProvenanceError>>,
    },
    NodeKind {
        id: NodeId,
        reply: mpsc::Sender<Result<Option<NodeKind>, ProvenanceError>>,
    },
    Close,
}

/// Blocking handle: a channel proxy over the engine worker thread.
pub struct ProvenanceStore {
    request_tx: mpsc::Sender<Op>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl ProvenanceStore {
    /// Open (or create) the provenance graph at `path`. A missing or empty
    /// directory is created; an existing graph is opened and its Chronicle
    /// stream is re-folded into a fresh read partition (reopen is
    /// deterministic: identical op history → identical reads).
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ProvenanceError> {
        let (request_tx, request_rx) = mpsc::channel::<Op>();
        let (open_tx, open_rx) = mpsc::channel::<Result<(), ProvenanceError>>();
        let worker_path = path.as_ref().to_path_buf();
        let worker = std::thread::Builder::new()
            .name("emath-provenance".into())
            .stack_size(WORKER_STACK_BYTES)
            .spawn(move || worker_entry(&worker_path, request_rx, open_tx))
            .map_err(|error| ProvenanceError::Open(format!("worker thread: {error}")))?;
        match open_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                request_tx,
                worker: Some(worker),
            }),
            Ok(Err(error)) => Err(error),
            Err(error) => Err(ProvenanceError::Open(format!(
                "worker channel closed: {error}"
            ))),
        }
    }

    /// Insert a node of `kind` under the caller-supplied sequence-ordered
    /// `id`. Re-inserting a live or spent id is refused
    /// ([`ProvenanceError::DuplicateNode`]).
    pub fn insert_node(&self, id: NodeId, kind: NodeKind) -> Result<NodeId, ProvenanceError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.request_tx
            .send(Op::InsertNode {
                id,
                kind,
                reply: reply_tx,
            })
            .map_err(|error| ProvenanceError::Query(format!("worker channel send: {error}")))?;
        reply_rx
            .recv()
            .map_err(|error| ProvenanceError::Query(format!("worker channel recv: {error}")))?
    }

    /// Insert an edge of `kind` from `src` to `dst` under the caller-supplied
    /// sequence-ordered `id`. Refused when either endpoint is missing, when
    /// `id` is already live, or when the edge would close a cycle in the
    /// lineage graph.
    pub fn insert_edge(
        &self,
        id: EdgeId,
        kind: EdgeKind,
        src: NodeId,
        dst: NodeId,
    ) -> Result<EdgeId, ProvenanceError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.request_tx
            .send(Op::InsertEdge {
                id,
                kind,
                src,
                dst,
                reply: reply_tx,
            })
            .map_err(|error| ProvenanceError::Query(format!("worker channel send: {error}")))?;
        reply_rx
            .recv()
            .map_err(|error| ProvenanceError::Query(format!("worker channel recv: {error}")))?
    }

    /// Ancestors of `seed` reachable along authored edges within
    /// `max_depth` steps, grouped and sorted deterministically. Errors with
    /// [`ProvenanceError::MissingNode`] when `seed` is not a node in the
    /// graph.
    pub fn lineage(&self, seed: NodeId, max_depth: usize) -> Result<Lineage, ProvenanceError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.request_tx
            .send(Op::Lineage {
                seed,
                max_depth,
                reply: reply_tx,
            })
            .map_err(|error| ProvenanceError::Query(format!("worker channel send: {error}")))?;
        reply_rx
            .recv()
            .map_err(|error| ProvenanceError::Query(format!("worker channel recv: {error}")))?
    }

    /// The persisted kind of a node, or `None` when the id is not live.
    pub fn node_kind(&self, id: NodeId) -> Result<Option<NodeKind>, ProvenanceError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.request_tx
            .send(Op::NodeKind {
                id,
                reply: reply_tx,
            })
            .map_err(|error| ProvenanceError::Query(format!("worker channel send: {error}")))?;
        reply_rx
            .recv()
            .map_err(|error| ProvenanceError::Query(format!("worker channel recv: {error}")))?
    }
}

impl Drop for ProvenanceStore {
    fn drop(&mut self) {
        let _ = self.request_tx.send(Op::Close);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Adjacency snapshot over the engine's read view, feeding the pure
/// std-only lineage/cycle algorithms.
struct FlatAdj {
    edges: Vec<AuthoredEdge>,
}

impl Adjacency for FlatAdj {
    fn edges(&self) -> Vec<AuthoredEdge> {
        self.edges.clone()
    }
}

/// Worker thread entry: build the runtime, open or create the graph, report
/// readiness, then serve ops until Close or channel disconnect.
fn worker_entry(
    path: &Path,
    request_rx: mpsc::Receiver<Op>,
    open_tx: mpsc::Sender<Result<(), ProvenanceError>>,
) {
    let runtime = match RuntimeBuilder::current_thread().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = open_tx.send(Err(ProvenanceError::Open(format!(
                "runtime driver: {error}"
            ))));
            return;
        }
    };
    let root = runtime.request_cx_with_budget(Budget::INFINITE);
    let contexts = PurposeContexts::narrow_runtime_root(&root);
    let cx = contexts.commit();
    let mut db = match open_or_create(&runtime, &cx, path) {
        Ok(db) => db,
        Err(error) => {
            let _ = open_tx.send(Err(error));
            return;
        }
    };
    let _ = open_tx.send(Ok(()));
    for op in request_rx {
        match op {
            Op::Close => break,
            Op::InsertNode { id, kind, reply } => {
                let mut batch = fgdb::WriteBatch::new(RelationId(1));
                batch.create_vertex(VId(u128::from(id.0)), vec![LabelId(kind.label())], vec![]);
                let result = runtime
                    .block_on(db.write(&cx, batch))
                    .map(|_| id)
                    .map_err(map_write);
                let _ = reply.send(result);
            }
            Op::InsertEdge {
                id,
                kind,
                src,
                dst,
                reply,
            } => {
                let result = insert_edge_drive(&runtime, &cx, &mut db, id, kind, src, dst);
                let _ = reply.send(result);
            }
            Op::Lineage {
                seed,
                max_depth,
                reply,
            } => {
                let result = lineage_drive(&db, seed, max_depth);
                let _ = reply.send(result);
            }
            Op::NodeKind { id, reply } => {
                let result = node_kind_drive(&db, id);
                let _ = reply.send(result);
            }
        }
    }
}

/// Create when the directory is missing or empty; open an existing graph.
fn open_or_create(
    runtime: &Runtime,
    cx: &CommitCx,
    path: &Path,
) -> Result<Database, ProvenanceError> {
    let (k_oid, namespace, dek) = dev_key_material();
    let keys = fgdb::DatabaseKeys {
        k_oid,
        namespace,
        dek,
    };
    let should_create = match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err(ProvenanceError::Open(format!(
                "{path:?} exists and is not a directory",
            )));
        }
        Ok(_) => std::fs::read_dir(path)
            .map_err(|error| ProvenanceError::Open(format!("read_dir {path:?}: {error}")))?
            .next()
            .is_none(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => return Err(ProvenanceError::Open(format!("stat {path:?}: {error}"))),
    };
    if should_create {
        runtime
            .block_on(Database::create(&cx, path, keys))
            .map_err(|error| ProvenanceError::Open(format!("create at {path:?}: {error:?}")))
    } else {
        runtime
            .block_on(Database::open(&cx, path, keys))
            .map_err(|error| ProvenanceError::Open(format!("open at {path:?}: {error:?}")))
    }
}

/// Insert an edge: existence + cycle pre-checks against the pinned read
/// view, then the engine write (which is the durable authority and still
/// refuses duplicates/spent identities/gaps).
fn insert_edge_drive(
    runtime: &Runtime,
    cx: &CommitCx,
    db: &mut Database,
    id: EdgeId,
    kind: EdgeKind,
    src: NodeId,
    dst: NodeId,
) -> Result<EdgeId, ProvenanceError> {
    let view = db.pinned_read_view().map_err(map_read)?;
    if view
        .vertex(VId(u128::from(src.0)))
        .map_err(map_read)?
        .is_none()
    {
        return Err(ProvenanceError::MissingNode(src));
    }
    if view
        .vertex(VId(u128::from(dst.0)))
        .map_err(map_read)?
        .is_none()
    {
        return Err(ProvenanceError::MissingNode(dst));
    }
    let adjacency = snapshot_adjacency(&view)?;
    if would_create_cycle(&adjacency, src, dst) {
        return Err(ProvenanceError::Cycle { from: src, to: dst });
    }
    drop(view);
    let mut batch = fgdb::WriteBatch::new(RelationId(kind.relation()));
    batch.add_edge(
        EId(u128::from(id.0)),
        VId(u128::from(src.0)),
        VId(u128::from(dst.0)),
        vec![],
    );
    runtime
        .block_on(db.write(&cx, batch))
        .map(|_| id)
        .map_err(map_write)
}

/// Lineage of `seed`: existence check, adjacency snapshot, pure closure.
fn lineage_drive(
    db: &Database,
    seed: NodeId,
    max_depth: usize,
) -> Result<Lineage, ProvenanceError> {
    let view = db.pinned_read_view().map_err(map_read)?;
    if view
        .vertex(VId(u128::from(seed.0)))
        .map_err(map_read)?
        .is_none()
    {
        return Err(ProvenanceError::MissingNode(seed));
    }
    let adjacency = snapshot_adjacency(&view)?;
    Ok(lineage_closure(
        &adjacency,
        seed,
        max_depth.min(MAX_LINEAGE_DEPTH),
    ))
}

/// Persisted kind of a node (its vertex label).
fn node_kind_drive(db: &Database, id: NodeId) -> Result<Option<NodeKind>, ProvenanceError> {
    let view = db.pinned_read_view().map_err(map_read)?;
    let row = view.vertex(VId(u128::from(id.0))).map_err(map_read)?;
    Ok(row.and_then(|vertex| {
        vertex
            .labels
            .first()
            .and_then(|label| NodeKind::from_label(label.0))
    }))
}

/// Build the adjacency snapshot the pure algorithms consume from the engine's
/// live-edge set (relation → kind; ids truncate u64→u128 and back losslessly
/// for this spike's identity range).
fn snapshot_adjacency(view: &fgdb::EmbeddedReadView) -> Result<FlatAdj, ProvenanceError> {
    let mut edges = Vec::new();
    for record in view.edges().map_err(map_read)? {
        let kind = EdgeKind::from_relation(record.entry.relation.0).ok_or_else(|| {
            ProvenanceError::Query(format!("unknown relation {}", record.entry.relation.0))
        })?;
        edges.push(AuthoredEdge {
            id: EdgeId(record.entry.eid.0 as u64),
            kind,
            src: NodeId(record.entry.src.0 as u64),
            dst: NodeId(record.entry.dst.0 as u64),
        });
    }
    Ok(FlatAdj { edges })
}

fn map_read(error: fgdb::ReadError) -> ProvenanceError {
    ProvenanceError::Query(format!("{error:?}"))
}

/// Map fgdb write refusals onto the crate error model. The engine's
/// pre-commit discipline (refuse BEFORE the two-fsync commit) is the durable
/// authority; the crate's pre-checks are the friendly fast path.
fn map_write(error: fgdb::WriteError) -> ProvenanceError {
    match error {
        fgdb::WriteError::AlreadyLive { elem } | fgdb::WriteError::IdentitySpent { elem } => {
            match elem {
                ElementId::Vertex(vid) => ProvenanceError::DuplicateNode(NodeId(vid.0 as u64)),
                ElementId::Edge(eid) => ProvenanceError::DuplicateEdge(EdgeId(eid.0 as u64)),
            }
        }
        fgdb::WriteError::DanglingEndpoint { endpoint, .. } => {
            ProvenanceError::MissingNode(NodeId(endpoint.0 as u64))
        }
        fgdb::WriteError::EmptyBatch => {
            ProvenanceError::Query("engine refused an empty batch".into())
        }
        other => ProvenanceError::Query(format!("{other:?}")),
    }
}

#[cfg(all(test, feature = "graphdb"))]
mod tests {
    use super::ProvenanceStore;
    use crate::{EdgeId, EdgeKind, Lineage, NodeId, NodeKind, ProvenanceError};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SCRATCH: AtomicU64 = AtomicU64::new(0);

    fn scratch_dir(name: &str) -> PathBuf {
        let n = SCRATCH.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "emath-provenance-{}-{name}-{n}",
            std::process::id()
        ))
    }

    fn make_chain(store: &ProvenanceStore) -> Result<(), ProvenanceError> {
        store.insert_node(NodeId(1), NodeKind::Goal)?;
        store.insert_node(NodeId(2), NodeKind::Plan)?;
        store.insert_node(NodeId(3), NodeKind::Artifact)?;
        store.insert_edge(EdgeId(1), EdgeKind::PlanOf, NodeId(2), NodeId(1))?;
        store.insert_edge(EdgeId(2), EdgeKind::ArtifactOf, NodeId(3), NodeId(2))?;
        Ok(())
    }

    #[test]
    fn happy_path_lineage_from_artifact() {
        let dir = scratch_dir("happy");
        let store = ProvenanceStore::open(&dir).expect("open");
        make_chain(&store).expect("chain");
        let lineage = store.lineage(NodeId(3), 16).expect("lineage");
        assert_eq!(
            lineage,
            Lineage {
                goals: vec![NodeId(1)],
                plans: vec![NodeId(2)],
                ..Lineage::default()
            }
        );
        assert_eq!(
            store.node_kind(NodeId(1)).expect("kind"),
            Some(NodeKind::Goal)
        );
        assert_eq!(
            store.node_kind(NodeId(3)).expect("kind"),
            Some(NodeKind::Artifact)
        );
        assert_eq!(store.node_kind(NodeId(9)).expect("kind"), None);
    }

    #[test]
    fn empty_graph_lineage_errors_missing_node() {
        let dir = scratch_dir("empty");
        let store = ProvenanceStore::open(&dir).expect("open");
        assert!(matches!(
            store.lineage(NodeId(7), 8),
            Err(ProvenanceError::MissingNode(NodeId(7)))
        ));
    }

    #[test]
    fn single_node_has_empty_lineage() {
        let dir = scratch_dir("single");
        let store = ProvenanceStore::open(&dir).expect("open");
        store.insert_node(NodeId(1), NodeKind::Goal).expect("node");
        let lineage = store.lineage(NodeId(1), 8).expect("lineage");
        assert!(lineage.is_empty());
    }

    #[test]
    fn duplicate_node_id_is_refused() {
        let dir = scratch_dir("dupnode");
        let store = ProvenanceStore::open(&dir).expect("open");
        store.insert_node(NodeId(1), NodeKind::Goal).expect("node");
        assert!(matches!(
            store.insert_node(NodeId(1), NodeKind::Plan),
            Err(ProvenanceError::DuplicateNode(NodeId(1)))
        ));
    }

    #[test]
    fn duplicate_edge_id_is_refused() {
        let dir = scratch_dir("dupedge");
        let store = ProvenanceStore::open(&dir).expect("open");
        store.insert_node(NodeId(1), NodeKind::Goal).expect("node");
        store.insert_node(NodeId(2), NodeKind::Plan).expect("node");
        store
            .insert_edge(EdgeId(1), EdgeKind::PlanOf, NodeId(2), NodeId(1))
            .expect("edge");
        assert!(matches!(
            store.insert_edge(EdgeId(1), EdgeKind::PlanOf, NodeId(2), NodeId(1)),
            Err(ProvenanceError::DuplicateEdge(EdgeId(1)))
        ));
    }

    #[test]
    fn cyclic_edge_is_rejected_before_write() {
        let dir = scratch_dir("cycle");
        let store = ProvenanceStore::open(&dir).expect("open");
        make_chain(&store).expect("chain");
        // goal → artifact closes artifact → plan → goal → artifact.
        let result = store.insert_edge(EdgeId(3), EdgeKind::PlanOf, NodeId(1), NodeId(3));
        assert!(matches!(
            result,
            Err(ProvenanceError::Cycle {
                from: NodeId(1),
                to: NodeId(3),
            })
        ));
        // A self-loop is also a cycle.
        assert!(matches!(
            store.insert_edge(EdgeId(4), EdgeKind::PlanOf, NodeId(1), NodeId(1)),
            Err(ProvenanceError::Cycle { .. })
        ));
        assert_eq!(store.lineage(NodeId(3), 16).expect("lineage").len(), 2);
    }

    #[test]
    fn edge_to_missing_node_is_refused() {
        let dir = scratch_dir("missing");
        let store = ProvenanceStore::open(&dir).expect("open");
        store.insert_node(NodeId(1), NodeKind::Goal).expect("node");
        assert!(matches!(
            store.insert_edge(EdgeId(1), EdgeKind::PlanOf, NodeId(2), NodeId(1)),
            Err(ProvenanceError::MissingNode(NodeId(2)))
        ));
        assert!(matches!(
            store.insert_edge(EdgeId(2), EdgeKind::PlanOf, NodeId(1), NodeId(9)),
            Err(ProvenanceError::MissingNode(NodeId(9)))
        ));
    }

    #[test]
    fn identical_inserts_produce_identical_results_across_stores() {
        let first = scratch_dir("det-a");
        let second = scratch_dir("det-b");
        let store_a = ProvenanceStore::open(&first).expect("open a");
        let store_b = ProvenanceStore::open(&second).expect("open b");
        make_chain(&store_a).expect("chain a");
        make_chain(&store_b).expect("chain b");
        assert_eq!(
            store_a.lineage(NodeId(3), 16).expect("lineage a"),
            store_b.lineage(NodeId(3), 16).expect("lineage b")
        );
        assert_eq!(
            store_a.node_kind(NodeId(2)).expect("kind a"),
            store_b.node_kind(NodeId(2)).expect("kind b")
        );
    }

    #[test]
    fn reopen_folds_identical_history() {
        let dir = scratch_dir("reopen");
        let lineage_before;
        let kinds_before;
        {
            let store = ProvenanceStore::open(&dir).expect("open");
            make_chain(&store).expect("chain");
            lineage_before = store.lineage(NodeId(3), 16).expect("lineage");
            kinds_before = store.node_kind(NodeId(2)).expect("kind");
        }
        let reopened = ProvenanceStore::open(&dir).expect("reopen");
        assert_eq!(
            reopened.lineage(NodeId(3), 16).expect("lineage after"),
            lineage_before
        );
        assert_eq!(
            reopened.node_kind(NodeId(2)).expect("kind after"),
            kinds_before
        );
    }

    #[test]
    fn max_depth_limits_lineage() {
        let dir = scratch_dir("depth");
        let store = ProvenanceStore::open(&dir).expect("open");
        make_chain(&store).expect("chain");
        store
            .insert_node(NodeId(4), NodeKind::Evidence)
            .expect("node");
        store
            .insert_edge(EdgeId(3), EdgeKind::EvidenceOf, NodeId(4), NodeId(3))
            .expect("edge");
        // evidence(4) → artifact(3) → plan(2) → goal(1).
        let depth1 = store.lineage(NodeId(4), 1).expect("depth 1");
        assert_eq!(depth1.artifacts, vec![NodeId(3)]);
        assert!(depth1.plans.is_empty());
        let depth2 = store.lineage(NodeId(4), 2).expect("depth 2");
        assert_eq!(depth2.artifacts, vec![NodeId(3)]);
        assert_eq!(depth2.plans, vec![NodeId(2)]);
        let depth3 = store.lineage(NodeId(4), 3).expect("depth 3");
        assert_eq!(depth3.goals, vec![NodeId(1)]);
        assert_eq!(depth3, store.lineage(NodeId(4), 16).expect("depth far"));
    }
}
