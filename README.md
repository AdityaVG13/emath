# emath

<div align="center">

[![Status](https://img.shields.io/badge/status-active%20development-2ea44f)](#status-and-roadmap)
[![Rust](https://img.shields.io/badge/rust-nightly%202026--08--27-b7410e)](rust-toolchain.toml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
[![Language](https://img.shields.io/badge/docs-language-0969da)](language/README.md)
[![Manual](https://img.shields.io/badge/docs-manual-0969da)](MANUAL.md)

</div>

> **Write known math. Invent new math. Run both.**

emath is a language for mathematics that computes. You write definitions
the way you would on a whiteboard, and the toolchain gives back an exact
value, a trajectory, a self-contained Rust crate, or a named reason it
cannot compute yet. The compiler is written in Rust; the mathematics is
written in emath.

## Why emath

Mathematical work usually lives in one tool (a computer algebra system, a
notebook, a prover, a hand-written solver) while the runnable program
lives in another. emath puts both in one language:

- **Exact by default.** `Int` is unbounded and `Rat` is an exact rational.
  `1/10 + 2/10 == 3/10` holds. Floating point is a carrier you choose.
- **Every method is a definition.** Sums, derivatives, linear solvers,
  Runge-Kutta steppers, and eigenvalue routines are ordinary `.emath`
  modules you can read, import, and replace. The compiler has no
  hidden math.
- **One question, every carrier.** `emath compare` runs a function with
  exact rationals, binary64, binary32, bfloat16, 64-bit integers,
  intervals, and affine forms, and shows where the answers part.
- **Code is a value.** `quote` makes programs inspectable and buildable
  at run time, so a discovery loop can propose, test, and keep new
  definitions in a library you own.
- **Honest answers.** Results carry a receipt (`execution`,
  `fulfillment`, `representation`, `evidence`, `remaining`). A missing
  bound is `partial`, and an unsupported method is a named refusal,
  never a silent guess.

## A taste

```emath
use fold.reduce

emath function SumOfSquares:
    inputs:
        n: Int
    outputs:
        total: Int
    definitions:
        total = sum(map(function k in Int: k * k, range(1, n)))
    tests:
        example <first_ten>:
            given n = 10
            expect total == 385
```

```console
$ emath test sum_of_squares.emath
ok first_ten
1 authored tests passed
$ emath run sum_of_squares.emath --function SumOfSquares --set n=100
338350
$ emath build sum_of_squares.emath --out squares/   # a standalone Rust crate
```

The same function under every carrier world:

```console
$ emath compare third.emath --function Third --set x=1/10
reference  0/1
exact      0/1
float64    0
float32    0.000000007450580596923828   (gap 7.450580596923828e-9)
bfloat16   0.00048828125   (gap 4.8828125e-4)
int64      0/1
interval   [-0.000000000000000027755575615628914, 0.000000000000000027755575615628914]   (width 5.551115123125783e-17)
affine     0/1
verdict: disagree
```

Here `Third` computes `x / 3 * 3 - x`, which is exactly zero.

## Getting started

emath builds from source with the pinned nightly toolchain in
[`rust-toolchain.toml`](rust-toolchain.toml); `rustup` picks it up
automatically. Stable Rust is not supported.

```console
$ git clone https://github.com/AdityaVG13/emath.git
$ cd emath
$ cargo build -p emath-cli
$ ./target/debug/emath check language/examples/intro/add-exact.emath
```

The first debug build takes a few minutes. Put `target/debug` on your
`PATH` (or symlink `emath` into `~/.cargo/bin`) to use `emath` directly.
The CLI finds the `language/` folder by walking up from the source file
or the working directory. To run it from elsewhere, set `EMATH_ROOT` to a
directory that contains `language/spec`.

Start a project with `emath new NAME`, then follow the
[quick start](language/QUICKSTART.md).

## Commands

| Command | Purpose |
| --- | --- |
| `emath check FILE` | Parse and admit a file |
| `emath run FILE --function F` | Evaluate a function or query and print its value or receipt |
| `emath test FILE` | Run the authored `tests:` |
| `emath build FILE` | Emit a self-contained, runnable Rust crate |
| `emath compare FILE --function F` | Evaluate under every carrier world and report the gaps |
| `emath dream FILE` | Launch a background discovery run with a live dashboard |
| `emath keep FILE --function F --to LIB` | Write a discovered program into your library |
| `emath step`, `inspect`, `verify` | Resume, read, or replay a saved run |
| `emath api --search TEXT` | Find callable names, import paths, and signatures |
| `emath new NAME` | Scaffold a project |

`emath help` lists every command and `emath help COMMAND` describes one.
The [manual](MANUAL.md) covers flags, exit codes, and JSON output.

## How it works

```text
.emath source
  -> parse
  -> admission (object / function / query)
  -> typed IR
  -> reference VM (run, test, compare, dream)  or  Rust emission (build)
  -> value or receipt
```

The Rust side is a small universal core: parsing, admission, a typed IR,
the reference VM, and code generation. Mathematics enters only as
authored `.emath` modules under [`language/modules/`](language/modules/README.md)
and worked objects under [`language/domains/`](language/domains/README.md).
When a method cannot be expressed, the core gains a general operation,
never a domain-specific one.

## Status and roadmap

emath is in active development and is not yet published to crates.io.
The repository is the deliverable today. Work proceeds in a fixed order:

- [ ] **1. Language and mathematical engine** (in progress): syntax,
  admission, the reference VM, Rust emission, and the module library
  across algebra, analysis, numerics, probability, optimization, and
  discovery.
- [ ] **2. WebAssembly and the browser workbench** (next): every Stage 1
  capability running in the browser through `emath-wasm`.
- [ ] **3. Ecosystem** (later): incremental compilation, an editor
  language server, external solver and proof-checker bridges, and a
  package registry.

### Boundaries

- **Compiling is not proving.** emath guarantees that the program is what
  you wrote, not that the mathematics is true. Finite checks are finite.
  An independent proof checker is planned as a separate step. See
  [Why not just Lean?](WHY_NOT_LEAN.md)
- Native Rust crates enforce authored refusals but not the reference
  VM's work budget.
- External engines (symbolic, tensor, solver, and proof backends) are
  planned behind adapters. None is wired in yet.

## Web workbench

An early in-browser workbench runs the compiler as WebAssembly, with no
server round trips:

```console
$ cargo xtask build-web
$ cargo xtask serve-web        # http://localhost:8080
```

See [`web/README.md`](web/README.md). The workbench is a Stage 2
surface and does not yet cover the full language.

## Documentation

| Document | What it covers |
| --- | --- |
| [Language home](language/README.md) | What emath is and where everything lives |
| [Quick start](language/QUICKSTART.md) | Your first program in a few minutes |
| [Capability](language/CAPABILITY.md) | What computes today and what is refused |
| [Reference](language/reference/README.md) | The normative specification |
| [Discovery](language/DISCOVERY.md) | Discovery loops, worlds, and research modules |
| [Manual](MANUAL.md) | The command-line tool in detail |
| [Constitution](implementation/CONSTITUTION.md) | Architectural principles |
| [Crate map](implementation/CRATE_MAP.md) | Workspace layout and crate roles |
| [Tests](tests/README.md) | Test layout and intent |
| [Why not just Lean?](WHY_NOT_LEAN.md) | How emath relates to proof assistants |
| [Security](SECURITY.md) | Security model and reporting |

## Contributing

Contributions are welcome. Please reach out to the author
([Aditya](https://x.com/adityavg13)) before you start, so we can agree
on the direction. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

emath is licensed under Apache-2.0 ([`LICENSE`](LICENSE)). Third-party
licenses are tracked separately under [`licenses/`](licenses/).
