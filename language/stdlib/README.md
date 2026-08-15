# emath Standard Library Plan

The standard library is split into semantic contract packages and implementation/provider packages. See `language/spec/12_STANDARD_LIBRARY_CONSTITUTION.md`.

## Phase 1 core

- scalar primitive types and strict `Float64` profile;
- records, variants, `Option`, `Result`;
- arithmetic and elementary functions;
- constructor predicates;
- artifact/evidence/runtime outcome vocabulary.

## Phase 2–5 core

- exact integers/rationals and numeric profiles;
- units/dimensions;
- shapes/tensors;
- intervals and domains;
- graph and state-machine contracts;
- calculus/optimization goal contracts.

## Provider packages

- symbolic simplification/differentiation;
- root/integration/ODE/optimization;
- tensor/AD backends;
- Modelica/Rumoca structural simulation;
- theorem/proof checkers;
- interval/certified numerics;
- hardware and remote execution.
