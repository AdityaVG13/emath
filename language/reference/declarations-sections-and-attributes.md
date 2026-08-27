# Chapter 4: Declarations, Sections and Attributes

## Declaration head

```emath
emath policy AdaptivePolicy<T, N>:
    ...
```

The head contains attributes, declaration form, qualified name/generics and kind, named directly as `emath <kind> <Name<Parameters>>:`. Attribute lines precede the `emath` keyword (`{ attribute }` in the grammar).

L2 named shorthand is the same head without required L3 sections:

```emath
emath function Square:
    y = x^2
    example x = 3
```

That desugars to inferred `inputs:` / `definitions:` / `tests:` on the same declaration IR. A name with no body is `E-SYN-143`, not L0 scratch. An explicit head-arg list whose names do not cover the body's free names is `E-SYN-149` (typed refusal, not a guessed coercion). An unknown callee such as `mystery(x)` with no `mystery = ?` hole is `E-SYN-150`. L0 files omit the `emath` header entirely (`2+2`). `emath expand` prints the contracted form.

## Section model

A section is a named structured payload. Declare only what you need:
`outputs:`, `goals:`, `exports:`, and `compile:` are optional. Definitions
are the surface - an omitted `outputs:` section exposes every definition -
and an omitted `goals:` section evaluates every definition (see spec 08):

```emath
inputs:
    x: Real

goals:
    evaluate <score>:
        produce rust.library
```

Schemas specify payload grammar, multiplicity, ordering semantics, defaults and lowering.

### Imported declaration kinds

`use std.kinds.capability` mounts the standard capability schema. A following
`emath capability Name:` declaration uses ordinary function-shaped
`inputs:`, exactly one `outputs:`, and `definitions:` sections. The name
`capability` is not a lexer keyword and does not add a stable-IR operation
variant. Missing the import is `E-KIND-001`; sections outside the mounted
schema are `E-SYN-101`.

```emath
use std.kinds.capability

emath capability Softmax:
    inputs:
        x: Float64
    outputs:
        probability: Float64
    definitions:
        probability = x
```

`use std.kinds.family` mounts the family generator schema. The implemented
`ElementwiseUnary<Op>` family takes an `instances:` list of at least three
string operation names and expands them into ordinary function-shaped
`capability` declarations:

```emath
use std.kinds.family

emath family ElementwiseUnary<Op>:
    inputs:
        x: Float64
    outputs:
        value: Float64
    definitions:
        value = x
    instances:
        "sin"
        "exp"
        "sqrt"
```

Each generated cell has its own declaration ID, `Float64 -> Float64`
contract, ordinary call expression, and therefore the same evaluator and
Rust projection path as a handwritten cell. Supported instance names are
`abs`, `cos`, `exp`, `ln`, `log10`, `log2`, `recip`, `sin`, `sqrt`, and
`tan`. An unknown family, parameter, member, duplicate, or a list shorter
than the pattern-of-three gate is `E-KIND-026`. Without the schema import,
`emath family` remains an unknown custom kind and refuses.

`std.kinds.theory`, `std.kinds.model`, and `std.kinds.morphism` mount the
finite categorical-algebra schemas. A `theory` declares binary structure and
named laws. A finite `model` supplies a modulus, identity, and the two
coefficients of `a * b = left*a + right*b (mod modulus)`; the compiler checks
every law tuple before admitting it. A `morphism` supplies a scale map and is
admitted only after exhaustive operation-preservation checking. Theory claims
remain `not-run`/E1; successful model and morphism checks produce E2 claims.
The bounded implementation requires `1 <= modulus <= 256`. Schema or reference
errors are `E-KIND-027`; a concrete counterexample is `E-LAW-003`.

### Executable laws

`emath law` is function-shaped sugar for named mathematics. It uses the
ordinary typed definition and execution path, while retaining the assumptions,
domain, provenance, citations, and evidence that bound the claim:

```emath
emath law NewtonSecond:
    assumptions:
        assume: "constant mass in an inertial frame"
        require mass >= 0 kg
    domain:
        name: "classical mechanics"
    provenance:
        source: "Newton, Principia (1687)"
    citations:
        cite: "SI Brochure"
    inputs:
        mass: Float64 in kg
        acceleration: Float64 in m/s^2
    outputs:
        force: Float64 in kg*m/s^2
    definitions:
        force = mass * acceleration
    evidence:
        claim <dimensional_consistency>:
            statement: "kg*m/s^2 is force"
            require dimensional_analysis
            level E2
```

Implemented today: a law admits and runs through the same unit, shape,
definition, goal, example, interpreter, and Rust generation machinery as a
stateless function. `assumptions:`, `domain:`, `provenance:`, `citations:`,
and at least one `evidence:` claim are mandatory. Missing metadata is
`E-LAW-002`; an evidence level outside `E0` through `E5` is `E-EVID-115`.
Assumptions are copied onto each evidence claim, and all law metadata remains
attached to the declaration during evaluation. See
[`newton-second.emath`](../examples/physics/newton-second.emath).

`assumptions:` may additionally contain `require <Bool expression>` over law
inputs. The runner checks these requirements before evaluating definitions; a
false requirement produces a typed refused verdict, so partial formulas such as
a Bayes ratio do not silently evaluate at an invalid denominator.

### Available sections (function)

| Section | Payload | Required | Purpose |
|---------|---------|----------|---------|
| `inputs:` | Fields | Optional | Input parameters with types |
| `outputs:` | Fields | Optional (defaults to definitions) | Named output fields with types |
| `state:` | Fields | Optional | State variables for models |
| `algebraic:` | Fields | Optional | Implicit algebraic unknowns for `emath model` (Newton-solved at each step) |
| `definitions:` | Suite | Yes (or `equations:`) | Named expressions: `name = expr` |
| `equations:` | Suite | Yes (or `definitions:`) | Model equations: `der(state) = rhs`, `name = expr` algebraic definitions, or `lhs == rhs` / bare residual constraints |
| `constructors:` | Suite | Optional | Constructor functions for state |
| `constraints:` | Suite | Optional | Bool expressions fed to optimizer as soft quadratic penalties |
| `invariant:` | Suite | Optional | Claim expressions (limits, series, asymptotic equivalence) admitted as stated truths |
| `tests:` | Suite | Optional | Example cases with `given`/`expect` |
| `goals:` | Commands | Optional | Compilation and provider goals |
| `exports:` | Commands | Optional | What to export from this declaration |
| `compile:` | Commands | Optional (default: rust/library/strict-f64) | Compile target and profile |
| `about:` | Commands | Optional | Prose metadata |
| `evidence:` | Suite | Optional | Evidence claims |
| `provenance:` | Binding sections | Optional | Identity-bearing scientific source for named bindings |
| `host:` | Suite | Optional | Host-language bindings |

`assumptions:`, `domain:`, and `citations:` are law-only.
For a law, `provenance:` remains the required list of `source: "..."`
entries described above. For an ordinary function, policy, or model,
`provenance:` instead attaches a closed provenance value to a declared
input, output, state, algebraic variable, or definition:

```emath
provenance:
    length:
        kind: "Citation"
        reference: "doi:10.1234/example"
        adjustment: "temperature corrected"
    correction:
        kind: "Assumed"
        reason: "small calibration offset"
```

The six kinds are `Exact` (`source`), `Citation` (`reference`, optional
`adjustment`), `InstrumentRun` (`file`, `processing`), `Fitted` (`fit_id`),
`Assumed` (optional `reason`), and `Unstated` (no extra keys). Unknown keys
or malformed payloads are `E-SYN-152`; an unknown binding is `E-NAME-028`.
Nothing is silently retained as untyped prose.

## Attributes

Attributes are scoped metadata with versioned semantics. The surface
grammar is fixed — `attribute = "@" , path , [ "(" , [ argument_list ] , ")" ] , newline`
— and arguments are identifiers, string literals, or bracket lists.

### Implemented today

The front-end admits exactly two item attributes end to end (parse,
format, type-check):

- `@capabilities(experimental-syntax)` — declares the experimental lane
  capability for the file. The capability is **file-scoped**: declaring
  it on any item admits `@experimental` items anywhere in the same
  source file. Unknown capability keys are refused with `E-PKG-065`.
- `@experimental` — marks an item as experimental syntax. It takes no
  arguments (`E-SYN-117`); without the declared `experimental-syntax`
  capability the item is refused with `E-PKG-064`, so experimental
  syntax never compiles silently in a stable package (see
  `elps/README.md`, experimental lane).

Every other attribute is refused with `E-SYN-118` — nothing is silently
dropped. The design vocabulary below (`@deterministic`, `@deprecated`,
`@repr`, `@evidence`, named arguments like `since = "0.3"`) is normative
surface that the front-end does not admit yet; each will land through an
ELP with its own semantics and admission path.

### Design vocabulary (not yet admitted)

```emath
@deterministic
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
