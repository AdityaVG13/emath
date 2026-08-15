# eMath Language Overview

## 1. Design center

The language is designed around a readable declaration envelope:

```emath
emath custom <Name<GenericParameters>> as Kind:
    section:
        content
```

`custom` means the declaration is constructed using a named kind schema. Built-in shorthand forms may exist later, but they lower to the same declaration model.

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
requests, strategies, evidence, budgets
compile, exports, host, tests, benchmarks
extensions
```

A kind schema decides which sections are required, optional, repeatable, mutually exclusive, or custom.

## 4. Meaning versus work

```emath
definitions:
    score = state.scale * x + state.bias

requests:
    evaluate <score>:
        produce rust.library
```

The definition states meaning. The request asks the compiler to perform work. This separation allows multiple algorithms and evidence levels without rewriting the mathematical definition.

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
