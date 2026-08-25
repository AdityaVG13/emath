# CONTRACT.md — emath-search

## Purpose and layer

Artifact *corpus search* (id + kind + path + claim text) over the pinned
frankensearch engine, spike pass 3 of the FrankenStack adoption
(CUTOVER_PLAN.md §5.4 / §9.12). Layer: search adapter external to the
protected set — it may import frankensearch; emath-core / emath-ir /
emath-goal / emath-plan / emath-artifact / emath-checker / emath-cli /
emath-lsp / emath-store / emath-provenance stay Franken-free.

This is a SPIKE, not a production search service. frankensearch is pinned to
an exact commit (33ef37a4c663535b832ae057b5c36296215e41f4) and is in active
churn; the crate is feature-gated (`search`, default OFF) and the default
build is std-only with zero third-party dependencies and no engine code
compiled.

## Public types and semantics

Always available (std-only, zero third-party deps):

- ArtifactDoc { id, kind, path: Option<String>, text } — one artifact metadata
  record; validates at construction.
- DOC_ID_SEPARATOR (`\x1f`) — unit separator of the single composite doc-id
  scheme.
- to_fs_doc_id(kind, id) / from_fs_doc_id(id) — the ONE encoding pair;
  `kind \x1f id`, both parts non-empty and separator-free (rejects otherwise).
- SearchError { InvalidArgument, Open, Build, Query, NotReady, WorkerDown } —
  typed, actionable Display, implements std::error::Error. No E-* codes;
  ERROR_CODES.md untouched.

Feature `search` only (blocking facade, emath-store / emath-provenance
worker-thread precedent):

- CorpusSearch::create(path, docs) — build via the engine's IndexBuilder
  in a sibling staging directory, then swap onto `path` and write
  `emath-search.index`. Empty corpus -> InvalidArgument (the engine
  refuses zero-document builds). A non-empty directory without the marker
  is InvalidArgument (never wiped).
- CorpusSearch::open(path) — open an existing index; typed Open error when
  none is present.
- CorpusSearch::reindex(docs) — same as create: build aside, swap when the
  new tree is marked. A failed rebuild leaves the previous on-disk index
  and the live searcher unchanged. A leftover `.emath-search-backup` from
  a crashed swap is restored only when dest is missing or empty; an
  unmarked non-empty dest is never wiped.
- CorpusSearch::remove_index() — delete index artifacts only when the
  directory is empty or marked `emath-search.index` (the directory itself
  remains); search then returns NotReady until a create/reindex.
- CorpusSearch::search(query, k) -> Vec<Hit> — blocking; trimmed query must
  be non-empty; k == 0 returns an empty vector without touching the engine.
  Result order is the ENGINE's authoritative order — this crate never
  re-sorts frankensearch results.
- Hit { doc_id, kind, id, score: f32, source } — engine score carried as the
  engine carries it (f32, never narrowed or re-derived).
- IndexStats { source_count, doc_count, error_count, quality_indexed,
  lexical: Option<LexicalArmStats> } — build receipts incl. the Quill arm.

## Invariants

- Determinism: identical corpus + identical query -> identical result
  sequence (doc ids AND bit-exact f32 scores), across two builds in different
  directories and across close/reopen — tested. No wall clock in the crate's
  surface; id ordering is the caller's. The searcher disables the engine's
  adaptive NQC down-weight (stateful across queries) and never reads
  FRANKENSEARCH_* environment variables (no `with_env_overrides`), so the
  spike lane is env-independent.
- Single-writer: one CorpusSearch owner per index directory.
- Composite-id integrity: every indexed doc id round-trips through
  from_fs_doc_id with the original kind/id (tested).
- Engine ordering is authoritative; the crate adds no second sort.

## Error model

SearchError as above. Engine SearchError maps: InvalidConfig ->
InvalidArgument, EmbedderUnavailable/QueryParseError/etc -> Query with engine
Debug text, IndexCorrupted -> Open. Build embedding failures aggregate into
Build with the per-document report. Channel loss -> WorkerDown.

## Determinism class

Deterministic modulo the engine's own ordering: the searcher's RRF fusion and
Quill BM25 are pure over (index, query). Verified by repeated identical
queries on one searcher, two independent builds, and reopen. Uncertain
upstream internals (tie-breaking among equal scores at this rev) are covered
by the no-claims below.

## Cancellation behavior

Blocking facade over a dedicated engine worker thread (64 MiB stack) holding
one asupersync current-thread runtime. Each call sends an op over an mpsc
channel and blocks on the typed reply. No cancellation surface is exposed; a
hung engine future can block the caller — acceptable for this spike lane.
CorpusSearch is Send (only channels cross threads); Drop sends Close and joins
the worker.

## Unsafe boundary

None. `#![forbid(unsafe_code)]` at the crate root; the workspace lint
(workspace.lints.rust.unsafe_code = forbid) forbids it everywhere else.
(Upstream frankensearch itself uses `deny`, not `forbid` — that is upstream,
not this crate.)

## Feature flags

- search (default OFF) — pulls pinned frankensearch (facade `frankensearch`
  v0.3.2 at rev 33ef37a4c663535b832ae057b5c36296215e41f4) with features
  `hash` + `quill`, plus asupersync. Feature set deliberately differs from the
  skill's recommended `hybrid`: hybrid pulls model2vec/fastembed (tokenizers,
  ORT binary acquisition) and auto-detect model discovery, which this offline
  deterministic spike does not want.
- asupersync unification: frankensearch declares asupersync as a crates.io
  VERSION RANGE (`>=0.4.4, <0.5`), not an internal git rev like
  frankengraphdb. This crate declares the same crates.io range, so Cargo
  unifies one crates.io asupersync instance for the whole emath-search graph
  (resolved to the workspace-locked 0.4.6 crates.io). The workspace git pins
  f0270d53... and c17e51931... (store/provenance graphs) are separate
  instances in other crates' graphs and never cross this boundary. Verified
  via `cargo tree -i asupersync`.
- Default build (no features): std-only, first-party-only, zero third-party
  deps, zero engine code.

## Conformance tests

- `tests/emath-search/tests/corpus.rs` (always): composite id round trip
  (`round_trip_composite_id`); empty-part rejection; separator-inside-part
  rejection; malformed decode -> None; ArtifactDoc construction validation.
- engine.rs (cfg(all(test, feature = "search"))): happy-path lexical hit with
  composite ids + finite scores; mismatch -> empty; k = 0 and k > corpus
  boundaries with repeated-query determinism; empty-query rejection; two
  independent builds -> identical result sequences (ids + bit-exact scores);
  reopen-preserves-results; reindex-replaces-corpus (old docs gone, new hit);
  remove_index -> NotReady until rebuild; empty-corpus refusal; open-missing
  index -> Open error; create refuses a foreign (unmarked) directory;
  a stranded `.emath-search-backup` is restored rather than deleted;
  rebuilds build aside and swap only a marked tree.

## No-claim boundaries

- SEMANTIC TIERS NOT WIRED: this pass drives the hash-control embedder stack
  (HashEmbedder::default_256). The engine warns that a non-semantic fast
  embedder makes semantic retrieval unavailable — results are lexical-only
  (Quill BM25) fused through the same TwoTierSearcher/RRF pipeline. No
  relevance or retrieval-quality claim of any kind. Unblock for semantic
  tiers: enable the `hybrid` facade feature + FRANKENSEARCH_MODEL_DIR models
  (model2vec/fastembed), then build with a semantic stack.
- open_hybrid() is NOT used: it refuses hash-identity generations by upstream
  design; the searcher is assembled per open_hybrid's documented manual shape
  (TwoTierSearcher::new + with_lexical(Quill reader)).
- No ability to open the engine's FSVI-v2 "admitted" product or the sync
  (SyncTwoTierSearcher / InMemoryTwoTierIndex) surface; neither is exercised.
- Quality refinement, rerank, ANN, storage/durability, graph ranking,
  federated search, daemon embedder, FTS5, and API surfaces are NOT
  exercised. Adaptive NQC is explicitly DISABLED (determinism), not
  characterized.
- Tie-breaking among equal fusion scores is the engine's internal behaviour
  at the pinned rev; the crate claims only that it is stable for identical
  inputs (tested), not what the tie-break rule is.
- Upstream build hazard at this rev: frankensearch's workspace member
  `tools/optimize_params` declares `fastcma = { path = "../../../fast_cmaes" }`
  — an escape-path absolute dependency that does not exist in a clean
  checkout (verified: pristine clone fails with "failed to read
  /tmp/fast_cmaes/Cargo.toml"). Any workspace that consumes frankensearch as a
  git dep must provide a stub crate at that absolute path (a 3-file lib named
  `fastcma` with feature `test_utils`) until upstream fixes the manifest
  (relative path, workspace member, or registry dep). This crate's build and
  tests depend on that environment prerequisite; it is a frankensearch
  manifest defect (the CASS /dp/ absolute-path class), not an emath defect.
- Not a drop-in replacement for any search library; only the documented
  surface is supported. The index layout on disk is frankensearch's and not
  stable across revs; reindex after any pin move.
