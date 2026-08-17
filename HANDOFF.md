# emath Handoff — Phase 1 Slice + Semantic Genesis G0–G3

Status date: 2026-08-15. This handoff summarizes what is implemented, how it
is validated, what remains, and how to continue.

## Phase 1 Vertical Slice

## What exists

A complete Phase 1 compiler workspace under `crates/` that compiles a
`.emath` spec into a standalone Cargo crate plus a verified artifact:

| Crate | Role |
| --- | --- |
| `emath-core` | Content ids (FNV-1a bootstrap), diagnostics, spans, limits |
| `emath-source` | Source files, line/col rendering |
| `emath-syntax` | Lexer (layout-sensitive, limits-bounded) + parser for the V5 corpus |
| `emath-ir` | Neutral SIR: package arena, expressions, types, goals, canonical bytes (`emath.sir.v1`) |
| `emath-goal` | Request elaboration into GIR (`evaluate`, `solve` refusal) |
| `emath-plan` | Deterministic resolution plans (`emath.plan.v1`) |
| `emath-sema` | Admission: typed refusals, constructor invariants, tests, `CompilerSession` |
| `emath-exec-ir` | Linear EMIR lowering (loads, ops, domain obligations) |
| `emath-rust-ir` | Minimal Rust AST + rustfmt-stable renderer |
| `emath-rust-backend` | EMIR → Rust codegen: struct + checked constructor + evaluate methods + example tests + Cargo.toml |
| `emath-build` | End-to-end pipeline: plan → seal → codegen → verify (`cargo test`) → compose → stage → publish → independent verify |
| `emath-artifact` | Deterministic JSON writers (`emath.artifact.v1`, source map, resolution plan, evidence bundle), staging, atomic publish, tamper verification |
| `emath-builder` | Programmatic `ModelBuilder` API lowering to the same SIR |
| `emath-lab-core` | Phase 10 scaffold: experiments, samplers, metrics, promotion policy |
| `emath-provider-api` | Phase 2 seam: `Provider`/`Adapter`/`ResultChecker` typed-refusal contracts |
| `emath-cli` | `check` / `plan` / `build` / `artifact check` / `architecture` / `help` with exit-code discipline (0/1/2) |

## The proving slice

`implementation/tests/valid/stateful.emath` (constructor + state + score
definition + requests + exports) builds into artifact
`fnv1a64:3ffcb7524056dc5a` whose `src/lib.rs` is byte-identical to the
committed crate `examples/generated/affine-policy-rs/`.

- `examples/demo-host` runs the full pipeline in `build.rs`, promotes the
  artifact, constructs via the checked constructor, evaluates, refuses the
  negative control at runtime, and independently re-verifies artifact
  fingerprints. Runtime output: `score(3.0) == 7`, negative control
  `new(-1.0, 0.5)` → `FailedPrecondition`, 6 artifact files verified.
- `examples/demo-host-independent` consumes the committed crate through a
  plain path dependency: `new(0.5, -2.0).score(10.0) == 3`, and
  `new(f64::NAN, 0.0)` is refused.
- `implementation/tests/valid/minimal.emath` (function + `tests:`
  `three_squared`) exercises example-test codegen end-to-end; the generated
  `#[test]` runs during `cargo test` verification.
- `examples/provider-skeleton` demonstrates the Phase 2 typed-refusal seam.

## Validation gate (all green on this machine)

```bash
cargo fmt --all -- --check          # OK
cargo test --workspace              # OK (all crates + examples)
cargo clippy --workspace --all-targets -- -D warnings   # OK
scripts/validate.sh                 # OK
```

`scripts/validate.sh` additionally proves: regenerated `src/lib.rs` is
byte-identical to the committed copy (determinism), and every fixture under
`implementation/tests/invalid/` is refused with a typed diagnostic.

## Key decisions made during implementation

See DECISION_LOG.md entries D-016 through D-021 (appended with this
handoff): raw attributes in output, tests attached to declarations in
admission, idempotent republish, signature-shorthands / `&self` receivers,
rustfmt-stable generated code, and the builder env including `state.*`.

## Not implemented (Phase 2+)

- Providers/adapters (Dew, Rumoca, Wrenfold, Franken) — the seam exists in
  `emath-provider-api` and is exercised by `provider-skeleton` as a
  typed refusal.
- Imports across files, generics, records, units/quantities
  (`E-UNIT-001`), binders, strings (`E-TYPE-…`), Booleans in definitions
  (`E-TYPE-012`), multiple constructors (`E-CTOR-036`) — all refuse with
  stable codes per `implementation/ERROR_CODES.md`.
- Release-grade content identity: `bootstrap_content_id` (FNV-1a) is
  explicitly not a cryptographic identity (AGENTS.md rule 7).

## Fixture inventory (Phase 1 subset)

Valid: `implementation/tests/valid/{minimal,stateful}.emath`.
Invalid (typed refusal): `duplicate_output` (E-NAME-020),
`missing_state_assignment` (E-CTOR-030), `recursive_kind` (E-KIND-100),
`unit_mismatch` (E-UNIT-001), plus others under
`implementation/tests/invalid/`.

## Semantic Genesis G0–G3 (V7, merged on top of V5)

The V7 Semantic Genesis spine is implemented and validated. V5/V6 material
is preserved (`history/v6-intent-compiler/`, `docs/v7-semantic-genesis/`,
`implementation/schemas/*-v1.schema.json`).

| Crate | Role |
| --- | --- |
| `emath-term` | Provider-neutral first-order terms, `Signature`, canonical form, `parse_canonical` round-trip (seed) |
| `emath-world-ir` | `WorldIr`, `WorldId`, `Fixity`, `MeaningHole`, `OperatorSemantics`, public `fnv1a64` (seed) |
| `emath-genesis` | `FirstOrderWorld` trait + generic `evaluate`, free/Boolean/modular-17 seed worlds, reference term (seed) |
| `emath-meaning-provider-api` | `MeaningProblem`, `WorldCandidate`, `MeaningProvider`, `WorldChecker` contracts (seed) |
| `emath-portfolio` | `InterpretationPortfolio` deterministic ordering (seed) |
| `emath-world-codegen-rust` | Deterministic parametric Rust artifact generator (`generate`/`WorldSpec`/`write_to`) |
| `emath-syntax` (extended) | `genesis` grammar module (G0) + `forest` module (G1): bounded parse forest, signature/fixity/type-variable inference, recovery holes |
| `emath-cli` (extended) | `parse --forest`, `signature`, `genesis`, `compile --parametric`, `world show`, `portfolio show` |
| `xtask` | `cargo xtask demo cache-policy` + `cargo xtask demo semantic-genesis` (both capstones) |

Pipeline (exact, per G0–G3 exit):

```
source bytes → glyphs (UTF-8 byte-exact) → parse forest → signature inference
→ Term IR → free world → world traits → generated crate
```

- Reference body `⧖(a ⋈ b) ⊛ ζ` yields the unique structural term
  `apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))` with arities
  `{⧖:1, ⋈:2, ⊛:2, ζ:0}`; parse/signature IDs are FNV-1a64.
- `emath genesis --out` emits `parse-forest.json`, `signature.json`,
  `free-term.json`, `meaning-problem.json`, `world-candidates/`,
  `world-admission.jsonl` (admitted + typed-deferred),
  `interpretation-portfolio.json`, `answer-receipt.json`.
- `emath compile --parametric --out` emits the zero-dependency generated
  crate `semantic-genesis-worlds` (self-contained `Term` + canonical parser,
  `World` trait, generic `evaluate`, FreeSymbolic/Boolean/Modular worlds,
  SwappedModular negative control, fixtures, in-crate tests), plus
  `manifest.json` and `source-map.json`.
- The capstone evaluates the fixture `a=4, b=7`: free-symbolic (canonical
  term), boolean `false`, modular-17 `6`; the swapped world yields `5` and
  is **rejected** (wrong-world negative control).
- Both generated crates (`affine-policy-rs`, `semantic-genesis-worlds`) are
  rustfmt-stable and regeneration is byte-identical (validated).

Validation gate (all green on this machine):

```bash
cargo fmt --all -- --check           # OK
cargo test --workspace               # OK
cargo clippy --workspace --all-targets -- -D warnings   # OK
scripts/validate.sh                  # OK (incl. both capstones, replay
                                     # fidelity, SG crate identity + fmt)
```

`scripts/validate.sh` now also proves: deterministic re-analysis matches
the committed replay bundle (`validation/semantic-genesis/replay/`), the
wrong world is rejected, and the regenerated SG crate is byte-identical +
rustfmt-stable.

## Receipts

- `CHANGELOG_SEMANTIC_GENESIS.md`, `MIGRATION_RECEIPT.json`,
  `TASK_CLOSURE.json`, `commands.txt`, `env.json` at repository root.

## How to continue

1. Phase 2: first provider adapter behind `emath-provider-api`, honor
   imports (`E-PKG-050`), multi-file packages through the planning layer.
2. Phase 3/4 breadth: quantities, binders, records; each feature needs a
   producer, consumer, positive+negative test and artifact consequence
   (AGENTS.md rule 3).
3. Semantic Genesis G4–G12 per `docs/v7-semantic-genesis/22_IMPLEMENTATION_PHASES_G0_G12.md`:
   World IR + built-in world catalog (G4), meaning holes + finite synthesis
   (G5), law checking + counterexamples (G6), interpretation portfolios
   beyond the seed (G7), semantic calibration (G8), agent-native proposals
   (G9), tuning (G10), cross-world translation (G11), public alpha (G12).
4. Replace FNV-1a bootstrap identity with the release identity scheme
   before any stable publication (AGENTS.md rule 7).

## Schema-id honesty (gauntlet-02, 2026-08-17)

Durable document shape claims were split and pinned; writer payloads are
byte-identical (no golden move):

- **Two source-map ids, one writer shape each.** The durable artifact
  source map stays `emath.source-map.v1`
  (`emath_artifact::write_source_map`, byte-range + `source_package`
  shape, reader `source_map_from_json`). The world-codegen provenance map
  written by semantic-genesis compilation is `emath.generated-crate-source-map.v1`
  (`emath_artifact::write_generated_crate_source_map`, `(generated,
  source, kind)` entries, reader `generated_crate_source_map_from_json`).
  Cross-loading is refused both directions (tests in
  `tests/emath-artifact/tests/schema_lanes.rs`).
- **Plan identity is a separate layer from the JSON `$schema`.** The plan
  document's schema id is `emath.resolution-plan.v1`; the plan identity
  preimage hashes a `plan:v1:` payload
  (`emath_ir::goal::plan_identity`, pinned by a goal.rs test). The split
  is documented on `PLAN_SCHEMA` / `RESOLUTION_PLAN_SCHEMA` and in
  `implementation/schemas/README.md`.
- **Parse-forest schema aligned to its writer.** `parse-forest-v1.schema.json`
  now describes `ParseForest::canonical_json` exactly
  (`world_name/body/ambiguity_count/node_count/holes/recovery`, optional
  `parse_id` and `canonical_term`); parse-back test in
  `crates/emath-genesis/tests/parse_forest_schema.rs`.
- **Reader gap closed:** `plan_from_json` now actually parses
  `excluded_candidates` (it previously dropped them on read).
- **validate.sh include-set documented and pinned.** The semantic-genesis
  identity diff excludes `manifest.json` + `source-map.json` (committed
  copy predates them; provenance embeds the source path) and pins them
  instead with a per-shape parse-back lane (schema ids, entry shape,
  file-list agreement with the committed tree). Exclusion-without-pin is
  gone.
- `bootstrap_content_id` remains FNV-1a bootstrap identity (AGENTS rule 7);
  nothing here claims it is release crypto. Unification of the two
  source-map shapes (Rewrite A) stays a versioned follow-up.

## Honest oracles (gauntlet-04, 2026-08-17)

- **Comparator identity gate.** Every comparison is a pair of distinct
  `EngineIdentity` values (subject/oracle/baseline/mutant); a comparison
  that cannot prove distinctness is refused with `E-HOST-016` (first
  check inside `evaluate_paired`). `crates/emath-lab-core/src/identity.rs`.
- **FailureBundle as the truthful failure document.** `emath.failure-bundle.v1`;
  `TRUE_DIVERGENCE_POINTER = /failure/true-divergence`; emitted only by
  `DriftMonitor::failure_bundle` after true divergence, never as a test
  failure. Determinism + identity-binding pinned in failure.rs tests.
- **Pin-of-5 for the world swap.** The generated dual-run contract tests
  (inside `LIB_TEMPLATE`, so regeneration preserves them) assert
  modular=6 vs swapped=5 on the demo term, a nested-shape kill
  (14 vs 2), and dual-run determinism; xtask pins `swapped == "5"`
  (derived oracle documented in the comment). `SeedContract`
  (`genesis-world-swap`, `consumes_rng: false`) records the metamorphic
  determinism claim.
- `E-HOST-016` registered in `implementation/ERROR_CODES.md`; annex
  regenerated (splice method: `python3 scripts/dump_error_codes.py >
  /tmp/annex; python3 - <<'PY'` replacing the `## Completeness annex`
  block; the script itself only prints).

## Unused WorldIr + Term oracle (gauntlet-03, 2026-08-17)

- **SURF-0008 typed refusal.** `generate` now takes each world's declared
  operator semantics (`WorldSpec.operators`, sourced from the analyzed
  `builtin_worlds` WorldIr `DeclaredExpression` entries) and refuses any
  map diverging from the fixed per-label interpretation with the unique
  new code **`E-GEN-094`** (`CodegenRefusal`, nonzero exit from
  `compile --parametric`). Empty/default maps keep today's label-only
  goldens byte-identical; tests: `unused_worldir_tests` in
  `crates/emath-world-codegen-rust` (run with the bead's invocation
  `cargo test -p emath-world-codegen-rust -- unused_worldir`).
  WorldIr Feature stays Partial; Rewrite A (consume WorldIr) remains a
  versioned codegen follow-up.
- **Term oracle pinned.** `emath-term` is the oracle EngineIdentity
  (`emath-term`), the generated parser is the Subject (`generated-sg`);
  `tests/emath-term/tests/term_oracle_differential.rs` pins agreement on
  the replay canonical and on the padded (trailing-whitespace) string,
  plus garbage → Err on both. The generated parser gained the oracle's
  `skip_whitespace` (named refresh; committed crate regenerated).
  `term_public_api.rs` added the padded round-trip pin (CONF-0004/0016).
- **CONF-0027 already remediated.** Canonical identity includes
  `postconditions` (`canonical.rs`), generated `new` checks each `ensure`/
  `invariant` after field init (`FailedPostcondition`, `rust-backend.rs`),
  and `demo-host` runs the negative control `new(0.0, 1.0)` refused.
- **CONF-0029 already remediated.** `synthesize_tables` enumerates the
  full `n^(n²)` space (`carrier_size * carrier_size`) with the 19683
  exhaustive pin at |carrier|=3 and a budget-cut not-exhaustive control.
- Cass CLI unavailable (`cass-cli-unavailable`); no cassette replay was
  used in this campaign.
- `E-GEN-094` registered in `implementation/ERROR_CODES.md` + annex
  (241 emitted / 241 documented).

## Tooling honesty + lossless round-trip (gauntlet-05, 2026-08-18)

- **CONF-0026 typed load refusal.** The agent envelope's `run_check` emits
  `E-PKG-080` ("cannot read source file") on load failure instead of an
  empty-diagnostics "admitted: true" (interactive `check` in lib.rs
  already had it). Pinned by the new `agent check /nonexistent.emath`
  lane in `scripts/validate.sh`.
- **SURF-0011 named call args refused.** `parse_call_args` previously
  stripped `name = expr` bindings; it now refuses named call arguments
  with the new code **`E-SYN-121`** (also registered in
  `ERROR_CODES.md` + annex; 242 emitted / 242 documented). Fixture:
  `tests/invalid/named_call_arg.emath`; new `assert_invalid` lane.
- **SURF-0009 LSP byte positions + utf-8.** `initialize` advertises
  `"positionEncoding": "utf-8"`; `Position` derives; three LSP tests pin
  utf-8 advertisement, byte-offset mapping on glyph lines, and
  glyph-bearing `didChange` round-trips (`server.rs` tests).
- **SURF-0013 canonical formatter closed the parse-back gaps.**
  `collect_segments_with_dots` now keeps joined dotted paths as ONE
  segment, so `produce rust.library` keeps its dot through the tree and
  renders back byte-identically (previously canonical was the space form
  `produce rust library`); expression paths render `.` (`state.scale`),
  and non-bare section generics render `evaluate <score>:` /
  `example <name>:` (angle, spaced; `record`/`variant`/`trait`/
  `implementation`/`predicate`/`type`/`implement` keep the bare two-word
  head their parser dispatch requires). Both corpus files are now
  byte-canonical: new regression tests
  `corpus_files_are_lossless_round_trip` and
  `corpus_canonical_reparse_is_stable` in `formatter.rs`. `affine_scorer`
  edited (non-canonical blank line dropped, example heads angle-ized);
  the artifact-identity lane still passes byte-identical.
- **SURF-0007 planner alive + refusal pinned.** `emath planner` used an
  empty static registry and refused every goal (dead command). It now
  registers the in-tree static `native.rust` capability
  (`evaluate.rust.library`, f64, exact, deterministic, E2,
  `sir-checker.v1`) mirroring the `provider list` table, so supported
  goals select a native plan and genuinely unplannable goals
  (`produce dew.jit` → E-GOAL-042) still exit 1. New gates: planner
  positive + refusal lanes, `fmt` canonical + non-canonical-refusal
  lanes, `bench` E-TLT-004 lane.
- **Language examples admit-or-refuse loop.** `validate.sh` now walks
  `language/examples/*.emath`: every example either admits (then must
  build) or refuses with a documented E-code; nothing is silently
  accepted or refused without a code.
- **CLI reference made real.** `implementation/CLI_REFERENCE.md` was a
  stale "# Planned CLI Reference" placeholder; rewritten to the actual
  implemented surface (commands, flags, exit classes, LSP) so
  `docs/P11_TOOLING_AND_DX.md`'s claim about it is honest.
- **Vestigial param removed.** `check_tree(tree, _unknown_sections: &())`
  → `check_tree(tree)` (call sites updated in `session.rs` and the
  orphaned `tests/emath-sema/admit_extern_negative.rs`).
- **Stale cache-policy claims fixed (SURF-0002).** The old
  `implementation/tests/valid/stateful.emath` →
  `examples/generated/affine-policy-rs` and `cargo xtask demo cache-policy`
  claims named paths/demos that no longer exist; corrected to
  `tests/valid/affine_scorer.emath` → `examples/generated/affine-scorer`
  (artifact `fnv1a64:d70202296d3f871c`) and `cargo xtask demo affine-scorer`
  across `BUILD_STATUS.md`, `docs/P12_1_0_DEMONSTRATIONS.md`,
  `docs/P12_1_0_FREEZE_AND_GATES.md`, and the v7 bundle
  (`BUNDLE_MANIFEST.json`, MASTER spec, G12 phase doc, PANES contract).
- Cass CLI unavailable (`cass-cli-unavailable`); no cassette replay used.

## Keep-gate harness + first measurement (gauntlet-06, 2026-08-18)

- **`[profile.release-perf]` added** to the workspace manifest
  (inherits release; opt-level 3, thin LTO, codegen-units 1,
  line-tables-only debug, strip false). Claims use
  `cargo bench --profile release-perf`, never `--release`.
- **Keep-gate driver** `crates/emath-cli/benches/keep_gate.rs`
  (`[[bench]] comprehensive_bench`, `harness = false`, std-only): the
  driver owns the clock (`Instant::now`), warmup 2 / min 3 / max 10 /
  5s target; six families — `parse`, `check`, `codegen-parametric`
  (in-memory analyze + generate, no rustc), `artifact-json`, `genesis-
  replay` (analyze-twice determinism pin inside the cell), `cli8p` (8
  parallel `emath check` subprocesses on disjoint files). Per-family
  `.bench-history/<family>.latest.json` (lab-core sorted-key JSON, raw
  samples retained) + `guard.json`
  (`DETERMINISTIC_CODEGEN`/`PHASE1_STD_ONLY`/GIT_SHA/TIMESTAMP
  equivalents).
- **`cv_pct` added to lab-core `Summary`** (population CV percent);
  `QUARANTINE_CV_PCT = 5.0`; `Summary::quarantined()` quarantines
  noisy cells. Injected-sample unit tests never read as baseline.
- **`identity=` SHA-256**: new std-only `emath-lab-core::sha256`
  (verified against all four NIST FIPS 180-4 vectors: empty, "abc",
  two-block, 1M×'a'). codegen-parametric cell emits
  `identity.value` + `golden_lib_rs_match` against the committed
  `examples/generated/semantic-genesis-worlds/src/lib.rs` — **true** on
  the real run (Phase 4 golden unchanged).
- **First real run committed** (`65b865a`): 7 `.bench-history` files
  from an actual `cargo bench --profile release-perf --bench
  comprehensive_bench` (git_sha `21c68aa`). Results: codegen-parametric
  0.11 ms cv 3.6% ok; genesis-replay 0.06 ms cv 0.6% ok; parse/check/
  artifact-json/cli8p measured but **quarantined** (cv > 5%) — these
  sub-millisecond cells on this host are too noisy for any claim; no
  keep is made.
- **PERF-0002..0005 closure:** measured (the four families above) with
  Form-1 retry predicates — no frame ≥0.1% can be attributed while the
  cells sit sub-ms/quarantined; PERF-0005 closed name-only (no
  FastPathGuard counter was added; the type name is not
  instrumentation). **No speedup claimed; no Pattern 1–10 source
  change** (codegen-parametric has no FastPathGuard path since PERF-0005
  is not confirmed).
- **`emath bench` stays E-TLT-004** (message now names the real
  harness + `.bench-history/`); CLI never EXIT_OK with empty output,
  refuses while comparison ruleset (Phase 4+) is absent. **No
  Performance category added** to any parity contract (none exists
  in-tree; F-EMATH-PERF-001 has no contract file here).
- **`E-SCHEMA-001` registered**: during the gate an untracked
  `crates/emath-schema/src/registry.rs` (thirteen-schema registry)
  appeared in the working tree (mtime 2026-08-17 07:21, mid-session,
  NOT created by this bead's commands — flagged for user awareness);
  its code was preserved, documented (issued list + prefix row +
  annex; now 243 emitted / 243 documented), and lint-fixed
  (format-arg polish only). `emath-schema` tests pass standalone.
- Full gate green: fmt, workspace tests, clippy `-D warnings`,
  `validate.sh` (including the still-refusing `emath bench` E-TLT-004
  lane).

## Generate-or-gate docs against HEAD (gauntlet-08, 2026-08-18)

- **`implementation/CRATE_MAP.md` rewritten (SURF-0003):** every row now
  names the directory that exists on disk — 47 implemented rows (38
  `crates/` + 4 test suites + 4 examples + `xtask`), 9 planned rows
  (never certifying), plus the recorded name corrections
  (`emath-macros`→`emath-macro`, `emath-lab`→`emath-lab-core`) and the
  planned provider repos. Tier labels reflect the actual crate purposes
  (read from crate doc headers; no invented responsibilities).
- **`implementation/PUBLIC_API_INVENTORY.md` rewritten (SURF-0001):**
  CompilerSession block is now the real HEAD surface (`new(limits)`,
  `load_package(&mut self, path)`, `load_text`, `parse_text(&self)`,
  `check(&mut self, FileId)`, `check_owned`, `plan(&mut self, FileId)`).
  The promise-typed surface (`LoadRequest`/`GoalRequest`/`BuildRequest`)
  is explicitly **Partial** and `[planned]`-marked — the gate refuses any
  line that names those tokens without the marker, so API-001..004-style
  doc-amendment-to-Passing is impossible. Provider/Runtime/Artifacts/
  Laboratory sections now honest: crate exists, surface evolving.
- **`scripts/check_doc_gates.py` (new):** std-only (tomllib + re)
  gate — R1: implemented rows exist on disk AND row name matches the
  directory (no name rewritten behind a surviving path); R2: every
  `crates/` dir is mapped; R3: every workspace member is mapped; planned
  names never masquerade as implemented; inventory fence must match
  `session.rs` method-for-method (name + receiver kind, brace-matched
  impl extraction); request-type tokens require `[planned]` on the line.
- **`scripts/validate.sh` doc-gates lane:** positive pin (crate map 47/47,
  inventory 7/7), negative controls on mutated copies (map name+path,
  session `&mut`→`&self`) must fail, and the policy lane:
  `git check-ignore` on `implementation/PUBLIC_API_INVENTORY.md` must be
  false. `.gitignore` changed: `/implementation/` → `/implementation/*`
  with re-includes only for `CRATE_MAP.md` + `PUBLIC_API_INVENTORY.md`
  (the rest of `implementation/` stays ignored).
- **`BUILD_STATUS.md` rewritten (MINED-C-002):** explicit "not a
  certifying oracle" header; every green claim names its non-empty test,
  capstone or artifact hash (registry_complete 243/243, corpus
  round-trip tests, affine artifact `fnv1a64:d70202296d3f871c`,
  capstones, keep-gate first run, doc gates); the stale "Planned"
  section now matches the crate map (lossless parser/formatter, planner,
  macro, LSP rows removed as implemented; Partial request-typed surface
  and planned crates named).
- **No compiler/source change and no behavior change in this bead**: no
  Rust edited (artifacts byte-identical by construction; artifact-identity
  lane re-verified), no `LoadRequest` implementation (out of scope per
  bead). Note: evidence pointers (`phase12_remediation_surface.md`,
  `GAUNTLET_EXPERIMENT_DESIGNS.md`, `SURFACE_DEFERRALS.md`,
  `docs/contracts/spec_version_contract.toml`, API-001..004 feature ids)
  do NOT exist in this workspace — implemented from the bead description;
  with no spec-pin file in-tree, "remove BUILD_STATUS from spec pin" is
  vacuous and the doc gate lane is the named contract (a doc correction
  is a named bump of the pinned files, never silent).

## Contract cross-check (gauntlet-d1, 2026-08-18)

- **Issued list completed (cluster 1):** the hand-maintained
  `ERROR_CODES.md` issued list named only 108 of 243 emitted codes.
  Added 135 evidence-backed entries (descriptions read from the emission
  sites; two family codes, `E-HOST-002` and `E-TYPE-310`, are honestly
  documented as having no construction site in HEAD). The issued list
  now names every production `E-*` exactly once; the gate refuses a
  duplicate bullet, an emitted code without an issued entry, and a
  stale issued entry absent from the annex.
- **Machine contract gates (new + wired into `scripts/validate.sh`):**
  - `check_doc_gates.py` now also scans the seven-doc contract set
    (AGENTS, ERROR_CODES, CLI_REFERENCE, HANDOFF, BUILD_STATUS,
    CRATE_MAP, PUBLIC_API_INVENTORY) and fails on the eight forbiddance
    patterns inherited from the d1 bead: the Dew-differential marker,
    the mined bound value and its phrase, an affirmative keep-gate
    victory claim, wordings asserting BUILD_STATUS certifies without
    negation (negated mentions pass), FNV never presented as
    cryptographic identity outside the caveated rule-7 label, and
    stub/placeholder/TBD/TODO tokens in CLI_REFERENCE.
  - `dump_error_codes.py --check` proves the annex is byte-current with
    the emitted set (243/243) and that the issued list and annex name
    the same codes.
  - `scripts/check_doc_pins.py` + `implementation/contract-pins.json`
    (NEW): the hashed-doc contract loader — SHA-256 pins for the six
    contract docs (HANDOFF, BUILD_STATUS, ERROR_CODES, CLI_REFERENCE,
    CRATE_MAP, PUBLIC_API_INVENTORY). A pinned doc that changed without
    a pin update + `bumps` note fails the gate: doc drift is a named
    bump, never silent (closure predicate Form 5).
- **Human grep of the forbidden strings: clean.** None of the mined
  upgrade-claim markers appear in any contract doc, no FNV-crypto claim
  (only bootstrap-labeled FNV mentions remain), no keep-gate victory
  claim (quarantined cells make no claim), no CLI stub documented as
  implemented.
- **Two-layer plan split + schema ids (cluster 2):** durable artifact
  layer (`emath.artifact.v1`, `emath.source-map.v1`,
  `emath.resolution-plan.v1`) vs genesis-generated provenance layer
  (`emath.generated-crate-manifest.v1`, `emath.generated-crate-source-
  map.v1`) stay separate: the provenance parse-back lane asserts genesis
  docs never claim the durable ids; the thirteen canonical
  `emath.<name>.v1` registry ids are minted by
  `crates/emath-schema/src/registry.rs` (anything else is `E-SCHEMA-001`).
  FNV remains labeled bootstrap content identity, never release
  crypto (AGENTS rule 7).
- **Cluster 4:** `EngineIdentity` values (subject/oracle/baseline/
  mutant) + the `emath-term` oracle pin remain recorded in HANDOFF; no
  doc claims Dew certifying.
- **Cluster 5:** `implementation/CLI_REFERENCE.md` documents only the
  implemented surface; spec-11 is not named; bench stays `E-TLT-004`.
- **Cluster 6:** harness runbook uses `cargo bench --profile
  release-perf`, never `--release`; first run committed (`65b865a`);
  no win claimed. **Cluster 8:** CRATE_MAP/PUBLIC_API_INVENTORY pinned
  by the gauntlet-08 gate; BUILD_STATUS has the not-a-certifying-oracle
  header and names non-empty evidence.
- Evidence pointers (`docs/contracts/spec_version_contract.toml`,
  `supported_surface_matrix.toml`, `REMEDIATION_PLAN.md`,
  `cass_blocker.md`) do not exist in this workspace — implemented from
  the bead description; the contract-pins file IS the in-tree
  spec-pin contract. Full gate green (fmt + validate.sh incl. the new
  annex/pins lanes).

## AGENTS.md operating-contract update (2026-08-17)

The user replaced `AGENTS.md` with a rewritten operating contract. This
section records what changed and what was tuned in support of it.

**Contract delta (from the previous emath-specific version):**

- Added: RULE 0 (user override), RULE 0.1 (value over process; no
  process porn; the value test; the stop rule), RULE 1 (no file
  deletion without express permission), irreversible-git break-glass
  rules, main-only / no branches / no worktrees (RULE 2), toolchain
  baseline (Rust 2024 edition, flat `emath-*` crates,
  `deny(unsafe_code)` where practical, feature-flagged frontier),
  unsafe-boundary policy, performance-program rules, code-editing
  discipline (no scripted rewrites, no file proliferation), backwards
  compatibility stance, CONTRACT.md-per-crate, output style, DSR-first
  verification (GitHub Actions is never the CI source of truth;
  workflows retained as manual specs), testing policy ("NEVER run full
  Cargo or RUSTC tests; targeted tests are law"), `ubs` before
  commits, RCH target-dir conventions, search tools (`zero`/`ast-grep`
  preference), Agent Mail MCP + file reservations as the isolation
  mechanism.
- Removed/superseded: the old authority hierarchy (neutral-IR
  constitution), hard rules 1–8 (including producer/consumer/test/
  artifact-per-feature, FNV bootstrap rule 7, rust-next immutability
  rule 8 — several still honored by existing code but no longer
  AGENTS.md law), and the "DO NOT CREATE USELESS TESTS"/validation-
  command table (superseded by targeted-tests-only + DSR).
- Conflict noted: the embedded beads `bv` template's "Session
  Protocol" (git add/commit/push) contradicts the Beads section's
  "commit only when the user asks or the workflow requires it";
  conservative reading adopted (no commits unless asked).

**Environment probe (2026-08-17):**

- `dsr`, `rch`, `ubs`, `ast-grep` present on PATH; `zero`/`asgrep`
  CLIs absent; the `zero___zero_execute` MCP is registered and its
  `zero.fs` read route works for byte payloads, but the zsx CodeMode
  sandbox forbids every JS decode primitive observed
  (`String.fromCharCode`, `TextDecoder`, `.apply`) — `zero.fs.read`
  cannot yield text content in this build, so harness Read/Edit tools
  were used as the reported, non-silent fallback for file editing.
- DSR doctor: exit 0, config valid; warnings: docker not running,
  `act`/`actrc` missing, minisign key unset, `syft` missing
  (local-build/SBOM gaps only). RCH: ready (one cleared worker
  warning). GitHub Actions `ci.yml` is workflow_dispatch-only with no
  `master` refs; retained untouched as a manual spec per AGENTS.md.

**Agent Mail wired (new contract):**

- Identity `SageCave` registered for this project (id 702, project
  emath-26ae536d34). Inbox empty; no prior threads. Sibling
  identities: OrangeFox, FrostyCrane, GoldRiver, AirTrafficControl
  (earlier bead lines).
- Edits under exclusive reservations: `HANDOFF.md`, `BUILD_STATUS.md`,
  `scripts/validate.sh`, `implementation/contract-pins.json`
  (granted as reservation ids 2053–2056, released at session end).
- No coordination messages sent: no active shared work and nothing to
  announce (inbox empty); RULE 0.1 says no ceremony for its own sake.

**Tuning applied:**

- `scripts/validate.sh`: header + compile/lint echo reworded to the
  new contract (DRS-first; never full cargo tests; the gate proves the
  artifact / negative-control / capstone / doc lanes only).
- `BUILD_STATUS.md`: operating-contract note added — the listed
  full-gate runs are archival evidence under the previous contract;
  current procedure is DSR + targeted tests.
- `implementation/contract-pins.json`: named bump for the two edited
  contract docs. `AGENTS.md` itself stays unpinned (user-edited
  frequently; the pinned set covers correctness-contract docs).
- `contract-pins.json` was touched externally earlier this session;
  content matched the last written state, so no change was adopted.

**Validation:** no full cargo runs this session (new contract). Ran the
targeted doc lanes: `check_doc_gates.py` (incl. the new AGENTS.md in
the scanned set — passes), `check_doc_pins.py`, `dump_error_codes.py
--check`, and `bash -n` on the edited script. All green.

**Flagged, not done (need a user decision):** Rust 2024 edition
baseline vs the workspace's edition 2021 / rust-version 1.74 (an
edition migration is a sweeping change, not small tuning); zero
CONTRACT.md files exist for the 38 crates; retention of `ci.yml` as a
manual spec is per AGENTS.md (no action).

## Rust 2024 edition migration + per-crate contracts (2026-08-18)

**Edition migration (user-approved "migrate now").** Root workspace
moved to `edition = "2024"` / `rust-version = "1.85"`; the
generator-emitted manifest literals (affine + semantic-genesis), the
four explicit-edition test crates and both golden manifests followed;
`PackageIdentity.edition` was updated as a metadata label (not part of
canonical bytes).

- Newly-active lints under 2024 were fixed at the edges:
  `f64::midpoint` ×3 (`emath-ir/domains.rs`, `emath-lab-core/stats.rs`)
  and `is_none_or` ×1 (`emath-adapter-rumoca/subset.rs`) —
  behavior-preserving patches.
- `cargo fmt --all`: rustfmt's style edition follows the manifest
  edition, so the whole tree was re-passed to 2024 style (118 tracked
  files; import sort order, `format!` indentation, `))` → `));`
  statement normalization).
- Golden identity after the style pass: the semantic-genesis golden's
  `main.rs` import order changed under rustfmt 2024, so `render_main`
  in `emath-world-codegen-rust` was updated to emit in sorted statement
  order (B < F < M). Regeneration is byte-identical again; the golden's
  three swap-mutation contract tests pass; the affine golden is still
  byte-identical.
- Annex re-splice: the extractor's 4-line snippet window lost the
  message context for `E-NAME-026` / `E-PROV-510` after fmt reflow (the
  messages were verified still present at the emission sites). The
  annex was re-spliced per the documented protocol; `--check` passes
  (243/243).
- Contract docs re-pinned (named bump) for the three changed files.

**Per-crate contracts (user-approved "full campaign").** 38/38
`crates/*/CONTRACT.md` files now exist (none before), each with the ten
sections AGENTS.md requires (purpose/layer per CRATE_MAP tier, public
types from source, invariants, error model with real E-* codes,
determinism class, cancellation, unsafe boundary, feature flags,
conformance tests, no-claim boundaries). Written by seven parallel
workers; the four files from the worker that hit backtick-escape
trouble were spot-checked for content and escaping.

**Validation (targeted only; no full cargo test runs):** `cargo check
--workspace`, `cargo clippy --workspace --all-targets -- -D warnings`
(exit 0), `cargo fmt --all -- --check`, targeted tests
(`emath-ir`, `emath-artifact`, `emath-term`, `emath-trust-gates`,
`emath-rust-backend`, `emath-lab-core`, `emath-world-codegen-rust`
4/4, golden crate 3/3), SG + affine identity lanes byte-identical,
doc gates + annex + pins lanes green.

**Still open:** the whole campaign (100+ modified/untracked files, the
CONTRACT set, both goldens) remains uncommitted — HEAD is the old
scaffold plus `65b865a`; per AGENTS.md, commit only when you ask.
`ci.yml` stays a manual spec (workflow_dispatch-only; no action).
