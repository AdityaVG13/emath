# Contributing to emath

emath is built through vertical slices. A contribution is complete when it
includes:

1. semantic contract;
2. implementation;
3. valid example;
4. invalid example;
5. deterministic diagnostic;
6. tests;
7. a negative control when evidence is involved;
8. artifact/provenance updates;
9. documentation;
10. a real consumer for end-to-end capabilities.

## Change classes

Every contribution declares one class:

- language semantic change;
- compiler implementation;
- neutral IR change;
- provider/adapter change;
- evidence/checker change;
- artifact/deployment change;
- documentation/tooling;
- upstream fork synchronization.

## Required change contents

A semantic or public API change includes:

1. motivation and rejected alternatives;
2. affected source syntax and canonical IR;
3. versioning/migration impact;
4. diagnostics and malformed cases;
5. normal, boundary and adversarial tests;
6. artifact/source-map impact;
7. provider compatibility impact;
8. evidence and trust impact;
9. performance/resource impact;
10. rollback plan.

## Contribution tracks

### Language

Syntax, HIR, types, constructors, custom kinds.

### Planner

Goal algebra, capabilities, plan search, explanation.

### Providers

Adapters, algorithms, code generation, checkers.

### Artifacts

Cargo output, manifests, source maps, continuations.

### Evidence

Certificates, proof adapters, translation validation, negative controls.

### Frontier engine

Candidate generation, benchmarking, Pareto analysis, promotion.

### Documentation and examples

Executable public examples and tutorials.

## RFC threshold

An RFC is required for changes to:

- grammar or layout rules;
- declaration-kind semantics;
- constructor authority;
- type/unit/shape/domain equivalence;
- neutral IR canonical encoding;
- goal or evidence taxonomy;
- provider trust model;
- artifact schemas;
- registry resolution;
- stable host ABI.

## Provider rule

Do not expose provider-owned types in the stable emath API.

## Claim rule

Documentation must distinguish implemented, experimental, planned, and research
capabilities. Claims stay within evidence: a feature is claimed only when its
producer, consumer, command, input lock, and retained result exist.

## Fork contributions

Follow `forks/FORK_GOVERNANCE.md`. Keep emath-specific code in adapters whenever
practical. An upstream patch must retain upstream style and tests; an emath
divergence must have a ledger entry and rebase owner.

## Evidence discipline

Do not claim a feature from a type definition, dormant function, mock,
generated fixture or historical run. Link the real producer, consumer, command,
input lock and retained result.
