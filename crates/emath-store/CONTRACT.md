# CONTRACT.md

## Purpose and layer

Evidence/artifact *state* store: per-artifact evidence rows backed by the
frankensqlite engine (fsqlite crate) instead of the single hashed
emath/artifact-manifest.json file. Layer: storage adapter external to the
protected set (CUTOVER_PLAN.md section 5.2 / section 9.10) — it may import
frankensqlite; emath-core / emath-ir / emath-artifact stay Frank-free.

The manifest JSON wire contract is untouched: emath-checker, ERROR_CODES.md
and emath-cli keep working as-is. This crate stores *state derived from* the
manifest (artifact + evidence rows) and re-derives those rows from a manifest
blob to prove store/manifest agreement. It never rewrites manifest files.

## Public types and semantics

Always available (std-only, zero third-party deps):

- schema::SCHEMA_SQL — deterministic DDL (artifacts + evidence tables).
- schema::VALID_CLAIM_STATUSES — closed status set: ok / fail / pending.
- schema::valid_claim_status(&str) -> bool — value-set membership check.
- `ObjectGraph` — deterministic in-memory object/relation index.
- `ObjectDraft` / `LibraryObject` — one envelope for cell, theory, proof,
  method, lesson, recipe, and validated custom kinds. ObjectID includes kind,
  MeaningID, and canonical semantic payload; presentation is excluded.
- `RelationDraft` / `Relation` — immutable typed edges (`depends_on`, `proves`,
  `equivalent_to`, `refines`, `implements`, `uses`, custom) with global,
  namespace, or object scope plus assumptions, authority, and evidence.
- `Space` — local named alias/policy/lock view over an `Arc<ObjectGraph>`.
  Branches clone only view metadata and share immutable objects.
- `SpaceSnapshot` / `LibraryLock` — deterministic alias/policy snapshot and
  closed dependency set. Lock verification refuses missing or revoked objects.
- `Space::semantic_merge` / `MergeReceipt` — additive, content-addressed merge
  over a named common snapshot. Alias, evidence, and policy alternatives union;
  no side wins by recency.
- `Reconciliation::{Choose,Rename,Morphism,ProveEquivalent}` — explicit merge
  operations, each backed by a stored reconciliation object and recorded in the
  receipt identity.

Feature sqlite-store only:

- Store::open(path: &str) -> Result<Store, StoreError> — open or create a
  store file (":memory:" works for in-memory stores); installs the schema
  idempotently and enables foreign keys.
- Store::put_artifact(id, kind, path) — idempotent insert (INSERT OR IGNORE).
- Store::add_evidence(artifact_id, claim, status, seq) — idempotent insert
  keyed on (artifact_id, claim, seq); status validated before SQL.
- Store::evidence_for(artifact_id) -> Vec<EvidenceRow> — rows ordered by
  (seq ASC, claim ASC); EvidenceRow { claim, status, seq }.
- Store::verify_manifest(manifest_json) — parses an emath.artifact manifest
  (emath_artifact::manifest_from_json) and checks that every declared file
  has artifact row (id == path, kind == "file") and exactly one evidence row
  "content-id=<fnv1a64:...>" with status ok and seq equal to the file index
  in deterministic (sorted) order.
- StoreOp \u2014 data-driven operation enum { PutArtifact, AddEvidence } used by
  Store::transaction.
- Store::transaction(&[StoreOp]) — BEGIN/COMMIT/ROLLBACK wrapper; any error (from an op or the engine) rolls back ALL
  writes made inside the batch.
- StoreError — local error enum { Open, Transaction, Query, Io } with
  actionable Display text. No E-* codes are introduced anywhere; the enum is
  internal to this crate (TransportError precedent in emath-lsp).

## Invariants

- Determinism: pure sequence. No wall-clock timestamps exist in the schema or
  API; seq is caller-supplied; row order is (seq, claim); identical write
  sequences produce identical rows (tested, including reopen).
- Idempotency: put_artifact / add_evidence are INSERT OR IGNORE; identical
  rewrites never duplicate rows.
- Failure atomicity: transaction() commits or rolls back atomically; a
  failing statement leaves prior committed state untouched.
- Foreign keys enforced (PRAGMA foreign_keys = ON at open); add_evidence for
  a missing artifact fails.
- Object and relation maps use canonical ID ordering. Assumption and evidence
  sets are sorted and deduplicated before RelationID construction.
- ObjectID and RelationID are domain-separated SHA-256 durable identities.
  Adding an edge never mutates either endpoint or its MeaningID.
- Alias values are sets, not last-writer-wins slots: conflicting ObjectIDs
  remain concurrently addressable. SnapshotID excludes the local space name
  but includes aliases, policy, and parent lock.
- Semantic merge requires both sides to identify the supplied common ancestor.
  The default result retains all alternatives; reducing or translating them
  requires a content-addressed explicit reconciliation.

## Error model

StoreError { Open(String) | Transaction(String) | Query(String) | Io(io::Error) }.
Engine failures (FrankenError) are mapped to Open/Query with the engine
message plus the failing SQL/context. Manifest verification mismatches are
Query-class errors carrying the exact expected-vs-actual row payload.

## Determinism class

Pure sequence (no clock, no randomness, no ambient environment). The engine's
storage layout is deterministic for identical op sequences; assertions about
row order use the explicit seq/claim ordering only, never physical layout.

## Cancellation behavior

Blocking facade over a dedicated engine worker thread (64 MiB stack). One
asupersync current-thread runtime is built on the worker at open; each call
sends an op over a channel and blocks on the reply. fsqlite futures recurse
deeply while polling, so engine execution always happens on the worker's
large stack, never on the caller's thread (default 2 MiB would overflow). No
cancellation surface is exposed; a hung engine future could block the caller — acceptable for the store lane. The async
transport lane (emath-lsp) is unaffected. Store is Send (only channels cross
threads), but each file is single-writer by design.

## Unsafe boundary

None. The crate root uses the standard Rust forbid attribute for unsafe_code
and the workspace lint (workspace.lints.rust.unsafe_code = forbid) also
forbids it. No unsafe leaves exist in this crate.

## Feature flags

Build-graph note: the lockfile carries two asupersync instances - git
9eb0600e (0.4.9) for emath-store's direct dep, and registry 0.4.9 pulled by
fsqlite's own manifest. They are separate compiled crates; the conformance
suite validates driving fsqlite engine futures from the git-pinned runtime.

- sqlite-store (default OFF) — pulls pinned fsqlite (frankensqlite facade,
  v0.3.11 at rev 8281cf285433b1746d9c29a598152e58cc205bd8) with default-features=false
  + native, plus the pinned asupersync runtime that drives it.
- Default build (no features): std-only, first-party-only (emath-core /
  emath-ir / emath-artifact), zero third-party deps.

## Conformance tests

- `tests/emath-store/tests/lib.rs` (always): schema DDL determinism
  (`schema_is_deterministic_ddl`); claim-status validation boundaries
  (`claim_status_validation_boundaries`).
- `src/store.rs` (cfg(all(test, feature = "sqlite-store"))): happy-path round
  trip with out-of-order seq; empty evidence + boundary seq (negative); bad
  status and missing-artifact (FK) rejection; open failure; identical writes
  across two fresh stores and reopen; verify_manifest pass; verify mismatch
  (wrong status) fails; failed transaction (bad status mid-batch) rolls back
  all writes; successful batch transaction commits; full EvidenceRow records
  equality.
- `tests/emath-store/tests/epic_emlib_nz1n_4_unit.rs`: presentation-independent
  ObjectID, typed `proves` relation, canonical assumption/evidence sets, and
  typed missing-endpoint/custom-kind refusals.
- `tests/emath-store/tests/epic_emlib_nz1n_6_unit.rs`: alias conflict
  preservation, shared-object branching, stable snapshots, and dangling/revoked
  lock refusals.
- `tests/emath-store/tests/epic_emlib_nz1n_9_unit.rs`: additive alias/evidence
  merge, no-last-writer-wins conflict preservation, ancestor refusal, and an
  explicit choose reconciliation receipt.

## No-claim boundaries

- Encryption is NOT wired: frankensqlite v0.3.4 does not implement PRAGMA
  key / PRAGMA rekey (verified against the pinned source). This crate makes
  no encryption claim.
- Not a SQLite drop-in: this adapter uses the fsqlite facade's native engine
  surface (async Connection + compat helpers), not a general SQL engine API;
  only the schema documented here is supported.
- verify_manifest proves store rows agree with the manifest blob's declared
  content ids; it does NOT re-read artifact file bytes. File-content truth is
  the checker's job (E-EVID-101) and remains unchanged.
- An empty manifest files map verifies trivially; manifest file-inventory
  completeness is the checker's contract (E-EVID-109, E-EVID-105).
- Concurrent writers to one store file are not supported (single-writer
  design; the engine's strict multi-process mode is not configured).
