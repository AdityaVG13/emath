# Chapter 4: Declarations, Sections and Attributes

## Declaration head

```emath
@deterministic
emath policy AdaptivePolicy<T, N>:
    ...
```

The head contains attributes, declaration form, qualified name/generics and kind, named directly as `emath <kind> <Name<Parameters>>:`.

## Section model

A section is a named structured payload. Declare only what you need:
`outputs:`, `goals:`, `exports:`, and `compile:` are optional. Definitions
are the surface — an omitted `outputs:` section exposes every definition —
and an omitted `goals:` section evaluates every definition (see spec 08):

```emath
inputs:
    x: Real

goals:
    evaluate <score>:
        produce rust.library
```

Schemas specify payload grammar, multiplicity, ordering semantics, defaults and lowering.

### Available sections (function)

| Section | Payload | Required | Purpose |
|---------|---------|----------|---------|
| `inputs:` | Fields | Optional | Input parameters with types |
| `outputs:` | Fields | Optional (defaults to definitions) | Named output fields with types |
| `state:` | Fields | Optional | State variables for models |
| `definitions:` | Suite | Yes (or `equations:`) | Named expressions: `name = expr` |
| `equations:` | Suite | Yes (or `definitions:`) | Model equations: `der(state) = rhs` |
| `constructors:` | Suite | Optional | Constructor functions for state |
| `constraints:` | Suite | Optional | Bool expressions fed to optimizer as penalties |
| `tests:` | Suite | Optional | Example cases with `given`/`expect` |
| `goals:` | Commands | Optional | Compilation and provider goals |
| `exports:` | Commands | Optional | What to export from this declaration |
| `compile:` | Commands | Optional (default: rust/library/strict-f64) | Compile target and profile |
| `about:` | Commands | Optional | Prose metadata |
| `evidence:` | Suite | Optional | Evidence claims |
| `host:` | Suite | Optional | Host-language bindings |

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
