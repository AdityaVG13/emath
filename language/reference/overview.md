# Chapter 1: emath Language Overview

## 1. Design center

The language is designed around a readable declaration envelope:

```emath
emath Kind Name<GenericParameters>:
    section:
        content
```

The declaration head names the kind directly (`emath policy AffineScorer:`,
`emath function Square<T: Real>:`, `emath kind ScoringPolicyKind:`, or a
user kind). `emath custom Name:` declares a custom or not-yet-classified
kind (for example a genesis world). Every spelling lowers to the same
kind-schema-validated declaration model.

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

## Implemented today

This chapter is the design of the whole language. The compiler currently
admits three declaration kinds:

```text
emath function   stateless formulas
emath policy     stateful objects with constructors
emath model      continuous ODEs you can simulate
```

Other kind spellings still parse. Admission then treats them as a
function or refuses their sections. That is a named refusal, not a
silent guess.

Working sections:

```text
inputs outputs state definitions equations equation algebraic
constructors constraints goals exports tests compile about evidence host
```

Anything else is `E-SEC-101`. `request:` / `requests:` were renamed to
`goals:`.

Admitted types:

```text
Float64  Bool  Nat  Int  Complex  GF<p>  GF<p>
Vector[n]  Matrix[r, c]  Tensor[…]
quantity / `T in unit` annotations
NonNegative<Float64> / Positive<Float64> / Probability<Float64>
Interval<Float64>
```

Value-level generic arguments at use sites: `Mod<7>`,
`Tensor<Float64, [N, N]>`, `GF<2, 3, modulus = x + 1>`.

Admitted expressions that compute:

```text
arithmetic  comparison  logic (and or not ==> <==>)
sum product integral forall exists  (binders, with optional `if` guard)
derivative(expr) wrt x         (forward-mode autodiff)
partial(expr) wrt x holding p  (partial derivative, held-fixed set — computes via autodiff)
total(expr) wrt t / d(expr) wrt t  (total/material derivative — computes via autodiff)
∂(expr) wrt x                  (Unicode alias for partial — computes via autodiff)
solve(expr) wrt x              (Newton's method root-finding)
minimize(expr) wrt x / maximize(expr) wrt x  (gradient descent/ascent)
einsum("ik,kj->ij", A, B)      (Einstein summation contraction)
factorial(n)                   (exact i64 factorial, n in [0,20])
mod_inv(a, m)                  (modular inverse via extended GCD)
congruence(a, b, m)                  (congruence test: (a-b) mod m == 0)
poly_eval_mod(coeffs, x, p)    (polynomial evaluation over GF(p), Horner's method)
rs_encode(coeffs, n, p)        (Reed-Solomon codeword: evaluate at 0..n over GF(p))
1 + 2i / 2i / 3.5i             (complex literals, Ni suffix — computes via Complex arithmetic)
unit of E / dimension of E     (compile-time queries, parse only)
```

What you can do with an admitted file:

- `emath check` — does this file make sense in the working subset?
- `emath run` / `emath test` — evaluate definitions and examples
- `emath build` — generate a Rust crate when there is an `evaluate` goal
- `emath simulate` — integrate an admitted `emath model`

The compiler will give you a number, a trajectory, generated Rust, or a
named refusal. It will not tell you that the math is true. That is later
evidence work, not the language.
