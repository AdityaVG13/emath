# CONTRACT.md

## Purpose and layer

Artifact emission: deterministic JSON writers for the four durable schemas, staging and atomic publish with content-identity verification, and an independent checker that never calls generator internals. Layer: `core/schema` (per CRATE_MAP.md).

## Public types and semantics

Frequently re-exported types (not exhaustive):

- `ArtifactManifest`, `ArtifactClass` (`emath.artifact.v1`): the manifest with files, providers, target, evidence level and linked document ids.
- `SourceMap`, `SourceMapEntry` (`emath.source-map.v1`): byte-range + `source_package` shape.
- `PlanRecord`, `OperationRecord` (`emath.resolution-plan.v1`): provider-free Phase 1 mirror of the GIR plan.
- `EvidenceBundleRecord` (`emath.evidence-bundle.v1`): claims, artifact paths, reproduction steps.
- `StagedFile`, `Staging`: staging with per-file bootstrap content ids and derived artifact id (`stage`).
- `JsonWriter`/`JsonObject`, `JsonValue`, `parse_json_document`, `manifest_files_declared`: the single std-only deterministic JSON writer and its parse-back reader.
- `GeneratedCrateSourceMap`, `GeneratedCrateSourceMapEntry` (`emath.generated-crate-source-map.v1`): world-codegen provenance map, distinct from the durable source map.
- `ArtifactError`: typed failure (variant list below).

## Invariants

- The four durable schemas are emitted by one deterministic std-only JSON writer (two-space indent, `BTreeMap` file order); serde is forbidden.
- `manifest_identity` is the single artifact identity: a deterministic hash of the manifest body excluding self-referential `artifact_id` and the manifest's own content-id entry.
- Publish is atomic: verified pre- and post-write under a temporary sibling directory, renamed into place, never overwriting a verified existing artifact.
- Symlinks, absolute staged paths and `..` traversal are refused; symlinks cannot smuggle files in or out.
- The generated-crate source map must never share an id with the durable artifact source map; other schemas are refused on parse-back (E-EVID-108 class shape refusal).

## Error model

`ArtifactError` enum: `MissingRequiredPath`, `UnstagedFile`, `StateDirMissing`, `VerificationMismatch`, `ManifestMalformed`, `InvalidStagedPath`. Typed parse-back of the reader uses `ManifestMalformed` for malformed documents; a corrupted manifest cannot silently disable content-identity verification.

## Determinism class

Deterministic. JSON emission is byte-stable given the same input; artifact identity is a deterministic bootstrap fingerprint over the required paths in fixed order.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface documented.

## Unsafe boundary

None. `#![forbid(unsafe_code)]` at crate root; workspace lint forbids unsafe_code.

## Feature flags

None. Cargo.toml has no `[features]`.

## Conformance tests

None on disk currently. No `tests/` directory and no inline `#[cfg(test)]` module in `lib.rs`.

## No-claim boundaries

- Artifact identity is the bootstrap FNV-1a fingerprint, not a release cryptographic identity.
- The checker is independent and only recomputes documented identities; no certification power beyond content-identity verification.
