# Custom Kinds, Schema and Lowering

## Kind declaration

```emath
emath kind ScoringPolicyKind:
    extends model

    schema:
        require section inputs
        require section outputs
        require section definitions
        allow section state
        allow section constructors
        require exactly_one output

    lower:
        model.inputs = section.inputs
        model.outputs = section.outputs
        model.state = section.state or []
        model.constructors = section.constructors or []
        model.definitions = section.definitions
```

## Schema responsibilities

- section names and payload types;
- required/optional/repeatable rules;
- order/duplicate policy;
- default insertion;
- local validations;
- core semantic fragments produced;
- identity-affecting fields;
- diagnostics and migration.

## Restricted lowering language

The declarative lowering language is total, bounded and cannot perform arbitrary filesystem/network/process actions. It manipulates typed schema values and emits core HIR fragments.

Operations include:

```text
field/section access
map/filter/fold with bounded collections
construct core nodes
validate predicates
emit diagnostic
canonical sort/deduplicate
attach provenance
```

## Recursive expansion

Kind expansion depth and total generated nodes are capped. Recursive kinds require a structurally decreasing schema proof or are refused.

## Native schema plugins

Complex semantics may use a sandboxed plugin under the versioned plugin interface. The plugin returns HIR plus provenance and diagnostics. Its output is revalidated by the core.

## Evolution

Kind version is part of declaration semantic identity. A lowering change that alters meaning requires a version bump and migration. Cosmetic diagnostics/formatting may remain compatible.

## Custom notation

Notation is scoped to imports and maps to existing operators/functions. It cannot change precedence globally without explicit namespace activation. Canonical rendering may use fully qualified core forms.
