# CLI Reference (constructor layer)

Live commands are `check`, `run`, `step`, `inspect`, `verify`, `test`,
`build`, and `api`. Exit codes: **0** completed, **2** admission, **3**
unmet/partial/suspended, **4** fault, **5** incompatible checkpoint.

`emath eval`, `plan`, `planner`, `simulate`, `genesis`, `solve`, and
`search` refuse `E-KIND-GONE`. Write an ordinary `emath function` or
`emath query` and `emath run`. There is no `goals:` layer and no
`emath-lab` execution surface.

## Semantic pipeline

| Command | Behavior |
| --- | --- |
| `emath check <file\|-> [--json]` | Parse and admit constructor source |
| `emath run <file> [--function NAME] [--set name=value] [--work N] [--json]` | Evaluate a function or query; print a constructor receipt |
| `emath step <checkpoint> [--work N] [--json]` | Resume a constructor-layer continuation |
| `emath inspect <checkpoint> [--json]` | Read a saved constructor checkpoint |
| `emath verify <checkpoint> [--json]` | Replay recorded observations; not a theorem |
| `emath test <file>` | Authored `tests:` |
| `emath build <file> [--out <dir>]` | Emit fully lowered runnable Rust |
| `emath api [--search TEXT] [--json]` | Constructor contracts and imported exports |

`--json` on `run` is `emath.constructor.v1`: `execution`, `fulfillment`,
`representation`, `payload`, `evidence`, `remaining`. Historical
`goal_met` is not a constructor field.

See [`../language/CAPABILITY.md`](../language/CAPABILITY.md) and
[`../crates/emath-cli/CONTRACT.md`](../crates/emath-cli/CONTRACT.md).
