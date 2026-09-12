# emath-cli-lab CONTRACT

## Purpose and layer
- Historical `emath-lab` binary. It is not a second constructor language.
- Constructor tokens forward to [`emath_cli::run`].
- Extracted tokens refuse `E-KIND-GONE` and name `emath run`.
- Command bodies stay on disk (RULE 1). They are unreachable from `run`.

## Commands
Live work uses `emath`: `check`, `run`, `step`, `inspect`, `verify`,
`test`, `build`, `api`, and the other constructor catalog tokens.

This crate still contains historical implementations of `eval`, `sweep`,
`genesis`, `fit`, `plan` helpers, and host tooling. Dispatch refuses
them. There is no `emath-lab eval` execution surface.

## Public types and semantics
- `run(&[String]) -> emath_cli::CliExit`: combined host used by tests.
- Re-exports extracted modules so old call sites compile.

## Invariants
- Extracted tokens refuse `E-KIND-GONE`.
- No new IR ops, FeatureDispatch arms, or domain-named compiler types.

## Error model
- Same `CliExit` mapping as production; typed E-* refusals stay on stderr.

## No-claim boundary
This crate does not restore genesis worlds, `emath model`, or a default
recipe catalog.
