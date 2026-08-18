# CONTRACT.md

## Purpose and layer

Provenance/evidence *lineage* store (goal → plan → artifact → evidence
traceability), spike pass 2 of the frankengraphdb adoption (CUTOVER_PLAN.md
§5.3 / §9.11). Layer: graph-store adapter external to the protected set — it
may import frankengraphdb; emath-core / emath-ir / emath-goal / emath-plan /
emath-artifact stay Franken-free.

This is a SPIKE, not a production-capable provenance store yet. frankengraphdb
is pre-1.0 with no releases; the surface is pinned to an exact commit and will
churn. The crate is feature-gated (`graphdb`, default OFF); the default build
is std-only with zero third-party dependencies and no engine code compiled.

## Public types and semantics

Always available (std-only, zero third-party deps):

- NodeKind { Goal, Plan, Artifact, Evidence } — typed node kinds and their
  stable label ids (`label()` 1..=4, `from_label` inverse).
- EdgeKind { PlanOf, ArtifactOf, EvidenceOf } — typed edge kinds, relation ids
  (`relation()` 1..=3), and destination-kind function (`dst_kind()`). All edges
  point from child to parent, so authored direction == lineage direction.
- NodeId(u64) / EdgeId(u64) — sequence-ordered identities; the CALLER supplies
  them monotonically (no wall clock, no engine allocation).
- AuthoredEdge { id, kind, src, dst } — one authored edge in adjacency order.
- Adjacency trait — minimal graph read contract (`edges()`) the deterministic
  algorithms are written against, so they are std-only and engine-free testable.
- Lineage { goals: Vec<NodeId>, plans, artifacts, evidences } — query result,
  each list sorted ascending by id; `is_empty`/`len`.
- lineage_closure(graph, seed, max_depth) — deterministic ancestor BFS over
  authored edges, depth-capped, sorted; seed itself never included.
- would_create_cycle(graph, src, dst) — true iff the edge closes a cycle.
- MAX_LINEAGE_DEPTH (1024) — hard cap applied to store lineage queries.
- ProvenanceError { DuplicateNode, DuplicateEdge, MissingNode, Cycle { from,
  to }, Open, Query } — typed, actionable Display, implements std::error::Error.
  No E-* codes are introduced; ERROR_CODES.md is untouched.

Feature graphdb only (blocking facade, emath-store sqlite precedent):

- ProvenanceStore::open(path) — open or create a graph directory (missing or
  empty dirs are created; existing graphs are opened and re-folded).
- ProvenanceStore::insert_node(id, kind) -> Result<NodeId> — create a typed
  node; duplicate/spent id refused (DuplicateNode).
- ProvenanceStore::insert_edge(id, kind, src, dst) -> Result<EdgeId> — create a
  typed edge; refused when an endpoint is missing (MissingNode), the id is live
  (DuplicateEdge), or the edge would close a cycle (Cycle).
- ProvenanceStore::lineage(seed, max_depth) -> Result<Lineage> — deterministic
  ancestry; MissingNode when seed is not live.
- ProvenanceStore::node_kind(id) -> Result<Option<NodeKind>> — persisted kind.

## Invariants

- Determinism: pure sequence. No wall-clock timestamps anywhere in the API or
  schema mapping; ids are caller-supplied; lineage output is grouped and sorted
  independent of edge insertion order, read-view order, or worker scheduling.
  Identical op sequences → identical graph → identical reads, including across
  a close/reopen (Chronicle re-fold) — tested.
- Acyclic lineage: insert_edge refuses any edge that would make a node
  reachable from itself (including self-loops) before anything durable happens.
- Refusal discipline: the engine refuses duplicate/spent identities and
  dangling endpoints BEFORE its two-fsync commit (fgdb pre-commit checks); the
  crate's own pre-checks are the friendly fast path, the engine is the durable
  authority. Cycle rejection is performed by the crate (domain semantics the
  engine does not know about).
- Single-writer: one ProvenanceStore owner per graph directory.

## Error model

ProvenanceError as above. Engine failures are mapped: fgdb WriteError
AlreadyLive/IdentitySpent → DuplicateNode/DuplicateEdge (by ElementId
Vertex/Edge), DanglingEndpoint → MissingNode, everything else → Query carrying
the engine Debug text. ReadError and OpenError map to Query/Open with Debug
text. No E-* codes.

## Determinism class

Pure sequence + deterministic engine fold. The engine itself is
content-addressed (Chronicle commit stream, no wall clock in our surface) and
advertises byte-identical reruns under its lab runtime; this crate's claim is
narrower and tested: identical caller op sequences produce identical lineage
results, and reopening a graph folds an identical read partition. Edge set
iteration order never influences output (sorted grouping).

## Cancellation behavior

Blocking facade over a dedicated engine worker thread (64 MiB stack). One
asupersync current-thread runtime is built on the worker at open; each call
sends an op over a channel and blocks on the reply. fgdb open/write futures
recurse deeply while polling, so engine execution always happens on the
worker's large stack, never on the caller's thread. No cancellation surface is
exposed; a hung engine future could block the caller — acceptable for this
spike lane. ProvenanceStore is Send (only channels cross threads), single-writer.

## Unsafe boundary

None. The crate root uses `#![forbid(unsafe_code)]` and the workspace lint
(workspace.lints.rust.unsafe_code = forbid) also forbids it.

## Feature flags

- graphdb (default OFF) — pulls pinned frankengraphdb facade `fgdb` plus
  `fgdb-types` / `fgdb-delta-types` (all rev 26b579de45a8d9ff439298982beda8e3b1e40217)
  and the same-rev asupersync git instance that frankengraphdb pins internally
  (rev c17e51931f3223d55bd4961ff13eb3c5c4022fdf, v0.4.7 line). This is a SECOND
  asupersync git instance next to the workspace pin f0270d53... (and the
  registry 0.4.6 fsqlite pulls) — required for type compatibility: fgdb's
  CommitCx wraps a capability Cx from ITS asupersync instance, and two crate
  instances are distinct types. Cargo unifies this dep with fgdb's own into one
  compiled instance.
- Default build (no features): std-only, first-party-only, zero third-party
  deps (the pure model + lineage/cycle algorithms + their tests).

## Conformance tests

- lib.rs (always): happy-path lineage chain; empty-graph lineage; single-node
  boundary; zero- and one-step depth boundaries; order-independence; evidence
  edges report artifact ancestors; cycle detection (self-loop, through-existing-
  path, acyclic extension accepted); kind/relation label stability.
- store.rs (cfg(all(test, feature = "graphdb"))): engine-backed happy-path
  lineage; empty-graph MissingNode; single-node empty lineage; duplicate node /
  duplicate edge refusal; cycle refusal before write (including self-loop) with
  graph damage check; missing-endpoint refusal (both ends); determinism across
  two fresh stores; reopen-folds-identical-history; max-depth truncation tiers.

## No-claim boundaries

- frankengraphdb is pre-1.0 with NO releases at the pinned commit
  26b579de45a8d9ff439298982beda8e3b1e40217; the surfaced subset (`fgdb` spine:
  open/write/read-neighbours/drop/reopen, bead fgdb-j0vu) is ~everything the
  facade exposes. Expect churn; this crate's API is not stable.
- No query language, path/traversal API, WCO/Loom executor, or plan
  certificates are exposed by fgdb at this rev — README claims to that effect
  are design docs, not surface. Lineage is this crate's own closed BFS over the
  engine's live-edge adjacency, capped at MAX_LINEAGE_DEPTH.
- None of the engine's MVCC/time-travel surface is wired (no AS-OF queries in
  the store API); time-travel claims are upstream, unverified here.
- Key management is NOT wired: fgdb at this rev has no key-management crate
  (bead fgdb-warden has not landed); DatabaseKeys must be caller-supplied. The
  spike uses fixed deterministic developer keys (upstream test-suite style
  [0x5a]/[0x77]/[0x3c]). No encryption/security claim of any kind.
- The engine recursively-mutable graph features (delete/update/CAS paths) exist
  in fgdb but this crate does not expose them; the spike is insert-only lineage.
- Evidence nodes are stored like other nodes; "evidence" carries no signature,
  digest, or attestation payload (no properties are written at all).
- Not a drop-in for any graph library (petgraph etc.); only the documented
  surface is supported. Concurrent writers to one graph directory are not
  supported (single-writer design).
