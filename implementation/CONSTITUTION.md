# emath Constitution -- Laws as Enforceable Invariants

Thirteen laws plus constitutional additions C1-C10, each tracked to a
code-level enforcement point. An entry names the invariant, the exact
anchor that enforces it today, and the check that exercises it. Where
enforcement is structural (the design makes violation inexpressible)
that is stated instead of pointing at a runtime guard.

Verification lanes referenced below:

- `cargo xtask demo semantic-genesis` -- end-to-end pipeline gate.
- `cargo test -p <crate> --lib` -- targeted unit lanes per anchor.

## The Thirteen Laws

### L1. Lowercase identity

The project identity is lowercase `emath` everywhere: crate names,
schema ids (`emath.*`), artifact families. Homoglyph and lookalike
identifiers are refused at admission (`crates/emath-sema/src/admit.rs`,
Cyrillic/Greek lowercase lookalike tables). Workspace layout contract:
`implementation/CRATE_MAP.md`.

### L2. Semantic sovereignty

Meaning lives in provider-neutral World IR. Provider-native types never
appear in the schema; provider references are string ids only
(`OperatorSemantics::ProviderBinding`,
`crates/emath-world-ir/src/lib.rs`, pinned by `WORLD_IR_VERSION` docs).

### L3. Constructors establish validity

Values exist only through constructors carrying require/ensure
obligations: `Constructor::obligation_matrix` and
`ConstructionReceipt` (`crates/emath-ir/src/constructor.rs`); the Rust
backend lowers runtime invariant checks and receipts
(`crates/emath-rust-backend/src/lib.rs`).

### L4. Meaning and goals separate

The genesis pipeline emits `meaning-problem.json` (what the symbols
mean) separately from `answer-receipt.json` (what was asked and
answered); explore/protect/answer are distinct sections of the source
(`crates/emath-syntax/src/genesis.rs`,
`crates/emath-cli/src/genesis_cmd.rs`).

### L5. Provider plurality

Interpretation is a portfolio, never a single oracle: five admitted
world classes ranked in `InterpretationPortfolio`
(`crates/emath-portfolio/src/lib.rs`; roster `ADMITTED_WORLDS` in
`crates/emath-cli/src/genesis_cmd.rs`). Gate: xtask demo asserts at
least five portfolio world classes.

### L6. Total artifact behavior

Every outcome is an artifact, including failure: seven artifact classes
with per-class required contents, `Diagnostic` included
(`ArtifactClass`, `required_paths_for_class`,
`crates/emath-artifact/src/lib.rs`; manifest pinned by
`ARTIFACT_MANIFEST_VERSION`).

### L7. Authority follows evidence

`Authority` is an ordered ladder Structural < Tested < Certified <
Proved (`crates/emath-portfolio/src/lib.rs`). The genesis lane stamps
Structural whenever `checker_receipts` is empty -- no `tested` stamp is
ever invented (`crates/emath-cli/src/genesis_cmd.rs`, portfolio and
answer-receipt construction).

### L8. No hidden semantics

Operator meaning is an explicit enum -- declared expression, structural
constructor, or named provider binding (`OperatorSemantics`). Codegen
refuses any semantics map it cannot honor (E-GEN-094, SURF-0008,
`crates/emath-world-codegen-rust/src/lib.rs`) instead of silently
dropping it.

### L9. Bidirectional traceability

One id chain from bytes to answer: `source-artifact.json` seals the raw
bytes and glyph stream; `source_hash -> parse_id -> signature_id ->
term_id -> world_id -> answer_id` are all FNV-1a64 content ids bound in
`answer-receipt.json`; generated crates ship `source-map.json`
(`crates/emath-cli/src/genesis_cmd.rs`, `crates/emath-artifact`).

### L10. Protected optimization

Optimization never replaces the strict baseline: `StrictFastPortfolio`
holds both worlds and only constructs when strict is present
(`crates/emath-world-ir/src/translation.rs`); tuning campaigns assume a
strict baseline world exists to deopt to (`crates/emath-tuning`).

### L11. Deoptimization first-class

`DeoptReason` and `FastPathGuard` are typed values; `select_world`
falls back to strict on any failed guard
(`crates/emath-world-ir/src/translation.rs`).

### L12. Unknowns explicit

Unknown meaning is a value, not an error: `MeaningHole` with id, kind,
constraints, and state on `WorldIr.holes`
(`crates/emath-world-ir/src/lib.rs`); unadmitted worlds defer with
E-GEN-090/091 instead of failing; the parametric lane emits
`hole-manifest.json` naming every open meaning parameter.

### L13. Providers optional

The zero-provider path is total: `free_symbolic_world` +
`FreeTermWorld` evaluate any admitted term structurally
(`crates/emath-genesis/src/lib.rs`); the demo pins its answer as the
free oracle.

## Constitutional Additions C1-C10

- **C1 meaning explicit** -- every `OperatorDef` carries a semantics
  value and an origin; there is no default meaning
  (`crates/emath-world-ir/src/lib.rs`).
- **C2 unknown representable** -- `MeaningHole{Id,Kind,State}` types;
  hole graph in `crates/emath-holes`.
- **C3 structural totality before semantic invention** -- the CSA
  totality baseline (ADR-003) is computed on every genesis run and
  labeled `CSA_MEANING_CLAIM`: it witnesses totality and never asserts
  intended meaning (`crates/emath-genesis/src/csa.rs`,
  `csa-baseline.json`).
- **C4 invention not self-authorizing** -- agent proposals pass
  `ChallengeLoop::admit`, which refuses execution authority claims
  (`crates/emath-agent-protocol/src/challenge.rs`); genesis answers
  stay Structural without checker receipts.
- **C5 portfolios over hidden guesses** -- the answer is a ranked
  `InterpretationPortfolio`; `keep: pareto N` budgets change the
  artifact instead of hiding candidates
  (`crates/emath-cli/src/genesis_cmd.rs`).
- **C6 custom = new world** -- a new interpretation is a new `WorldIr`
  with its own content-bound identity; builtin classes enumerate the
  roster (`crates/emath-world-ir/src/builtin.rs`).
- **C7 parametric output = success** -- when meaning is open, `compile
  --parametric` emits a working generated crate over the `World` trait
  (`COMPILED_WORLDS` lane) plus manifest and hole manifest; the plan
  layer has a Parametric artifact class.
- **C8 hard constraints monotone** -- adding providers or enlarging
  budgets never destroys the artifact class: test
  `adding_providers_or_budget_preserves_the_artifact_class`
  (`crates/emath-plan/src/planner.rs`).
- **C9 host usefulness empirical** -- every build writes
  `benchmark-receipt.json` with phase durations and counters
  (`crates/emath-build/src/metrics.rs`); performance claims require
  measurements (AGENTS.md performance program).
- **C10 effects = capabilities** -- `WorldIr` declares `effects` and
  `capabilities` explicitly; both are identity-bound (mutation matrix
  test in `crates/emath-world-ir/src/lib.rs`). No ambient effects.

## Authority Firewall (folded from emath-dha7)

Strict declarations claim known meaning; genesis declarations authorize
constructed meaning. The lanes are separate parsers and separate CLI
surfaces (`crates/emath-syntax/src/parser.rs` vs
`crates/emath-syntax/src/genesis.rs`; `emath build` vs `emath
genesis`), so a silent fallback between them is not expressible.

## No-claim boundary

These are engineering invariants enforced by types, refusals, and
targeted tests -- not machine-checked proofs. Laws whose full scope
exceeds current implementation (L10 protected optimization at frontier
scale, C9 across real hosts) are enforced at the boundary that exists
today and noted in the relevant crate CONTRACT.md.
