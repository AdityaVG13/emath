# Chapter 5: Types, Units, Shapes and Domains

## Type families

```text
Bool, Nat, Int, Rat, Real
Float16, BFloat16, Float32, Float64
Fixed, Decimal, Interval, Affine, Dual, Complex
Option, Result, Sequence, Stream, Map, Set
Vector, Matrix, Tensor, SparseMatrix
Graph, Field, Distribution, StateMachine
record, variant, trait, opaque/external
```

`Real` is mathematical and does not by itself prescribe `f64`.

## Refinement types

```emath
type Probability = Real where 0 <= self and self <= 1
type Positive<T> = T where self > 0
```

Refinements may be established statically, by constructor checks, by provider certificates or remain caller obligations.

## Units and dimensions

```emath
unit Token = base "token"
unit TokenRate = Token / s
unit DollarPerMillionToken = USD / (1_000_000 Token)
```

A quantity has dimension, scale, offset rules and representation. Affine units such as absolute temperature cannot be treated as ordinary multiplicative units.

## Shapes

```emath
shape Hidden = [Batch, Sequence, Width]
input: Tensor<Float32, Hidden>
```

Shape constraints include equality, broadcasting, rank, extent arithmetic and layout. Unknown extents remain symbolic when supported.

## Domains

```emath
domain Time = Interval(0 s, 10 s)
domain Ω = Box([0 m, 1 m], [0 m, 1 m])
domain Nodes = vertices(graph)
```

Domains define admissible values, integration measures, boundaries, branch conventions and topology as applicable.

## Conversions

Conversions are classified:

- exact representation-preserving;
- exact value with representation change;
- checked narrowing;
- wrapping/saturating;
- rounded with declared policy;
- approximate with error;
- unit scale/offset;
- shape/layout transformation.

Implicit conversions are deliberately limited and source-mapped.

## Numeric profiles

A compile profile selects representations and math behavior:

```text
exact
strict-f64
fast-f64
interval-f64
fixed-point
provider-selected under constraints
```

The chosen profile enters artifact identity and evidence.
