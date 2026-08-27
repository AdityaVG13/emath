# emath

<div align="center">

[![Status](https://img.shields.io/badge/status-active%20Rust%20workspace-2ea44f)](#what-exists-now)
[![Rust](https://img.shields.io/badge/rust-nightly%202026--08--04-b7410e)](rust-toolchain.toml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
[![Language](https://img.shields.io/badge/docs-language%2Freference-0969da)](language/reference/overview.md)

</div>

> **Write known math. Invent new math. Compile both.**

**emath** is a Rust-first language and compiler for mathematics that runs. You write mathematical intent (settled theorems, half-formed models, or structures invented today) and the toolchain lowers it into ordinary, inspectable Cargo artifacts.

A `.emath` source can carry formulas and tensors; dynamical systems and constraints; evaluation, differentiation, solve, integrate, optimize, and simulate goals; units, shapes, domains, and error bounds; evidence requirements; and Rust host interfaces.

Intent is resolved through a deterministic pipeline (typed semantic IR → goals → providers → generated Rust), validated by hard gates (`emath check`, `emath build --verify`, independent artifact check), and published as software you can link like any other crate.

## TL;DR

**The problem:** mathematical intent usually lives in one tool (a CAS, a notebook, a prover, a hand-written solver) while the runnable artifact lives in another. Units, shapes, evidence, and host integration fall through the cracks, and "it compiled" is treated as "it is true."

**The solution:** emath is a single language surface for mathematics that computes. Declarations lower through a typed semantic IR, interchangeable providers, and hard verification gates into ordinary Rust crates. Interpretation is data (candidate *worlds*, kept as a portfolio), and refusals are named rather than silent.

### What exists now

| Area | Current implementation |
|------|------------------------|
| Language surface | `emath function`, `policy`, `model` (ODE simulate); vectors, matrices, rank-3 tensors; units; Nat/Int indexes; named refusals for unsupported surface |
| Pipeline | Parse → admit → typed semantic IR → goals → exec IR → Rust codegen → Cargo publish under `target/emath` |
| Gates | `emath check`, `emath build --verify`, `emath artifact check`; demos exit 0 with `ok` |
| Capstone demos | `cargo xtask demo all` (affine-scorer + semantic-genesis) |
| Web playground | `emath web` (in-page WASM compiler; Stage 1 subset today) |
| Providers (Phase 1) | Std-only; in-tree Dew/Rumoca stand-ins; Wrenfold / Franken* planned behind adapters |
| Docs of record | [`language/reference/`](language/reference/overview.md), [`language/examples/`](language/examples/), [`MANUAL.md`](MANUAL.md) |

### Honest boundaries

- **Compiling is not proving.** The pipeline guarantees the artifact matches what you asked for, never that the idea is true. Lean / FrankenLean is planned as hired evidence, not authority.
- Illustrative README sketches may parse and then refuse unimplemented parts with a named error. That is expected.
- There is no crates.io product claim yet. The workspace is the deliverable today.
- Upstream engines are not absorbed into emath; adapters only, and not consumed yet in Phase 1.

## What emath is not

- It is not merely a computer algebra system.
- It is not merely Modelica rewritten in Rust.
- It is not a Rust macro that prints arithmetic expressions.
- It is not a theorem prover pretending every theorem is executable.
- It is not one giant vendored workspace of unrelated repositories.
- It is not an AI code generator whose output is accepted without deterministic checking.

## More than a compiler

Any finite mathematical structure that is structurally well-formed admits: textbook math, a jumbled formula, an idea for a problem nobody has posed yet. The same glyphs can carry many legitimate meanings, so emath represents interpretation as data: candidate *worlds*, chosen deterministically and kept as a portfolio rather than collapsed into an unlabeled guess. The validation suite runs one spec through three worlds today: `free_symbolic → apply`, `Boolean_algebra → false`, `modular_numeric → 6`.

You may not get the answer you wanted. You always get *an* answer, honestly labeled (a value, a canonical term, or a refusal with a name). That freedom serves three lanes:

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
$ emath run hello/src/main.emath
```

`emath new hello` writes a manifest and one source file (`src/main.emath`):

```emath
emath function Greeter:
    inputs:
        x: Float64
    definitions:
        y = x
```

Declare only what you need: `inputs:`, `outputs:`, `goals:`, `exports:`, and `compile:` are optional. A bare input name (`x`) defaults to `Float64`. Definitions are the surface; an omitted `goals:` section evaluates every definition and `emath run` admits, builds, publishes under `target/emath`, and executes the example tests (`emath test <file>` reports them, `emath build <file> [--out <dir>]` publishes without running).

**Prerequisites:** a nightly Rust toolchain. The repo pins `nightly-2026-08-27` via `rust-toolchain.toml` (with `rustfmt` and `clippy`). Rustup follows it automatically on first build; stable is not supported. Default features and demos are std-only; optional storage, search, and async-runtime features pull the exact dependencies recorded in `forks/UPSTREAM_LOCK.json`.

**First build:** allow a few minutes for a debug build of the workspace (subsequent runs are incremental).

```console
$ git clone <repo-url> && cd emath
$ cargo xtask demo all
```

(The `<repo-url>` is filled in when the public repository is reserved; inside a checkout the second line is enough.)

`cargo xtask demo all` runs both capstones; each prints `ok` and exits 0 on success:

- **affine-scorer**: the Phase 1 vertical slice. Compiles `tests/valid/affine_scorer.emath` into a Cargo artifact with `--verify`, runs the host integration (`examples/demo-host`) proving `score(3.0) == 7`, constructor invariant enforcement (`new(-1.0, 0.5)` refused), and the runtime negative control.
- **semantic-genesis**: the G0-G3 pipeline. Parses the reference glyph body, runs the analysis twice and proves byte-identical output, regenerates the parametric crate, runs its in-crate fixture tests, and rejects the wrong world (swapped modular yields `5`, not `6`).

Exit criteria: both demos reach their final `ok` lines; the command exits 0. Language contract and CLI surface: start at [`language/reference/overview.md`](language/reference/overview.md). Test surface: [`tests/README.md`](tests/README.md). Security: [`SECURITY.md`](SECURITY.md).

## Example

The target language looks like this (illustrative; the implemented subset today is smaller: see `tests/valid/`):

```emath
emath policy CachePriority:
    input:
        candidate: CacheCandidate

    state:
        alpha: NonNegative<Real>
        decay: NonNegative<Per<Second>>

    constructor new(
        alpha: Real,
        decay: Per<Second>,
    ) -> Result<Self, ConfigError>:
        require alpha >= 0
        require decay >= 0 / s

        Self:
            alpha = alpha
            decay = decay

    define score(candidate) -> Real:
        candidate.reuse_probability ^ self.alpha
        * (candidate.rebuild_cost / 1 ms)
        * exp(-(self.decay * candidate.age))
        / (1 + candidate.bytes / 1 MiB)

    goal compile score for rust.library
    goal differentiate score wrt [alpha, decay]

    evidence:
        require finite over CacheCandidate::admitted_domain
        require max_relative_error <= 1e-10

    host rust:
        implement cache_core::Policy:
            method score = score
```

What actually runs today is smaller and more concrete than that sketch:

- `emath function` formulas (`tests/valid/square.emath`, `language/examples/intro/hello-square.emath`)
- `emath policy` with a constructor (`tests/valid/affine_scorer.emath`)
- `emath model` ODEs you can `emath simulate` (`language/examples/numerical/explicit-mass-spring.emath`)
- vectors, matrices, rank-3 tensors, slices, units, and Nat/Int indexes

The rest of the sketch is the target language. The compiler will parse a lot of it and then refuse the parts it cannot run yet, with a named error. That is expected. Compiling is not proving.

## Core composition

```text
.emath source
  → package/module loader
  → syntax and schema expansion
  → typed semantic IR
  → mathematical goals
  → resolver/provider planning
  → executable math IR
  → evidence plan
  → structured Rust IR
  → Cargo artifact
  → host integration
  → protected baseline/candidate experiment
```

## Command surface

Implemented today:

| Command | Purpose |
|---------|---------|
| `emath check` | Semantic admission |
| `emath plan` | Deterministic resolution plan |
| `emath build [--out <dir>]` | Generate + verify Cargo artifact (default: `target/emath`) |
| `emath simulate` | Integrate an admitted emath model |
| `emath artifact check` | Independent artifact validation |
| `emath parse --forest` | G0/G1: glyphs → bounded parse forest |
| `emath signature` | Signature / arity inference |
| `emath genesis --out <dir>` | Semantic genesis analysis pipeline |
| `emath compile --parametric --out` | Deterministic generated crate |
| `emath world show` / `portfolio show` | Introspection |
| `emath architecture` / `help` | Stable docs entry |
| `emath web` | Localhost web playground (Ctrl-C to stop) |

Also implemented: `serve` (alias for `web`), `new`, `fmt`, `explain`, `run`, `test`, `bench` (typed refusal until the Phase 4 harness), `verify`, `inspect`, `diff`, `doctor`, `vendor`, `provider list|inspect|test`, `fork status|sync`, `agent check|plan|build`, and `import modelica`. Planned (see `language/reference/diagnostics-and-tooling-contract.md`): `migrate`.

## Web playground

After a source checkout, build the browser pane and WASM engine, then serve them locally:

```console
$ cargo xtask build-web
$ emath web
```

`cargo run -p emath-cli -- web` is the same. The command prints `http://127.0.0.1:7878/` (or the `--port` you pass) and opens a browser; Ctrl-C stops the server. Use `--no-open` to skip the browser, and `--dist PATH` or `EMATH_WEB_DIST` to point at a built `web/dist`.

Everything in the pane executes in-page through a C-ABI WASM build of the compiler (no server round-trips, no cargo, nothing leaves the machine):

- **Check / Plan / Intent Graph / Generate Rust / Format**: the same deterministic pipeline as the CLI.
- **Run**: executes example tests through a strict-f64 interpreter over the lowered execution IR, honestly labeled `interpreted-strict-f64`. The compiled-Rust tier (`emath run` / `emath test`) remains the native lane; agreement between the two tiers is checked differentially, not assumed.
- **Worked examples**: an `example` with only `given` bindings (no `expect`) is not an error: it computes and displays the values, claiming nothing. Add an `expect` and it becomes a test with a pass/fail verdict.

Edit the source, hit Run, watch the values move. That loop is the point. A lone expression or assignment in the pane (`y = x * x`, `3 * 7 + 1`) is wrapped to a declaration (the wrapped text is shown, not hidden) and declared inputs appear as fields you can wiggle without editing source.

## The provider model

emath is built on the shoulders of giants. When a capability already exists in an established engine, emath does not reimplement or absorb it. Adapters bridge to those engines and hand work to them as ordinary crates.

emath owns what makes it distinct: its language, semantic IR, goals, evidence model, artifact format, and runtime outcome contract. For anything already done well elsewhere, emath calls out through an adapter rather than duplicating it.

Adapters, honest status (Phase 1 is std-only; no upstream engine is consumed yet, as in-tree adapter crates ship native stand-ins):

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

Answers are labeled: a value, a canonical term, or a named refusal. Worlds stay as a portfolio. Nothing is trusted on assertion alone.

### Refusal is better than a silent wrong answer

Unsupported surface is refused with a name. Compiling is not proving. Partial sketches are welcome; incomplete capability is not disguised as success.

### `language/` is the source of truth

If a capability is not written down in `language/`, it does not exist for the user. The reference, examples, and grammar stay current with every admitted or refused feature.

## Why Rust

Rust provides predictable native deployment, zero-cost abstractions for generated code, strong type and ownership boundaries, a mature package model through Cargo, practical integration with systems software, and a suitable language for implementing the compiler itself.

The core toolchain is Rust-first. Optional providers may use other implementation languages behind stable adapters.

## Documentation map

| Doc | Role |
|-----|------|
| [`language/reference/overview.md`](language/reference/overview.md) | Normative language contract (start here) |
| [`language/examples/`](language/examples/) | Runnable programs by category |
| [`MANUAL.md`](MANUAL.md) | Operator / developer manual |
| [`tests/README.md`](tests/README.md) | Test surface and intent |
| [`SECURITY.md`](SECURITY.md) | Security notes |
| [`AGENTS.md`](AGENTS.md) | Agent operating contract for this repo |

## Contributing

We are more than happy to welcome contributions. Before you start, please reach out to the author ([Aditya](https://x.com/adityavg13)) first so we can make sure we're all on the same page.

## License

The emath-owned code license is Apache-2.0 (`LICENSE`). Provider and corpus licenses remain independently tracked and reproduced in release bundles.
