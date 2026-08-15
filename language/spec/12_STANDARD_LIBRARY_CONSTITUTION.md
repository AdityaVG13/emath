# Standard Library Constitution

## Purpose

The standard library defines portable semantic contracts and small trusted implementations. Large solver portfolios and hardware backends remain provider packages.

## Initial package families

```text
core::prelude
core::logic
core::numbers
core::units
core::shapes
core::domains
core::collections
core::linear_algebra
core::calculus
core::optimization
core::graphs
core::probability
core::state
core::evidence
core::artifact
core::host
```

## Admission requirements

A stable standard item has:

- mathematical and executable semantics;
- domain/partiality behavior;
- numeric-profile behavior;
- tests and negative cases;
- canonical identity/version;
- Rust mapping where applicable;
- provider extension points;
- evidence/no-claim statement.

## Minimality rule

An algorithm does not enter `core` merely because it is popular. Contracts and small reference implementations belong in core; broad/high-performance portfolios live in packages.

## Provider contracts

Examples:

```text
RootSolver
LinearSolver
Integrator
Optimizer
Differentiator
ProofChecker
TensorBackend
SimulationBackend
```

Contracts include result/certificate/error/budget semantics, not only function signatures.
