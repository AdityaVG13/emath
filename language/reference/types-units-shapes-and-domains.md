# Types, Units, Shapes, and Domains

## Compute types

The admitted compute types are:

```text
Bool  Nat  Int  Float64  Complex  Rat
Interval<Float64>  Mod<p>  GF<p>
Vector<T, [n]>  Matrix<T, [r, c]>  Tensor<T, [...]>
quantities written as T in unit
```

`Real` names the mathematical concept and is not a compute alias. Bare `Real` is never silently
`f64`: every type site routes to exactly one deterministic `E-NUM-004` naming the three
sanctioned spellings; `Float64` (the strict-f64 profile), `Interval<Float64>` (the
certified-interval realization of a real), or a `representation Real => Float64` directive. The
route is total and shape-independent: input/output/state fields, `Vector`/`Matrix`/`Tensor`
elements (nested shapes included), `Option` and both `Result` arms, `Set`/`Interval`/refinement
elements, `in [lo, hi]` domain and `in unit` bases, event parameters, constructor parameters and
returns, and `observations:` type annotations all diagnose identically. Write the shape you mean:
`Float64` for the machine float, `Interval<Float64>` for certified bounds on a real.

Not admitted as general compute values: bare `Real`, records, arbitrary refinement predicates, and continuous measure values. Some standard-library Rust APIs expose additional carriers without adding a `.emath` surface. `Option`, `Result`, `Graph`, and `Field<p>`/`GF<p>` are admitted composite *declaration* types (see "Composite types").

## Scalars and integers

`Nat` and `Int` use exact `i64` arithmetic for addition, subtraction, multiplication, and negation. Overflow is a labeled runtime `fault`, never wrapping. Mixed integer and `Float64` arithmetic widens to `Float64`; mixed comparisons preserve the exact integer value.

A constant negative index is `E-SHAPE-006`. A runtime negative, fractional, or out-of-range index is an evaluation fault, not a panic.

`Float64` follows IEEE-754 binary64. For example, `sqrt(-1)` is NaN and `log(0)` is negative infinity. Domain restrictions must be written as assumptions or checked constraints.

## Measurement literals

Measurements have a central value, standard uncertainty, distribution tag, and provenance.

```emath
m = 1.50 ± 0.02
G = 6.67430(15)e-11
u = 10.0 ± 0.5 ~ uniform
```

Parenthetical digits attach immediately and scale with the final mantissa digits. `f(2)` remains a call and `1.50 (2)` is not a measurement literal. Distribution tags are `normal`, `uniform`, and `lognormal`; the default is `normal`.

Unstated provenance remains visible. Structured binding provenance uses `Exact`, `Citation`, `InstrumentRun`, `Fitted`, `Assumed`, or `Unstated`.

## Units profiles

`@units_profile(level)` declares quantity strictness for one declaration.

| Level | Bare quantity | Provenance |
|---|---|---|
| `permissive` | admitted | optional |
| `lab` | admitted | optional |
| `engineering` | diagnosed (`E-UNIT-106`) | optional |
| `publication` | diagnosed (`E-UNIT-106`) | required (`E-UNIT-107`) |

A profile may strengthen checks but cannot weaken dimensional admission. Unknown or duplicate profile declarations are diagnosed.

## Domain annotations

A bounded scalar domain is written after the type:

```emath
emath function Windowed:
    inputs:
        x: Float64 in [0.0, 1.0]
    definitions:
        y = x * (1.0 - x)
```

The spelling is `Type in [lo, hi]`. Bounds participate in type and semantic identity. The base must be a scalar numeric type; `Bool in [0, 1]` is `E-TYPE-001`.

General `Type where predicate` refinements are not admitted.

## Units and quantities

A simple unit follows the literal. A compound unit uses a bracketed unit expression:

```emath
length = 2.0 m
acceleration = 9.81 [unit m/s^2]
energy = 100.0 [unit kg*m^2/s^2]
```

Unit expressions support `*`, `/`, `^`, and parentheses. Multiplication and division associate left, so `m/s*s` is length, not acceleration. Write `m/(s*s)` or `m/s^2` when that is the intended dimension.

The core table includes SI units and prefixes, time and information units, common astronomical and geodetic lengths, angle units, `L`/`liter`/`litre`, and temperature units `K`, `degC`, `degF`, and `degR`.

Implemented today: the named unit catalog, its identity aliases, and the currency/time-zone refusal list are capsule data under `std.capability.units.catalog` (`language/spec/capabilities/units-catalog.emath`), capsule-active. Rust parses that table and retains only the generic dimension-vector algebra; the dimensional-analysis operations are capsule-bound capabilities: `dimension_compose` (`std.capability.units.dimension-compose`), `dimension_negate`, `dimension_power`, `homogeneity_check`, `dimension_rank`, and `dimensionless_groups` (Buckingham pi basis). Affine scaling between absolute and reference scales is `affine_scale` (`std.capability.units.affine-scale`). Fractional dimension exponents diagnose `E-UNIT-001`.

Currencies and time zones are socially versioned and diagnose in core (`E-UNIT-CURRENCY-1`). Unknown units are `E-UNIT-104`; incompatible dimensions are `E-UNIT-101`.

### Affine units

Absolute temperatures are affine points:

```text
0 degC      = 273.15 K
32 degF     = 273.15 K
22 degC - 10 degC = 12 K
0 degC + 1 K      = 1 degC
```

Two affine points cannot be added, and an affine point cannot be multiplied. Those operations are `E-UNIT-102`.

### Formatting and significant figures

Significant figures are presentation metadata, not uncertainty propagation.

```emath
@significant_figures(display)
emath function Report:
    definitions:
        result = 1.230
```

`@significant_figures(enforce, 3)` records under-reporting as a warning receipt. `emath fmt --value` can round to a declared count or report in a compatible preferred unit. Formatting does not change semantic identity.

Implemented today: the count and rounding policy is capsule-active as `std.capability.precision.sigfig-count` and `std.capability.precision.sigfig-round`, backed by the generic `decimal-significance-count` / `decimal-significance-round` kernels. A literal with no nonzero digit, and a negative count, diagnose `E-PRECISION-001`. Sig-figs never merge with uncertainty: they are different evidence kinds, and mixing them is a warning receipt (`E-SF-MIXED-KINDS`), never a silent coercion.

## Shapes and literals

Nested list literals determine rank:

```emath
v = [1, 2, 3]
m = [[1, 2], [3, 4]]
t = [[[1], [2]], [[3], [4]]]
```

Semicolon rows are equivalent matrix syntax:

```emath
m = [1, 2; 3, 4]
column = [1; 2; 3]
```

Rows must have equal length. Indexing drops rank; `:` keeps the selected axis:

```emath
x = v[0]
y = m[0, 1]
plane = t[0, :, :]
```

Tensor addition and subtraction require matching extents or an extent of `1` on the broadcast side.

A numeric table has named columns and closed rows:

```emath
data = |x y| 1, 2 | 3, 4 |
```

At least two headers are required. Ragged or non-numeric rows are diagnosed.

## Sets and records

Finite set values, membership, and path-prefixed records are admitted:

```emath
small = {1, 2, 3}
has_two = 2 in small
point = Point { x: 1.0, y: 2.0 }
```

Set comprehensions use the binder form over a finite literal integer
range, `{n in 0..100 if is_prime(n)}`; the domain must be a literal
integer range (`E-TYPE-010` otherwise). The `for`-comprehension form
(`{x for x in small if ...}`) does not parse today; it diagnoses with
`E-SYN-102`.

A bare `{name: value}` is ambiguous and diagnoses with `E-SYN-154`; record literals require their type path.

## Complex values

Complex literals use the `Ni` suffix:

```emath
z = 2.0 + 3.0i
```

The identifier `i` is the imaginary unit unless shadowed. Complex addition, subtraction, multiplication, division, equality, principal roots, logarithms, exponentials, reciprocals, and modulus are supported.

## Exact rationals

The authored authority for this surface is
[`spec/capabilities/exact/rational.emath`](../spec/capabilities/exact/rational.emath).
The three active FeatureIDs resolve aliases before dispatching to domain-neutral
machine kernels:

| FeatureID | Surface | Machine signature |
|---|---|---|
| `std.capability.exact.rat` | `rat(n, d)` | `kernel=normalize-ratio;arity=2;inputs=Int,Int;output=Rat;diagnostic=E-RAT-001` |
| `std.capability.exact.rat-add` | `rat_add(a, b)` | `kernel=add-ratios;arity=2;inputs=Rat,Rat;output=Rat;diagnostic=exact-overflow` |
| `std.capability.exact.rat-normalize` | `rat_norm(a)` | `kernel=normalize-ratio;arity=1;inputs=Rat;output=Rat;diagnostic=exact-overflow` |

`Rat` is an exact rational: an `i128` numerator over a positive `i128`
denominator, kept gcd-reduced at every step. Construct with `rat`, combine
with `rat_add`, and canonicalize with `rat_norm`:

```emath
emath function probe:
    outputs:
        c: Rat
    definitions:
        c = rat_add(rat(1, 3), rat(1, 6))
```

`rat_add(1/3, 1/6)` is the exact canonical `1/2`, never a rounded `f64`.
A denominator that would lose precision as `Float64` (for example
`rat(1, 1000000000000000007)`) stays exact. `rat(n, 0)` is diagnosed
(`E-RAT-001`); never a panic, never a silent zero.
Overflow of the `i128` carrier is a labeled runtime fault, never a wrap. Rat
values execute in the interpreter; the strict Rust backend does not
compute them yet (a declared world-capability gap, diagnosed typed, not
a language law)
rather than demote an exact value to `Float64`.

## Certified intervals

```emath
bounds = interval(1.0, 2.0)
overlap = intersect(bounds, interval(1.5, 3.0))
```

Bounds must be finite and ordered. Arithmetic encloses the corresponding range. Division by an interval containing zero is a labeled runtime fault. Intervals do not silently widen from or to scalars. Interval operations execute in the interpreter; the strict Rust backend does not compute them yet (a declared world-capability gap, diagnosed typed).

## Modular arithmetic

`Mod<p>` and `GF<p>` values use exact integers. Authored operation authority
lives in
[`spec/capabilities/exact/number-theory.emath`](../spec/capabilities/exact/number-theory.emath).
The active exact surfaces and domain-neutral machine signatures are:

| FeatureID | Surface and meaning | Machine signature |
|---|---|---|
| `std.capability.exact.factorial` | `factorial(n)`; exact `n!` for `0 <= n <= 20` | `kernel=bounded-product;arity=1;inputs=Int;output=Int;diagnostic=factorial-domain` |
| `std.capability.exact.mod-inverse` | `mod_inv(a, m)`; inverse from extended GCD | `kernel=extended-gcd-inverse;arity=2;inputs=ExactInt,PositiveExactInt;output=ExactInt;diagnostic=noninvertible` |
| `std.capability.exact.field-inverse` | `field_inv(a, p)`; prime-field alias of the same kernel | `kernel=extended-gcd-inverse;arity=2;inputs=ExactInt,PrimeModulus;output=ExactInt;diagnostic=noninvertible` |
| `std.capability.exact.pow-mod` | `pow_mod(b, e, m)`; square-and-multiply | `kernel=modular-power;arity=3;inputs=ExactInt,Nat,PositiveExactInt;output=ExactInt;diagnostic=invalid-modulus-or-exponent` |
| `std.capability.exact.sqrt-mod` | `sqrt_mod(a, p)`; least Tonelli-Shanks root | `kernel=modular-square-root;arity=2;inputs=ExactInt,PrimeModulus;output=ExactInt;diagnostic=nonresidue-or-invalid-modulus` |
| `std.capability.exact.int-rem` | `int_rem(a, m)`; exact Euclidean remainder | `kernel=euclidean-remainder;arity=2;inputs=ExactInt,PositiveExactInt;output=ExactInt;diagnostic=invalid-modulus` |
| `std.capability.exact.congruence` | `congruence(a, b, m)`; Euclidean residue equality | `kernel=euclidean-congruence;arity=3;inputs=ExactInt,ExactInt,PositiveExactInt;output=Bool;diagnostic=invalid-modulus` |
| `std.capability.exact.poly-eval-mod` | `poly_eval_mod(c, x, p)`; ascending coefficients | `kernel=modular-horner;arity=3;inputs=Vector<ExactInt>,ExactInt,PositiveExactInt;output=ExactInt;diagnostic=nonintegral-coefficient-or-invalid-modulus` |
| `std.capability.exact.rs-encode` | `rs_encode(c, n, p)`; evaluate at `0..n` | `kernel=modular-evaluation-sequence;arity=3;inputs=Vector<ExactInt>,Nat,PositiveExactInt;output=Vector<ExactInt>;diagnostic=invalid-length-or-modulus` |

`mod(a, m)` remains floating-point remainder and is not an alias of
`int_rem`. The modular kernels use i128 intermediates on the i64 lane and the
exact big lane for supported widths. `sqrt_mod` returns `min(x, p-x)` and
refuses non-residues; inverse operations refuse non-coprime inputs; all
non-positive modulus and invalid-length cases diagnose rather than panic.

**Stage-2 big-integer lane (emath-t63iz).** The six modular builtins
(`mod_inv`/`field_inv`, `sqrt_mod`, `pow_mod`, `int_rem`,
`poly_eval_mod`, `rs_encode`) are width-overloaded: if ANY operand is a
big integer, the whole call dispatches to the big lane and the result
follows the operands (`BigInt`, or `Vector<BigInt>` for `rs_encode`).
`BigInt` is an exact NON-NEGATIVE field-element type with |F| < 2^256;
it never coerces to `Float64`/`Int`, binary IEEE-style arithmetic on it
diagnoses (`E-TYPE-012`), and only the six builtins admit it. Integer
literals beyond `i64::MAX` (up to 2^256) lower to big constants; a
literal at or beyond 2^256 diagnoses typed at the emitter. Small operands
stay on the i64 lane bit-for-bit; an i64 field element promotes into
the big lane through the same exact-Euclidean reduction the i64 lane
uses. Worked example at the Curve25519 prime:
`language/examples/research/mersenne-field-p25519.emath`.

## Composite types

`Option`, `Result`, `Graph`, and `Field`/`GF` are executable declaration
types. Recognition is recursive:
each generic argument is itself mapped, so nested spellings admit
(`Option<Result<Int, Bool>>`), and semantically distinct spellings map to
distinct type nodes (`Option<Float64> ≠ Option<Int> ≠ Result<Int, Bool>`).
At the term/VM layer these are value-carrying (the interpreter holds
`Option`/`Result` values and the prime-field node carries its modulus).
Conformance counts: emath-sema-tests
`option_result_graph_field` = 51, emath-ir-tests `option_result_values` = 36,
emath-rust-backend-tests `lib` = 41.

- **`Option<T>`**; admits with **exactly one** type argument (`E-TYPE-010`
  arity diagnosis otherwise), lowering to an `OptionType<T>` node; nesting
  descends (`Option<Option<Int>>`). The EXPRESSION surface **computes from
  `.emath` text**: `option_some(v)`, `option_none()`, `option_is_some(o)`,
  `option_unwrap_or(o, default)` (pinned by the option/result semantics tests).
  A payload is a concrete scalar or a nested Option/Result carrier; others
  diagnose with `E-TYPE-012`. `unwrap_or` is total (value or the injected default
 ; no panicking unwrap). A same-kind carrier default (e.g. `option_none`)
  survives for nested extraction; a MISMATCHED carrier kind in the default
  slot diagnoses typed `E-TYPE-012` (kind-matched).
- **`Result<T, E>`**; admits with **exactly two** type arguments, lowering
  to a `Result { ok, error }` node. Expression surface computes from text:
  `result_ok(v)`, `result_err(v)`, `result_is_ok(r)`, `result_unwrap_or(r,
  default)`, `result_error_of(r)` (Err → `Option::Some(err)`, Ok → none).
  `.emath` `map`: compose the builtins with `if … : … else : …` over the
  declared-carrier predicates (no function-valued args). Kind-mismatches
  and foreign-carrier defaults diagnose with `E-TYPE-012`.
- **`Graph`**; an **alias** for the dense `Matrix<Float64>` adjacency
  carrier (decision b). The graph ops check shapes, not the type node, so
  a `Graph`-typed field feeds the closed compute surface unchanged:
  `reachability`, `bfs_order`, `shortest_distances`, `out_degrees`,
  `graph_laplacian`, `graph_symmetrize`, `bellman_ford`, `sparse_triplets`,
  `sparse_from_triplets`. Bare `Graph` only; `Graph<T>` is a typed arity
  diagnosis (`E-TYPE-010`). The alias is **bidirectional**: `Graph` and
  `Matrix<Float64>` are the same carrier node, so a graph value admits into
  a `Matrix<Float64>`-typed field and a `Graph`-typed field feeds any
  matrix-consuming graph op; the two spellings interchange freely.
  `graph { <nodes> ; <edges> }` computes the dense adjacency and the
  reachability/degree/distance kernels run from text (pinned by the graph/field semantics tests).
- **`Field<p>` / `GF<p>`**; one prime-field spelling (GF canonical;
  `Field` the declared alias). The prime is a **type-level constant**: the
  argument must be a single **prime integer literal** `2 ≤ p ≤ i32::MAX`,
  else `E-TYPE-010`. `Field<7>` and `GF<7>` are the same distinct
  `FieldPrime { modulus: 7 }` type (never the silent `Int` collapse; a
  non-prime, non-literal, or overdarge modulus is a typed diagnosis naming
  the constraint). Values are **exact i64**. Field arithmetic **computes
  from `.emath` text as capability-cell data over the universal `int_rem`,
  the exact-Euclidean i64 remainder `a.rem_euclid(m)`**: e.g. `field7_add`
  `int_rem(a + b, 7)`, `field7_mul` `int_rem(a * b, 7)`, `field7_inv`
  `field_inv(a, 7)` (pinned by the Field-arithmetic tests). `field_inv`/`mod_inv` and
  `int_rem` are Phase-1 surface builtins; there is **no field-named EmirOp
  or parser branch**; the function NAMES are user data over the generic
  primitive. **Modular width (stages 1 and 2)**: the number-theory builtins
  (`mod_inv`, `pow_mod`, `sqrt_mod`, `int_rem`, `poly_eval_mod`,
  `rs_encode`) run i128 intermediates (exact for moduli up to 2^63 — a
  naive i64 product overflows past p ≈ 3.04e9 = sqrt(2^63), pinned by
  the width tests at the Mersenne prime 2^61−1) or the stage-2 big
  lane (exact for moduli < 2^256, pinned by the width tests at the
  Curve25519 prime 2^255−19); the lane follows the operands. Integer
  literals in (i64, 2^256) admit as big constants (see the stage-2
  lane note above); a literal at or beyond 2^256 diagnoses typed at the
  emitter (shared by the interpreter and generated Rust). **Exactness conformance**: a `Field<p>` OUTPUT diagnoses a float
  definition (`E-TYPE-012`; F64 does not numerically widen into an exact
  integer field type; plain `Int` keeps the legacy F64→Int widening); an
  integer literal or an `int_rem`/`field_inv` result admits (valid exact
  elements).

The diagnosis family is summarized by **`E-TYPE-010`**: wrong arity (extra
or missing type arguments), non-prime modulus, non-literal modulus
(including `GF<n>` and computed `GF<7+1>`), and modulus outside
`[2, i32::MAX]` all diagnose with a message naming the spelling and the exact
constraint; never a silent collapse, never `TypeNode::Int`. Carrier/field
misuse diagnoses with `E-TYPE-012`. `int_rem` with a non-positive modulus (or a
non-whole `Float64` operand) is a **typed runtime fault, never a panic**:
m ≤ 0 → `modulus must be positive`; a fractional operand → `type confusion`
(`i64_of`/`finite_whole_i64`, interpreter layer).

## Generic arguments

Generic arguments may be types, values, shapes, or named values:

```emath
v: Vector<Float64, [3]>
m: Mod<7>
grid: Tensor<Float64, [N, N]>
field: GF<2, 3, modulus = x + 1>
```

Value and named arguments parse generally. Their semantic admission depends on the declaration kind or standard-library schema that consumes them.

## Numeric profiles

A package may declare `strict-f64`, `interval-f64`, or another supported profile. The profile enters artifact identity. Omitted `numeric:` means `strict-f64`. `interval-f64` is accepted as a profile label; ordinary scalar computation still uses `Float64` unless interval values are constructed explicitly.

## Stochastic values

Randomness is explicit and replayable:

- the seed is supplied by the run or campaign;
- the generator identity is recorded;
- an optional named stream path defines deterministic splits;
- call order does not determine the stream.

Normal, uniform, and Bernoulli sampling use `(parameters, seed, draws[, "stream.path"])`. Omitting the path selects the root stream. Undeclared entropy access and unknown generator algorithms diagnose.

## Library-only carriers

The Rust standard-library layer also defines measured datasets, descriptive statistics, discrete signals with declared sampling, and explicit discrete or Lebesgue measures. These do not imply corresponding `.emath` syntax. Consult the contracts under `language/stdlib/cells/` for APIs and diagnosis codes.

## Graphs and adjacency

A graph is a dense weighted adjacency carrier: `graph { <nodes> ; <edges> }`
desugars to a tuple that evaluates to a square `Float64` matrix (row-major),
one row and column per vertex in declaration order. Nodes are vertex
labels (used in declaration order, so `0..n-1` labels are conventional).
Edges admit the spellings `u --> v` (unweighted, weight 1), `u -[w]-> v`
(`u` to `v` with weight `w`), `u - v` and `u -[w]- v` (bidirectional as two
directed edges). A missing node list, a dangling edge, or a malformed
weight bracket is a parse error; never a graph with fewer vertices.
Weight and endpoint spellings must be finite literal token sequences,
with an optional unary sign: `1`, `-1.0`, and `+2.0` (and nested sign
chains) admit, so `u -[-1.0]-> v` is a negative edge with weight −1.0.
Any non-literal form; a named weight (`-[w]->`), an arithmetic
computed weight (`-[1 + 2]->`), or a signed non-finite literal; is
diagnosed `E-TYPE-012` at admission. Negative weights are first-class in
the kernels and compose through either carrier spelling: the signed
graph literal above, or `sparse_from_triplets` (triplet weights may be
negative; `E-GRAPH-004` diagnoses non-finite weights). The closed calls
consume both carriers identically.

The call surface (closed set) computes on adjacency carriers:

| Call | Returns | Diagnoses |
|---|---|---|
| `reachability(g, s)` | `1.0/0.0` mask of vertices reached from `s` (`s` reaches itself) | `E-GRAPH-001` non-square, `E-GRAPH-003` source, `E-GRAPH-004` non-finite |
| `bfs_order(g, s)` | visit order from `s`, breadth-first, neighbors in ascending index (never depth-first); unreachable vertices absent | same |
| `shortest_distances(g, s)` | Dijkstra distances (`+Inf` unreachable); deterministic ties to the lowest index | plus `E-GRAPH-002` negative edge weight (Dijkstra's precondition) |
| `out_degrees(g)` | count of nonzero entries per row (a self-loop counts; `0.0` is no edge) | `E-GRAPH-001`, `E-GRAPH-004` |
| `graph_laplacian(g)` | `L = D − A`, `D` the out-degree diagonal | plus `E-GRAPH-002` negative entry |
| `graph_symmetrize(g)` | `S = (A + Aᵀ)/2`, the weight-preserving convention (not max, not boolean-or); a user choice, never applied silently | plus `E-GRAPH-002` |
| `bellman_ford(g, s)` | shortest distances with negative edge weights admitted; unreachable `+Inf` | plus `E-GRAPH-005` reachable negative cycle (no answer exists) |
| `sparse_triplets(g)` | COO stream `[u, v, w, ...]` ascending `(u, v)`; explicit `0.0` entries are not edges and are skipped | `E-GRAPH-001`, `E-GRAPH-004` |
| `sparse_from_triplets(n, t)` | dense carrier rebuilt from a triplet stream; duplicate `(u, v)` entries sum (parallel edges add) | `E-GRAPH-003` index, `E-GRAPH-004` non-finite, `E-GRAPH-006` stream length not a multiple of three |

The spectrum composes through the existing symmetric machinery:
`eigvals(graph_laplacian(g))` computes the Laplacian spectrum of an
undirected (or explicitly symmetrized) carrier; a directed carrier
is diagnosed (`E-LINALG-002`); never a silent
diagonalization. Determinism class: vertices are indices, neighbor scans
ascend, Dijkstra ties break to the lowest index; identical inputs are
bit-identical. Vertex relabeling is a metamorphic symmetry: reachability
masks, distances, and degrees permute with the relabel, and Laplacian
spectra are invariant. See
`language/examples/numerical/graph-router.emath` for a runnable
router.
