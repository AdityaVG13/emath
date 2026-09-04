# emath Capability Matrix

> Single source of truth for what parses, which worlds run it, and what
> label comes back. Updated with every language change. When this file
> and a reference chapter disagree, the reference is normative.
>
> Router framing ([VISION](../implementation/VISION.md)): nothing is
> diagnosed at the door, and nothing crosses the exit unlabeled. A `no`
> cell in these tables is a routed diagnosis: a stable code naming what
> no in-scope world could compute and the repair or world that completes
> it, never a dead end. Runtime faults surface as `fault` values, not
> crashes.

## Source

| Form | Parses | Admits | Computes |
|------|--------|--------|----------|
| Empty / comment-only / whitespace-only / package-only file | yes (zero declarations) | no (`E-PKG-081`) | no |
| L0 scratch (`2+2`, `plot`/`solve`/`convert` lines) | yes (desugars to `emath function Scratch:`) | yes | yes (definitions evaluate; inspect with `emath expand`) |
| L1 guided relationship (`y = x^2 + 4` plus `example x = 3`) | yes (desugars; conflicting example types `E-SYN-142`) | yes | yes (example `given`) |
| L2 named shorthand (`emath function Name:` body without required sections) | yes (desugars to inferred `inputs:`/`definitions:`; bodyless `E-SYN-143`; conflicting head-args `E-SYN-149`; unknown callee without a hole `E-SYN-150`) | yes | yes |
| Goal-first verbs (`plot`, `solve`, `simulate`, `compile`, `differentiate`, `integrate`, `convert`) | yes (lower to definitions/goals; `solve` emits a labeled candidate menu) | yes | plot/solve/convert/differentiate/integrate compute; simulate records inspectable intent |
| `simplify <target>` goal | yes | yes | native exact scalar structural simplification during planning |
| `emath solve --check` | yes | yes | labeled completions for `solve x^2 = 2` (real ±, complex, modular+modulus hole, symbolic, numeric); never a naked `1.414…` |
| Unlabeled unique numeric `solve` | no (`E-SYN-151`) | no | no |
| Hidden desugar (`# emath:hide-desugar`, `@hide_desugar`, `hide alternatives`) | no (`E-SYN-144` / `E-SYN-146`) | no | no |
| Exactness ledger (`emath exactness`, `--raise units`) | yes | yes | display only |
| Typed hole (`f(x) = ?`) | yes (`Hole`, `N-HOLE-001`) | yes (durable hole object: constraints, labeled candidates, rejections, continuation) | no (not invented) |
| Intent verbs `find` `show` `prove` `compare` `share` `build` | yes (Goal IR) | yes | inspectable intent |
| `emath freeze` / `why` / `assumptions` | yes | yes | freeze writes expanded source plus versioned `emath.freeze.lock.v1`; does not raise evidence |
| `emath explain E-LAW-001` | yes | yes | Cayley witness from the finite checker (`tutor-check/v1`) |
| `emath explain <file> --json` | yes | yes | `PlanInspection::to_json` (`emath.plan-explanation v1`) |
| `emath explain <file> --provenance` | yes | yes | deterministic binding provenance DAG; `--json` schema `emath.provenance-explanation.v1` |
| Stage-2 big-integer modular builtins (`mod_inv`/`field_inv`, `sqrt_mod`, `pow_mod`, `int_rem`, `poly_eval_mod`, `rs_encode` with any `BigInt` operand) | yes | yes (integer literals in (i64, 2^256) admit as big constants; `BigInt` never coerces to Float64/Int and only the six builtins admit it) | yes (exact field arithmetic for |F| < 2^256 — pinned at the Curve25519 prime 2^255−19 by `tests/emath-rt/tests/bigmod.rs` (10 tests) and `tests/emath-exec-ir/tests/bigint_stage2.rs` (8 tests); interpreter and generated Rust share the same kernels via the SOURCE embed; a literal at/beyond 2^256 diagnoses typed) |
| Compiled function-spec probe (`emath build <file> --bin <entrypoint>`) | yes (single-output function entrypoints with bindable inputs: Float64, Int, Nat, BigInt, Vector[Float64], Vector[Int], Vector[Nat]) | yes | yes (`emath build` emits a standalone native binary with the same `--set` contract as `emath eval` — strict parsing, `Value::Display`-mirroring output, typed one-line diagnoses — plus a receipt line: `engine=compiled-probe`, build-time `meaning_id`, FNV-1a `inputs_hash`, typed `world`/`method` `not-applicable-to-function-probes` markers since `--world` does not apply to function files). Interpreter stays the reference semantics; parity battery in `tests/emath-build/tests/probe_parity.rs` (p=29 MCA census: 591136 in 0.34s compiled vs 29.2s interpreted) |
| Claim exact with open holes | no (`E-SYN-147`) | no | no |
| Unknown intent verb | no (`E-SYN-148`) | no | no |
| Scratch mixed with an explicit `emath` declaration | no (`E-SYN-141`) | no | no |
| Scratch line that is not an expression, assignment, example, or intent | no (`E-SYN-145`) | no | no |

## Declaration kinds

| Kind | Parses | Admits | Runs |
|------|--------|--------|------|
| `emath function` | yes | yes | yes (definitions evaluate) |
| `emath capability` (cell) | yes | yes (schema `emath.capability-cell.v1`: closed 10-class taxonomy, required version + migration policy, bounded arity 64; unknown class `E-CELL-001`, missing version `E-CELL-002`, policy-diagnosed mutation `E-CELL-003`, arity `E-CELL-004`, bare-leaf name `E-CELL-005`, pure cell without explicit numeric policy / non-finite logit `E-CELL-006`) | the std cell REGISTRY computes via cell reference semantics in the reference VM; cohort: `std.tensor.softmax` (stable-max, strict-f64; shift invariance, nonnegativity, normalization) + `std.math.{add,mul,sin,exp,sqrt,lt}` (scalar; unguarded-scalar policy: NaN propagates) + `std.tensor.sum` (vector reduction; finite policy: non-finite element diagnoses `E-CELL-006`, never a silent NaN sum); every cell a quoted `emath-term` compiled to generic VM bytecode (`vector-map` / `vector-map-scalar` / `vector-reduce` / generic comparison ops over the closed builtin registry; no per-op VM function; malformed/out-of-vocabulary reference terms diagnose typed at compile time; matmul/RK4 diagnose the missing nucleus typed), and each compiled cell matches its handwritten reference bit-for-bit (dual-path differential, seeded-mutant caught); identity frozen: an identity-affecting numeric-policy mutation diagnoses `E-CELL-003`; never core IR enum variants |
| `emath policy` | yes | yes | yes (stateful objects) |
| `emath model` | yes | yes | yes (`emath simulate` integrates ODEs) |
| `emath kind` | yes | yes (definition: schema validation + registered marker) | yes for function-shaped custom kinds: applications lower through ordinary typed definitions and execute in the reference VM/backend; undefined kinds diagnose `E-KIND-100` |
| imported `theory` / `model` / `morphism` kinds | yes | yes (finite carriers, exhaustive laws and preservation) | compile-time decision procedure |
| `emath family` with `use std.kinds.family` | yes | yes (`ElementwiseUnary<Op>`) | expands to ordinary capability cells |
| `emath custom` | yes | treats as function or diagnoses | no |
| other kinds | yes | diagnoses with named error | no |

## Sections

| Section | Status |
|---------|--------|
| `inputs` `outputs` `state` | admitted |
| `definitions` `equations` `equation` | admitted |
| `algebraic` | admitted (implicit DAE unknowns) |
| `constructors` | admitted |
| `constraints` | admitted (auto penalty method) |
| `invariant` | admitted |
| `goals` | admitted (`evaluate`, `differentiate`, `optimize`) |
| `exports` `tests` `compile` | admitted |
| `about` `evidence` `host` | admitted |
| `provenance` | admitted (closed binding provenance on ordinary declarations; source list on laws) |
| `events` | admitted: event-triggered scheduled firing executes in `emath simulate` (payload condition + action, rising-edge / t0-hold, 40-iteration bisection; ch7) |
| `transitions` | admitted: event-triggered transition dispatch executes in `emath simulate` (`on <Event>:` rules re-assign declared input/state slots on firing; ch7) |
| other | `E-SEC-101` |

## Types

| Type | Parses | Admits | Computes |
|------|--------|--------|----------|
| `Float64` | yes | yes | yes |
| `Bool` | yes | yes | yes |
| `Nat` `Int` | yes | yes | yes (Int → exact i64 output) |
| `Complex` | yes | yes | yes (Complex value type, `i` constant, arithmetic, principal `sqrt`/`ln`/`exp`) |
| `GF<p>` `Field<p>` | yes | yes (exactly one prime-integer-LITERAL modulus `2 ≤ p ≤ i32::MAX`; distinct `FieldPrime{p}`, never silent `Int`; `E-TYPE-010` otherwise) | yes; exact i64; arithmetic is capability-cell data over the universal `int_rem` (`a.rem_euclid(m)`) + `field_inv(a,p)` surface builtins (`field7_add`/`field7_mul`/`field7_inv`); float def in a `Field` output diagnoses `E-TYPE-012` (exactness conformance); m≤0/fractional `int_rem` = typed runtime fault, never a panic |
| `Option<T>` | yes | yes (exactly one arg; recursive; `OptionType<T>`) | yes; `option_some`/`option_none`/`option_is_some`/`option_unwrap_or` from text; total unwrap, nesting, kind-matched carrier defaults (`E-TYPE-012` misuse) |
| `Result<T, E>` | yes | yes (exactly two args; recursive; `Result{ok,error}` node) | yes; `result_ok`/`result_err`/`result_is_ok`/`result_unwrap_or`/`result_error_of` from text; `map` via `if …: … else: …` composition |
| `Graph` | yes | yes (alias for dense `Matrix<Float64>` adjacency; bare only, `Graph<T>` `E-TYPE-010`) | yes; feeds closed compute surface: `reachability`, `bfs_order`, `shortest_distances`, `out_degrees`, `graph_laplacian`, `graph_symmetrize`, `bellman_ford`, `sparse_triplets`, `sparse_from_triplets` |
| `Vector[n]` `Matrix[r,c]` `Tensor[...]` | yes | yes | yes |
| `NonNegative<R>` `Positive<R>` `Probability<R>` | yes | yes | yes |
| `Interval<F>` | yes | yes | yes |
| `Measured<T>` | yes | no (neutral `core::measure` schema/API exists; record-value lowering is separate) | - |
| `T in unit` | yes | yes | yes |
| `Rat` `Rational` | yes | yes (exact rational: i128 num/den, gcd-reduced, `den > 0`; zero denominator diagnoses `E-RAT-001`) | yes; `rat`/`rat_add`/`rat_norm` cells; `+ - * /` over Rat stays exact (integer operands fold in); overflow and zero divisors are runtime faults, never panics; codegen diagnoses Rat with a typed error rather than demoting to f64 |
| bare `Real` | yes | no; NEVER silently `f64`: every type site emits exactly one `E-NUM-004` naming the three sanctioned spellings (`Float64` under the strict-f64 profile, `Interval<Float64>`, or `representation Real => Float64`). The gate is total and shape-independent: fields, shape elements (nested included), `Option`/`Result` arms, `Set`/`Interval`/refinement elements, domain/unit bases, event parameters, constructor params/returns, `observations:` annotations | - |

## Generic arguments at use sites

| Form | Example | Parses |
|------|---------|--------|
| Type only | `Vector<Float64>` | yes |
| Integer literal | `Mod<7>` | yes |
| Bracket-list extent | `Tensor<Float64, [N, N]>` | yes |
| Named argument | `GF<2, 3, modulus = x + 1>` | yes |

## Expressions

| Form | Example | Parses | Computes |
|------|---------|--------|----------|
| Arithmetic | `a + b * c` | yes | yes |
| Comparison | `x >= 1` | yes | yes (Float64 IEEE; mixed Int/Float64 exact, not a 2^53 widen) |
| Logic connectives | `a ==> b`, `a <==> b` | yes | yes |
| Binders (sum/product/integral/forall/exists) | `sum i in 0..n: f(i)` | yes | yes |
| Multi-binder folds | `sum i in 0..n, j in 0..m: f(i, j)` (desugars to nested folds, leftmost outermost; filter binds innermost) | yes | yes |
| Filtered binders (`if` guard) | `sum i in 0..n if i > 0: f(i)` | yes | yes |
| Derivative (autodiff) | `derivative(y) wrt x` | yes | yes |
| Partial derivative | `partial(H) wrt T holding p` | yes | yes (same autodiff path) |
| Total derivative | `total(t) wrt t` / `d(t) wrt t` | yes | yes (same autodiff path) |
| Unicode partial | `∂(T) wrt x holding p` | yes | yes (same autodiff path; holding required) |
| Solve (Newton) | `solve(f) wrt x` | yes | yes |
| Optimize | `minimize(loss) wrt x` | yes | yes (Newton on ∇f = 0) |
| einsum | `einsum("ik,kj->ij", A, B)` | yes | yes |
| Complex literal | `2i`, `3.5i`, `1 + 2i` | yes | yes (Complex arithmetic) |
| Quantity literal | `1 s`, `1 ms`, `1 km`, `3//2 s`, `0 degC` | yes | yes (SI scale; affine `degC` uses offset, cannot add two points) |
| Unit query | `unit of E` / `dimension of E` | yes | yes (compile-time unit comparison: `unit of x == m`, query-to-query, `!=`; derives E's unit through arithmetic; bare `unit of E` as a value stays a named diagnose) |
| Extended unit table (core::units_ext) | `AU` `pc` `ly` `nmi` `mi` `ft`, angle units `rad` `deg` `arcmin` `arcsec` `grad` `turn` (dimensionless by declaration), `degR`, `A` `mol` `C` `V` `J` `Pa` `bar` `eV` `min` `g`, systematic SI prefixes (`nm`, `kPa`, `MJ`, `mK`, …) | yes | yes (exact-by-definition scales; prefix strips resolve the base recursively; `mUSD` keeps the currency diagnosis) |
| Currency/time zone in core | `USD` `EUR` `UTC` … (and behind prefixes) | no (`E-UNIT-CURRENCY-1`) | no (versioned packages, never the nucleus; distinct from the generic `E-UNIT-104` miss) |
| Physics law contracts (core::physics) | `emath law` with quantity-typed fields, undirected relation via `require` + residual | yes | yes (relation machine-checked through quantity types; seeded wrong-output diagnoses `E-UNIT-101` at admission; direction assigned only by a goal) |
| Certified intervals | `interval(lo, hi)`, `intersect(a, b)`, `Interval<Float64>`, interval `+ - * /` | yes | yes (certified bound propagation in the interp world; ill-formed bounds and zero-containing divisors are typed run diagnoses; scalar/interval mix diagnoses as type confusion) |
| Notation declarations | `notation infixl 40 "⊕" => core::math::pow` | yes | yes (glyphs and aliases desugar to calls of the canonical target; non-letter glyphs do not glue to adjacent identifiers (`x⊕y`, `√a`); custom operators bind above the core ladder at precedence ≥ 11) |

## Builtins

| Function | Arity | Computes |
|----------|-------|----------|
| `exp` `ln` `log` `sqrt` `sin` `cos` `tan` `tanh` | 1 | yes |
| `abs` `floor` `ceil` `round` `sign` `log2` `log10` | 1 | yes |
| `sinh` `cosh` `atan` `cbrt` `recip` `fract` `is_finite` `neg` | 1 | yes |
| `norm` `transpose` `length` `mean` | 1 | yes |
| `add` `sub` `mul` `div` | 2 | yes (same as `+` `-` `*` `/`) |
| `min` `max` `atan2` `pow` `mod` `hypot` `dot` | 2 | yes |
| `lerp` `clamp` | 3 | yes |
| `laplacian` `laplacian_neumann` `laplacian_2d` `laplacian_2d_neumann` | 2 | yes |
| `laplacian_dirichlet` | 4 | yes |
| `gradient` `gradient_2d_x` `gradient_2d_y` | 2 | yes |
| `sum` `product` | 1 (reduction) | yes |
| `einsum` | variable (≥2) | yes |
| `factorial` | 1 | yes (i64, n ∈ [0,20]) |
| `mod_inv` | 2 | yes (i64, extended GCD) |
| `congruence` | 3 | yes (Bool) |
| `poly_eval_mod` | 3 | yes (i64, Horner over GF(p)) |
| `rs_encode` | 3 | yes (Vector, RS codeword) |
| `hamming_distance` | 2 | yes (i64, positions where vectors differ) |

## Wave-16 stdlib capability cells

Capability-cell surface data (schema `emath.capability-cell.v1`), not parser
keywords or core IR variants. Contracts live under `language/stdlib/cells/`.

| Cell | Parses | Computes |
|------|--------|----------|
| `std.probability.markov` | yes | yes (capability-cell data; one-step, two-step, and stationary Markov laws; `emath check` + `emath test` green) |
| `std.probability.montecarlo` | yes | yes (capability-cell data; deterministic-seeded mean and second-moment estimates) |
| `std.probability.bayes` | yes | yes (capability-cell data; discrete-grid Bayes posterior, odds update, law of total probability) |
