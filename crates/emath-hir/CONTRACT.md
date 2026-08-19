# emath-hir

## Purpose and layer

Tier 1 resolved declaration representation. Compiler glue between the syntax
tree and the neutral SIR: collects section families, attributes, generics,
documentation and extension payloads with provenance into a HIR, mounts
scoped notation, and migrates bootstrap-era declarations into the open
framework under its bootstrap schema.

## Public types and semantics

- `OpenDecl`, `OpenField`, `OpenPayload`, `OpenAttr`, `SectionFamily`,
  `SectionManifest`: the open declaration framework.
- `SectionViolation`, `SectionViolationReason`: section admission failures.
- `Hierarchy`, `Spread`: declaration hierarchy and spread structure.
- `NotationContext`, `NotationEntry`, `UseKind`, `NotationIssue`: scoped
  notation state and admission.
- `NotationSet`: mounted notation on a HIR.
- `migrate_declaration`, `MigrationIssue`: bootstrap-era migration entry point
  and its issue type.
- `check_use_arity`, `mount_notation`: notation operations.
- Modules: `migrate`, `notation`, `open`.

## Invariants

- HIR carries provenance for attributes, generics, docs and payloads.
- Scoped notation is mounted onto the HIR before SIR construction.
- Migration produces an open declaration under the bootstrap schema only.
- `requests:` is not a Goals-family alias. Live `request:` / `requests:`
  sections are unknown to the kind schema and refused with `E-KIND-016`
  plus a `goals:` migration hint. Bootstrap `request:` is rewritten to
  `goals:` by `migrate_declaration` (`E-MIGR-002`).

## Error model

Typed issue enums for each stage: `SectionViolation` for section admission,
`NotationIssue` for notation, `MigrationIssue` for migration, alongside stable
diagnostics through `emath_core::Diagnostics`.

## Determinism class

No stronger guarantee documented beyond deterministic, order-stable
collection of sections and notation entries.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. Crate root is `#![forbid(unsafe_code)]`.

## Feature flags

None.

## Conformance tests

- `tests/registry_complete.rs`: integration test present on disk.
- `open.rs`: `requests_section_is_refused_with_goals_migration_hint`.
- `migrate.rs`: `migrate_maps_singular_request_to_goals`.

## No-claim boundaries

Migration path covers the bootstrap framework and its schema only; it makes no
claim over arbitrary upstream or post-bootstrap declaration forms.
