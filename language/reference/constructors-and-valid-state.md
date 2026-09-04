# Chapter 6: Constructors and Valid-State Semantics

## Purpose

A constructor is the boundary by which an untrusted tuple of values becomes an admitted instance of a declaration.

```emath
constructors:
    public fn new(scale: Real, bias: Real) -> Result<Self, ConfigError>:
        require scale >= 0

        Self:
            scale = scale
            bias = bias
```

## Constructor stages

1. bind and type parameters;
2. normalize units/representations;
3. check or discharge preconditions;
4. compute derived fields;
5. assign every required field exactly once;
6. establish postconditions/invariants;
7. emit valid instance or declared error;
8. optionally emit construction evidence.

## Constructor forms

- direct value constructor;
- fallible validating constructor;
- factory/fit constructor from data;
- conversion constructor;
- compile-time template constructor;
- resolution-plan constructor selecting providers/representations.

## Delegation

Constructors may call other constructors. Delegation cycles and paths that bypass stronger invariants route to typed diagnoses.

## Visibility and forging

If a public invariant relies on private fields, generated Rust fields remain private. Serialization/deserialization routes re-run admission or carry checked evidence; raw struct construction is not generated publicly.

## Invariant consumption

Constructor facts become assumptions available for specialization and proof only within their scope. For example, `scale >= 0` can justify a branch elimination or monotonicity rule. The compiler records which generated optimization consumed which fact.

## Runtime changes

State transitions must preserve declared invariants or return a structured diagnosis. Mutable host access cannot expose fields in a way that invalidates the model without revalidation.

## Error model

Errors are typed variants with stable semantic reasons, not only strings. Generated Rust may add diagnostic context while retaining machine-readable variants.

## Constructor levels

Three constructor levels are distinguished:

### Value constructors

The `constructors:` section shown above: validates, builds, and emits an admitted
instance inside a declaration (`emath policy`, `emath function`). Deterministic:
the same source produces the same meaning identity.

### World constructors

`emath custom Name:` with a `world constructor <name>:` body declares a named
constructor of a *world*: bounded expansion strategies, protection guarantees,
and one labeled portfolio output.

```emath
emath custom AlienWorld:
    world constructor invent:
        strategies:
            free_symbolic
            finite_table
        protect:
            total
            deterministic
        output: "InterpretationPortfolio"
```

Rules:

- body clauses are exactly `strategies:`, `protect:`, `output:` (others diagnose
  with `E-KIND-027`);
- the declaration is evidence-neutral: it carries one E1/not-run claim with no
  checker and can never mint evidence authority by declaration alone
  (`authority:` body sections diagnose with `E-KIND-027`);
- expansion must be deterministic; a falsifier pins "expansion is
  non-deterministic or mints evidence".

### Artifact constructors

`artifact constructor <name>:` is not admitted and diagnoses with
`E-KIND-001`.

## Declarative world interpretations

`emath world Name:` (with `use std.kinds.world`) declares a named world that
interprets custom/open terms through operator maps:

```emath
use std.kinds.world

emath world Mod17:
    operators:
        "⊕" => core::math::add
        "⊗" => core::math::mul
    interpretations:
        total
        deterministic
    output: "Mod17Interpretation"
```

Rules:

- `operators:` entries are exactly `"glyph" => target` (parsed as
  `operator <glyph>` commands with a path target); anything else diagnoses
  `E-KIND-027`;
- `interpretations:` are untyped guarantee fields (`total`, `deterministic`);
  `protect:` is optional;
- exactly one `output: "Portfolio"` names the interpretation portfolio
  (missing or duplicated diagnoses `E-KIND-003`);
- the world is evidence-neutral: one E1/not-run claim with no checker. A
  world never mints evidence authority by declaration alone;
- the interpretation is world-local: strict source never inherits it. A
  strict use of a world-mapped glyph diagnoses `E-TYPE-003` (unknown name);
  the strict/genesis firewall of the custom lane holds.
