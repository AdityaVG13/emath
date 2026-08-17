# Declarations, Sections and Attributes

## Declaration head

```emath
@deterministic
emath policy AdaptivePolicy<T, N>:
    ...
```

The head contains attributes, declaration form, qualified name/generics and kind, named directly as `emath <kind> <Name<Parameters>>:`.

## Section model

A section is a named structured payload:

```emath
inputs:
    x: Real

goals:
    evaluate <score>:
        produce rust.library
```

Schemas specify payload grammar, multiplicity, ordering semantics, defaults and lowering.

## Attributes

Attributes are scoped metadata with versioned semantics:

```emath
@deprecated(since = "0.3", use = NewPolicy)
@repr(rust = "transparent")
@evidence(min = E3)
```

Unknown attributes are rejected unless admitted through a package namespace or explicitly retained as opaque metadata by the kind schema.

## Generics

Generic parameters may range over types, dimensions, shapes, units, domains, constants, providers and capabilities:

```emath
emath function Kernel<T: Real, N: Nat, U: Unit>:
```

Generic constraints enter SIR and generated Rust bounds or runtime checks according to what can be expressed statically.

## Capabilities

Capabilities describe semantic/effect properties, not unverified marketing:

```text
Pure
Deterministic
Differentiable(order = 2)
Stateful
MayAllocate
RequiresNetwork
Certified(kind = interval)
```

A claimed capability may require evidence before publication or provider selection.

## Extension section

Namespaced extensions preserve data not understood by the core:

```emath
extensions:
    my_org::hardware:
        preferred_device: "gpu"
```

An extension declares whether it affects semantic identity, planning only, presentation or evidence.
