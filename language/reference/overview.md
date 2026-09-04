# Chapter 1: emath Language Overview

## 0. Reading order: Real first

A new user meets `Real`; the mathematical concept of a real number;
before any machine type. `Real` is the type named in worked examples and
pedagogical diagnostics; `Float64` (and `Int`, `Nat`, …) are compute
representations a user moves to deliberately. As a compute type, a `Real`
binding maps to `Float64` under the declared numeric profile; it is never
silently reinterpreted (see chapter 5's numeric-profile rules).

## 1. Design center

The language is designed around a readable declaration envelope:

```emath
emath Kind Name<GenericParameters>:
    section:
        content
```

The declaration head names the kind directly (`emath policy AffineScorer:`,
`emath function Square<T: Real>:`, `emath kind ScoringPolicyKind:`, or a
user kind). `emath custom Name:` is the labeled genesis-world lane; strict
custom kinds use a registered `emath kind` schema.

## 2. Language layers

```text
surface syntax
→ kind-schema validated sections
→ declaration HIR
→ provider-independent semantic IR
→ goals and resolution plans
```

The surface language is open through imported kinds and notations, but durable semantics are versioned and explicit.

## 3. Core declaration sections

The complete framework recognizes these core section families:

```text
about, use, generics, capabilities
attributes, types, units, domains, shapes, data
inputs, outputs, state, constants
constructors, functions, definitions, equations, relations
constraints, invariants, objectives
transitions, events
goals, strategies, evidence, budgets
compile, exports, host, tests, benchmarks
extensions
```

A kind schema decides which sections are required, optional, repeatable, mutually exclusive, or custom.

## 4. Meaning versus work

```emath
definitions:
    score = state.scale * x + state.bias

goals:
    evaluate <score>:
        produce rust.library
```

The definition states meaning. The goal asks the compiler to perform work. This separation allows multiple algorithms and evidence levels without rewriting the mathematical definition.

## 5. Values and relations

The language supports both directed definitions and undirected relations:

```emath
definitions:
    area = width * height

equations:
    mass * derivative(velocity) == force
```

An equation is not assigned a direction until semantics or a solver plan establishes one.

## 6. Extensibility

Extensibility is layered:

- packages add types, functions, units, goals and declarations;
- kind schemas add structured declaration forms;
- notations add scoped syntax aliases;
- providers add algorithms and execution capabilities;
- adapters import/export other ecosystems;
- Rust plugins add native performance or host services.

## 7. Output promise

Every admitted request yields a typed artifact disposition. Unsupported execution does not erase the admitted semantic declaration; it can produce a parametric, continuation, exploration or diagnostic artifact according to policy.

## 8. Package editions and replay

`edition = "2026"` in the nearest package `emath.toml` selects the parser
epoch for every file in that package. `2030` is also shipped. A compiler
session opened on a package reads this field before parsing; unknown
values diagnose with `E-PKG-EDITION-UNKNOWN` (route: pick a shipped
edition) rather than selecting a default.

Historical grammar is retained. Thus a 2026 package using a deprecated
form still parses and lowers reproducibly under a newer toolchain, while
the same source under edition 2030 receives the edition-specific hidden
form diagnostic. Edition is provenance, not a semantic meaning parameter.
The first laddered form is the L1 top-level `example x = 3` shorthand:
2026 emits `W-EDITION-DEPRECATED` and migration moves it into a named
`tests:` example; 2030 hides the shorthand with `E-EDITION-HIDDEN`.

## Core language forms

Progressive exactness is one language. L0 scratch (`2+2`, `plot`/`solve`/
`convert`), L1 relationships (`y = x^2 + 4` plus `example x = 3`), L2
named shorthand (`emath function Name:` without required sections), and
goal-first verbs (`plot`, `solve`, `simulate`, `compile`, `differentiate`,
`integrate`) all desugar to the same contracted declaration IR. Inspect
the rewrite with `emath expand`. `solve` without a domain emits a labeled
candidate menu (`emath solve --check`); it does not print a naked numeric
root. The WASM `solve_candidates` operation returns the same menu as an
`emath.world-result` bundle. Applying a candidate rewrites the selected
domain into source and returns a meaning delta; modular and numeric
candidates retain `modulus` and `tolerance` holes until the author supplies
them. Hidden desugaring is `E-SYN-144`.

The admitted declaration kinds are:

```text
emath function   stateless formulas
emath policy     stateful objects with constructors
emath model      continuous ODEs you can simulate
```

Other kind spellings still parse. Nothing is turned away at the door:
an unknown kind enters as labeled data, each section no world computes
comes back as a routed diagnosis (the code names the repair), and the
artifact carries an explicit label, never a silent guess.

Sections:

```text
inputs outputs state definitions equations equation algebraic
constructors constraints invariant goals exports tests compile about evidence provenance host
```

Anything else is `E-SEC-101`. `request:` / `requests:` were renamed to
`goals:`.

Types:

```text
Float64  Bool  Nat  Int  Complex  GF<p>
Vector[n]  Matrix[r, c]  Tensor[…]
quantity / `T in unit` annotations
NonNegative<Float64> / Positive<Float64> / Probability<Float64>
Interval<Float64>
```

Option and Result operations compute in the interpreter over explicit
carriers (`some(x)` / `none`, where none carries
nothing, never a hidden zero) with TOTAL unwraps (`option-unwrap-or` /
`result-unwrap-or`; no panicking unwrap exists), polarity reads
(`option-is-some` / `result-is-ok`), and `result-error-of` yielding
the error as an option (errors compose with the Option ops; Err
payloads are preserved, never swallowed). `Option` and `Result` are not
admitted as general declaration types. Graph algorithms use
`Matrix<Float64>` adjacency carriers; finite fields use exact integers.

Value-level generic arguments at use sites: `Mod<7>`,
`Tensor<Float64, [N, N]>`, `GF<2, 3, modulus = x + 1>`.

Computing expressions:

```text
arithmetic  comparison  logic (and or not ==> <==>)
sum product integral forall exists  (binders, with optional `if` guard)
derivative(expr) wrt x         (forward-mode autodiff)
partial(expr) wrt x holding p  (partial derivative, held-fixed set - computes via autodiff)
total(expr) wrt t / d(expr) wrt t  (total/material derivative - computes via autodiff)
∂(expr) wrt x                  (Unicode alias for partial - computes via autodiff)
solve(expr) wrt x              (Newton's method root-finding)
minimize(expr) wrt x / maximize(expr) wrt x  (Newton on ∇f = 0)
einsum("ik,kj->ij", A, B)      (Einstein summation contraction)
factorial(n)                   (exact i64 factorial, n in [0,20])
mod_inv(a, m)                  (modular inverse via extended GCD; i64 or big lane per operand width)
pow_mod(b, e, m)               (modular exponentiation; i128 intermediates up to 2^63, big lane up to 2^256)
sqrt_mod(a, p)                 (Tonelli-Shanks modular square root; non-residues diagnose typed)
congruence(a, b, m)                  (congruence test: (a-b) mod m == 0)
poly_eval_mod(coeffs, x, p)    (polynomial evaluation over GF(p), Horner's method over i128 intermediates)
rs_encode(coeffs, n, p)        (Reed-Solomon codeword: evaluate at 0..n over GF(p), shared Horner kernel)
hamming_distance(a, b)         (count positions where two vectors differ)
1 + 2i / 2i / 3.5i             (complex literals, Ni suffix - computes via Complex arithmetic)
unit of E / dimension of E     (compile-time unit comparisons - computes; bare query as a value is a named diagnosis)
```

Commands:

- `emath check` - check syntax and semantics
- `emath run` / `emath test` - evaluate definitions and examples
- `emath build` - generate a Rust crate when there is an `evaluate` goal;
  `--bin <entrypoint>` additionally emits a compiled function-spec probe —
  a standalone native binary with the same `--set` CLI contract as
  `emath eval` (same strict parsing and value display, plus a receipt
  line: `engine=compiled-probe`, the build-time `meaning_id`, an
  FNV-1a `inputs_hash`, and typed `world`/`method`
  `not-applicable-to-function-probes` markers, since `--world` does not
  apply to function files). The interpreter stays the
  reference semantics; the compiled path must match it exactly on the
  parity battery workloads (see `tests/emath-build/tests/probe_parity.rs`).
- `emath simulate` - integrate an admitted `emath model`

Simulating a model with `algebraic:` unknowns (an index-1 DAE) carries a
disposition record beside the trajectory; structural index, the
differential/constraint partition, and the consistent-initialization
verdict. When initialization cannot be honored the run exits with
`E-DAE-INIT` and a continuation note (supply the missing algebraic
guess, or regularize); the constraint is never silently dropped and a
partial result is never presented as the DAE solution.

The compiler returns a number, trajectory, generated Rust, or a routed
diagnosis, each labeled (`exact`, `approximate(±bound)`, `symbolic-only`,
`hole-open`, `fault`). It does not claim that the submitted mathematics is
true; truth claims require declared evidence.
