# emath User Manual

emath is a language for mathematics that runs. You write `emath object`,
`emath function`, or `emath query`. The toolchain returns a value, a
receipt, generated Rust, or a named refusal.

This page is the operator manual. The language constitution lives in
[`language/`](language/README.md). Status of what computes:
[`language/CAPABILITY.md`](language/CAPABILITY.md).

## What you write

```emath
emath function AddExact:
    inputs:
        a: Int
        b: Int
    outputs:
        result: Int
    definitions:
        result = a + b
    tests:
        example <two_plus_one>:
            given a = 2
            given b = 1
            expect result == 3
```

Declaration kinds are only those three. `emath model`, `emath policy`,
and `emath kind` refuse. `goals:`, `equations:`, `state:`, and
`compile:` are not constructor sections.

The five constructor families are `object`, `function`, `recur`,
`quote`, and `query`. Named recipes (`sum`, `derivative`, `solve`,
`sin`, `norm`) are ordinary modules behind `use`, or local code.

## Commands

```bash
emath check language/examples/intro/add-exact.emath
emath run language/examples/intro/add-exact.emath
emath run language/examples/intro/add-exact.emath --set a=2 --set b=1 --json
```

| Command | Meaning |
|---------|---------|
| `emath check FILE` | Parse and admit |
| `emath run FILE` | Evaluate a function or query |
| `emath loop FILE` | Drive a research-loop session surface (step, run, show, grow-case, save, load, export-native) |
| `emath step CHECKPOINT` | Resume a constructor continuation |
| `emath inspect CHECKPOINT` | Read saved question and artifacts |
| `emath verify CHECKPOINT` | Replay recorded observations |
| `emath test FILE` | Authored `tests:` |
| `emath build FILE` | Emit runnable Rust |
| `emath api --search TEXT --json` | Constructor contracts and imported exports |

`emath simulate`, `plan`, `eval`, `sweep`, `genesis`, and `solve`
refuse `E-KIND-GONE`. Write an ordinary function and `emath run`.

Exit codes: 0 completed; 2 admission failure; 3 unmet or partial; 4
execution fault; 5 incompatible checkpoint. A suspended run is 3.

`--json` prints `execution`, `fulfillment`, `representation`,
`payload`, `evidence`, and `remaining`. Historical `goal_met` labels
are not the constructor receipt.

## Types

`Bool`, `Int`, `Rat`, `Float64`, `sequence(T)`, `T[n]`, records and
variants from `emath object`, `A -> B`, and `Code[T]`. There is no
`Real` carrier. `/` on integers yields `Rat`.

## Modules

Optional algorithms live under [`language/modules/`](language/modules/README.md).
They are not auto-imported. Example: `use fold.reduce`.

## Teaching programs

Start at [`language/examples/README.md`](language/examples/README.md)
and [`language/QUICKSTART.md`](language/QUICKSTART.md).

## What this manual is not

It is not a workbench, desugarer, or alias catalog. Unicode identifiers
may name variables. Keyword spelling is the specified ASCII. There is
no runtime alias registry.
