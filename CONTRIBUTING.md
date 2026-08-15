# Contributing to eMath

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

## Fork contributions

Follow `forks/FORK_GOVERNANCE.md`. Keep eMath-specific code in adapters whenever practical. An upstream patch must retain upstream style and tests; an eMath divergence must have a ledger entry and rebase owner.

## Evidence discipline

Do not claim a feature from a type definition, dormant function, mock, generated fixture or historical run. Link the real producer, consumer, command, input lock and retained result.
