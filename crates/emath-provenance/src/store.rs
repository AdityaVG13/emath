//! Blocking provenance store over the async frankengraphdb engine (graphdb).
//!
//! The engine (`fgdb` + asupersync) is async-native for open/create/write and
//! sync for reads, so the `Database` and the asupersync runtime live on a
//! dedicated worker thread with a large stack; `ProvenanceStore` is a channel
//! proxy over that worker (emath-store sqlite facade precedent, `CUTOVER_PLAN`
//! §9.10). Single-writer by design (one owner per graph directory); see
//! CONTRACT.md. All public methods are blocking.

use std::path::Path;
use std::sync::mpsc;

use asupersync::runtime::{Runtime, RuntimeBuilder};
use asupersync::Budget;
use fgdb::Database;
use fgdb_delta_types::{ElementId, LabelId, RelationId};
use fgdb_types::context::{CommitCx, PurposeContexts};
use fgdb_types::{DatabaseSecurityNamespaceId, EId, VId};

use crate::{
    lineage_closure, would_create_cycle, Adjacency, AuthoredEdge, EdgeId, EdgeKind, Lineage,
    NodeId, NodeKind, ProvenanceError, MAX_LINEAGE_DEPTH,
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
            .spawn(move || worker_entry(&worker_path, request_rx, &open_tx))
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
    fn edges(&self) -> &[AuthoredEdge] {
        &self.edges
    }
}

/// Worker thread entry: build the runtime, open or create the graph, report
/// readiness, then serve ops until Close or channel disconnect.
fn worker_entry(
    path: &Path,
    request_rx: mpsc::Receiver<Op>,
    open_tx: &mpsc::Sender<Result<(), ProvenanceError>>,
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
                "{} exists and is not a directory",
                path.display(),
            )));
        }
        Ok(_) => std::fs::read_dir(path)
            .map_err(|error| {
                ProvenanceError::Open(format!("read_dir {}: {error}", path.display()))
            })?
            .next()
            .is_none(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => {
            return Err(ProvenanceError::Open(format!(
                "stat {}: {error}",
                path.display()
            )));
        }
    };
    if should_create {
        runtime
            .block_on(Database::create(cx, path, keys))
            .map_err(|error| {
                ProvenanceError::Open(format!("create at {}: {error}", path.display()))
            })
    } else {
        runtime
            .block_on(Database::open(cx, path, keys))
            .map_err(|error| ProvenanceError::Open(format!("open at {}: {error}", path.display())))
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
    let view = db.pinned_read_view().map_err(|e| map_read(&e))?;
    if view
        .vertex(VId(u128::from(src.0)))
        .map_err(|e| map_read(&e))?
        .is_none()
    {
        return Err(ProvenanceError::MissingNode(src));
    }
    if view
        .vertex(VId(u128::from(dst.0)))
        .map_err(|e| map_read(&e))?
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
        .block_on(db.write(cx, batch))
        .map(|_| id)
        .map_err(map_write)
}

/// Lineage of `seed`: existence check, adjacency snapshot, pure closure.
fn lineage_drive(
    db: &Database,
    seed: NodeId,
    max_depth: usize,
) -> Result<Lineage, ProvenanceError> {
    let view = db.pinned_read_view().map_err(|e| map_read(&e))?;
    if view
        .vertex(VId(u128::from(seed.0)))
        .map_err(|e| map_read(&e))?
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
    let view = db.pinned_read_view().map_err(|e| map_read(&e))?;
    let row = view
        .vertex(VId(u128::from(id.0)))
        .map_err(|e| map_read(&e))?;
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
    for record in view.edges().map_err(|e| map_read(&e))? {
        let kind = EdgeKind::from_relation(record.entry.relation.0).ok_or_else(|| {
            ProvenanceError::Query(format!("unknown relation {}", record.entry.relation.0))
        })?;
        edges.push(AuthoredEdge {
            id: edge_from_u128(record.entry.eid.0)?,
            kind,
            src: node_from_u128(record.entry.src.0)?,
            dst: node_from_u128(record.entry.dst.0)?,
        });
    }
    Ok(FlatAdj { edges })
}

fn map_read(error: &fgdb::ReadError) -> ProvenanceError {
    ProvenanceError::Query(format!("{error:?}"))
}

/// Lossless u128 → u64 node id for the spike's identity range (ids are
/// u64-origin; the engine widens to u128). Refuses overflow instead of
/// truncating.
fn node_from_u128(v: u128) -> Result<NodeId, ProvenanceError> {
    u64::try_from(v)
        .map(NodeId)
        .map_err(|_| ProvenanceError::Query(format!("node id exceeds u64: {v}")))
}

/// Lossless u128 → u64 edge id; see `node_from_u128`.
fn edge_from_u128(e: u128) -> Result<EdgeId, ProvenanceError> {
    u64::try_from(e)
        .map(EdgeId)
        .map_err(|_| ProvenanceError::Query(format!("edge id exceeds u64: {e}")))
}

/// Map fgdb write refusals onto the crate error model. The engine's
/// pre-commit discipline (refuse BEFORE the two-fsync commit) is the durable
/// authority; the crate's pre-checks are the friendly fast path.
fn map_write(error: fgdb::WriteError) -> ProvenanceError {
    match error {
        fgdb::WriteError::AlreadyLive { elem } | fgdb::WriteError::IdentitySpent { elem } => {
            match elem {
                ElementId::Vertex(vid) => match node_from_u128(vid.0) {
                    Ok(id) => ProvenanceError::DuplicateNode(id),
                    Err(error) => error,
                },
                ElementId::Edge(eid) => match edge_from_u128(eid.0) {
                    Ok(id) => ProvenanceError::DuplicateEdge(id),
                    Err(error) => error,
                },
            }
        }
        fgdb::WriteError::DanglingEndpoint { endpoint, .. } => match node_from_u128(endpoint.0) {
            Ok(id) => ProvenanceError::MissingNode(id),
            Err(error) => error,
        },
        fgdb::WriteError::EmptyBatch => {
            ProvenanceError::Query("engine refused an empty batch".into())
        }
        other => ProvenanceError::Query(format!("{other:?}")),
    }
}
