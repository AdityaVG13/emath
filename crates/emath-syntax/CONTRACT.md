# emath-syntax

## Purpose and layer

Tier 1 bootstrap syntax crate: layout lexer and recursive-descent parser.
Provider-free. Implements the kernel `SourceParser` seam so `emath-sema` can
admit without depending on this crate. The syntax tree is owned by
`emath-core` (`emath_core::tree`).

## Public types and semantics

- `parse(text, file, limits) -> (SyntaxTree, Diagnostics)`: parse with bounds.
- `parse_str(text) -> (SyntaxTree, Diagnostics)`: parse with default limits.
- `parse_lossless(text, file, limits) -> LosslessParse`: parse retaining
  comments and spans for formatting round-trips.
- `format_lossless(&LosslessParse) -> String`: canonical, idempotent,
  comment-preserving formatting.
- `LosslessParse`: `{ tree, diagnostics, comments }`.
- `SyntaxParser`: process-wide default `SourceParser` unit.
- `install_source_parser()`: idempotently installs `SyntaxParser`; hosts must
  call once per process before first parse.
- Modules: `formatter`, `genesis`, `lexer`, `parser`, `token`, `tree`, `scratch`, `exactness`.
- Scratch / meaning-budget (crate-root re-exports from `scratch`): `expand_scratch`, `apply_solve_candidate`, `ScratchExpansion`, `ExpansionOutcome`, `ScratchLevel`, `ScratchRewriteLevel`, `ScratchNote`, hole types, `SolveIntent`, `SolveWorld`.
- `apply_solve_candidate(source, SolveWorld) -> Result<(String, String), String>`: pin one closed-menu world. String labels stop at `SolveWorld::parse_label`. No `&str` overload; `SolveCandidate` is not a type (removed, not re-exported).
- Exactness (crate-root re-exports from `exactness`): `ExactnessDimension`, `ExactnessEntry`, `ExactnessLedger`, `ExactnessStatus`, `exactness_ledger`, `exactness_ledger_raised`, `explanation_notes`. `from_raise_token` admits only `units` / `unit`. Not crate-root: `exactness::claims_exactness_with_open_holes` (used by freeze).

## Invariants

- No panics on arbitrary UTF-8 input.
- Exact spans on every token and node.
- Indentation enforcement, duplicate-section checks, precedence handling.
  `example <name>:` (and `example name:`) may have an empty body and
  admits as a worked example; other `:` heads still require `E-SYN-112`.
- Stateless `emath function name(args) -> T:` head-args parse into
  `Declaration.signature` (untyped names store the `Infer` marker). Mixing
  head-args with an `inputs:` / `outputs:` section is `E-SYN-122`. Head-args
  on a stateful or non-function declaration are `E-SYN-123`.
- Bounded source/token/nesting limits.
- Recovery at statement boundaries.
- Formatting is idempotent, comment-preserving and parse-stable.
- `genesis` parses `emath custom` world declarations (G0 only).

## Error model

Stable diagnostics through `emath_core::Diagnostics`, returned alongside the
tree. No panic on malformed input. Head-args mixing is `E-SYN-122`; head-args
on a stateful or non-function declaration is `E-SYN-123`.

## Determinism class

Deterministic. Tokenization and parsing are pure over source bytes; lossless
formatting is canonical and idempotent.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. Crate root is `#![forbid(unsafe_code)]`.

## Feature flags

None.

## Conformance tests

No `crates/emath-syntax/tests/` directory and no inline `#[cfg(test)]`
modules in `src/`. Conformance lives in the standalone `tests/emath-syntax`
package: `parser_refusals_negative.rs`, `unit_brackets.rs`, `genesis.rs`,
`edge_cases.rs`, `formatter.rs`, `limits_series.rs`, `cases_expr.rs`,
`head_args.rs`, and `src/lib.rs`.

## No-claim boundaries

Parser is a bootstrap implementation and is replaceable by the Phase 4
lossless parser. Surface recognition is a universal mechanism: a spelling,
syntax node, parser branch, or successful parse grants no FeatureID authority
and makes no mathematical, world, exactness, or evidence claim. Named meaning
comes only from the authored capsule selected by the verified Language Image.
The G1 world/forest stage (bounded parse forest plus signature inference) lives
in `emath-genesis`; this crate carries no emath-genesis dependency and makes no
claim on it.
