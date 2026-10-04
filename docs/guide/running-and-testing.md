# Running and testing

This page covers the commands you use on a program while you write it:
`check`, `test`, `run`, `compare`, and `build`. The examples use the
programs in [`docs/guide/examples/programs/`](examples/programs/). To
write programs, see [writing-programs.md](writing-programs.md). To
prove results in Lean, see [lean.md](lean.md).

| Command | Use it to |
| --- | --- |
| `emath check FILE` | Find syntax and type errors without running anything |
| `emath test FILE` | Run every example in the file's `tests:` |
| `emath run FILE --function F --set x=1` | Compute one function on inputs you choose |
| `emath compare FILE --function F --set x=1` | See how the answer changes in floats, intervals, and other number systems |
| `emath build FILE` | Turn the file into a Rust crate |
| `emath export lean DIR --check` | Prove the examples and laws in Lean ([lean.md](lean.md)) |

Every command prints its flags with `emath help COMMAND`.

## check

```console
$ emath check docs/guide/examples/programs/basics.emath
$ echo $?
0
```

No output means the file is fine. A problem prints the file, line, and
column, the code, and the line with a marker under the spot, and
exits 2:

```console
$ emath check bad.emath
bad.emath:7:18: E-TYPE-012: in `bad`: definition `result` does not have the declared output type: declared Int, got Rat
          result = n / 2
                   ^~~~~
```

`emath explain E-TYPE-012` describes the code. `test`, `run`,
`compare`, `keep`, `build`, and `fmt` check the file the same way
before they do anything else, so a broken file gets the same message
and exit 2 from all of them. With `--json`, the result is a document
with an `admitted` field and a `diagnostics` list. Each diagnostic has
`line` and `column` when it points at a spot in the file:

```json
{
  "schema_version": "emath.constructor.v1",
  "command": "check",
  "status": "refused",
  "admission": "refused",
  "admitted": false,
  "diagnostics": [
{
  "code": "E-TYPE-012",
  "line": 7,
  "column": 18,
  "severity": "error",
  "message": "in `bad`: definition `result` does not have the declared output type: declared Int, got Rat"
}
  ],
  ...
}
```

`emath check -` reads the program from standard input.

## test

```console
$ emath test docs/guide/examples/programs/basics.emath
ok twelve_eighteen --
ok seventeen_by_five --
ok three --
ok inside --
ok above --
ok year --
ok four --
ok thousand --
8 authored tests passed
```

Each line is one `example`. A failing example says which, and the
command exits 1:

```console
$ emath test wrong.emath
...
failed four -- expect produced false
error: 1 authored test(s) failed
```

A test that demands a refusal says so:
`ok empty -- demanded fault `empty_list``.

Tests run under a work budget. An example that needs more work fails
with `budget_exhausted`; give it more with `--work N`, for example
`--work 100000000`.

## run

`emath run` computes one function. Always name it with `--function`,
and give every input with `--set`:

```console
$ emath run docs/guide/examples/programs/basics.emath --function harmonic --set n=4
25/12
$ emath run docs/guide/examples/programs/basics.emath --function divide --set a=17 --set b=5
divide(q=3, r=2)
```

A function with several outputs prints them as a record.

**Writing inputs.**

| Type | `--set` form |
| --- | --- |
| `Int` | `n=4`, `n=-7` |
| `Rat` | `x=3/2` |
| `Bool` | `flag=true` |
| `Float64` | `x=0.25` |
| `sequence(...)` | `'xs=[12, 18, 30]'` (quote it for the shell) |
| An object | Not possible. Pass objects in a test, or write a function that builds them from numbers |

For many inputs, put them in a JSON file and pass `--set-file`.
`--set` flags override the file:

```console
$ cat xs.json
{"xs": [12, 18, 30]}
$ emath run docs/guide/examples/programs/lists.emath --function gcd_all --set-file xs.json
6
```

**JSON receipts.** With `--json`, `run` prints a receipt. `payload` is
the answer. `fulfillment` is `satisfied` when the answer is complete:

```console
$ emath run docs/guide/examples/programs/basics.emath --function gcd --set a=12 --set b=18 --json
{
  "schema_version": "emath.constructor.v1",
  "command": "run",
  "admission": "ok",
  "execution": "returned",
  "fulfillment": "satisfied",
  "representation": "exact_scalar",
  "payload": "6",
  "evidence": [],
  "remaining": []
}
```

A refusal exits 4 and names the reason. With `--json`, `fulfillment` is
`unmet` and `diagnostics` holds the code:

```console
$ emath run docs/guide/examples/refusal.emath --function safe_mean --set 'xs=[]'
error: empty_list: mathematical method refused: empty_list
```

**Work budgets.** Every run has a work budget, so a runaway recursion
stops instead of hanging. Running out exits 3 with `budget_exhausted`.
Raise the budget with `--work N`. With `--work`, a run that runs out is
suspended: it writes a checkpoint you can continue with `emath step`:

```console
$ emath run docs/guide/examples/programs/basics.emath --function harmonic --set n=200 --work 100 --out ck
suspended: work=101 (checkpoint written; resume with `emath step <checkpoint> --work N`)
$ emath step ck/constructor-checkpoint.json --work 1000000
734304501393663047454128920370690990011701612756404750324309881998409657620477...
```

With `--work` and `--json`, the receipt also carries `work_consumed`,
the work the run actually used. Use it to choose a budget.

## compare

`emath compare` runs one function in several number systems
("worlds") and shows every answer and how far each is from the exact
one. It never picks a winner.

```console
$ emath compare docs/guide/examples/programs/basics.emath --function harmonic --set n=10
reference  7381/2520
exact      7381/2520
float64    2.9289682539682538   (gap -1.917337540939953e-16)
float32    2.9289684295654297   (gap 1.7559717571924604e-7)
bfloat16   2.9375   (gap 8.531746031746031e-3)
int64      7381/2520
interval   [2.9289682539682524, 2.928968253968255]   (width 2.6645352591003757e-15)
affine     7381/2520
verdict: disagree
```

| World | Numbers |
| --- | --- |
| `reference` | The program as written |
| `exact` | Exact integers and fractions |
| `float64`, `float32`, `bfloat16` | Machine floats of each size |
| `int64` | 64-bit integers; past that range it refuses `machine_overflow` |
| `interval` | An interval sure to contain the exact answer |
| `affine` | Affine arithmetic, a tighter kind of interval |

`verdict: disagree` means the worlds gave different answers, here
because floats round. Use `compare` to decide whether a fast float
version is close enough, or to find inputs where it is not.

`--grid` tries several inputs at once and reports the worst:

```console
$ emath compare docs/guide/examples/programs/basics.emath --function harmonic --grid n=1,5,20
grid: 3 points; agree 1, disagree 2, partial 0
disagree  n=5   max gap 1.3541666666666667e-2 (bfloat16)
disagree  n=20   max gap 1.163534285631809e-2 (bfloat16)
worst: n=5   gap 1.3541666666666667e-2 (bfloat16)
```

A grid may also be a range: `--grid n=1..100` tries every integer, and
`--grid x=0..1/4` splits 0 to 1 into four steps (five points).
`--world float64` shows one world only.

## build

`emath build` writes a Rust crate with one Rust function per emath
function. It has no dependencies: the runtime is included in
`src/lib.rs`.

```console
$ emath build docs/guide/examples/programs/basics.emath --dry-run
dry-run: emath build `docs/guide/examples/programs/basics.emath`
target directory: target/emath
crate: gcd (package fnv1a64:273f52f21c07186b)
$ emath build docs/guide/examples/programs/basics.emath --out basics-rs
constructor emission runnable → basics-rs (runnable)
```

The generated functions return `Result`, so a refusal in emath is an
`Err` in Rust:

```rust
pub fn gcd(a: i64, b: i64) -> Result<i64, String>
pub fn harmonic(n: i64) -> Result<emath_rt::ExactRatio, String>
```

`Int` becomes `i64` in Rust. Use `emath compare --world int64` to see
whether your inputs fit.

## A working loop

1. Write or edit the file.
2. `emath check FILE`. Fix what it names.
3. `emath test FILE`. Every example should print `ok`.
4. `emath run FILE --function F --set ...` on new inputs. Add any
   interesting one as an example.
5. `emath compare` if the program will run in floats or as Rust.
6. `emath export lean DIR --check` to prove the examples and laws.

For agents: branch on exit codes, not on text. 0 is done, 1 is a
failed test or claim, 2 is fix the file, 3 is partial (raise `--work`
or read what was left out), 4 is a fault or refusal. The full table is
in the [guide README](README.md#exit-codes).
