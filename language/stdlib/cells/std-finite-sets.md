# `std.finite.sets`; finite-set world class (authoring draft)

Status: parse vertical admitted by bead `emath-r3-sets-tub8`
(`ExprKind::Set`, `SetComprehension`, `BinaryOp::In`). Evaluation is
refused with `E-TYPE-113` until the emath-ir Phase B lane (fjxh) lands
`TypeNode::Set` / `Value::Set` lowering. This page documents the world
contract as stdlib surface data; nothing here grows a core IR enum
variant beyond the parse-tree nodes already admitted.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.finite.sets` |
| Class | `world` (carrier + laws for a value domain, not a function) |
| Version | `0.1.0` |
| Migration | `experimental` (Phase B pending) |

The `world` class extends the cell machinery's `pure` function class; the
machinery-side class enum and carrier validation are Phase B scope, so
this cell is a contract document, not yet a validated capability cell.

## Carrier

Finite subsets of a declared universe `U`:

- literal `{2, 3, 5}`; finite; order-irrelevant; duplicate elements
  collapse under extensionality;
- comprehension `{n in 0..100 if is_prime(n)}`; the subset of the finite
  domain whose elements satisfy the guard;
- membership `v in s` (ASCII for ∈); total on declared carriers,
  result `Bool`.

## Laws (Phase B test targets)

1. **Extensionality**; `s == t` iff `(v in s) == (v in t)` for every `v`
   in `U`; literal order and duplicates never change the value.
2. **Comprehension membership**; `v in {x in d if p(x)} == (v in d) &&
   p(v)`; the guard is the only admission criterion.
3. **Finite enumeration**; every admitted set has a finite canonical
   enumeration; the world refuses an infinite domain at the admission
   seam instead of diverging.

## Refusals (typed, never silent)

- Record spelling without a path prefix (`{x: 1}`) is ambiguous between a
  one-field record and a malformed set: refuses `E-SYN-154`
  (`tests/invalid/r3_sets_tub8_ambiguous.emath`).
- Evaluating any set value before Phase B: `E-TYPE-113`. No partial
  evaluation, no silent set-to-list reinterpretation.

## Surface spelling

```emath
emath function Probe:
    definitions:
        primes = {n in 0..100 if is_prime(n)}
        has_two = 2 in primes
```
