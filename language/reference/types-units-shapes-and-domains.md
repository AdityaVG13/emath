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

`Real` is the mathematical concept of real numbers. As a compute type,
use `Float64` - `Real` is no longer an admitted type alias.

## Refinement types

```emath
type Probability = Float64 where 0 <= self and self <= 1
type Positive<T> = T where self > 0
```

Refinements may be established statically, by constructor checks, by provider certificates or remain caller obligations.

## Domain annotations (U5)

A domain annotation constrains a numeric input to a bounded interval:

```emath
emath function clamp01(x: Float64 in [0.0, 1.0]) -> Float64:
    definitions:
        f = x * (1.0 - x)
```

Syntax: `Type in [lo, hi]` where `lo` and `hi` are numeric expressions
(typically literals). The `in` keyword is disambiguated by the following
token: `in [` is a domain annotation, `in <identifier>` is a unit
annotation, `in <range>` in binder position is a binder variable.

Domain annotations map to `TypeNode::Refinement` with a predicate
encoding the bounds (`domain[lo,hi]`). They participate in type
identity: two declarations that differ only in domain bounds are
distinct declarations.

Domain annotations require a scalar numeric base type (`Float64`,
`Nat`, `Int`, or an existing refinement). A domain annotation on a
non-numeric type (e.g., `Bool`) is refused with `E-TYPE-001`.

## Units and dimensions

```emath
unit Token = base "token"
unit TokenRate = Token / s
unit DollarPerMillionToken = USD / (1_000_000 Token)
```

A quantity has dimension, scale, offset rules and representation. Affine units such as absolute temperature cannot be treated as ordinary multiplicative units.

### Compound-unit bracket syntax (F7/U4)

A simple unit attaches directly to a numeric literal:

```emath
9.81 m          # simple unit (meters)
1.0 s           # simple unit (seconds)
```

A compound unit uses bracket notation with the `unit` contextual keyword:

```emath
9.81 [unit m/s^2]       # acceleration (m s^-2)
100.0 [unit kg*m^2/s^2] # energy (joules)
1.0 [unit m/(s*s)]      # acceleration, parenthesized denominator
```

The `unit` keyword inside brackets disambiguates from indexing.
Without it, `[m]` after a numeric literal is not a unit bracket (C3 fix).

Unit expressions are left-associative for `*` and `/`:

```emath
1.0 [unit m/s*s]        # left-assoc: ((m/s)*s) = dimension length, NOT acceleration
1.0 [unit m/s^2]        # acceleration: m^1 * s^-2
```

This is the C2 trap: `m/s*s` and `m/s^2` have different dimensions.
Use parentheses in denominators to avoid the trap: `m/(s*s)`.

## Shapes

```emath
shape Hidden = [Batch, Sequence, Width]
input: Tensor<Float32, Hidden>
```

Shape constraints include equality, broadcasting, rank, extent arithmetic and layout. Unknown extents remain symbolic when supported.

## Generic arguments at use sites (C10)

Generic arguments instantiate a parameterized type at a use site. The
grammar admits three kinds of argument:

- **Type argument** - a type expression: `Vector<Float64>`, `NonNegative<Real>`
- **Value argument** - a literal or expression: `Mod<7>`, `Tensor<Float64, [N, N]>`
- **Named argument** - `name = expression`: `GF<2, 3, modulus = x + 1>`

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
function spaces) will arrive with the domain-specific features that
use them.

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
Float64  Bool  Nat  Int  Complex  GF<p>  GF<p>
Vector[n]  Matrix[r, c]  Tensor[…]
quantity / `T in unit` annotations
```

`Nat` and `Int` are indexes and small integer values. Integer add, sub,
mul, and negate that stay in `i64` are exact; overflow is a named
runtime fault, not wrap or a rounded float. Mixed `Int`/`Float64`
arithmetic still widens to `Float64`. Mixed `Int`/`Float64` comparisons
(`==` `!=` `<` `<=` `>` `>=`) are exact: `(2^53+1) == 2^53.0` is false
because the integer is not that float. Same-kind `Float64` comparison
stays IEEE-754, so `-0.0 == 0.0`. When an output is declared `Int`,
a whole finite `Float64` result is converted to exact `i64`. This makes
`product i in 1..=20: i` with `Int` output give the exact factorial, not
a float approximation. A negative constant index is `E-SHAPE-006`.
Runtime negative, fractional, or out-of-range indices are a named fault
(`EvalFault::IndexOutOfBounds` in interp; `Result<_, String>` from
`rust.library`), never wrap, saturate, or panicking `[]`.

Shapes:

- rank-1 / rank-2 literals stay `Vector` / `Matrix`
- rank-3+ literals become `Tensor`
- `v[i]`, `m[i, j]`, `t[i, j, k]` drop rank
- `t[0, :, :]` and other `:` axes keep rank
- tensor add/sub only when extents match, or one side is `1`

Units that the compiler knows (`Duration`, `MiB`, `1 s`, `ms`, `m`,
`km`, `m/s`, `degC`, …) admit. A quantity literal is converted to SI by scale
(and offset, for affine units): `1 km + 1 m` is `1001` metres, `1 s +
1 ms` is `1.001` seconds, `1 MiB / 1 B` is `1048576`, `0 degC` is
`273.15 K`. `1 m * 1 m` is area (`m^2`); `1 m / 1 m` is dimensionless.
Affine points cannot be added to each other or multiplied
(`1 degC + 1 degC` and `(1 degC) * 2` are `E-UNIT-102`); adding a
linear interval is a shift (`0 degC + 1 K` is `1 degC`). A quantity
state in a model must have a matching state/time rate. Unknown units
and unit clashes are named refusals (`E-UNIT-104`, `E-UNIT-105`,
`E-UNIT-101`). Information units never mix with dimensionless SI
(`1 + 1 MiB` is `E-UNIT-101`). `T in unit` annotations are dimension
tags: assigning a duration to a length is `E-TYPE-012` and names
those dimensions (`duration` vs `length`), not an internal dump.
`Float64 in m*m` and `Float64 in m^2` are area; `Float64 in m/s*s` is
length (C2 trap), not acceleration.

Compound-unit bracket syntax (`9.81 [unit m/s^2]`) parses and lowers
to combined dimensions. Unit expressions support `*`, `/`, `^`, and
parenthesized groups. Left-associativity means `m/s*s` has dimension
length, not acceleration (C2 trap).

Numeric models:

- omitted `numeric:` means `strict-f64`
- `numeric interval-f64` is accepted as a label
- the machine still computes in `Float64`
- writing `Real` without a model is `E-NUM-004`
- `strict-f64` libm builtins are IEEE-754 binary64, not a real-number
  domain check: `sqrt(-1)` and `ln(-1)` are NaN, `log(0)` is `-Inf`,
  `pow(0, 0)` / `0^0` are `1`, `atan2(0, 0)` is `+0`. Those domain
  obligations are recorded as assumptions. `mod_inv` is exact i64 and
  named-refuses when `gcd(a, m) != 1` or `m` is not positive. Generated
  Rust uses `f64::from_bits` for folded NaN/Inf so the crate compiles.

Not admitted as compute types yet: `Rat`, bare `Real`,
`Option`, `Result`, `Graph`, `Field`, records as values, refinement
predicates such as `NonNegative<Float64>`.

`Complex` is admitted as a type (e.g., `Complex`, `Vector<Complex, [2]>`).
Complex literals use the `Ni` suffix (e.g., `2i`, `3.5i`) which desugars
to `N * i` where `i` is the imaginary unit. The identifier `i` is a
named constant (not a reserved keyword) - it is recognized only when not
shadowed by an input or definition. Complex arithmetic (add, sub, mul,
div, neg, eq, ne) is fully supported in the interpreter via a native
`Complex { re, im }` value type. Principal `sqrt`, `ln`/`log`, `exp`,
`log2`, `log10`, `recip`, and `abs` (modulus) are admitted on Complex:
`sqrt(-1 + 0i) = i`, `ln(-1 + 0i) = iπ`. Float64 `sqrt(-1)` remains IEEE
NaN.

`GF<p>` and `GF<p>` are admitted as `Int` types (e.g., `Mod<7>`, `GF<2>`).
Values are exact i64 integers; modular reduction is an operational concern
handled by the builtins (`mod`, `mod_inv`, `congruence`), not by the type system.

### Modular arithmetic builtins

| Builtin | Arity | Returns | Description |
|---------|-------|---------|-------------|
| `mod(a, m)` | 2 | Float64 | `a % m` (floating-point remainder) |
| `factorial(n)` | 1 | Int | `n!` as exact i64 (n must be in [0, 20]) |
| `mod_inv(a, m)` | 2 | Int | Modular inverse of `a` mod `m` via extended GCD; errors if `gcd(a, m) != 1` |
| `congruence(a, b, m)` | 3 | Bool | Congruence check: `(a - b) mod m == 0` |
