# Chapter 13: Standard Library Constitution

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

## Curated law packages

Named mathematics is source, not a compiler builtin. The first embedded
package is `physics::classical`, documented in
[`../stdlib/laws/INDEX.md`](../stdlib/laws/INDEX.md). Its algebraic laws
execute through ordinary function lowering and retain law metadata. Continuum
laws remain explicit deferrals until their assumptions can be checked.

## Measurement types

`core::measure` defines the neutral `Measured<T>` record schema and the
closed six-variant `Provenance` type. These are data-driven stdlib schemes,
not parser keywords. Provenance attached to a source binding participates in
package identity, while mathematical `MeaningID` remains provenance-neutral.
The `±` surface and record-literal lowering are owned by their dedicated
language slices and are not fabricated here.

## Imported declaration-kind schemas

`std.kinds.capability`, `std.kinds.family`, `std.kinds.theory`,
`std.kinds.model`, and `std.kinds.morphism` mount declaration schemas only when
explicitly imported. The last three expose a bounded finite-algebra checker:
theory claims do not self-certify, while model laws and morphism preservation
gain E2 authority only after exhaustive native checking.

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
