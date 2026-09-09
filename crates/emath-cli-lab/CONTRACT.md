# emath-cli-lab CONTRACT

## Purpose and layer
- Extracted lab/host CLI (`emath-lab`) for commands that are agent/process
  tooling, genesis/lab, or remaining host surface rather than the documented
  production `emath` compiler (bead `emath-qbk53`).
- Depends on `emath-cli` (not the reverse). Production `emath` never links this
  crate. Tests dispatch through [`run`] so keep-commands and extracted commands
  share one host.

## Commands
Production `emath` keeps: `check`, `plan`, `planner`, `build`, `parse`,
`compile`, `simulate`, `new`, `fmt`, `migrate`, `explain`, `run`, `test`,
`verify`, `inspect`, `diff`, `doctor`, `help`, `version`.

This crate owns:

- agent / process: `agent`, `capabilities`, `robot-docs`, `coverage`, `web`,
  `serve`
- genesis / lab: `genesis`, `eval`, `sweep`, `repl`, `world`, `portfolio`,
  `meaning`, `fit`
- remaining host: `import`, `artifact`, `architecture`, `freeze`, `why`,
  `assumptions`, `signature`, `expand`, `solve`, `exactness`, `vendor`,
  `provider`, `fork`, `bench`

Keep-command tokens forwarded to `emath_cli::run`.

## Public types and semantics
- `run(&[String]) -> emath_cli::CliExit`: combined host used by tests.
- Re-exports extracted modules (`eval_cmd`, `coverage_cmd`, `serve_cmd`,
  `agent_protocol`, JSON document builders from the former `cli_scratch`).

## Invariants
- Same exit codes as `emath-cli` (0/1/2).
- Semantic commands still verify the Language Image via
  `emath_cli::refuse_unverified_language_image`.
- No new IR ops, FeatureDispatch arms, or domain-named compiler types.

## Error model
- Same `CliExit` mapping as production; typed E-* refusals stay on stderr.

## Determinism class
- Deterministic JSON and catalog text; `web`/`serve` are localhost process
  hosts (not a numeric claim).

## Cancellation behavior
- Same as production CLI: synchronous except the long-running web server.

## Unsafe boundary
- None: `#![forbid(unsafe_code)]`.

## Feature flags
- None.

## Conformance tests
- Workspace suites in `tests/emath-cli` dispatch through this crate's `run`.

## No-claim boundaries
- This crate is not the production user surface. `emath eval` on the
  production binary is an unknown-command hint to `emath-lab`.
