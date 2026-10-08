# emath manual

This manual covers the `emath` command-line tool: how to run, test,
compile, compare, and explore `.emath` programs. For the language itself,
start at [`language/README.md`](language/README.md). For what computes
today, see [`language/CAPABILITY.md`](language/CAPABILITY.md).

For the end-to-end flow (write, run, solve a problem, certify it in
Lean), start at the [guide](docs/guide/README.md).

`emath help` lists every command. `emath help COMMAND` prints one
command's flags and examples. `emath capabilities --json` is the
machine-readable contract.

## Contents

- [Locating the language](#locating-the-language)
- [Everyday commands](#everyday-commands)
- [Comparing carrier worlds](#comparing-carrier-worlds)
- [Discovery](#discovery)
- [Saved runs](#saved-runs)
- [Project and tooling commands](#project-and-tooling-commands)
- [Exit codes](#exit-codes)
- [JSON receipts](#json-receipts)
- [Environment variables](#environment-variables)

## Locating the language

Every command needs the `language/` folder, which holds the Language
Image and the module library. The CLI looks for it by walking up from
the source file, then from the working directory. Outside the repository,
set `EMATH_ROOT` to a directory that contains `language/spec`. A set but
wrong `EMATH_ROOT` refuses with `E-LANG-IMAGE`; there is no silent
fallback.

## Everyday commands

| Command | Purpose |
| --- | --- |
| `emath check FILE` | Parse and admit. Nothing runs. `-` reads from standard input |
| `emath run FILE --function F` | Evaluate one function or query on the reference VM |
| `emath test FILE` | Run the file's authored `tests:` |
| `emath build FILE` | Emit a self-contained, runnable Rust crate |
| `emath fmt FILE` | Check canonical formatting |
| `emath api --search TEXT` | Find constructor contracts and module exports |

```console
$ emath run language/examples/intro/add-exact.emath --function AddExact --set a=2 --set b=1
3
```

### Inputs

`--set name=value` supplies one input. Values may be integers, exact
rationals (`1/3`), decimals, Booleans, or lists (`--set 'xs=[1, 2, 3]'`).
`--set-file path.json` supplies a JSON object of inputs; `--set` wins for
a name given in both. A decimal is exact under a `Rat` input; a
`Float64` input needs a floating literal such as `3.0`.

### Work budgets

The reference VM counts work in deterministic engine steps, not time.
`--work N` sets the budget for `run`, `test`, `compare`, `keep`, and
`step`. `emath test` gives each example its own budget, so a large case
that stops with `budget_exhausted` needs a larger `--work`.

### Building Rust

`emath build FILE --out DIR` writes a Cargo crate with one typed entry
per runnable function. The crate embeds its runtime, has no
dependencies, and builds offline. (A function that uses `quote.cost`,
`quote.world`, or `quote.worlds` instead runs in the reference engine,
which the crate vendors with a pinned `Cargo.lock`.)
`--dry-run` reports the plan without writing. Native code enforces your
authored refusals but not the VM's work budget.

## Comparing carrier worlds

`emath compare FILE --function F` evaluates one function under every
carrier world and prints each answer, the exact gap from the first
answer, and a verdict (`agree`, `disagree`, or `partial`).

| World | How it reads real data |
| --- | --- |
| `reference` | As written |
| `exact` | Every real as an exact rational |
| `float64` | Data as binary64; the program's own constants stay exact |
| `float32`, `bfloat16` | The same with narrower floats |
| `int64` | Integers past signed 64 bits refuse `machine_overflow` |
| `interval` | Outward-rounded enclosures that contain the exact answer |
| `affine` | Affine forms, so a reused variable cancels (`x - x` is `0`) |

- `--world NAME` restricts the run to chosen worlds (repeatable).
- `--grid name=v1,v2,...`, `name=LO..HI`, or `name=LO..HI/N` sweeps an
  input and maps where the worlds diverge.
- Each answer carries an `unchecked` claim with a reproducible id.
  emath never picks a winner.

Details: [carrier worlds and claims](language/reference/diagnostics-and-tooling-contract.md#carrier-worlds-and-claims).

## Discovery

| Command | Purpose |
| --- | --- |
| `emath dream FILE --target StepFn` | Launch a background discovery run under `.emath/runs/NAME/`; attaching opens the live dashboard (plain-text status in a `--no-default-features` build) |
| `emath dream watch\|resume\|fork\|export RUN`, `emath dream list` | Manage runs. Detaching never stops a run |
| `emath keep FILE --function F --to LIB` | Evaluate a function that returns a closed quoted program and write it into a library as an ordinary `emath function` |
| `emath export lean FILE\|DIR` | Write a Lean 4 project of the functions and their examples for review; add `--check` to have the Lean kernel prove every example and every `certify_equal` and `certify_all` law (laws use a shared prebuilt Mathlib); `--work N` raises the emath budget each example runs under, as in `emath test` |
| `emath loop FILE` | Drive a research loop interactively (`step`, `run`, `show`, `grow-case`, `save`, `load`) or replay a script with `--script` |
| `emath experiment MANIFEST` | Measure a baseline and a candidate on one workload and return an authored evaluator's decision |

`--worlds w1,w2` on `dream`, `run`, and `build` sets the carrier worlds
that `quote.worlds()` returns. `keep` refuses to overwrite different
existing text unless you pass `--overwrite`. See
[`language/DISCOVERY.md`](language/DISCOVERY.md).

## Saved runs

`emath run --function F --work N` writes `constructor-checkpoint.json`
under `--out DIR` (default `target/emath/runs`) when the budget stops
the run.

| Command | Purpose |
| --- | --- |
| `emath inspect CHECKPOINT` | Read the saved function, source id, and work without executing |
| `emath step CHECKPOINT --work N` | Resume the saved continuation |

An incompatible checkpoint refuses to resume (exit 5). See
[budgets and checkpoints](language/reference/diagnostics-and-tooling-contract.md#budgets-and-checkpoints).

## Project and tooling commands

| Command | Purpose |
| --- | --- |
| `emath new NAME` | Scaffold a project (`emath-package.toml`, `src/main.emath`) |
| `emath migrate FILE` | Respell source into canonical form (`--check`, `--dry-run`, `--fix`) |
| `emath diff A B` | Compare two sources by content id |
| `emath explain CODE` | Look up a CLI diagnostic code (`--list-codes` lists them) |
| `emath doctor` | Check for `rustc`, `cargo`, `rustfmt`, and `clippy` |
| `emath catalog`, `emath capabilities` | Export the command matrix and the machine contract |
| `emath triage`, `emath next` | Orient in a file and suggest next actions |
| `emath version` | Print the CLI version |

`emath simulate`, `plan`, `eval`, `solve`, and `web` are not commands of
the language; they refuse with `E-KIND-GONE`. Write an ordinary function
and use `emath run`.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | The command completed |
| `1` | A test or checker rejected a claim (`emath test` had a failing example, `export lean --check` found a false example or law, `emath fmt` found non-canonical layout), or a command refused a well-formed request, such as resuming a saved run whose source changed |
| `2` | Admission failure: syntax, type, or input. Every command that reads a program (`check`, `test`, `run`, `compare`, `keep`, `build`, `fmt`) answers a broken one with exit 2 and `file:line:column` |
| `3` | Unmet, partial, or suspended answer, including `budget_exhausted` |
| `4` | Execution fault (including a named refusal such as `constructor_refuse`), or a file could not be read or written |
| `5` | Incompatible or corrupt checkpoint |

## JSON receipts

`emath run --json` prints a receipt with independent fields:

```json
{
  "schema_version": "emath.constructor.v1",
  "command": "run",
  "admission": "ok",
  "execution": "returned",
  "fulfillment": "satisfied",
  "representation": "exact_scalar",
  "payload": "3",
  "evidence": [],
  "remaining": []
}
```

`execution` says whether evaluation returned, faulted, or suspended.
`fulfillment` says whether the question was answered. `representation`
says what kind of value came back. `evidence` and `remaining` list what
supports the answer and what is still open.

## Environment variables

| Variable | Effect |
| --- | --- |
| `EMATH_ROOT` | Directory containing `language/spec`, for use outside the repository |
| `EMATH_LOG` | Diagnostic log level (`error`, `warn`, `info`, `debug`) |
| `NO_COLOR` | Disable ANSI colors |
| `TERM=dumb` | Disable styling and cursor movement |
| `CI` | Suppress interactive prompts |
| `SOURCE_DATE_EPOCH` | Fixed timestamp for generated artifacts |
