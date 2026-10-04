# Searching for definitions with dream

`emath dream` runs a search you write in `.emath`, saves its progress,
and keeps useful programs in a library you own. It does not contain an
AI model or invent a search strategy for you. Your program decides what
to try, what counts as a good result, and when to stop.

This page searches for a formula for the triangular numbers:
0, 1, 3, 6, 10, 15. The runnable file is
[`docs/guide/examples/dream.emath`](examples/dream.emath).
Commands below run from the emath repo root. See [setup](README.md#setup)
if emath is not on your PATH.

## 1. Define a small search

Try formulas of the form

$$f_s(n) = \frac{n(n+s)}{2}, \qquad s = 0, 1, 2, 3.$$

The first candidate, $s=0$, fails at $n=1$. The second, $s=1$,
matches all six samples. This is a deliberately small search so you
can follow every step. A larger search can propose different program
shapes instead of changing one constant.

A dream has two functions with matching names:

- `SeedFormula(unused)` builds the initial state.
- `StepFormula(state, budget)` tries one batch and returns the next state.
  The command selects it with `--target StepFormula`.

The state is an ordinary object. Only `batch: Int` and `verdict: Int`
are required by the host:

```emath
emath object FormulaState:
    representation:
        batch: Int
        verdict: Int
        used: Int
        next: Int
        program: Code
```

Here `next` is the shift to try, `used` counts tried candidates, and
`program` holds the last candidate. Each Step must advance `batch` by
exactly one. Verdicts tell the host what happened:

| Verdict | Meaning |
| --- | --- |
| 0 | Keep running |
| 1 | The authored goal was attained |
| 2 | The search domain was exhausted |
| 3 | The search reached a plateau |
| 4 | The authored budget was exhausted |

The important part of the Step is:

```emath
        template = quote(recur f in Int -> Rat: function n in Int: n * (n + shift) / 2)
        program = quote.substitute(template, shift, state.next)
        candidate = quote.evaluate(program)
        train = [0 / 1, 1 / 1, 3 / 1, 6 / 1]
        held = [10 / 1, 15 / 1]
        matches = all(function i in Int: candidate(i) == train[i], range(0, 4)) and all(function i in Int: candidate(i + 4) == held[i], range(0, 2))
        verdict = if matches: 1 else: if state.next >= 3: 2 else: 0
```

`quote` makes a program into data. Inside the quote, `shift` is an
unbound name, not automatically captured from the surrounding function.
`quote.substitute` replaces it with the current integer. The resulting
program is closed, meaning it has no unbound inputs apart from its
declared argument `n`. `quote.evaluate` turns it into a callable function.

The `recur f in Int -> Rat` wrapper gives the kept definition its input
and output types. This formula never calls `f`. Currently, keeping a
program requires this wrapper even when the body is not recursive.

The first four samples are training cases; the next two are held-out
checks. Here the family and sample split are fixed in advance. If you
change the search after seeing the held-out answers, choose new held-out
cases before calling the result independently checked.

## 2. Check the loop before launching it

```console
$ emath check docs/guide/examples/dream.emath
$ emath test docs/guide/examples/dream.emath
ok first_candidate_rejected --
ok second_candidate_matches --
ok no_budget_no_candidate --
3 authored tests passed
```

These examples check rejection, success, and a zero-budget stop.
They run the Step directly, without starting a background process.

## 3. Start a bounded run

Choose a fresh run root so this walkthrough does not touch existing runs:

```console
$ runs=$(mktemp -d)
$ emath dream docs/guide/examples/dream.emath --target StepFormula --run formula --root "$runs" --detach --budget 4 --max-batches 4 --checkpoint-every 1
formula
driver started · emath dream watch formula
```

The launch command returns immediately. Exit 0 here means the driver
started, not that the search succeeded. Give it a moment, then read
the saved status:

```console
$ emath dream list --root "$runs"
formula  StepFormula  goal_attained  b2  -
```

The run closed after two batches. The final `-` means this simple loop
does not provide an incumbent score; it is not an error.

Without `--root`, runs live in `.emath/runs/` under your working dir.
Use a new `--run` name for a new run. An existing name is not replaced
by default.

There are separate limits:

- **`--budget 4`:** the authored Step's logical-unit watermark. This
  example charges one unit per candidate, not per VM instruction.
- **`--budget-step N`:** how far the host raises that watermark when
  the loop needs another allocation. `--max-units N` caps it.
- **`--work N`:** engine work allowed per evaluation. Raise it if one
  Step is too expensive for the default cap.
- **`--max-batches 4`:** a host ceiling on total batches. This keeps a
  mistaken stopping rule from running forever.

The host does not impose mathematical meaning on `used`. Your Step must
charge honestly and enforce its budget.

## 4. Watch and inspect

```console
$ emath dream watch formula --root "$runs"
```

In a terminal, this opens the dashboard. Press `q` to detach. Detaching
does not pause or kill the driver. Without a terminal, `watch` prints
a snapshot. After this example finishes:

```text
◆ GOAL
L1 · batch 2
incumbent -
budget 2 / 4 u
b2 ■ level closed · goal_attained · no emission
b2 ■ RUN ENDED · ◆ GOAL · no emission seam
```

“No emission seam” means the loop did not supply the optional
world-emission hooks. It still kept its formula.

Files under `$runs/formula/` include:

| File | What to read it for |
| --- | --- |
| `run.json` | Config and final outcome, including a fault's explanation |
| `journal.jsonl` | Ordered events: batches, kept definitions, and run end |
| `metrics.jsonl` | Measurements, separate from the search result |
| `checkpoints/` | Saved states for resume and fork |
| `world/dream.emath` | The run's copy of your source |
| `world/kept.emath` | Definitions the loop kept |

Export a table without scraping the dashboard:

```console
$ emath dream export formula --root "$runs" --format csv
batch,verdict,verdict_name,used,delta_used,watermark,plateau_streak,incumbent_key,incumbent_score,promoted,quarantined,case_count,archive_len
1,0,running,1,1,4,0,,,,,0,0
2,1,goal_attained,2,1,4,0,,,,,0,0
```

Add `-o batches.csv` to write a file, or use `--format jsonl`.
`--table records` exports archive records when your loop has an archive.
This loop keeps its program in state, so those archive fields are empty.

## 5. Use the kept definition

The optional `dream_keep(state)` function returns named programs:

```emath
emath function dream_keep:
    # The driver writes this closed program into world/kept.emath.
    inputs:
        state: FormulaState
    outputs:
        result: sequence(KeptFormula)
    definitions:
        result = if state.verdict == 1: [KeptFormula: {name: quote(triangular), program: state.program}] else: []
```

After a successful batch, the driver writes an ordinary exported
`emath function triangular` into `world/kept.emath`. Run it directly:

```console
$ emath run "$runs/formula/world/kept.emath" --function triangular --set n=10
55/1
```

Or save this as `$runs/use_kept.emath`:

```emath
use formula.world.kept

emath function ten:
    inputs:
        unused: Int
    outputs:
        result: Rat
    definitions:
        result = triangular(10)
```

```console
$ emath run "$runs/use_kept.emath" --function ten --set unused=0
55/1
```

No compiler or shipped library edits are involved. Copy kept definitions
into a project you own when you want to use them independently of a run.
`emath keep` is the separate command for saving a returned quoted program;
see `emath help keep`.

## 6. Resume or fork

To see a resume, start a second run capped at one batch:

```console
$ emath dream docs/guide/examples/dream.emath --target StepFormula --run paused --root "$runs" --detach --budget 4 --max-batches 1 --checkpoint-every 1
paused
driver started · emath dream watch paused
```

After the driver closes, `dream list` reports
`paused  StepFormula  batch_cap  b1  -`. Raise the total batch ceiling:

```console
$ emath dream resume paused --root "$runs" --detach --max-batches 4
paused
driver started · emath dream watch paused
```

It continues from the saved state and reaches `goal_attained` at batch 2.
For an engine-work stop, use `resume ... --work N` instead.

A fork creates another run from a saved checkpoint:

```console
$ emath dream fork formula --at 1 --run alternative --root "$runs" --detach --max-batches 4
alternative
driver started · emath dream watch alternative
```

The original run remains intact. This example checkpoints every batch,
so batch 1 is available. `--at` selects a saved checkpoint, not an
arbitrary point in the source program.

## What the result means

`goal_attained` means **your authored goal passed**. Here it means six
sample values match. It is not a theorem about every integer and not
evidence that the formula is new to mathematics.

To prove a formula for all nonnegative inputs, state a separate law with
`search.certify`, then use [Lean export](lean.md). See
[solving a math problem](solving-a-problem.md) for that workflow.

For richer searches, look up `search.synthesis`, `search.learning`,
`search.research`, and `search.evolve` with `emath api --search`.
The installed `language/DISCOVERY.md` also covers external evidence,
multi-level loops, and pure `fact_` result caching. All are optional
authored modules, not hidden domain logic in the dream driver.
