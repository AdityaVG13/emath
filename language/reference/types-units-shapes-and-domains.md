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
`f64`: every type site rejects it with exactly one deterministic `E-NUM-004` naming the three
sanctioned spellings — `Float64` (the strict-f64 profile), `Interval<Float64>` (the
certified-interval realization of a real), or a `representation Real => Float64` directive. The
gate is total and shape-independent: input/output/state fields, `Vector`/`Matrix`/`Tensor`
elements (nested shapes included), `Option` and both `Result` arms, `Set`/`Interval`/refinement
elements, `in [lo, hi]` domain and `in unit` bases, event parameters, constructor parameters and
returns, and `observations:` type annotations all refuse identically. Write the shape you mean:
`Float64` for the machine float, `Interval<Float64>` for certified bounds on a real.

Not admitted as general compute values: bare `Real`, records, arbitrary refinement predicates, and continuous measure values. Some standard-library Rust APIs expose additional carriers without adding a `.emath` surface. `Option`, `Result`, `Graph`, and `Field<p>`/`GF<p>` are admitted composite *declaration* types (see "Composite types").

## Scalars and integers

`Nat` and `Int` use exact `i64` arithmetic for addition, subtraction, multiplication, and negation. Overflow is a runtime refusal, never wrapping. Mixed integer and `Float64` arithmetic widens to `Float64`; mixed comparisons preserve the exact integer value.

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
| `engineering` | refused (`E-UNIT-106`) | optional |
| `publication` | refused (`E-UNIT-106`) | required (`E-UNIT-107`) |

A profile may strengthen checks but cannot weaken dimensional admission. Unknown or duplicate profile declarations are refused.

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

Currencies and time zones are socially versioned and refuse in core (`E-UNIT-CURRENCY-1`). Unknown units are `E-UNIT-104`; incompatible dimensions are `E-UNIT-101`.

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

At least two headers are required. Ragged or non-numeric rows are refused.

## Sets and records

Finite set values, comprehensions, membership, and path-prefixed records are admitted:

```emath
small = {1, 2, 3}
even = {x for x in small if mod(x, 2) == 0}
has_two = 2 in small
point = Point { x: 1.0, y: 2.0 }
```

A bare `{name: value}` is ambiguous and refuses with `E-SYN-154`; record literals require their type path.

## Complex values

Complex literals use the `Ni` suffix:

```emath
z = 2.0 + 3.0i
```

The identifier `i` is the imaginary unit unless shadowed. Complex addition, subtraction, multiplication, division, equality, principal roots, logarithms, exponentials, reciprocals, and modulus are supported.

## Exact rationals

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
`rat(1, 1000000000000000007)`) stays exact. `rat(n, 0)` is refused with a
typed diagnostic (`E-RAT-001`) — never a panic, never a silent zero.
Overflow of the `i128` carrier is a runtime refusal, never a wrap. Rat
values execute in the interpreter; the strict Rust backend refuses them
rather than demote an exact value to `Float64`.

## Certified intervals

```emath
bounds = interval(1.0, 2.0)
overlap = intersect(bounds, interval(1.5, 3.0))
```

Bounds must be finite and ordered. Arithmetic encloses the corresponding range. Division by an interval containing zero refuses. Intervals do not silently widen from or to scalars. Interval operations execute in the interpreter; the strict Rust backend refuses them.

## Modular arithmetic

`Mod<p>` and `GF<p>` values use exact integers. Reduction is performed by explicit builtins:

| Builtin | Meaning |
|---|---|
| `factorial(n)` | Exact `n!` for `0 <= n <= 20` |
| `mod(a, m)` | Floating-point remainder |
| `mod_inv(a, m)` | Exact modular inverse; refuses when no inverse exists |
| `field_inv(a, p)` | `a^-1 mod p` over prime `p` — same exact kernel as `mod_inv` |
| `int_rem(a, m)` | Exact Euclidean i64 remainder `a.rem_euclid(m)`; typed fault when `m ≤ 0` or `a` is not a whole integer |
| `congruence(a, b, m)` | Congruence predicate |

## Composite types

`Option`, `Result`, `Graph`, and `Field`/`GF` are executable declaration
types (emath-option-result-graph-field-aj8d). Recognition is recursive:
each generic argument is itself mapped, so nested spellings admit
(`Option<Result<Int, Bool>>`), and semantically distinct spellings map to
distinct type nodes (`Option<Float64> ≠ Option<Int> ≠ Result<Int, Bool>`).
At the term/VM layer these are value-carrying (the interpreter holds
`Option`/`Result` values and the prime-field node carries its modulus).
Conformance counts (emath-option-result-graph-field-aj8d): emath-sema-tests
`option_result_graph_field` = 51, emath-ir-tests `option_result_values` = 36,
emath-rust-backend-tests `lib` = 41.

- **`Option<T>`** — admits with **exactly one** type argument (`E-TYPE-010`
  arity refusal otherwise), lowering to an `OptionType<T>` node; nesting
  descends (`Option<Option<Int>>`). The EXPRESSION surface **computes from
  `.emath` text**: `option_some(v)`, `option_none()`, `option_is_some(o)`,
  `option_unwrap_or(o, default)` (pinned by the `aj8d_text_*` sema tests).
  A payload is a concrete scalar or a nested Option/Result carrier; others
  refuse `E-TYPE-012`. `unwrap_or` is total (value or the injected default
  — no panicking unwrap). A same-kind carrier default (e.g. `option_none`)
  survives for nested extraction; a MISMATCHED carrier kind in the default
  slot refuses typed `E-TYPE-012` (kind-matched).
- **`Result<T, E>`** — admits with **exactly two** type arguments, lowering
  to a `Result { ok, error }` node. Expression surface computes from text:
  `result_ok(v)`, `result_err(v)`, `result_is_ok(r)`, `result_unwrap_or(r,
  default)`, `result_error_of(r)` (Err → `Option::Some(err)`, Ok → none).
  `.emath` `map`: compose the builtins with `if … : … else : …` over the
  declared-carrier predicates (no function-valued args). Kind-mismatches
  and foreign-carrier defaults refuse `E-TYPE-012`.
- **`Graph`** — an **alias** for the dense `Matrix<Float64>` adjacency
  carrier (decision b). The graph ops check shapes, not the type node, so
  a `Graph`-typed field feeds the closed compute surface unchanged:
  `reachability`, `bfs_order`, `shortest_distances`, `out_degrees`,
  `graph_laplacian`, `graph_symmetrize`, `bellman_ford`, `sparse_triplets`,
  `sparse_from_triplets`. Bare `Graph` only; `Graph<T>` is a typed arity
  refusal (`E-TYPE-010`). The alias is **bidirectional**: `Graph` and
  `Matrix<Float64>` are the same carrier node, so a graph value admits into
  a `Matrix<Float64>`-typed field and a `Graph`-typed field feeds any
  matrix-consuming graph op — the two spellings interchange freely.
  `graph { <nodes> ; <edges> }` computes the dense adjacency and the
  reachability/degree/distance kernels run from text (pinned by the
  `aj8d_graph_field_*` and `aj8d_meta_graph_relabel_*` tests).
- **`Field<p>` / `GF<p>`** — one prime-field spelling (GF canonical;
  `Field` the declared alias). The prime is a **type-level constant**: the
  argument must be a single **prime integer literal** `2 ≤ p ≤ i32::MAX`,
  else `E-TYPE-010`. `Field<7>` and `GF<7>` are the same distinct
  `FieldPrime { modulus: 7 }` type (never the silent `Int` collapse; a
  non-prime, non-literal, or overdarge modulus is a typed refusal naming
  the constraint). Values are **exact i64**. Field arithmetic **computes
  from `.emath` text as capability-cell data over the universal `int_rem`,
  the exact-Euclidean i64 remainder `a.rem_euclid(m)`**: e.g. `field7_add`
  `int_rem(a + b, 7)`, `field7_mul` `int_rem(a * b, 7)`, `field7_inv`
  `field_inv(a, 7)` (pinned by the `aj8d_field*` and
  `aj8d_meta_field7_distribution_law` tests). `field_inv`/`mod_inv` and
  `int_rem` are Phase-1 surface builtins; there is **no field-named EmirOp
  or parser branch** — the function NAMES are user data over the generic
  primitive. **Exactness conformance**: a `Field<p>` OUTPUT refuses a float
  definition (`E-TYPE-012` — F64 does not numerically widen into an exact
  integer field type; plain `Int` keeps the legacy F64→Int widening); an
  integer literal or an `int_rem`/`field_inv` result admits (valid exact
  elements).

The refusal family is summarized by **`E-TYPE-010`**: wrong arity (extra
or missing type arguments), non-prime modulus, non-literal modulus
(including `GF<n>` and computed `GF<7+1>`), and modulus outside
`[2, i32::MAX]` all refuse with a message naming the spelling and the exact
constraint — never a silent collapse, never `TypeNode::Int`. Carrier/field
misuse refuses `E-TYPE-012`. `int_rem` with a non-positive modulus (or a
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

Normal, uniform, and Bernoulli sampling use `(parameters, seed, draws[, "stream.path"])`. Omitting the path selects the root stream. Undeclared entropy access and unknown generator algorithms refuse.

## Library-only carriers

The Rust standard-library layer also defines measured datasets, descriptive statistics, discrete signals with declared sampling, and explicit discrete or Lebesgue measures. These do not imply corresponding `.emath` syntax. Consult the contracts under `language/stdlib/cells/` for APIs and refusal codes.

## Graphs and adjacency

A graph is a dense weighted adjacency carrier: `graph { <nodes> ; <edges> }`
desugars to a tuple that evaluates to a square `Float64` matrix (row-major),
one row and column per vertex in declaration order. Nodes are vertex
labels (used in declaration order, so `0..n-1` labels are conventional).
Edges admit the spellings `u --> v` (unweighted, weight 1), `u -[w]-> v`
(`u` to `v` with weight `w`), `u - v` and `u -[w]- v` (bidirectional as two
directed edges). A missing node list, a dangling edge, or a malformed
weight bracket is a parse error — never a graph with fewer vertices.
Weight and endpoint spellings must be finite literal token sequences,
with an optional unary sign: `1`, `-1.0`, and `+2.0` (and nested sign
chains) admit, so `u -[-1.0]-> v` is a negative edge with weight −1.0.
Any non-literal form — a named weight (`-[w]->`), an arithmetic
computed weight (`-[1 + 2]->`), or a signed non-finite literal — is
refused `E-TYPE-012` at admission. Negative weights are first-class in
the kernels and compose through either carrier spelling: the signed
graph literal above, or `sparse_from_triplets` (triplet weights may be
negative; `E-GRAPH-004` refuses non-finite weights). The closed calls
consume both carriers identically.

The call surface (closed set) computes on adjacency carriers:

| Call | Returns | Refusals |
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
refuses the symmetric gate (`E-LINALG-002`) — never a silent
diagonalization. Determinism class: vertices are indices, neighbor scans
ascend, Dijkstra ties break to the lowest index; identical inputs are
bit-identical. Vertex relabeling is a metamorphic symmetry: reachability
masks, distances, and degrees permute with the relabel, and Laplacian
spectra are invariant. See
`language/examples/numerical/graph-router.emath` for a runnable
router.
