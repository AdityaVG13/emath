# eMath Fork Constellation Implementation Foundation V5

This repository package contains the complete strategy, language design, architecture, phased implementation plan, upstream fork policy, Rust API foundations, schemas, examples, tests, validation tools, and historical prototypes needed to begin implementing eMath.

## One-sentence product definition

eMath is a **mathematical package and goal compiler**: users define typed mathematical systems in `.emath`; the Rust compiler elaborates them into neutral semantic and execution IRs; compatible providers solve, compile, search, verify, or simulate requested goals; and the system emits evidence-carrying Cargo artifacts that can be benchmarked and promoted inside real Rust programs.

## What eMath is not

- It is not merely a computer algebra system.
- It is not merely Modelica rewritten in Rust.
- It is not a Rust macro that prints arithmetic expressions.
- It is not a theorem prover pretending every theorem is executable.
- It is not one giant vendored workspace of unrelated repositories.
- It is not an AI code generator whose output is accepted without deterministic checking.

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

## Principal upstream substrates

- Rumoca: structural modeling, class/package instantiation, equations and DAE-oriented compiler phases.
- Dew: expression representation, Rust/code-token output, JIT and accelerator backend patterns.
- Wrenfold: optional symbolic provider, derivative/code-generation reference, and differential oracle.
- Modelica Standard Library: compatibility and scientific component corpus.
- Franken repositories: optional numerical, tensor, AD, simulation, verification, proof, replay, and runtime providers.

No upstream internal type appears in eMath's stable public IR.

Begin with `START_HERE.md`.
