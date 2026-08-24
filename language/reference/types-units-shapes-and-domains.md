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

## Generic arguments at use sites (C10)

Generic arguments instantiate a parameterized type at a use site. The
grammar admits three kinds of argument:

- **Type argument** — a type expression: `Vector<Float64>`, `NonNegative<Real>`
- **Value argument** — a literal or expression: `Mod<7>`, `Tensor<Float64, [N, N]>`
- **Named argument** — `name = expression`: `GF<2, 3, modulus = x + 1>`

```emath
inputs:
    v: Vector<Float64>            # type-only (unchanged)
    m: Mod<7>                     # integer literal value arg
    grid: Tensor<Float64, [N, N]> # type arg + bracket-list extent
    field: GF<2, 3, modulus = x + 1>  # value args + named arg
```

The first argument to `Vector` / `Matrix` / `Tensor`, if it is a
recognized element type (`Float64`, `Real`, …), is the element type;
remaining arguments are extents. If the first argument is not an
element type, all arguments are extents and the element defaults to
`Float64`.

Value-level and named generic arguments **parse** today. Semantic
admission of non-type arguments (modular arithmetic, finite fields,
function spaces) will arrive with the domain-specific language beads
that use them.

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

## Implemented today

Admitted compute types:

```text
Float64  Bool  Nat  Int  Complex  Mod<p>  GF<p>
Vector[n]  Matrix[r, c]  Tensor[…]
quantity / `T in unit` annotations
```

`Nat` and `Int` are indexes and small integer values. Arithmetic with
them still evaluates as `Float64` internally, but when an output is
declared `Int`, the result is converted to exact `i64` — no
floating-point rounding in the final value. This makes `product i in
1..=20: i` with `Int` output give the exact factorial, not a float
approximation. A negative constant index is `E-SHAPE-006`.

Shapes:

- rank-1 / rank-2 literals stay `Vector` / `Matrix`
- rank-3+ literals become `Tensor`
- `v[i]`, `m[i, j]`, `t[i, j, k]` drop rank
- `t[0, :, :]` and other `:` axes keep rank
- tensor add/sub only when extents match, or one side is `1`

Units that the compiler knows (`Duration`, `MiB`, `1 s`, `m/s`, …)
admit. A quantity state in a model must have a matching state/time
rate. Unknown units and unit clashes are named refusals
(`E-UNIT-104`, `E-UNIT-105`, `E-UNIT-101`).

Numeric models:

- omitted `numeric:` means `strict-f64`
- `numeric interval-f64` is accepted as a label
- the machine still computes in `Float64`
- writing `Real` without a model is `E-NUM-004`

Not admitted as compute types yet: `Rat`, bare `Real`,
`Option`, `Result`, `Graph`, `Field`, records as values, refinement
predicates such as `NonNegative<Real>`.

`Complex` is admitted as a type (e.g., `Complex`, `Vector<Complex, [2]>`).
Complex literals use the `Ni` suffix (e.g., `2i`, `3.5i`) which desugars
to `N * i` where `i` is the imaginary unit. The identifier `i` is a
named constant (not a reserved keyword) — it is recognized only when not
shadowed by an input or definition. Complex arithmetic (add, sub, mul,
div, neg, eq, ne) is fully supported in the interpreter via a native
`Complex { re, im }` value type.

`Mod<p>` and `GF<p>` are admitted as `Int` types (e.g., `Mod<7>`, `GF<2>`).
Values are exact i64 integers; modular reduction is an operational concern
handled by the builtins (`mod`, `mod_inv`, `cong`), not by the type system.

### Modular arithmetic builtins

| Builtin | Arity | Returns | Description |
|---------|-------|---------|-------------|
| `mod(a, m)` | 2 | Float64 | `a % m` (floating-point remainder) |
| `factorial(n)` | 1 | Int | `n!` as exact i64 (n must be in [0, 20]) |
| `mod_inv(a, m)` | 2 | Int | Modular inverse of `a` mod `m` via extended GCD; errors if `gcd(a, m) != 1` |
| `cong(a, b, m)` | 3 | Bool | Congruence check: `(a - b) mod m == 0` |
