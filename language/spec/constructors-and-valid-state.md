# Chapter 6: Constructors and Valid-State Semantics

## Purpose

A constructor is the boundary by which an untrusted tuple of values becomes an admitted instance of a declaration.

```emath
constructors:
    public fn new(scale: Real, bias: Real) -> Result<Self, ConfigError>:
        require scale >= 0

        Self:
            scale = scale
            bias = bias
```

## Constructor stages

1. bind and type parameters;
2. normalize units/representations;
3. check or discharge preconditions;
4. compute derived fields;
5. assign every required field exactly once;
6. establish postconditions/invariants;
7. emit valid instance or declared error;
8. optionally emit construction evidence.

## Constructor forms

- direct value constructor;
- fallible validating constructor;
- factory/fit constructor from data;
- conversion constructor;
- compile-time template constructor;
- resolution-plan constructor selecting providers/representations.

## Delegation

Constructors may call other constructors. Delegation cycles and paths that bypass stronger invariants are rejected.

## Visibility and forging

If a public invariant relies on private fields, generated Rust fields remain private. Serialization/deserialization routes re-run admission or carry checked evidence; raw struct construction is not generated publicly.

## Invariant consumption

Constructor facts become assumptions available for specialization and proof only within their scope. For example, `scale >= 0` can justify a branch elimination or monotonicity rule. The compiler records which generated optimization consumed which fact.

## Runtime changes

State transitions must preserve declared invariants or return a structured refusal. Mutable host access cannot expose fields in a way that invalidates the model without revalidation.

## Error model

Errors are typed variants with stable semantic reasons, not only strings. Generated Rust may add diagnostic context while retaining machine-readable variants.
