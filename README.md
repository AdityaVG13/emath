# emath

> **Write the mathematics you mean. emath builds what you can run.**

**emath** is a Rust-first language, compiler, and optimization lab for turning mathematical intent into executable, inspectable Cargo components.

A `.emath` program can describe:

- formulas, tensors, graphs, dynamic systems, constraints, and search spaces;
- constructors and legal object states;
- goals such as evaluate, differentiate, solve, integrate, optimize, simulate, search, verify, compile, and tune;
- numerical semantics, units, shapes, domains, precision, and error limits;
- evidence requirements;
- Rust host interfaces and protected performance objectives.

emath resolves that intent through interchangeable providers, generates one or more implementations, validates them, and packages the result as ordinary Rust software.

## What emath is not

- It is not merely a computer algebra system.
- It is not merely Modelica rewritten in Rust.
- It is not a Rust macro that prints arithmetic expressions.
- It is not a theorem prover pretending every theorem is executable.
- It is not one giant vendored workspace of unrelated repositories.
- It is not an AI code generator whose output is accepted without deterministic checking.

## What emath does, and how Factory / Droid builds it

emath turns mathematical intent into runnable code through a deterministic
pipeline: source → typed semantic IR → mathematical goals → a resolution plan
→ generated Rust → Cargo artifact → verified host integration. Every stage
is reproducible: deterministic output, byte-comparable across runs, with
`emath check`, `emath build --verify`, and an independent artifact check
acting as hard gates. Nothing produced by the toolchain is trusted on
assertion alone; it must pass those checks, just as `fmt`, `test`, and
`clippy` must stay green for a change to land.

Droid is doing the work to build out emath. An autonomous agent works
directly over the repository: designing features, writing the compiler and
its crates, running the demos and validation suite, and driving changes to
completion. The deterministic pipeline is exactly why that works: the output
is reproducible and gated, so the agent can iterate until the evidence says
the change is real, and the gates keep the work honest.

## Quickstart

**Prerequisites:** a stable Rust toolchain (`rustup default stable`; the repo pins stable via `rust-toolchain.toml` and needs the `rustfmt` and `clippy` components). That is all: the workspace has zero third-party dependencies and the demos are std-only.

**First build:** allow a few minutes for a debug build of the workspace (subsequent runs are incremental).

```console
$ git clone <repo-url> && cd emath
$ cargo xtask demo all
```

(The `<repo-url>` here is filled in when the public repository is reserved; inside a checkout the second line is all that is needed.)

`cargo xtask demo all` runs both capstones; each prints `ok` and exits 0 on success:

- **cache-policy**: the Phase 1 vertical slice: compiles `tests/valid/affine_policy.emath` into a Cargo artifact with `--verify`, runs the host integration (`examples/demo-host`) proving `score(3.0) == 7`, constructor invariant enforcement (`new(-1.0, 0.5)` refused), and the runtime negative control.
- **semantic-genesis**: the G0–G3 pipeline: parses the reference glyph body, runs the analysis twice and proves byte-identical output, regenerates the parametric crate, runs its in-crate fixture tests, and rejects the wrong world (swapped modular yields `5`, not `6`).

Exit criteria: both demos reach their final `ok` lines; the command exits 0. The language contract, evidence boundaries, and CLI surface are spelled out in `language/spec/` (start with `language/spec/00_LANGUAGE_OVERVIEW.md`); the test surface is documented in `tests/README.md`; security notes live in `SECURITY.md`.

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

The implemented vertical slice covers a scalar subset: a declaration with validated `state`, one checked constructor, scalar `define`d methods, `tests:` blocks, and `host rust` export: compiled and verified end to end as `examples/generated/affine-policy`.

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

```console
emath check                       semantic admission
emath plan                        deterministic resolution plan
emath build --out <dir> --verify  generate + verify Cargo artifact
emath artifact check              independent artifact validation
emath parse --forest              G0/G1: glyphs → bounded parse forest
emath signature                   signature/fixity inference
emath genesis --out <dir>         semantic genesis analysis pipeline
emath compile --parametric --out  deterministic generated crate
emath world show / portfolio show introspection
emath architecture / help         stable docs entry
```

Planned (see `language/spec/11_DIAGNOSTICS_AND_TOOLING_CONTRACT.md`): `new`, `fmt`, `explain`, `run`, `test`, `bench`, `verify`, `inspect`, `diff`, `migrate`, `doctor`, `vendor`, `provider list|inspect|test`.

## The provider model

emath is built on the shoulders of giants. The numerical and symbolic
computing ecosystem has already solved many hard problems, and we have no
intention of rewriting that work. When a capability already exists in an
established engine, emath does not reimplement or absorb it, nor does its
internals become part of emath. Instead, adapters bridge to those engines and
let emath hand work to them as ordinary crates.

emath owns what makes it distinct: its language, semantic IR, goals, evidence
model, artifact format, and runtime outcome contract. For anything that is
already done well elsewhere, emath calls out to it through an adapter rather
than duplicating it. We build our own thing on top of theirs.

Available and planned adapters:

```text
Dew                 expression optimization, Rust codegen, JIT, GPU backends
Rumoca              component models, equations, flattening, DAE analysis
Wrenfold            optional symbolic oracle and code-generation reference
FrankenJAX          tensors, automatic differentiation, transforms
FrankenSciPy        numerical solvers, optimization, integration
FrankenSim          operator graphs, kernels, certified numerical paths
FrankenLean         optional theorem and proof evidence
native providers    exact arithmetic, intervals, search, basic numerics
```

Providers are pinned dependencies behind adapters (Phase 2+). They do not
define emath's public semantics, and no upstream internals appear in emath's
stable public IR.

## Why Rust

Rust provides:

- predictable native deployment;
- zero-cost abstractions for generated code;
- strong type and ownership boundaries;
- a mature package model through Cargo;
- practical integration with systems software;
- multiple JIT, GPU, numerical, and proof ecosystems;
- a suitable language for implementing the compiler itself.

The core toolchain is Rust-first. Optional providers may use other implementation languages behind stable adapters.

## Contributing

We are more than happy to welcome contributions. Before you start, please reach out to the author ([Aditya](https://x.com/adityavg13)) first so we can make sure we're all on the same page.

## License

The emath-owned code license is Apache-2.0 (`LICENSE`). Provider and corpus licenses remain independently tracked and reproduced in release bundles.
