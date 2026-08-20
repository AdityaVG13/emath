# Vision (positioning)

Umbrella for bead `emath-b3e`. Constitution (`emath-k4u`) and Neutral IR
(`emath-ir`) outrank this copy. Launch/adoption copy is `emath-ln2`; moat
copy is `emath-7j5`. Terminology follows `language/spec/00_LANGUAGE_OVERVIEW.md`.

## Frame

Three commitments, one pipeline:

1. **Any mathematics as intent.** A `.emath` declaration states meaning
   (definitions, equations, constructors, laws). Goals name work. Partial,
   unproven, or invented structure is admitted when it is structurally
   well-formed; missing execution is a typed disposition, not a silent drop
   (`language/spec/14_TOTAL_COMPILATION_PROTOCOL.md`).
2. **Executable portfolios.** Underconstrained math may have several
   coherent interpretations. The compiler retains them as worlds and
   artifacts (native, parametric, exploration, continuation, diagnostic)
   rather than collapsing to one unlabeled guess.
3. **Protected optimization.** Candidates may vary meaning or implementation
   only inside an admission / measurement / Pareto / promotion envelope.
   Rank never raises evidence authority.

V6 named this a mathematical-intent compiler. V7 added genesis: glyphs and
partial constraints can become checked worlds before ordinary compilation.

## North star

Compile any finite mathematical intent into an honestly labeled executable
artifact (or typed refusal); retain competing interpretations as a
portfolio; optimize only under a protection envelope that rejects a wrong
or host-worse candidate and keeps a receipted baseline.

## Language

**Today.** Spec is `language/spec/`. Surface crates: `emath-syntax` (lexer,
lossless tree, parser, formatter, G0 `emath custom` worlds), `emath-schema`
(kind-schema registry), `emath-hir`, `emath-term`, `emath-source`,
`emath-core`. `emath-lsp` is a deterministic admission-backed skeleton.
`emath check` / `emath fmt` cover the Phase 1 scalar subset
(`tests/valid/`; see README: the implemented subset is smaller than the
spec). Meaning-versus-work is already in the language: `definitions:` vs
`goals:`.

**Aspirational.** Full section families from spec 00 (packages, units,
shapes, domains, events, prove/optimize goals). Planned crates on
`implementation/CRATE_MAP.md` are not implemented: `emath-package`,
`emath-types`, `emath-units`, `emath-shapes`, `emath-domains`,
`emath-format`, `emath-canonical`. Zero-math learnability is horizon.
Surface minimalism is doctrine: admission demands only what meaning
requires (`outputs:`, `goals:`, `tests:`, `compile:`, `exports:` are
optional; an example without `expect` is a worked example — it computes
and displays, claiming nothing). The WASM playground is present (see
Compiler); graphing and AI-assisted learning in the pane are horizon.

## Compiler

**Today.** `emath-sema` (`CompilerSession`) admits and plans. Phase 1 plans
`evaluate` → `rust.library` only; other goal kinds refuse (`E-GOAL-043`).
`emath-ir` holds SIR/GIR/plan/EMIR/evidence, the MIG, and the ten-layer
registry. `emath-goal` elaborates requests. `emath-plan` is a deterministic
native planner with total dispositions and parametric lift; no external
providers are installed. Lowering: `emath-exec-ir` → `emath-rust-backend` /
`emath-rust-ir` → `emath-artifact` (seven artifact classes) → `emath-build`
(check → plan → generate → compile; `--verify` is honest about missing
tests). Host: `emath-cli` (`check`, `plan`, `planner`, `build`, `run`,
`test`, `explain`, `web`, genesis family). Playground: `emath-wasm`
(C-ABI engine, no bindgen) + `emath web` host a local browser pane;
`emath-exec-ir` carries a strict-f64 Tier-0 interpreter so `run` works
in-browser without cargo, labeled `interpreted-strict-f64` — the
compiled Rust tier stays the evidence pipeline's native lane, and
tier agreement is a differential gate. Genesis substrate: `emath-genesis`
(built-in worlds, VM, CSA totality baseline), `emath-world-ir`,
`emath-world-codegen-rust`, `emath-portfolio`. Adapters
`emath-adapter-dew` and `emath-adapter-rumoca` are native stand-ins.
Working demos: affine-scorer and semantic-genesis.

**Aspirational.** Full goal set (differentiate, solve, optimize, simulate,
prove, …). Live Dew / Wrenfold / Franken* providers. Planned:
`emath-provider-host`, `emath-host`. `emath bench` remains `E-TLT-004`.
`migrate` is specified, not shipped.

## Frontier engine

**Today.** Library scaffold, not a shipped campaign product.
`emath-lab-core`: experiment manifests, quality gates, promotion policy,
Pareto archive, keep-gate identity. `emath-tuning`: campaign receipts, five
promotion gates, fallback when nothing promotes. Adjacent:
`emath-calibration`, `emath-holes`, `emath-law-check`, `emath-evidence`,
`emath-checker` (independent check + negative-control battery),
`emath-agent-protocol`. Tier 8 (`emath-store`, `emath-provenance`,
`emath-search`) is feature-gated spike adapters. `examples/demo-host`
exercises promotion + a negative control on the affine slice. P12
demonstration 7 (protected host promotion and rollback) is planned.

**Aspirational.** A live protection envelope against real host metrics:
candidate construction, admission, measurement, Pareto archive, promotion
receipt, and rollback, with `emath bench` as a comparison ruleset rather
than a typed refusal.

## Success criteria

| Lane | Pass means |
| --- | --- |
| **Semantic** | Admitted meaning is explicit (world, plan, disposition). Unsupported work is a typed refusal or diagnostic artifact. Portfolio rank does not escalate authority (`Structural` / `Tested` / `Certified` / `Proved` stay honest). |
| **Execution** | A successful build is a Cargo artifact with recomputable identity. `run` / `test` execute generated tests; unverified steps are `not-run`. |
| **Evidence** | Independent checker; claim language cannot exceed checks (`E-EVID-201`). Wrong or host-worse candidate rejected; baseline fallback preserved; promotion receipted. |
