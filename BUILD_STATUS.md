# Build Status and Evidence Boundary

Status date: 2026-08-18.

> MINED-C-002: `BUILD_STATUS.md` is a narrative index, **not a certifying
> oracle**. Every green claim below names the non-empty test, capstone or
> artifact hash that proves it; prose without such evidence does not
> certify anything. The doc-drift gate (`scripts/validate.sh`
> crate-map/inventory lane) additionally pins `implementation/CRATE_MAP.md`
> and `implementation/PUBLIC_API_INVENTORY.md` against HEAD, and the
> "planned" rows below match the crate tree in that map.

## Semantic Genesis G0–G3 (V7 addendum, merged)

The V7 Semantic Genesis spine is implemented on top of V5 (V5/V6/V7 material
preserved; see `docs/v7-semantic-genesis/`, `history/v6-intent-compiler/`).

**Compiler-validated and integration-validated (2026-08-18).** The full gate
is green: `cargo fmt --all -- --check`, `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`bash scripts/validate.sh`.

What is proven (named non-empty evidence):

- **G0 glyph substrate:** `⧖(a ⋈ b) ⊛ ζ` survives lex/parse byte-exact
  (UTF-8 by default; arbitrary glyphs are identifiers per the existing
  lexer).
- **G1 parse forest + signature inference:** the reference body yields the
  unique structural term
  `apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))`, arities
  `{⧖:1, ⋈:2, ⊛:2, ζ:0}`, fixities `{⧖:prefix, ⋈:infix, ⊛:infix, ζ:constant}`
  and variables `[a, b]`; bounded ambiguity and budget caps are enforced
  (`E-SYN-210/211` issue typed diagnostics; no dedicated test covers them
  yet, so no test claim is made).
- **G2 Term IR + free structural world:** canonical round-trip
  (`Term::parse_canonical` == `Term::canonical`) is pinned by the
  `emath-term` suite (`tests/emath-term`), free-symbolic world preserves
  the term.
- **G3 parametric Rust world artifacts:** `emath compile --parametric`
  generates the zero-dependency crate
  `examples/generated/semantic-genesis-worlds`; the semantic-genesis
  crate-identity and provenance-pin lanes in `scripts/validate.sh` prove
  regeneration is byte-identical and the committed copy rustfmt-stable.
- **Capstones:** `cargo xtask demo semantic-genesis` (determinism across
  two runs, generated-crate tests, differential wrong-world rejection:
  swapped modular world yields `5`, refuting the replayed `6`) and
  `cargo xtask demo affine-scorer` (V5 pipeline + host promotion +
  negative control) both pass in the gate.
- `cargo run -p demo-host-independent` prints `independent host ok`:
  fingerprint-free behavioral asserts (constructor invariants + a known
  score) against the committed generated crate under `--verify` and a
  path dependency.
- `scripts/validate.sh` lanes: fork-type identity, artifact determinism,
  provider reality, negative controls, lossless fmt, doc gates, planner
  gate, typed tooling refusals, examples admit-or-refuse, artifact
  battery + tamper negative, SG capstone + crate identity + provenance
  parse-back + rustfmt, affine-scorer capstone, demo-host-independent,
  genesis honesty (no invented `tested` authority, `keep: pareto`
  honored, receipts agree with `compile --parametric` values).

Artifacts added by G0–G3: `parse-forest.json`, `signature.json`,
`free-term.json`, `meaning-problem.json`, `world-candidates/`,
`world-admission.jsonl`, `interpretation-portfolio.json`,
`answer-receipt.json`, generated `manifest.json` + `source-map.json`
(see `implementation/schemas/*-v1.schema.json`).

Worlds beyond the G0–G3 slice (`matrix`, `graph`) are recorded as typed
deferred entries (`E-GEN-090`) in the admission log, never silently ignored.
Content identity remains the bootstrap FNV-1a id (AGENTS.md rule 7).

## Included and source-complete

- The V5 integrated strategy, architecture, language plan, fork governance, source locks, schemas, phase plans, implementation backlog and acceptance matrix.
- The full V4 language/framework reference under `bootstrap/v4-open-framework-reference/`.
- The full V3 executable-mathematics Rust prototype under `bootstrap/v3-prototype/` and as an immutable ZIP under `history/`.
- A new emath-owned Rust foundation under `implementation/rust-next/` defining neutral data structures and adapter boundaries.
- `.emath` grammar, blank templates, valid examples and invalid fixtures.
- Scripts for cloning pinned upstream repositories, checking source locks, inventorying licenses, validating package structure and generating hashes.
- Exact upstream commit pins current at package construction.

## Locally validated

- All JSON files parse.
- All TOML files parse with Python's standard `tomllib`.
- Shell scripts pass `bash -n`.
- Relative Markdown links resolve or are explicitly external/reference-only.
- Required package files and phase documents exist.
- The V3 independent validator passes its retained 14-check suite.
- Package source hashes and ZIP CRC are checked during finalization.
- Rust source receives static delimiter, forbidden-unsafe and manifest-path checks.

## Compiler-validated (Phase 1 workspace, `crates/`)

The active Phase 1 workspace **is compiled and validated on a Rust-equipped
machine** (2026-08-18). The full gate is green (listed above).

> Operating-contract note (2026-08-17): per the current `AGENTS.md`,
> repo-level verification no longer runs full `cargo test --workspace`;
> compile/lint lanes run via DSR when available and tests are targeted
> only. The full-gate runs listed here remain true as archival evidence
> under the previous contract, not as the required operating procedure.

Named
evidence behind the claims:

- **Diagnostic registry complete:** `registry_complete` (`emath-hir`
  suite) proves 243 emitted codes are 243 documented codes
  (`implementation/ERROR_CODES.md`).
- **Lossless formatting:** both corpus fixtures
  (`tests/valid/square.emath`, `tests/valid/affine_scorer.emath`) are
  byte-canonical under `emath fmt`; a non-canonical file is refused
  (fmt gate + `formatter::tests::corpus_files_are_lossless_round_trip`
  and `corpus_canonical_reparse_is_stable` in `emath-syntax`).
- **Planning:** `emath planner tests/valid/square.emath` selects
  `goal ... disposition=native` with `checks=sir-checker.v1`;
  `produce dew.jit` is a typed refusal `E-GOAL-042` at plan and build.
- **Artifact identity:** regenerated `src/lib.rs` for
  `tests/valid/affine_scorer.emath` is **byte-identical** to the
  committed generated crate (`examples/generated/affine-scorer/`,
  artifact `fnv1a64:d70202296d3f871c`).
- **Negative controls:** every fixture under `tests/invalid/` is
  **refused** with its documented code (exit 1) — including
  `E-NAME-020`, `E-CTOR-030`, `E-KIND-100`, `E-UNIT-001`, `E-SEC-101`,
  `E-SYN-101`, `E-TYPE-110`, `E-NAME-024`, `E-SYN-115`, `E-SYN-121` —
  and `check --json` carries the code and message.
- **Independence:** tampered/missing/stale/wrong-goal/incomplete artifact
  controls are refused with `E-EVID-*` by the artifact battery lane;
  a mutated byte in a real artifact's `lib.rs` refuses with `E-EVID-101`.
- **Keep-gate first measurement:** `cargo bench --profile release-perf
  --bench comprehensive_bench` ran and committed `.bench-history/`
  (`65b865a`): codegen-parametric 0.11 ms cv 3.6% (golden match true),
  genesis-replay 0.06 ms cv 0.6%; parse/check/artifact-json/cli8p cells
  measured but quarantined (cv > 5%, sub-ms on this host) — no claim is
  made on those.
- **Doc-drift gate:** CRATE_MAP ↔ workspace members ↔ `crates/` and
  PUBLIC_API_INVENTORY ↔ `CompilerSession` signatures are pinned by
  `scripts/validate.sh`; negative controls (mutated path, mutated
  signature) and the not-gitignored policy are asserted in the same lane.

Note: `implementation/rust-next/` remains the immutable imported seed and is
unchanged by this work; the active implementation lives under `crates/`.

The status distinction is intentional:

```text
source-complete ≠ compiler-validated ≠ integration-validated ≠ release-proven
```

Phase 1 reaches *compiler-validated* (structured, deterministic artifact
with evidence claims) and *integration-validated* (demo-host promotion
runs); it is not *release-proven*: content identity is still the FNV-1a
bootstrap id, replaced before stable publication (AGENTS.md rule 7).

### Rust 2024 edition migration + per-crate contracts (2026-08-18)

- Workspace migrated to `edition = "2024"` / `rust-version = "1.85"`
  (generator-emitted manifest literals, explicit-edition test crates and
  goldens included); compile/lint lanes green under the new baseline:
  `cargo check --workspace`, `cargo clippy --workspace --all-targets
  -- -D warnings` (exit 0; `f64::midpoint` ×3 and `is_none_or` ×1 fixed
  as newly-active lints), `cargo fmt --all -- --check` (2024 style pass).
- Golden identity re-proven after the style pass: semantic-genesis
  regeneration byte-identical (template emission order synced), golden
  rustfmt-stable with its three swap-mutation contract tests passing;
  affine-scorer regeneration byte-identical.
- 38/38 `crates/*/CONTRACT.md` per-crate contracts now exist, each with
  all ten sections the repository contract requires.

## Planned, not yet implemented (rows match the crate map)

- Request-typed session surface (`LoadRequest`/`GoalRequest`/
  `BuildRequest`): **Partial** in `PUBLIC_API_INVENTORY.md`, never
  presented as implemented; adoption is a behavior-changing API redesign,
  not a doc amendment.
- Schema-defined custom declaration kinds in the language beyond the
  thirteen-schema registry (`crates/emath-schema`).
- Complete type/unit/shape/domain inference (planned crates
  `emath-types`, `emath-units`, `emath-shapes`, `emath-domains`).
- Dew provider execution beyond the std-only scalar lanes
  (`produce dew.jit` refused `E-GOAL-042`).
- Wrenfold differential provider and Franken provider adapters (planned
  repositories; see `implementation/CRATE_MAP.md`).
- Production evidence store and registry (the in-tree `emath-registry`
  is a slice).
- Per-artifact translation validation.
- Protected host promotion controller wired to a production application.
- Planned crates not yet in `crates/`: `emath-canonical`, `emath-format`
  (lossless formatter currently lives inside `emath-syntax`),
  `emath-package`, `emath-provider-host`, `emath-host` (see
  `implementation/CRATE_MAP.md`).

These are governed by the phase documents rather than represented as
shipped functionality; the crate map above is the source of truth for
what exists on disk.
