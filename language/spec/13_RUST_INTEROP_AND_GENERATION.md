# Rust Interop and Generation

## Type mapping

The Rust backend maps semantic types according to target profile. Examples:

```text
Bool → bool
Float64 → f64
Nat exact → BigNat provider or bounded representation with proof/check
Result<T,E> → Result<T,E>
record → struct
variant → enum
Tensor<T,S> → generated static type or provider tensor
```

`Real` requires a selected representation; it never maps to `f64` without profile evidence.

## Constructors

Validating constructors become associated functions returning `Result`. Fields remain private when required for invariants. Unsafe unchecked constructors are not generated publicly by default.

## Ownership

The backend chooses owned, borrowed or shared types based on host/API contract. Provider buffers use explicit lifetime/layout contracts. No provider pointer leaks into durable artifacts.

## Errors

Semantic failure variants remain typed. Provider/backend detail may be wrapped without losing stable reason codes.

## Generics

Static dimensions and types map to Rust generics/const generics where practical. Constraints without a Rust type-system representation become private checks or sealed evidence tokens.

## Build scripts and macros

The package workflow is authoritative. Build-script and macro conveniences invoke the same compiler and lock resolution. They cannot create a separate mini-language with different semantics.

## Generated documentation

Public APIs include mathematical definition, assumptions, numeric profile, source reference, evidence level and fallback behavior.
