# emath V19 — Master Specification

## 1. Identity

Every stable semantic feature has:

```text
FeatureID = <authority>.<feature-class>.<path>@<major>
```

Examples:

```text
std.kind.function@1
std.symbol.math.add@1
std.binder.sum@1
std.type.int@1
std.capability.field.mod_inverse@1
std.theory.monoid@1
std.world.symbolic.free@1
std.artifact.research_state@1
```

Surface names, Rust types, enum variants, and file paths are not language identity.

## 2. Feature Capsule

A Feature Capsule defines:

```text
identity and maturity
dependencies
surface spellings and grammar role
canonical lowering and identity fields
mathematical semantics
type/unit/shape/domain constraints
exactness and effects
world applicability
reference execution or provider-only disposition
artifact consequence
diagnostics and routes
projection closure
conformance closure
compatibility and migration
agent owner, reads, edit classes, hazards, and completion
```

## 3. Meaning Spine

A typed dependency graph over FeatureIDs. It computes load, impact, conformance, migration, and
agent-context closures.

## 4. Stage 0

Permanent bootstrap supports only:

```text
lossless UTF-8 and layout
generic `emath <kind> Name: suite`
generic named sections
use/import
identifiers, paths, literals, lists, records
calls, indexing, field access
local let
registry-driven operators
generic `keyword var in domain if guard: body`
holes and unknown glyph tokens
```

Stage 0 does not know `cipher`, `campaign`, `frontier`, `softmax`, or any scientific domain.

## 5. Language Definition Compiler

`emath-langc` consumes accepted capsules and emits:

```text
Language Image
symbol/operator tables
kind/section validators
lowering programs
type and constraint rules
reference-semantic programs
world/method/provider manifests
diagnostic constructors
formatter and LSP metadata
reference and capability views
conformance manifests
migration tables
agent index and impact graph
```

## 6. Bootstrap fixed point

```text
Stage 0 + constitution → Image 1
Image 1 + same constitution → Image 2

MeaningHash(Image 1) == MeaningHash(Image 2)
```

Canonical byte identity is preferred; canonical semantic identity is mandatory.

## 7. Fifteen projection classes

```text
identity
surface
parse
lowering
static-semantics
reference
worlds
execution
artifact
diagnostics
documentation
tooling
conformance
migration
agent-view
```

A required projection may be `not-applicable` only with a typed reason and checker.

## 8. Orthogonal status

```text
maturity:
    cataloged | proposed | accepted | stable | deprecated | retired

surface:
    absent | parse-only | canonical

semantics:
    absent | diagnostic | admitted

execution:
    none | reference | native | provider | multiple

evidence:
    structural | tested | certified | proved

artifact:
    native | hybrid | parametric | exploration | continuation | diagnostic

world coverage:
    explicit WorldID → disposition
```

“Supported” is not a sufficient status.

## 9. Spec Holes

Every unresolved semantic choice is a named, owned, versioned Spec Hole. Stable publication fails
on a blocking hole. An agent cannot fill it silently.

## 10. Golden semantics

Canonical cases can pin:

```text
source bytes and glyphs
tokens and lossless CST
expanded source
Core AST
typed HIR
FeatureID resolution
world plan
reference result
diagnostic JSON
artifact manifest
semantic hash
migration output
```

Goldens change only through an accepted semantic decision.

## 11. Independent implementations

Internal code may differ. Conformance requires agreement on canonical meaning, observable results,
labels, diagnostics, worlds, artifacts, and migrations.

## 12. Agent-native operation

Every task begins with `AGENT_START.json` or a Task Capsule. Context is generated from the Meaning
Spine; it is not reconstructed from chat history or broad recursive reading.

## 13. Agent accretion

Every completed task emits a Change Receipt with baseline, FeatureIDs, decisions, files, gates,
negative controls, new holes, learned reusable knowledge, cost, and rollback.

## 14. Multi-agent safety

Parallel work is separated by FeatureID and projection. Semantic conflicts are resolved through
identity, conformance, ELP decisions, and explicit human authority, never timestamps.

## 15. Cross-wave realization

```text
V15:
    kinds + capabilities + worlds + methods + artifacts

V16:
    catalog promotion ledger; catalog presence never means support

V17:
    campaign/workload/target/schedule kinds and existing lab artifacts

V18:
    frontier/criterion kinds and research-state artifacts
```

## 16. Agent performance targets

Initial measurement targets:

```text
L0 orientation: <= 600 tokens
leaf-feature orientation: <= 2,500 tokens
ordinary task context: <= 8,000 tokens
unnecessary file reads: <= 10%
silent semantic decisions: 0
orientation to first targeted gate: <= 3 tool calls
receipt completeness: 100%
```

They are calibration targets, not assumed achievements.

## 17. Final law

> **Prose proposes. Feature Capsules define. Language Image distributes. Conformance demonstrates.
> Change Receipts accumulate.**
