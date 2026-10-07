# emath-schema

## Purpose and layer

Language-owned records at the foundation layer: the section schemas of the
three core declaration kinds (`object`, `function`, `query`) and Feature
Capsule records. `emath-ir` re-exports `kind_schema` and `capsule_record`;
the Language Image compiler in `emath-exec-ir` parses capsules here, and
`cargo xtask generate-language` rewrites their semantic hashes.

## Public types and semantics

- `kind_schema`: `KindSchema`, `SectionSchema`, `RepeatPolicy`,
  `PayloadPolicy`, `CoreKind`, `payload_allows`, and the constructors
  `core_function_schema`, `core_object_schema`, `core_query_schema`.
- `capsule_record`: `FeatureCapsule`, `FeatureClass`, `Maturity`,
  `CapsuleSlot`, `CapsuleEdge`, `CapsuleProjection`, `ProjectionDisposition`,
  `LegacyCellMapping`, `CapsuleRecordError`, `FEATURE_CAPSULE_SCHEMA`.
- `feature_capsule`: `capsule_documents`, `parse_feature_capsule`,
  `validate_capsule`, `validate_maturity_transition`,
  `capsule_semantic_hash`, `rewrite_declared_semantic_hashes`,
  `parse_capsule_slot`, `parse_projection_disposition`, `CapsuleIssue`,
  `ClassRule`, `CLASS_RULES`, `CAPSULE_EDGE_KINDS`, `SemanticHashRewrite`.

## Invariants

- Sections follow required/optional/repeatable policies with payload
  policies, defaults and predicates. An absent section schema means an
  unknown section; the caller refuses.
- Only `object`, `function` and `query` have kind schemas. `model`,
  `policy` and `kind` are not core kinds.
- A capsule's semantic hash covers its semantic fields, including raw
  reference-default text. Hash rewriting never changes a capsule body.

## Error model

`CapsuleIssue` and `CapsuleRecordError` for capsules. Kind-schema
violations are reported by the admitting caller through
`emath_core::Diagnostics`.

## Determinism class

Deterministic. Parsing, validation and hashing are pure functions of the
input text.

## Cancellation behavior

Not applicable. Std-only synchronous crate.

## Unsafe boundary

None. `#![forbid(unsafe_code)]`.

## Feature flags

None.

## Conformance tests

`tests/emath-schema`: the `*_dumps_absent` suites pin that catalog and
domain-kind dumps stay out of the shipped language image.

## No-claim boundaries

User-defined declaration kinds are not supported.

## Authored reference defaults

Feature capsules retain optional `reference_defaults` with required
`reference_body` and `reference_params`. `reference_signature` is
inferred from the body when omitted.
The Language Image compiler checks default terms against the preceding
parameters and the declared arity range. Raw default text participates in
the capsule semantic hash, as other semantic fields do.
