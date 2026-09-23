# Hot-path experiment

A measured baseline-versus-candidate experiment. The host builds both
crates, runs them on the same workload, records timings and outputs, and
passes them to the authored evaluator `HotpathEvaluation` in
[`rust_hotpath.emath`](../../language/templates/research_targets/rust_hotpath.emath).
The evaluator decides; the host only reports what it observed.

| Path | Role |
|---|---|
| `baseline/` | Counts primes by trial division |
| `candidate/` | Counts primes with one sieve |
| `wrong_candidate/` | Faster sieve that also counts 1 (wrong output) |
| `hang_candidate/` | Never answers (for the timeout and cancellation paths) |
| `workload.txt` | Audit workload, passed to each run on stdin |
| `experiment.json` | Manifest (`emath.experiment.v1`) |

## Run

From the repository root:

```console
emath experiment examples/hotpath-experiment/experiment.json
```

State goes to `target/emath-experiment/prime-count-sieve/`. The shared
audit ledger is `target/emath-experiment/audit-ledger.json`. Running the
same command again replays the committed decision without new
measurements. Use `--stop-after-pairs N` to stop early; rerun with the
same `--state` to resume. After Ctrl-C, the pair that was running is
recorded as `uncertain`, not counted, and run again.

The ledger records each consumed audit workload. If you delete the state
directory and run again with the same ledger, the evaluator returns
status -1: that workload has already been used as an audit. Supply a new
workload to get a fresh audit.

## Your own experiment

Each arm is a Cargo crate with a `Cargo.lock`, built with
`cargo build --release --locked --offline`. It reads the workload on
stdin and prints whitespace-separated integers. Copy `experiment.json`
and set the two crates, the workload, the evaluator function, the pair
count, and the budget. Evaluator inputs must be host facts; see
`HOST_FACTS` in `crates/emath-tui/src/experiment/mod.rs`.

## What the numbers mean

Timings are host-clocked wall nanoseconds per process, including process
and sandbox start-up for both arms, with about 50 µs of polling
granularity. Compilation is timed separately and never enters samples.
Warmup runs are recorded, but they are not samples. Status 1 means that
every observed paired gain was positive on this machine and workload. It
is not a confidence interval, a significance test, or a claim about
other inputs.
