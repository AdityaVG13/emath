//! Blocking store adapter over the async fsqlite engine (sqlite-store).
//!
//! All public methods are blocking. The engine (fsqlite + asupersync) is
//! async-native and its futures recurse deeply while polling, so the
//! Connection and the asupersync runtime live on a dedicated worker thread
//! with a large stack; Store is a channel proxy over that worker. This keeps
//! engine stack usage off the caller's thread (default 2 MiB would overflow).
//! Single-writer by design (one Store owner per file); see CONTRACT.md.

use std::fmt;
use std::sync::mpsc;

use emath_artifact::manifest_from_json;
use fsqlite::compat::{ConnectionExt, ParamValue, RowExt};
use fsqlite::{Connection, SqliteValue};

use crate::schema::{self, SCHEMA_SQL};

/// One evidence row in deterministic order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceRow {
    /// Claim text (checker-style, e.g. "content-id=fnv1a64:...").
    pub claim: String,
    /// One of `schema::VALID_CLAIM_STATUSES`.
    pub status: String,
    /// Deterministic ordering key, supplied by the caller (never wall-clock).
    pub seq: i64,
}

/// Store errors. Internal to this crate; no E-* codes are introduced.
#[derive(Debug)]
pub enum StoreError {
    /// Opening or initializing the database failed.
    Open(String),
    /// A transaction or worker-runtime step failed.
    Transaction(String),
    /// A statement or verification mismatch failed.
    Query(String),
    /// Reserved for host I/O class failures.
    Io(std::io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(message) => write!(formatter, "emath-store open: {message}"),
            Self::Transaction(message) => write!(formatter, "emath-store transaction: {message}"),
            Self::Query(message) => write!(formatter, "emath-store query: {message}"),
            Self::Io(error) => write!(formatter, "emath-store io: {error}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// One data-driven operation for `Store::transaction`. Ops run as one atomic
/// unit on the engine worker; any error rolls all of them back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreOp {
    /// Insert (or keep) one artifact row.
    PutArtifact {
        /// Artifact id.
        id: String,
        /// Artifact kind.
        kind: String,
        /// Artifact path.
        path: String,
    },
    /// Insert (or keep) one evidence row.
    AddEvidence {
        /// Existing artifact id (foreign key).
        artifact_id: String,
        /// Claim text.
        claim: String,
        /// One of `schema::VALID_CLAIM_STATUSES`.
        status: String,
        /// Deterministic ordering key.
        seq: i64,
    },
}

/// Engine operation dispatched to the worker thread.
enum Op {
    Write(StoreOp),
    EvidenceFor { artifact_id: String },
    Verify { manifest_json: String },
    Batch { ops: Vec<StoreOp> },
}

/// Request envelope: one op plus its reply channel.
enum Request {
    Op {
        op: Op,
        reply: mpsc::Sender<Response>,
    },
    Shutdown,
}

/// Reply from the engine worker.
enum Response {
    Ok,
    Rows(Vec<EvidenceRow>),
    Error(StoreError),
}

/// Worker stack: fsqlite futures recurse deeply while polling, so the engine
/// runs on a large-stack thread, never on the caller's thread.
const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

/// Blocking handle: a channel proxy over the engine worker thread.
pub struct Store {
    request_tx: mpsc::Sender<Request>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Store {
    /// Open (or create) the store at path and install the schema. The
    /// ":memory:" name is accepted for in-memory stores. Schema installation
    /// is idempotent (CREATE TABLE IF NOT EXISTS).
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let (request_tx, request_rx) = mpsc::channel::<Request>();
        let (open_tx, open_rx) = mpsc::channel::<Result<(), StoreError>>();
        let worker_path = path.to_string();
        let worker = std::thread::Builder::new()
            .name("emath-store".to_string())
            .stack_size(WORKER_STACK_BYTES)
            .spawn(move || worker_entry(&worker_path, request_rx, &open_tx))
            .map_err(|error| StoreError::Open(format!("worker thread: {error}")))?;
        match open_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                request_tx,
                worker: Some(worker),
            }),
            Ok(Err(error)) => Err(error),
            Err(error) => Err(StoreError::Open(format!("worker channel closed: {error}"))),
        }
    }

    /// Insert (or keep) one artifact row. Idempotent: re-inserting an
    /// existing id is a no-op.
    pub fn put_artifact(&self, id: &str, kind: &str, path: &str) -> Result<(), StoreError> {
        let response = self.call(Op::Write(StoreOp::PutArtifact {
            id: id.to_string(),
            kind: kind.to_string(),
            path: path.to_string(),
        }))?;
        expect_ok(response)
    }

    /// Insert (or keep) one evidence row for an existing artifact. The status
    /// must be one of `schema::VALID_CLAIM_STATUSES`; other values are rejected
    /// with `StoreError::Query` before SQL runs (the CHECK constraint is a
    /// backstop). Rows are keyed on `(artifact_id, claim, seq)`, so re-inserting
    /// an identical row is a no-op.
    pub fn add_evidence(
        &self,
        artifact_id: &str,
        claim: &str,
        status: &str,
        seq: i64,
    ) -> Result<(), StoreError> {
        let response = self.call(Op::Write(StoreOp::AddEvidence {
            artifact_id: artifact_id.to_string(),
            claim: claim.to_string(),
            status: status.to_string(),
            seq,
        }))?;
        expect_ok(response)
    }

    /// Return all evidence rows for `artifact_id`, ordered by (`seq`, `claim`).
    /// Deterministic: order never depends on wall-clock or insertion order.
    pub fn evidence_for(&self, artifact_id: &str) -> Result<Vec<EvidenceRow>, StoreError> {
        let response = self.call(Op::EvidenceFor {
            artifact_id: artifact_id.to_string(),
        })?;
        match response {
            Response::Rows(rows) => Ok(rows),
            Response::Error(error) => Err(error),
            Response::Ok => Err(StoreError::Query("evidence_for: unexpected Ok".into())),
        }
    }

    /// Re-derive the hash rows implied by a manifest JSON blob and check them
    /// against the store (checker claim style: content-id=fnv1a64:...).
    pub fn verify_manifest(&self, manifest_json: &str) -> Result<(), StoreError> {
        let response = self.call(Op::Verify {
            manifest_json: manifest_json.to_string(),
        })?;
        expect_ok(response)
    }

    /// Run ops as one engine transaction on the worker: COMMIT on success,
    /// ROLLBACK on any error. No partial write leaks out of a failed
    /// transaction.
    pub fn transaction(&self, ops: &[StoreOp]) -> Result<(), StoreError> {
        let response = self.call(Op::Batch { ops: ops.to_vec() })?;
        expect_ok(response)
    }

    /// Dispatch one op to the worker and wait for the reply.
    fn call(&self, op: Op) -> Result<Response, StoreError> {
        let (reply_tx, reply_rx) = mpsc::channel::<Response>();
        self.request_tx
            .send(Request::Op {
                op,
                reply: reply_tx,
            })
            .map_err(|error| StoreError::Transaction(format!("worker channel send: {error}")))?;
        reply_rx
            .recv()
            .map_err(|error| StoreError::Transaction(format!("worker channel recv: {error}")))
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = self.request_tx.send(Request::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn expect_ok(response: Response) -> Result<(), StoreError> {
    match response {
        Response::Ok => Ok(()),
        Response::Error(error) => Err(error),
        Response::Rows(_) => Err(StoreError::Query("unexpected rows response".into())),
    }
}

/// Worker thread entry: build the runtime, open the engine, install the
/// schema, report readiness, then serve requests until Shutdown.
fn worker_entry(
    path: &str,
    request_rx: mpsc::Receiver<Request>,
    open_tx: &mpsc::Sender<Result<(), StoreError>>,
) {
    let runtime = match asupersync::runtime::RuntimeBuilder::current_thread().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = open_tx.send(Err(StoreError::Open(format!("runtime driver: {error}"))));
            return;
        }
    };
    let connection = match engine_drive(&runtime, Connection::open(path)) {
        Err(error) => {
            let _ = open_tx.send(Err(StoreError::Open(format!("open {path:?}: {error}"))));
            return;
        }
        Ok(connection) => connection,
    };
    if let Err(error) = engine_exec(&runtime, &connection, SCHEMA_SQL) {
        let _ = open_tx.send(Err(StoreError::Open(format!(
            "schema install at {path:?}: {error}"
        ))));
        return;
    }
    if let Err(error) = engine_exec(&runtime, &connection, "PRAGMA foreign_keys = ON") {
        let _ = open_tx.send(Err(StoreError::Open(format!(
            "foreign key pragma at {path:?}: {error}"
        ))));
        return;
    }
    let _ = open_tx.send(Ok(()));
    worker_main(&runtime, &connection, request_rx);
}

/// Serve ops on the worker thread until Shutdown.
fn worker_main(
    runtime: &asupersync::runtime::Runtime,
    connection: &Connection,
    request_rx: mpsc::Receiver<Request>,
) {
    for request in request_rx {
        match request {
            Request::Shutdown => break,
            Request::Op { op, reply } => {
                let response = dispatch(runtime, connection, op);
                if reply.send(response).is_err() {
                    break;
                }
            }
        }
    }
}

/// Run one op against the engine.
fn dispatch(runtime: &asupersync::runtime::Runtime, connection: &Connection, op: Op) -> Response {
    match op {
        Op::Write(write) => match apply_store_op(runtime, connection, &write) {
            Ok(()) => Response::Ok,
            Err(error) => Response::Error(error),
        },
        Op::EvidenceFor { artifact_id } => match op_evidence_for(runtime, connection, &artifact_id)
        {
            Ok(rows) => Response::Rows(rows),
            Err(error) => Response::Error(error),
        },
        Op::Verify { manifest_json } => {
            match op_verify_manifest(runtime, connection, &manifest_json) {
                Ok(()) => Response::Ok,
                Err(error) => Response::Error(error),
            }
        }
        Op::Batch { ops } => match run_batch(runtime, connection, &ops) {
            Ok(()) => Response::Ok,
            Err(error) => Response::Error(error),
        },
    }
}

/// Poll one engine future on the worker's runtime.
fn engine_drive<T, E>(
    runtime: &asupersync::runtime::Runtime,
    future: impl std::future::Future<Output = Result<T, E>>,
) -> Result<T, E> {
    runtime.block_on(future)
}

/// Execute a statement; engine errors surface as Query.
fn engine_exec(
    runtime: &asupersync::runtime::Runtime,
    connection: &Connection,
    sql: &str,
) -> Result<(), StoreError> {
    engine_drive(runtime, connection.execute(sql))
        .map_err(|error| StoreError::Query(format!("execute {sql:?}: {error}")))?;
    Ok(())
}

/// Execute a parameterized statement; engine errors surface as Query.
fn engine_exec_params(
    runtime: &asupersync::runtime::Runtime,
    connection: &Connection,
    sql: &str,
    values: &[SqliteValue],
) -> Result<(), StoreError> {
    engine_drive(runtime, connection.execute_with_params(sql, values))
        .map_err(|error| StoreError::Query(format!("execute {sql:?}: {error}")))?;
    Ok(())
}

/// Insert (or keep) one artifact row on the worker.
fn op_put_artifact(
    runtime: &asupersync::runtime::Runtime,
    connection: &Connection,
    id: &str,
    kind: &str,
    path: &str,
) -> Result<(), StoreError> {
    if id.is_empty() {
        return Err(StoreError::Query("put_artifact: empty artifact id".into()));
    }
    let sql = "INSERT OR IGNORE INTO artifacts (id, kind, path) VALUES (?1, ?2, ?3)";
    let values = [
        SqliteValue::from(id),
        SqliteValue::from(kind),
        SqliteValue::from(path),
    ];
    engine_exec_params(runtime, connection, sql, &values)
}

/// Insert (or keep) one evidence row on the worker.
fn op_add_evidence(
    runtime: &asupersync::runtime::Runtime,
    connection: &Connection,
    artifact_id: &str,
    claim: &str,
    status: &str,
    seq: i64,
) -> Result<(), StoreError> {
    if !schema::valid_claim_status(status) {
        return Err(StoreError::Query(format!(
            "add_evidence: status {status:?} not in {:?}",
            schema::VALID_CLAIM_STATUSES
        )));
    }
    let sql =
        "INSERT OR IGNORE INTO evidence (artifact_id, claim, status, seq) VALUES (?1, ?2, ?3, ?4)";
    let values = [
        SqliteValue::from(artifact_id),
        SqliteValue::from(claim),
        SqliteValue::from(status),
        SqliteValue::Integer(seq),
    ];
    engine_exec_params(runtime, connection, sql, &values)
}

/// Read all evidence rows for one artifact, ordered by (seq, claim).
fn op_evidence_for(
    runtime: &asupersync::runtime::Runtime,
    connection: &Connection,
    artifact_id: &str,
) -> Result<Vec<EvidenceRow>, StoreError> {
    let sql = "SELECT claim, status, seq FROM evidence WHERE artifact_id = ?1 ORDER BY seq ASC, claim ASC";
    let values = [SqliteValue::from(artifact_id)];
    let mut rows = Vec::new();
    engine_drive(
        runtime,
        connection.query_with_params_for_each(sql, &values, |row| {
            rows.push(EvidenceRow {
                claim: row.get_typed(0)?,
                status: row.get_typed(1)?,
                seq: row.get_typed(2)?,
            });
            Ok(())
        }),
    )
    .map_err(|error| StoreError::Query(format!("evidence for {artifact_id:?}: {error}")))?;
    Ok(rows)
}

/// Re-derive the hash rows implied by a manifest JSON blob and check them
/// against the store (checker claim style: content-id=fnv1a64:...).
fn op_verify_manifest(
    runtime: &asupersync::runtime::Runtime,
    connection: &Connection,
    manifest_json: &str,
) -> Result<(), StoreError> {
    let manifest = manifest_from_json(manifest_json)
        .map_err(|error| StoreError::Query(format!("verify_manifest: manifest JSON: {error}")))?;
    for (index, (path, content_id)) in manifest.files.iter().enumerate() {
        let seq = i64::try_from(index).map_err(|_| {
            StoreError::Query(format!("verify_manifest: file index {index} exceeds i64"))
        })?;
        let artifact_count: i64 = engine_drive(
            runtime,
            connection.query_row_map(
                "SELECT count(*) FROM artifacts WHERE id = ?1 AND kind = 'file' AND path = ?1",
                &[ParamValue::from(path.as_str())],
                |row| row.get_typed(0),
            ),
        )
        .map_err(|error| {
            StoreError::Query(format!("verify_manifest: artifact {path:?}: {error}"))
        })?;
        if artifact_count != 1 {
            return Err(StoreError::Query(format!(
                "verify_manifest: artifact row for {path:?} missing or duplicated (expected exactly one)"
            )));
        }
        let expected = vec![EvidenceRow {
            claim: format!("content-id={}", content_id.0),
            status: schema::CLAIM_STATUS_OK.to_string(),
            seq,
        }];
        let actual = op_evidence_for(runtime, connection, path.as_str())?;
        if actual != expected {
            return Err(StoreError::Query(format!(
                "verify_manifest: evidence mismatch for {path:?}: expected {expected:?}, store has {actual:?}"
            )));
        }
    }
    Ok(())
}

/// Run a data-driven transaction on the worker: COMMIT on success, ROLLBACK
/// on any error (including pre-SQL validation errors).
fn run_batch(
    runtime: &asupersync::runtime::Runtime,
    connection: &Connection,
    ops: &[StoreOp],
) -> Result<(), StoreError> {
    engine_exec(runtime, connection, "BEGIN").map_err(|e| transaction_context(&e))?;
    let outcome = run_ops(runtime, connection, ops);
    match outcome {
        Ok(()) => engine_exec(runtime, connection, "COMMIT").map_err(|e| transaction_context(&e)),
        Err(error) => {
            let _ = engine_exec(runtime, connection, "ROLLBACK");
            Err(error)
        }
    }
}

fn apply_store_op(
    runtime: &asupersync::runtime::Runtime,
    connection: &Connection,
    op: &StoreOp,
) -> Result<(), StoreError> {
    match op {
        StoreOp::PutArtifact { id, kind, path } => {
            op_put_artifact(runtime, connection, id, kind, path)
        }
        StoreOp::AddEvidence {
            artifact_id,
            claim,
            status,
            seq,
        } => op_add_evidence(runtime, connection, artifact_id, claim, status, *seq),
    }
}

fn run_ops(
    runtime: &asupersync::runtime::Runtime,
    connection: &Connection,
    ops: &[StoreOp],
) -> Result<(), StoreError> {
    for op in ops {
        apply_store_op(runtime, connection, op)?;
    }
    Ok(())
}

fn transaction_context(error: &StoreError) -> StoreError {
    StoreError::Transaction(error.to_string())
}

#[cfg(all(test, feature = "sqlite-store"))]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    use emath_artifact::{write_artifact_manifest, ArtifactClass, ArtifactManifest};
    use emath_core::{ContentId, SchemaId};
    use emath_ir::{EvidenceLevel, TargetProfile};

    use super::{EvidenceRow, Store, StoreError, StoreOp};
    use crate::schema::{CLAIM_STATUS_OK, CLAIM_STATUS_PENDING};

    static NEXT_DB: AtomicU64 = AtomicU64::new(0);

    /// Unique scratch path under the OS temp dir; never cleaned up (the host
    /// reclaims the temp area). Strictly sequential counter -- no wall clock.
    fn scratch_db() -> String {
        std::env::temp_dir()
            .join(format!(
                "emath-store-test-{}-{}.db",
                std::process::id(),
                NEXT_DB.fetch_add(1, Ordering::Relaxed)
            ))
            .to_string_lossy()
            .into_owned()
    }

    fn sample_manifest(seed: &str) -> String {
        let mut files = BTreeMap::new();
        files.insert(
            "src/lib.rs".to_string(),
            ContentId(format!("fnv1a64:{seed}00")),
        );
        files.insert(
            "emath/artifact-manifest.json".to_string(),
            ContentId(format!("fnv1a64:{seed}11")),
        );
        files.insert(
            "Cargo.toml".to_string(),
            ContentId(format!("fnv1a64:{seed}22")),
        );
        let manifest = ArtifactManifest {
            schema: SchemaId("emath.artifact".to_string()),
            artifact_id: ContentId(format!("fnv1a64:{seed}art")),
            class: ArtifactClass::Native,
            source_package: ContentId(format!("fnv1a64:{seed}src")),
            compiler: ContentId(format!("fnv1a64:{seed}cc")),
            target: TargetProfile {
                family: "host".to_string(),
                triple: None,
                features: Vec::new(),
            },
            numeric_profile: "p0".to_string(),
            providers: Vec::new(),
            evidence_level: EvidenceLevel::E3,
            public_exports: Vec::new(),
            assumptions: Vec::new(),
            files,
            source_map: ContentId(format!("fnv1a64:{seed}sm")),
            resolution_plan: ContentId(format!("fnv1a64:{seed}rp")),
            evidence_bundle: ContentId(format!("fnv1a64:{seed}eb")),
        };
        write_artifact_manifest(&manifest)
    }

    fn seed_manifest_rows(store: &Store, json: &str) -> ArtifactManifest {
        let manifest = emath_artifact::manifest_from_json(json).expect("manifest parses");
        for (index, (path, content_id)) in manifest.files.iter().enumerate() {
            let seq = i64::try_from(index).expect("index fits i64");
            store
                .put_artifact(path.as_str(), "file", path.as_str())
                .expect("put artifact");
            store
                .add_evidence(
                    path.as_str(),
                    &format!("content-id={}", content_id.0),
                    CLAIM_STATUS_OK,
                    seq,
                )
                .expect("put evidence");
        }
        manifest
    }

    #[test]
    fn happy_path_round_trip() {
        let store = Store::open(&scratch_db()).expect("open");
        store.put_artifact("a1", "file", "src/lib.rs").expect("put");
        store
            .add_evidence("a1", "content-id=fnv1a64:001", CLAIM_STATUS_OK, 0)
            .expect("ev0");
        store
            .add_evidence("a1", "content-id=fnv1a64:002", CLAIM_STATUS_OK, 2)
            .expect("ev2");
        store
            .add_evidence("a1", "content-id=fnv1a64:003", CLAIM_STATUS_PENDING, 1)
            .expect("ev1");
        let rows = store.evidence_for("a1").expect("query");
        assert_eq!(rows.len(), 3);
        // inserted with seq 2 before seq 1 -> returned in seq order
        assert_eq!(rows[0].seq, 0);
        assert_eq!(rows[1].seq, 1);
        assert_eq!(rows[2].seq, 2);
        assert_eq!(rows[1].status, CLAIM_STATUS_PENDING);
    }

    #[test]
    fn empty_evidence_and_boundary_seq() {
        let store = Store::open(&scratch_db()).expect("open");
        assert!(store.evidence_for("missing").expect("query").is_empty());
        store.put_artifact("a1", "file", "p").expect("put");
        store
            .add_evidence("a1", "claim", CLAIM_STATUS_OK, 0)
            .expect("seq0");
        // negative seq is legal and orders deterministically
        store
            .add_evidence("a1", "neg", CLAIM_STATUS_PENDING, -5)
            .expect("neg");
        let rows = store.evidence_for("a1").expect("query");
        assert_eq!(rows[0].seq, -5);
        assert_eq!(rows[1].seq, 0);
    }

    #[test]
    fn rejects_bad_status_and_missing_artifact() {
        let store = Store::open(&scratch_db()).expect("open");
        store.put_artifact("a1", "file", "p").expect("put");
        let bad = store.add_evidence("a1", "claim", "bogus", 0);
        assert!(
            matches!(bad, Err(StoreError::Query(_))),
            "bad status rejected pre-SQL: {bad:?}"
        );
        // FK: evidence for an artifact that was never stored
        let fk = store.add_evidence("ghost", "claim", CLAIM_STATUS_OK, 0);
        assert!(
            matches!(fk, Err(StoreError::Query(_))),
            "missing artifact rejected: {fk:?}"
        );
        store
            .put_artifact("a1", "file", "p")
            .expect("put again (idempotent)");
        let ok = store.add_evidence("a1", "claim", CLAIM_STATUS_OK, 0);
        assert!(ok.is_ok(), "control write succeeds: {ok:?}");
    }

    #[test]
    fn open_failure_is_reported() {
        // A directory path is not openable as a database file.
        let store = Store::open("/");
        assert!(store.is_err(), "opening '/' must fail");
    }

    #[test]
    fn identical_writes_yield_identical_rows() {
        let db_a = scratch_db();
        let db_b = scratch_db();
        for db in [&db_a, &db_b] {
            let store = Store::open(db).expect("open");
            store.put_artifact("a1", "file", "src/lib.rs").expect("put");
            store
                .add_evidence("a1", "c1", CLAIM_STATUS_OK, 0)
                .expect("e0");
            store
                .add_evidence("a1", "c2", CLAIM_STATUS_OK, 1)
                .expect("e1");
            // identical re-write is a no-op (INSERT OR IGNORE)
            store
                .add_evidence("a1", "c2", CLAIM_STATUS_OK, 1)
                .expect("e1 again");
        }
        let rows_a = Store::open(&db_a)
            .expect("reopen")
            .evidence_for("a1")
            .expect("query");
        let rows_b = Store::open(&db_b)
            .expect("reopen")
            .evidence_for("a1")
            .expect("query");
        assert_eq!(rows_a.len(), 2);
        assert_eq!(rows_a, rows_b);
        // persistence: reopening the same file yields identical rows
        let rows_a2 = Store::open(&db_a)
            .expect("reopen 2")
            .evidence_for("a1")
            .expect("query");
        assert_eq!(rows_a, rows_a2);
    }

    #[test]
    fn verify_manifest_matches_derived_rows() {
        let json = sample_manifest("5eed");
        let store = Store::open(&scratch_db()).expect("open");
        seed_manifest_rows(&store, &json);
        let result = store.verify_manifest(&json);
        assert!(result.is_ok(), "derived rows must verify: {result:?}");
    }

    #[test]
    fn verify_manifest_reports_mismatch() {
        let json = sample_manifest("bad5");
        let store = Store::open(&scratch_db()).expect("open");
        let manifest = emath_artifact::manifest_from_json(&json).expect("parse");
        // Seed every row but stamp the first one with the wrong status.
        for (index, (path, content_id)) in manifest.files.iter().enumerate() {
            let seq = i64::try_from(index).expect("index fits i64");
            store
                .put_artifact(path.as_str(), "file", path.as_str())
                .expect("put artifact");
            let status = if index == 0 { "fail" } else { CLAIM_STATUS_OK };
            store
                .add_evidence(
                    path.as_str(),
                    &format!("content-id={}", content_id.0),
                    status,
                    seq,
                )
                .expect("put evidence");
        }
        let result = store.verify_manifest(&json);
        assert!(
            matches!(result, Err(StoreError::Query(_))),
            "mismatch must fail: {result:?}"
        );
    }

    #[test]
    fn failed_transaction_rolls_back_all_writes() {
        let db = scratch_db();
        let store = Store::open(&db).expect("open");
        let outcome = store.transaction(&[
            StoreOp::PutArtifact {
                id: "a1".into(),
                kind: "file".into(),
                path: "p".into(),
            },
            StoreOp::PutArtifact {
                id: "a2".into(),
                kind: "file".into(),
                path: "q".into(),
            },
            StoreOp::AddEvidence {
                artifact_id: "a1".into(),
                claim: "claim".into(),
                status: "bogus".into(),
                seq: 0,
            },
        ]);
        assert!(
            matches!(outcome, Err(StoreError::Query(_))),
            "failed tx must error: {outcome:?}"
        );
        // rollback: a1/a2 are gone, so FK rejects new evidence
        let ghost_a1 = store.add_evidence("a1", "claim", CLAIM_STATUS_OK, 0);
        assert!(
            matches!(ghost_a1, Err(StoreError::Query(_))),
            "a1 must be rolled back: {ghost_a1:?}"
        );
        let ghost_a2 = store.add_evidence("a2", "claim", CLAIM_STATUS_OK, 0);
        assert!(
            matches!(ghost_a2, Err(StoreError::Query(_))),
            "a2 must be rolled back: {ghost_a2:?}"
        );
    }

    #[test]
    fn successful_transaction_commits() {
        let store = Store::open(&scratch_db()).expect("open");
        store
            .transaction(&[
                StoreOp::PutArtifact {
                    id: "a1".into(),
                    kind: "file".into(),
                    path: "p".into(),
                },
                StoreOp::AddEvidence {
                    artifact_id: "a1".into(),
                    claim: "c1".into(),
                    status: CLAIM_STATUS_OK.into(),
                    seq: 0,
                },
            ])
            .expect("commit");
        let rows = store.evidence_for("a1").expect("query");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].claim, "c1");
    }

    #[test]
    fn evidence_rows_are_full_deterministic_records() {
        let store = Store::open(&scratch_db()).expect("open");
        store.put_artifact("a1", "file", "p").expect("put");
        store
            .add_evidence("a1", "c1", CLAIM_STATUS_OK, 1)
            .expect("e");
        let rows = store.evidence_for("a1").expect("query");
        assert_eq!(
            rows,
            vec![EvidenceRow {
                claim: "c1".to_string(),
                status: "ok".to_string(),
                seq: 1,
            }]
        );
    }
}
