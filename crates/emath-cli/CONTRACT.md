# emath-cli CONTRACT

Constructor-layer CLI. This file is the live command contract. Historical
planner, lab, genesis, and `goal_met` text is not admission.

## Purpose and layer

- Host entry is `run(&[String]) -> CliExit`.
- Live commands: `check`, `run`, `step`, `inspect`, `verify`, `test`,
  `build`, `api`, plus `new`, `fmt`, `migrate`, `help`, `version`,
  `doctor`, `diff`.
- `emath simulate`, `plan`, `planner`, `eval`, `sweep`, `genesis`,
  `solve`, `search`, and other extracted tokens refuse `E-KIND-GONE`
  and name `emath run`. There is no `emath-lab` execution surface.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Completed constructor value or successful inspect/help |
| 2 | Syntax, type, or input admission failure |
| 3 | Unmet, partial, or suspended requested answer |
| 4 | Execution or backend fault |
| 5 | Incompatible or corrupt checkpoint |

A suspended run is 3. `inspect` may return 0 when it reads a suspended
checkpoint.

## Receipt

`emath run --json` prints `emath.constructor.v1` with orthogonal fields
`execution`, `fulfillment`, `representation`, `payload`, `evidence`,
`remaining`. It does not print `goal_met`.

## Invariants

- Constructor files are admitted as `emath object`, `emath function`,
  or `emath query` only.
- Semantic commands load the verified `language/` image. The image is
  constructor contracts, scalar carriers, and the generic ABI. Discovery
  never suggests removed aliases or default recipes.
- `emath migrate` is format-only. It is not a recipe translator.
- `emath api --search` returns constructor contracts and imported
  module exports.
- Checkpoint resume requires the constructor-layer ABI
  (`constructor-layer/continuation-abi`). A foreign image refuses.

## Public types

- `CliExit` and `run(&[String])`.
- Constructor evaluation lives in `emath_exec_ir::constructor_layer`.
- Constructor emission lives in `emath_exec_ir::constructor_emir`.

## No-claim

- Completing a run is not a theorem.
- `verify` replays recorded observations.
- Leftover planner / simulate / `goal_met` functions may remain in the
  crate (no file deletion). Constructor files do not use them.
