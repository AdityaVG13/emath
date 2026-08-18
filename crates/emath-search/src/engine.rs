//! Blocking search facade over the async frankensearch engine (`search`).
//!
//! frankensearch is asupersync-native (no tokio verified at the pinned rev):
//! `IndexBuilder::build` and `TwoTierSearcher::search_collect` are async and
//! require `&Cx`. Mirroring the emath-store / emath-provenance worker-thread
//! precedent (`CUTOVER_PLAN` §9.10 / §9.11), the engine + an asupersync runtime
//! live on a dedicated worker thread with a large stack; `CorpusSearch` is a
//! channel proxy over that worker. All public methods are blocking. The
//! engine's result ordering is authoritative — this crate never re-sorts
//! frankensearch results (skill: f64-precision ordering must be trusted).
//!
//! The skill's integration shape (SKILL.md Phase 1 + API-REFERENCE.md) is
//! applied: `IndexBuilder` for indexing, `TwoTierSearcher` (NOT hand-rolled
//! fusion) for search, `EmbedderStack` for the embedder pair, one composite
//! doc-id encoding scheme, `RrfConfig`-driven hybrid fusion inside the
//! searcher. This pass drives a hash-control embedder stack (offline,
//! deterministic) with the native Quill BM25 lexical arm; the semantic model
//! tiers are documented no-claims (CONTRACT.md).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

use asupersync::Budget;
use asupersync::runtime::{Runtime, RuntimeBuilder};
use frankensearch::quill::{QuillConfig, RootBoundQuillSearchIndex};
use frankensearch::{
    Cx, Embedder, EmbedderStack, HashEmbedder, IndexBuilder, IndexableDocument, LexicalRead,
    ScoreSource, ScoredResult, SearchError as FsError, TwoTierConfig, TwoTierIndex,
    TwoTierSearcher,
};

use crate::ArtifactDoc;
use crate::corpus::from_fs_doc_id;
use crate::error::SearchError;

/// Worker stack: frankensearch build/search futures recurse deeply while
/// polling, so the engine runs on a large-stack thread, never on the caller's
/// thread (emath-provenance precedent).
const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

/// Index config: stock defaults, deliberately WITHOUT `with_env_overrides()`
/// so no `FRANKENSEARCH_*` environment can inject nondeterminism into the
/// spike lane.
fn index_config() -> TwoTierConfig {
    TwoTierConfig::default()
}

/// Build the embedder stack for this pass: explicit hash-control stack (never
/// auto-detect, which would attempt model discovery and refuse hash-only
/// stacks). Deterministic and offline.
fn control_stack() -> EmbedderStack {
    let fast = Arc::new(HashEmbedder::default_256()) as Arc<dyn Embedder>;
    EmbedderStack::from_parts(fast, None)
}

/// Engine operation dispatched to the worker thread. Each op carries its own
/// reply channel typed to the op result.
enum Op {
    Build {
        docs: Vec<ArtifactDoc>,
        reply: mpsc::Sender<Result<IndexStats, SearchError>>,
    },
    Open {
        reply: mpsc::Sender<Result<(), SearchError>>,
    },
    Search {
        query: String,
        k: usize,
        reply: mpsc::Sender<Result<Vec<Hit>, SearchError>>,
    },
    Reindex {
        docs: Vec<ArtifactDoc>,
        reply: mpsc::Sender<Result<IndexStats, SearchError>>,
    },
    RemoveIndex {
        reply: mpsc::Sender<Result<(), SearchError>>,
    },
    Close,
}

/// Blocking handle: a channel proxy over the engine worker thread. One handle
/// owns one index directory (single-writer).
pub struct CorpusSearch {
    request_tx: mpsc::Sender<Op>,
    worker: Option<thread::JoinHandle<()>>,
}

impl CorpusSearch {
    /// Create a fresh index at `path` from `docs`: drop any previous index
    /// artifacts, build via the engine's `IndexBuilder`, then open the
    /// searcher (with the Quill lexical arm when the build wrote one).
    pub fn create(path: impl AsRef<Path>, docs: &[ArtifactDoc]) -> Result<Self, SearchError> {
        let mut handle = Self::spawn(path)?;
        let (reply_tx, reply_rx) = mpsc::channel();
        let send = handle.send_op(Op::Build {
            docs: docs.to_vec(),
            reply: reply_tx,
        });
        if send.is_err() {
            handle.shutdown();
            return Err(SearchError::WorkerDown);
        }
        match reply_rx.recv().map_err(|_| SearchError::WorkerDown)? {
            Ok(_) => Ok(handle),
            Err(error) => {
                handle.shutdown();
                Err(error)
            }
        }
    }

    /// Open an existing index at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SearchError> {
        let mut handle = Self::spawn(path)?;
        let (reply_tx, reply_rx) = mpsc::channel();
        handle
            .send_op(Op::Open { reply: reply_tx })
            .map_err(|_| SearchError::WorkerDown)?;
        match reply_rx.recv().map_err(|_| SearchError::WorkerDown)? {
            Ok(()) => Ok(handle),
            Err(error) => {
                handle.shutdown();
                Err(error)
            }
        }
    }

    /// Rebuild the index at this handle's directory from `docs` (drop + build
    /// + reopen). Existing search states are replaced.
    pub fn reindex(&self, docs: &[ArtifactDoc]) -> Result<IndexStats, SearchError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.send_op(Op::Reindex {
            docs: docs.to_vec(),
            reply: reply_tx,
        })
        .map_err(|_| SearchError::WorkerDown)?;
        reply_rx.recv().map_err(|_| SearchError::WorkerDown)?
    }

    /// Remove all index artifacts under this handle's directory (the
    /// directory itself is left in place). Subsequent searches return
    /// [`SearchError::NotReady`] until a create/reindex.
    pub fn remove_index(&self) -> Result<(), SearchError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.send_op(Op::RemoveIndex { reply: reply_tx })
            .map_err(|_| SearchError::WorkerDown)?;
        reply_rx.recv().map_err(|_| SearchError::WorkerDown)?
    }

    /// Search the corpus. `k == 0` returns an empty result list without
    /// touching the engine; `k` larger than the corpus returns all hits.
    /// Result order is the engine's authoritative order — never re-sorted.
    pub fn search(&self, query: &str, k: usize) -> Result<Vec<Hit>, SearchError> {
        let query = query.trim();
        if query.is_empty() {
            return Err(SearchError::InvalidArgument {
                field: "query",
                reason: "must be non-empty".into(),
            });
        }
        if k == 0 {
            return Ok(Vec::new());
        }
        let (reply_tx, reply_rx) = mpsc::channel();
        self.send_op(Op::Search {
            query: query.to_string(),
            k,
            reply: reply_tx,
        })
        .map_err(|_| SearchError::WorkerDown)?;
        reply_rx.recv().map_err(|_| SearchError::WorkerDown)?
    }

    fn spawn(path: impl AsRef<Path>) -> Result<Self, SearchError> {
        let (request_tx, request_rx) = mpsc::channel::<Op>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), SearchError>>();
        let path = path.as_ref().to_path_buf();
        let worker_path = path.clone();
        let worker = thread::Builder::new()
            .name("emath-search".into())
            .stack_size(WORKER_STACK_BYTES)
            .spawn(move || worker_entry(&worker_path, request_rx, &ready_tx))
            .map_err(|error| SearchError::Open {
                path: path.display().to_string(),
                reason: format!("worker thread: {error}"),
            })?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(CorpusSearch {
                request_tx,
                worker: Some(worker),
            }),
            Ok(Err(error)) => Err(error),
            Err(error) => Err(SearchError::Open {
                path: path.display().to_string(),
                reason: format!("worker channel closed: {error}"),
            }),
        }
    }

    fn send_op(&self, op: Op) -> Result<(), mpsc::SendError<Op>> {
        self.request_tx.send(op)
    }

    fn shutdown(&mut self) {
        let _ = self.request_tx.send(Op::Close);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for CorpusSearch {
    fn drop(&mut self) {
        let _ = self.request_tx.send(Op::Close);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Per-document result of a search.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// Frankensearch document id (`kind \x1f id`).
    pub doc_id: String,
    /// Decoded artifact kind (empty when the id is not a composite this crate
    /// produced).
    pub kind: String,
    /// Decoded artifact id.
    pub id: String,
    /// Engine score (f32 is the engine's own carrier; never narrowed).
    pub score: f32,
    /// Backend that produced the hit.
    pub source: HitSource,
}

/// Surfaced subset of the engine's `ScoreSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitSource {
    Lexical,
    Semantic,
    Hybrid,
    HashControl,
    Reranked,
    Other,
}

/// Aggregate result of a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStats {
    /// Valid source documents submitted to the build.
    pub source_count: usize,
    /// Documents indexed into the fast vector tier.
    pub doc_count: usize,
    /// Documents whose fast embedding failed.
    pub error_count: usize,
    /// Documents indexed into the quality vector tier (always 0 at this
    /// pass — hash control stack writes no quality tier).
    pub quality_indexed: usize,
    /// Lexical (Quill BM25) arm receipt, when one was written.
    pub lexical: Option<LexicalArmStats>,
}

/// Receipt for the lexical indexing arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalArmStats {
    pub backend: &'static str,
    pub path: String,
    pub attempted: usize,
    pub indexed: usize,
    pub errors: Vec<(String, String)>,
}

/// Worker-thread index state.
struct Worker {
    runtime: Runtime,
    cx: Cx,
    path: PathBuf,
    searcher: Option<SearcherState>,
}

/// Open searcher held by the worker.
struct SearcherState {
    searcher: TwoTierSearcher,
}

/// Worker thread entry: build the runtime, report readiness, then serve ops
/// until Close or channel disconnect.
fn worker_entry(
    path: &Path,
    request_rx: mpsc::Receiver<Op>,
    ready_tx: &mpsc::Sender<Result<(), SearchError>>,
) {
    let runtime = match RuntimeBuilder::current_thread().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready_tx.send(Err(SearchError::Open {
                path: path.display().to_string(),
                reason: format!("runtime driver: {error}"),
            }));
            return;
        }
    };
    let cx = runtime.request_cx_with_budget(Budget::INFINITE);
    let mut worker = Worker {
        runtime,
        cx,
        path: path.to_path_buf(),
        searcher: None,
    };
    let _ = ready_tx.send(Ok(()));
    for op in request_rx {
        match op {
            Op::Close => break,
            Op::Build { docs, reply } => {
                let result = build_drive(&mut worker, &docs);
                let _ = reply.send(result);
            }
            Op::Open { reply } => {
                let result = open_drive(&mut worker);
                let _ = reply.send(result);
            }
            Op::Search { query, k, reply } => {
                let result = search_drive(&worker, &query, k);
                let _ = reply.send(result);
            }
            Op::Reindex { docs, reply } => {
                let result = (|| {
                    remove_drive(&worker.path)?;
                    build_drive(&mut worker, &docs)
                })();
                let _ = reply.send(result);
            }
            Op::RemoveIndex { reply } => {
                let result = remove_drive(&worker.path);
                worker.searcher = None;
                let _ = reply.send(result);
            }
        }
    }
}

/// Build state, and note on `Build` op: it both builds and reopens so the
/// handle is immediately searchable. Errors leave any previous state intact.
fn build_drive(worker: &mut Worker, docs: &[ArtifactDoc]) -> Result<IndexStats, SearchError> {
    if docs.is_empty() {
        return Err(SearchError::InvalidArgument {
            field: "docs",
            reason: "corpus must be non-empty (engine refuses zero-document builds)".into(),
        });
    }
    // Drop stale artifacts so a re-create over an existing directory starts
    // from a clean slate (deterministic reindex).
    remove_drive(&worker.path)?;

    let documents: Vec<IndexableDocument> = docs
        .iter()
        .map(|doc| IndexableDocument {
            id: doc.fs_doc_id().expect("validated composite id"),
            content: doc.text.clone(),
            title: None,
            metadata: HashMap::default(),
        })
        .collect();

    let builder = IndexBuilder::new(&worker.path)
        .with_embedder_stack(control_stack())
        .with_config(index_config())
        .add_documents(documents);
    let stats = worker
        .runtime
        .block_on(builder.build(&worker.cx))
        .map_err(map_fs_error)?;

    // Reopen so the handle is searchable immediately.
    // We need the built stats; open_drive sets the searcher state.
    open_drive(worker)?;

    let lexical = stats.lexical.as_ref().map(|receipt| LexicalArmStats {
        backend: receipt.backend,
        path: receipt.path.display().to_string(),
        attempted: receipt.attempted,
        indexed: receipt.indexed,
        errors: receipt.errors.clone(),
    });
    let result = IndexStats {
        source_count: stats.source_count,
        doc_count: stats.doc_count,
        error_count: stats.error_count,
        quality_indexed: stats.quality_indexed,
        lexical,
    };
    if stats.error_count > 0 {
        return Err(SearchError::Build {
            report: format!(
                "{} of {} documents failed to embed: {}",
                stats.error_count,
                stats.source_count,
                stats
                    .errors
                    .iter()
                    .map(|(id, err)| format!("{id}: {err}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        });
    }
    Ok(result)
}

/// Open the index at the worker's directory into fresh searcher state.
fn open_drive(worker: &mut Worker) -> Result<(), SearchError> {
    let vectors = Arc::new(
        TwoTierIndex::open(&worker.path, index_config())
            .map_err(|error| map_open(worker, &error))?,
    );
    let fast = Arc::new(HashEmbedder::default_256());
    let mut searcher = TwoTierSearcher::new(vectors.clone(), fast.clone(), index_config())
        .with_nqc_dense_downweight_disabled();

    // Lexical arm: attach the Quill BM25 reader when the build wrote one.
    // (open_hybrid cannot be used here: it refuses hash-identity generations
    // by design; we mirror its documented manual shape — TwoTierSearcher::new
    // + with_lexical — instead.)
    let lexical_dir = worker.path.join("lexical");
    if lexical_dir.is_dir() {
        let index = worker
            .runtime
            .block_on(RootBoundQuillSearchIndex::open(
                &worker.cx,
                &lexical_dir,
                QuillConfig::default(),
            ))
            .map_err(|error| SearchError::Open {
                path: lexical_dir.display().to_string(),
                reason: format!("quill reader: {error}"),
            })?;
        let read: Arc<dyn LexicalRead> = Arc::new(index);
        searcher = searcher.with_lexical(read);
    }

    worker.searcher = Some(SearcherState { searcher });
    Ok(())
}

/// Run a query against the open searcher. `NotReady` when no index is open.
fn search_drive(worker: &Worker, query: &str, k: usize) -> Result<Vec<Hit>, SearchError> {
    let state = worker
        .searcher
        .as_ref()
        .ok_or_else(|| SearchError::NotReady {
            reason: "no index is built or open at this directory".into(),
        })?;
    let (results, _metrics) = worker
        .runtime
        .block_on(state.searcher.search_collect(&worker.cx, query, k))
        .map_err(map_fs_error)?;
    Ok(results.iter().map(map_hit).collect())
}

/// Remove every artifact under the index directory (the directory itself is
/// left in place).
fn remove_drive(path: &Path) -> Result<(), SearchError> {
    match std::fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|error| SearchError::Query {
                    reason: format!("read dir entry {}: {error}", path.display()),
                })?;
                let target = entry.path();
                if target.is_dir() {
                    std::fs::remove_dir_all(&target).map_err(|error| SearchError::Query {
                        reason: format!("remove dir {}: {error}", target.display()),
                    })?;
                } else {
                    std::fs::remove_file(&target).map_err(|error| SearchError::Query {
                        reason: format!("remove file {}: {error}", target.display()),
                    })?;
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(SearchError::Query {
            reason: format!("read dir {}: {error}", path.display()),
        }),
    }
}

fn map_hit(result: &ScoredResult) -> Hit {
    let (kind, id) = from_fs_doc_id(&result.doc_id)
        .unwrap_or_else(|| (String::new(), result.doc_id.to_string()));
    Hit {
        doc_id: result.doc_id.to_string(),
        kind,
        id,
        score: result.score,
        source: match result.source {
            ScoreSource::Lexical => HitSource::Lexical,
            ScoreSource::SemanticFast | ScoreSource::SemanticQuality => HitSource::Semantic,
            ScoreSource::Hybrid => HitSource::Hybrid,
            ScoreSource::Reranked => HitSource::Reranked,
            ScoreSource::HashControl => HitSource::HashControl,
        },
    }
}

fn map_open(worker: &Worker, error: &FsError) -> SearchError {
    SearchError::Open {
        path: worker.path.display().to_string(),
        reason: format!("{error}"),
    }
}

/// Map engine errors onto the crate error model (skill error-mapping shape:
/// typed variants, engine debug text preserved on the fallback arm).
fn map_fs_error(error: FsError) -> SearchError {
    match error {
        FsError::InvalidConfig {
            field,
            value,
            reason,
        } => SearchError::InvalidArgument {
            field: "index_config",
            reason: format!("{field} = {value}: {reason}"),
        },
        FsError::EmbedderUnavailable { model, reason } => SearchError::Query {
            reason: format!("embedder unavailable for {model}: {reason}"),
        },
        FsError::IndexCorrupted { path, detail } => SearchError::Open {
            path: path.display().to_string(),
            reason: format!("index corrupted: {detail}"),
        },
        other => SearchError::Query {
            reason: format!("{other}"),
        },
    }
}

#[cfg(all(test, feature = "search"))]
mod tests {
    use super::CorpusSearch;
    use crate::{ArtifactDoc, Hit, SearchError};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SCRATCH: AtomicU64 = AtomicU64::new(0);

    fn scratch_dir(name: &str) -> PathBuf {
        let n = SCRATCH.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("emath-search-{}-{name}-{n}", std::process::id()))
    }

    fn corpus() -> Vec<ArtifactDoc> {
        vec![
            ArtifactDoc::new(
                "1",
                "artifact",
                None,
                "rust compiler borrow checker lifetimes",
            )
            .unwrap(),
            ArtifactDoc::new(
                "2",
                "artifact",
                None,
                "distributed consensus raft algorithms",
            )
            .unwrap(),
            ArtifactDoc::new(
                "3",
                "evidence",
                None,
                "verified claim about the borrow checker",
            )
            .unwrap(),
            ArtifactDoc::new(
                "4",
                "goal",
                None,
                "produce reproducible deterministic builds",
            )
            .unwrap(),
        ]
    }

    fn hit_ids(hits: &[Hit]) -> Vec<String> {
        hits.iter().map(|h| h.doc_id.clone()).collect()
    }

    #[test]
    fn happy_path_lexical_hit() {
        let dir = scratch_dir("happy");
        let search = CorpusSearch::create(&dir, &corpus()).expect("create");
        let hits = search.search("borrow checker", 10).expect("search");
        assert!(!hits.is_empty());
        // BM25 lexical hits carry composite ids and a score.
        for hit in &hits {
            assert!(
                hit.doc_id.contains('\u{1f}'),
                "composite id: {}",
                hit.doc_id
            );
            assert!(hit.score.is_finite());
        }
        // The document containing the exact phrase ranks first.
        assert_eq!(hits[0].id, "1");
    }

    #[test]
    fn mismatch_returns_empty() {
        let dir = scratch_dir("mismatch");
        let search = CorpusSearch::create(&dir, &corpus()).expect("create");
        let hits = search.search("zzzzqqqqxx", 10).expect("search");
        assert!(hits.is_empty());
    }

    #[test]
    fn k_boundaries() {
        let dir = scratch_dir("kbounds");
        let search = CorpusSearch::create(&dir, &corpus()).expect("create");
        let zero = search.search("borrow", 0).expect("k=0");
        assert!(zero.is_empty());
        let over = search.search("borrow", 100).expect("k>corpus");
        assert!(over.len() <= 4);
        // Deterministic ordering shape: identical query twice, identical ids.
        let again = search.search("borrow", 100).expect("repeat");
        assert_eq!(hit_ids(&over), hit_ids(&again));
    }

    #[test]
    fn empty_query_rejected() {
        let dir = scratch_dir("emptyq");
        let search = CorpusSearch::create(&dir, &corpus()).expect("create");
        assert!(matches!(
            search.search("   ", 5),
            Err(SearchError::InvalidArgument { field: "query", .. })
        ));
    }

    #[test]
    fn determinism_across_two_builds() {
        let a = scratch_dir("det-a");
        let b = scratch_dir("det-b");
        let sa = CorpusSearch::create(&a, &corpus()).expect("build a");
        let sb = CorpusSearch::create(&b, &corpus()).expect("build b");
        let q = "deterministic builds";
        let ha = sa.search(q, 10).expect("search a");
        let hb = sb.search(q, 10).expect("search b");
        assert_eq!(hit_ids(&ha), hit_ids(&hb));
        assert_eq!(
            ha.iter().map(|h| h.score).collect::<Vec<_>>(),
            hb.iter().map(|h| h.score).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reopen_preserves_results() {
        let dir = scratch_dir("reopen");
        let before;
        {
            let search = CorpusSearch::create(&dir, &corpus()).expect("create");
            before = search.search("raft consensus", 10).expect("search");
            assert!(!before.is_empty());
        }
        let reopened = CorpusSearch::open(&dir).expect("reopen");
        let after = reopened.search("raft consensus", 10).expect("search after");
        assert_eq!(hit_ids(&before), hit_ids(&after));
    }

    #[test]
    fn reindex_replaces_corpus() {
        let dir = scratch_dir("reindex");
        let search = CorpusSearch::create(&dir, &corpus()).expect("create");
        assert!(!search.search("borrow checker", 10).expect("old").is_empty());
        let replaced: Vec<ArtifactDoc> = vec![
            ArtifactDoc::new(
                "9",
                "artifact",
                None,
                "quantum error correction surface codes",
            )
            .unwrap(),
        ];
        let stats = search.reindex(&replaced).expect("reindex");
        assert_eq!(stats.source_count, 1);
        assert!(
            search
                .search("borrow checker", 10)
                .expect("old gone")
                .is_empty()
        );
        assert_eq!(
            search.search("surface codes", 10).expect("new hit").len(),
            1
        );
    }

    #[test]
    fn remove_index_disables_search_until_rebuild() {
        let dir = scratch_dir("remove");
        let search = CorpusSearch::create(&dir, &corpus()).expect("create");
        search.remove_index().expect("remove");
        assert!(matches!(
            search.search("borrow", 5),
            Err(SearchError::NotReady { .. })
        ));
        search.reindex(&corpus()).expect("rebuild");
        assert!(
            !search
                .search("borrow", 5)
                .expect("after rebuild")
                .is_empty()
        );
    }

    #[test]
    fn empty_corpus_refused() {
        let dir = scratch_dir("emptycorpus");
        assert!(matches!(
            CorpusSearch::create(&dir, &[]),
            Err(SearchError::InvalidArgument { field: "docs", .. })
        ));
    }

    #[test]
    fn open_missing_index_errors() {
        let dir = scratch_dir("missing");
        let Err(error) = CorpusSearch::open(&dir) else {
            panic!("open must fail on a missing index");
        };
        assert!(matches!(error, SearchError::Open { .. }));
    }
}
