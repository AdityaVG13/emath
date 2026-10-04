# The emath guide

Start here. This guide shows how to use emath from beginning to end:
writing a program, running and testing it, using it on a math problem,
and certifying the result in Lean. It is written so that a person or
an agent given only this page can find everything else.

## What emath is

emath is a language for mathematics that runs. You write definitions
the way you would on paper: functions, objects, sums, loops, exact
rationals. The `emath` command then does one of these:

- computes them exactly (`run`, `test`, `compare`);
- compiles them to a Rust crate (`build`);
- searches for new results (`dream`, `keep`);
- proves claims about them, in emath (`search.certify`) and in the Lean
  kernel (`export lean`).

Every answer is labeled. When emath cannot compute something, it says
why instead of guessing.

## Setup

1. Install emath: build the repository with `cargo build -p emath-cli`
   and put `target/debug/emath` on your PATH.
2. emath needs the `language/` folder, which holds the module library.
   Inside the emath repository it is found automatically. Elsewhere,
   set:

   ```console
   $ export EMATH_ROOT=/path/to/emath
   ```

3. For Lean certification only: install Lean with
   [elan](https://github.com/leanprover/elan). See [lean.md](lean.md#setup).

Check that it works:

```console
$ emath test docs/guide/examples/problem/weighted.emath
ok none --
ok four --
2 authored tests passed
```

## The flow

```
  write  ──►  check  ──►  test  ──►  run / compare  ──►  certify  ──►  export lean --check
 (.emath)    (admits?)   (examples)  (compute, explore)  (proof in     (proof in the
                                           │               emath)       Lean kernel)
                                           ▼
                                   dream / keep (search for new
                                   definitions, keep them in a library)
```

1. **Write** a `.emath` file with `emath function` definitions and
   `tests:` examples.
2. **Check** it: `emath check FILE`. This parses and type-checks the
   file without running it. It prints nothing when the file is fine.
3. **Test** it: `emath test FILE`. This runs every example.
4. **Run** it: `emath run FILE --function F --set x=1`. Use
   `emath compare` to see how the answer changes under floats,
   intervals, and other number systems.
5. **Certify** laws ("these two computations agree for every n") with
   `certify_equal` from `search.certify`.
6. **Export to Lean**: `emath export lean DIR --check`. The Lean kernel
   proves the examples and laws again, independently of emath.

## I want to…

| Goal | Read |
| --- | --- |
| Solve a math problem: find a formula and prove it | [solving-a-problem.md](solving-a-problem.md) |
| Prove my examples and laws in Lean | [lean.md](lean.md) |
| Learn the syntax of an emath program | [Quick start](../../language/QUICKSTART.md), then [the language README](../../language/README.md) |
| Look up any command and its flags | [MANUAL.md](../../MANUAL.md), or `emath help COMMAND` |
| Find a library function | `emath api --search "words"` |
| Understand an error | The message says what to change. For command-line codes, `emath explain E-CODE`; language codes are described in [the reference](../../language/reference/README.md) |
| Run a simulation | The template [`language/templates/simulation.emath`](../../language/templates/simulation.emath) |
| Run a discovery loop (`dream`) | [Discovery](../../language/DISCOVERY.md) |
| Know what computes and what is refused | [CAPABILITY.md](../../language/CAPABILITY.md) |

Pages on writing programs, running and testing, dream, and simulations
are coming to this guide. Until then, the links above cover them.

## A program in one screen

```emath
use fold.reduce

exports: cubes

emath function cubes:
    # 0^3 + 1^3 + ... + (n - 1)^3
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = sum(tabulate(function i in Int: i * i * i, n))
    tests:
        example <five>:
            given n = 5
            expect result == 100
```

- `use` imports a module from the library (`language/modules/`) or a
  file next to this one.
- `exports:` lists the names other files may `use`.
- `inputs:` and `outputs:` are typed. Common types are `Int`, `Rat`
  (exact rationals), `Bool`, `sequence(T)`, and objects you define.
- `definitions:` gives each output a value. Helper names may be defined
  too.
- `function i in Int: ...` is an anonymous function.
- `tests:` holds examples. `emath test` runs them, and Lean proves them.

## Exit codes

Every command uses the same codes, so scripts and agents can branch on
them.

| Code | Meaning |
| --- | --- |
| 0 | Done |
| 1 | A checker rejected a claim (`export lean --check` found a false example or law) |
| 2 | The file does not pass `emath check` (syntax, type, or input error) |
| 3 | Partial: something was refused, not translated, not proven, or a tool is missing |
| 4 | A fault while running, or a file could not be written |
| 5 | An incompatible or corrupt checkpoint |

## Tips for agents

- Run `emath check` after every edit. It is fast and names the line.
- Always pass `--function F` to `emath run`.
- Give inputs with `--set name=value`. Rationals are written `1/3`,
  lists `'[1, 2, 3]'`.
- If something stops with `budget_exhausted`, it needs more work, not a
  fix. Add `--work N`.
- Prefer exact types (`Int`, `Rat`) for anything you want to prove.
  Floats do not export to Lean.
- `proven=false` is not "false". Read `points`: 0 means emath could
  not bound a side, more than 0 means a checked point disagreed or ran
  out of `limit`.
- Run `emath help COMMAND` for the flags of any command.
  `emath capabilities --json` gives the full machine-readable contract.
