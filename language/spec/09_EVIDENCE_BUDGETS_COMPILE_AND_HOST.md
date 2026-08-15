# Evidence, Budgets, Compilation and Host Sections

## Evidence declarations

```emath
evidence:
    claim <score_finite>:
        statement is_finite(score)
        require runtime-check

    claim <monotone_reuse>:
        statement derivative(score, reuse_probability) >= 0
        require certificate
```

A claim separates statement, scope, assumptions and required evidence.

## Budgets

```emath
budgets:
    compile:
        wall_time: 30 s
        memory: 1 GiB
    runtime:
        evaluations: 1_000_000
        iterations: 10_000
```

Budgets are semantic policy and enter planning/artifact identity when they affect results.

## Compile section

```emath
compile:
    target rust
    profile library
    numeric strict-f64
    unresolved parametric
    safety forbid-unsafe
```

This sets output requirements, not the mathematical meaning of definitions unless numeric semantics are intentionally part of the declaration.

## Exports

Exports select public values, types, constructors, evidence and provider interfaces. Non-exported internals may still appear in source maps/evidence according to privacy policy.

## Host section

```emath
host:
    rust:
        crate_name "generated_policy"
        implement cache_core::Policy for Self:
            method score = score
```

Host mappings declare conversions, ownership, errors and fallback.

## Tests and benchmarks

Tests are executable specifications. Benchmarks define comparison methodology and never substitute for semantic evidence.
