# CLI Reference (constructor layer)

Live commands are `check`, `run`, `step`, `inspect`, `test`,
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
| `emath run <file> [--function NAME] [--set name=value] [--work N] [--json]` | Evaluate a function or query; print a constructor receipt. Without `--function`, run the file's single entry, or refuse `E-RUN-USAGE` listing the entries |
| `emath verify <library> [--work N] [--json]` | Recheck every kept claim in a library through the frozen `check_claim` on its stored certificate; one row per claim, `--json` emits the same rows. A row reads settled only when the reference VM and the emitted Rust agree on the checker verdict; a checker the emitter cannot lower reports vm-only with the emit refusal named. `--work N` budgets each claim's producer evaluation and checker replay as `emath run --work` budgets an example; a claim that exhausts the budget refuses `E-VERIFY-WORK` naming the claim and the budget |
| `emath step <checkpoint> [--work N] [--json]` | Resume a constructor-layer continuation |
| `emath inspect <checkpoint> [--json]` | Read a saved constructor checkpoint |
| `emath test <file>` | Authored `tests:` |
| `emath build <file> [--out <dir>]` | Emit fully lowered runnable Rust |
| `emath api [--search TEXT] [--json]` | Constructor contracts and imported exports |
| `emath export lean <file\|dir> [--out <dir>] [--check] [--work N]` | Translate a project (or a kept library) into a Lean 4 project; `--check` proves the examples, laws, and settled witness claims by the kernel. Only settled claims export: a vm-only claim or another settled kind is refused by name, and a rejected certificate refuses the export, naming the claim |

`--json` on `run` is `emath.constructor.v1`: `execution`, `fulfillment`,
`representation`, `payload`, `evidence`, `remaining`. Historical
`goal_met` is not a constructor field.

See [`../language/CAPABILITY.md`](../language/CAPABILITY.md) and
[`../crates/emath-cli/CONTRACT.md`](../crates/emath-cli/CONTRACT.md).
