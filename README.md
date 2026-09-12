# emath

<div align="center">

[![Status](https://img.shields.io/badge/status-active%20Rust%20workspace-2ea44f)](#what-exists-now)
[![Rust](https://img.shields.io/badge/rust-nightly%202026--08--04-b7410e)](rust-toolchain.toml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
[![Manual](https://img.shields.io/badge/docs-manual-0969da)](MANUAL.md)
[![Constitution](https://img.shields.io/badge/docs-constitution-blue)](implementation/CONSTITUTION.md)

</div>

> **Write known math. Invent new math. Compile both.**

**emath** is a Rust-first language and compiler for mathematics that runs. You write mathematical intent (settled theorems, half-formed models, or structures invented today) and the toolchain lowers it into ordinary, inspectable Cargo artifacts.

A `.emath` source can carry objects, functions, queries, recursion, and quoted code; ordinary imported methods; scalar carriers; and host-observed receipts. Named recipes, `emath model` / `emath policy`, and `emath simulate` are not the language.

Intent is resolved through a deterministic pipeline (parse → constructor admission → typed IR → reference VM or emitted Rust), validated by `emath check` / `emath run`, and published as software you can link like any other crate. There is no `goals:` planner layer.

## TL;DR

**The problem:** mathematical intent usually lives in one tool (a CAS, a notebook, a prover, a hand-written solver) while the runnable artifact lives in another. Units, shapes, evidence, and host integration fall through the cracks, and "it compiled" is treated as "it is true."

**The solution:** emath is a single language surface for mathematics that computes. Declarations lower through a typed semantic IR, world interpretations, and verification gates into artifacts you can inspect and run. Interpretation is data (candidate *worlds*, kept as a portfolio). Nothing is refused at the door: unsupported work comes back labeled, with a route to the world that can do it.

### What exists now

| Area | Current implementation |
|------|------------------------|
| Language surface | `emath object`, `emath function`, `emath query`; `recur` and `quote` in expressions; scalar carriers; optional modules behind `use`. `model` / `policy` / `kind` are not core kinds |
| Pipeline | Parse → admit constructor kinds → typed IR → reference VM (`emath run`) or Rust emission (`emath build`) |
| Gates | `emath check`, `emath run`, `emath build` |
| Capstone demos | `cargo xtask demo all` (affine-scorer + semantic-genesis) |
| Web playground | `emath web` (in-page WASM compiler; Stage 1 subset today) |
| Providers | Std-only; in-tree Dew/Rumoca stand-ins; Wrenfold / Franken* planned behind adapters |
| Docs of record | [`MANUAL.md`](MANUAL.md), [`implementation/CONSTITUTION.md`](implementation/CONSTITUTION.md), [`examples/`](examples/README.md) |

### Honest boundaries

- **Compiling is not proving.** The pipeline guarantees the artifact matches what you asked for, never that the idea is true. Lean / FrankenLean is planned as hired evidence, not authority.
- Illustrative README sketches may parse and then return labeled partial results for unimplemented parts (a symbol, a bound, an open hole) with a route to what would compute them. That is expected.
- There is no crates.io product claim yet. The workspace is the deliverable today.
- Upstream engines are not absorbed into emath; adapters only, and not consumed yet.

## What emath is not

- It is not merely a computer algebra system.
- It is not merely Modelica rewritten in Rust.
- It is not a Rust macro that prints arithmetic expressions.
- It is not a theorem prover pretending every theorem is executable.
- It is not one giant vendored workspace of unrelated repositories.
- It is not an AI code generator whose output is accepted without deterministic checking.

## More than a compiler

Any finite mathematical structure that is structurally well-formed admits: textbook math, a jumbled formula, an idea for a problem nobody has posed yet. The same glyphs can carry many legitimate meanings, so emath represents interpretation as data: candidate *worlds*, chosen deterministically and kept as a portfolio rather than collapsed into an unlabeled guess. The validation suite runs one spec through three worlds today: `free_symbolic → apply`, `Boolean_algebra → false`, `modular_numeric → 6`.

You may not get the answer you wanted. You always get *an* answer, honestly labeled (a value, a bound, a symbolic form, or an open hole with its constraints). Nothing is refused at the door; nothing crosses the exit unlabeled. That freedom serves three lanes:

- **Production software.** A `.emath` goal becomes an ordinary, verified Cargo artifact your Rust code links against.
- **Teaching and exploration.** Write a declaration, run it in the browser playground, change a value, watch the output move.
- **Open problems.** An unproven conjecture cannot be checked in a proof assistant until the proof exists. emath compiles it today into evidence-producing machinery: counterexample hunts, finite verdicts with certified bounds, byte-reproducible forever. The artifact is the progress.

One line emath will not cross: **compiling is not proving.**

## How Factory / Droid builds it

emath turns mathematical intent into runnable code through a deterministic pipeline: source → typed semantic IR → mathematical goals → a resolution plan → generated Rust → Cargo artifact → verified host integration. Every stage is reproducible: deterministic output, byte-comparable across runs, with `emath check`, `emath build --verify`, and an independent artifact check acting as hard gates. Nothing produced by the toolchain is trusted on assertion alone; it must pass those checks, just as `fmt`, `test`, and `clippy` must stay green for a change to land.

Factory / Droid is the autonomous agent building emath. It works directly in the repository: designing language surface and compiler crates, running demos and the validation suite, and driving changes to completion. The deterministic, gate-checked pipeline is what makes that viable. Reproducible artifacts and hard verification mean the agent can iterate until the evidence says the change is real, rather than trusting assertion alone.

## Roadmap

emath ships in a fixed order. Language correctness first; interactive WASM second; production Rust packaging and ecosystem tooling third.

- [ ] **1. Language & mathematical engine** *(in progress)*  
  Surface syntax, admission, EMIR, solvers, numerics, units, demos. Mathematics that parses, type-checks, and computes.
- [ ] **2. WASM & interactive surface** *(next)*  
  Full Stage 1 capability in `emath-wasm` and the browser playground.
- [ ] **3. Production Rust artifacts & ecosystem** *(later)*  
  Host-ready Cargo components, then incremental compile, LSP, and provider bridges.

## Quickstart

The shortest path from nothing to a running program:

```console
$ emath new hello
$ emath run hello/src/main.emath --set x=3
```

`emath new hello` writes a manifest and one source file (`src/main.emath`):

```emath
emath function Greeter:
    inputs:
        x: Float64
    definitions:
        y = x
```

Declare only what you need: `inputs:`, `outputs:`, `definitions:`, and `tests:` are the function sections. `goals:` and `compile:` are not constructor sections. `emath run` admits the source and evaluates definitions or source examples on the reference VM. It prints a constructor receipt. Use `emath test <file>` for authored tests. Use `emath build <file>` to emit Rust.

### CLI change: 2026-09-08

`run` and `step` now commit each complete case. Retries reuse saved results; revision locks prevent competing commits. `--cancel-file` stops between cases. Failed definitions retain independent values without turning failure into success.

`run --branch-from <checkpoint> --relation <relation>` keeps changed problems separate from the original target. `run --measure N` records real reference-execution timings, not generated-kernel speedups. [Runtime controls and limits](implementation/CLI_REFERENCE.md#durable-execution-controls-2026-09-08). No mathematical capability or `language/` file changed in this update.

### CLI change: 2026-09-07

`run` now returns mathematical results, not Cargo test output. Its `--out` directory holds saved runs, not generated crates. Existing scripts that need generated Rust tests must use `emath test`. `emath api --json` reports the command interface and verified Language Image features. `step`, `inspect`, and `verify` accept saved run files. [Execution contract](language/reference/diagnostics-and-tooling-contract.md#saved-mathematical-runs).

**Prerequisites:** a nightly Rust toolchain. The repo pins `nightly-2026-08-27` via `rust-toolchain.toml` (with `rustfmt` and `clippy`). Rustup follows it automatically on first build; stable is not supported. Default features and demos are std-only; optional storage, search, and async-runtime features pull the exact dependencies recorded in `forks/UPSTREAM_LOCK.json`.

**First build:** allow a few minutes for a debug build of the workspace (subsequent runs are incremental).

```console
$ git clone <repo-url> && cd emath
$ cargo xtask demo all
```

(The `<repo-url>` is filled in when the public repository is reserved; inside a checkout the second line is enough.)

`cargo xtask demo all` runs both capstones; each prints `ok` and exits 0 on success:

- **affine-scorer**: the current vertical slice. Compiles `tests/valid/affine_scorer.emath` into a Cargo artifact with `--verify`, runs the host integration (`examples/demo-host`) proving `score(3.0) == 7`, constructor invariant enforcement (`new(-1.0, 0.5)` refused), and the runtime negative control.
- **semantic-genesis**: the G0-G3 pipeline. Parses the reference glyph body, runs the analysis twice and proves byte-identical output, regenerates the parametric crate, runs its in-crate fixture tests, and rejects the wrong world (swapped modular yields `5`, not `6`).

Exit criteria: both demos reach their final `ok` lines; the command exits 0. Manual: [`MANUAL.md`](MANUAL.md). Architecture: [`implementation/CONSTITUTION.md`](implementation/CONSTITUTION.md). Test surface: [`tests/README.md`](tests/README.md). Security: [`SECURITY.md`](SECURITY.md).

## Example

Constructor-layer source looks like this:

```emath
use fold.reduce

emath function Total:
    inputs:
        xs: sequence(Int)
    outputs:
        result: Int
    definitions:
        result = reduce(0, add) x in xs: x
```

What actually runs today:

- `emath object`, `emath function`, and `emath query` (`tests/fixtures/constructor/`)
- `recur` and `quote` as expression constructors
- scalar `Bool` / `Int` / `Rat` / `Float64` operations
- ordinary modules behind explicit `use` (`language/modules/`)

`emath model`, `emath policy`, and `emath kind` are not core kinds. `emath simulate` refuses. Write an ordinary function and `emath run`. Compiling is not proving.

## Core composition

```text
.emath source
  → parse
  → constructor admission (object / function / query)
  → typed IR
  → reference VM (`emath run`) or Rust emission (`emath build`)
  → receipt (execution, fulfillment, representation, evidence, remaining)
```

## Command surface

Implemented today:

| Command | Purpose |
|---------|---------|
| `emath check` | Constructor admission |
| `emath run` | Evaluate a function or query; print a receipt |
| `emath step` | Resume a constructor-layer continuation |
| `emath inspect` | Read a saved checkpoint |
| `emath verify` | Replay recorded observations |
| `emath test` | Authored `tests:` |
| `emath build` | Emit fully lowered runnable Rust |
| `emath api --search` | Constructor contracts and imported exports |
| `emath simulate` / `plan` / `eval` / `solve` | Refuse `E-KIND-GONE`; write an ordinary function and `emath run` |

See [`language/CAPABILITY.md`](language/CAPABILITY.md) and [`MANUAL.md`](MANUAL.md).

## Web playground

After a source checkout, build the browser pane and WASM engine, then serve them locally:

```console
$ cargo xtask build-web
$ emath web
```

`cargo run -p emath-cli -- web` is the same. The command prints `http://127.0.0.1:7878/` (or the `--port` you pass) and opens a browser; Ctrl-C stops the server. Use `--no-open` to skip the browser, and `--dist PATH` or `EMATH_WEB_DIST` to point at a built `web/dist`.

Everything in the pane executes in-page through a C-ABI WASM build of the compiler (no server round-trips, no cargo, nothing leaves the machine):

- **Check / Plan / Intent Graph / Generate Rust / Format**: the same deterministic pipeline as the CLI.
- **Run**: executes example tests through a strict-f64 interpreter over the lowered execution IR, honestly labeled `interpreted-strict-f64`. `emath run` returns reference-VM results in the terminal. `emath test` runs generated Rust tests. Agreement between the tiers requires a separate comparison.
- **Worked examples**: an `example` with only `given` bindings (no `expect`) is not an error: it computes and displays the values, claiming nothing. Add an `expect` and it becomes a test with a pass/fail verdict.

Edit the source, hit Run, watch the values move. That loop is the point. A lone expression or assignment in the pane (`y = x * x`, `3 * 7 + 1`) is wrapped to a declaration (the wrapped text is shown, not hidden) and declared inputs appear as fields you can wiggle without editing source.

## The provider model

emath is built on the shoulders of giants. When a capability already exists in an established engine, emath does not reimplement or absorb it. Adapters bridge to those engines and hand work to them as ordinary crates.

emath owns what makes it distinct: its language, semantic IR, goals, evidence model, artifact format, and runtime outcome contract. For anything already done well elsewhere, emath calls out through an adapter rather than duplicating it.

Adapters, honest status (std-only today; no upstream engine is consumed yet, as in-tree adapter crates ship native stand-ins):

```text
Dew (in-tree)          scalar strict-f64 mapping + Rust source/token backends
Rumoca (in-tree)       Modelica subset scan + native structural/DAE/Euler
Wrenfold               planned (Phase 2+ symbolic oracle adapter)
FrankenJAX             planned (tensors, autodiff, transforms)
FrankenSciPy           planned (solvers, optimization, integration)
FrankenSim             planned (operator graphs, kernels, certified numerics)
FrankenLean            planned (theorem and proof evidence)
native providers       exact arithmetic, intervals, search, basic numerics
```

Upstream engines (Dew JIT/GPU, the full Rumoca compiler, Wrenfold, Franken*) are pinned dependencies behind adapters in Phase 2+, never presented as implemented before then. They do not define emath's public semantics, and no upstream internals appear in emath's stable public IR. See `emath provider list` for the machine-readable status table.

## Design principles

### Determinism is a contract

Every stage of the pipeline is reproducible. Byte-comparable plans, gated builds, and independent artifact checks are the default, not optional polish.

### Evidence travels with results

Answers are labeled: a value, a bound, a symbol, or an open hole. Worlds stay as a portfolio. Nothing is trusted on assertion alone.

### Refusal is better than a silent wrong answer

Unsupported surface is named and routed, never silently guessed. Compiling is not proving. Partial sketches are welcome; incomplete capability is not disguised as success.

### Capability specification is the source of truth

Every mathematical capability, algorithm, and syntactic contract is authored as a declarative capability capsule. Rust implements the thin universal nucleus (parse, semantic admission, IR lowering, VM, and code generation), while mathematical theories, solvers, and contracts compile against that nucleus.

## Why Rust

Rust provides predictable native deployment, zero-cost abstractions for generated code, strong type and ownership boundaries, a mature package model through Cargo, practical integration with systems software, and a suitable language for implementing the compiler itself.

The core toolchain is Rust-first. Optional providers may use other implementation languages behind stable adapters.

## Documentation map

| Doc | Role |
|-----|------|
| [`MANUAL.md`](MANUAL.md) | Operator and developer manual |
| [`implementation/CONSTITUTION.md`](implementation/CONSTITUTION.md) | Formal architectural principles and laws |
| [`implementation/CRATE_MAP.md`](implementation/CRATE_MAP.md) | Workspace layout and crate roles |
| [`implementation/PUBLIC_API_INVENTORY.md`](implementation/PUBLIC_API_INVENTORY.md) | Public interfaces and ABI inventory |
| [`implementation/ERROR_CODES.md`](implementation/ERROR_CODES.md) | Compiler error code specifications |
| [`examples/README.md`](examples/README.md) | Host integration examples |
| [`web/README.md`](web/README.md) | WebAssembly interactive workbench |
| [`tests/README.md`](tests/README.md) | Test surface and verification intent |
| [`SECURITY.md`](SECURITY.md) | Security policy |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | Contribution guidelines |

## Contributing

We are more than happy to welcome contributions. Before you start, please reach out to the author ([Aditya](https://x.com/adityavg13)) first so we can make sure we're all on the same page.

## License

The emath-owned code license is Apache-2.0 (`LICENSE`). Provider and corpus licenses remain independently tracked and reproduced in release bundles.
