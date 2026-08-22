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
inputs outputs state definitions equations equation
constructors goals exports tests compile about evidence host
```

Anything else is `E-SEC-101`. `request:` / `requests:` were renamed to
`goals:`.

What you can do with an admitted file:

- `emath check` — does this file make sense in the working subset?
- `emath run` / `emath test` — evaluate definitions and examples
- `emath build` — generate a Rust crate when there is an `evaluate` goal
- `emath simulate` — integrate an admitted `emath model`

The compiler will give you a number, a trajectory, generated Rust, or a
named refusal. It will not tell you that the math is true. That is later
evidence work, not the language.
