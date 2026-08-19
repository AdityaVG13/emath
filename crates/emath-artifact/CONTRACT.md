# CONTRACT.md

## Purpose and layer

Artifact emission: deterministic JSON writers for the four durable schemas, staging and atomic publish with content-identity verification, and an independent checker that never calls generator internals. Layer: `core/schema` (per CRATE_MAP.md).

## Public types and semantics

Frequently re-exported types (not exhaustive):

- `ArtifactManifest`, `ArtifactClass` (`emath.artifact`, manifest v1 via `ARTIFACT_MANIFEST_VERSION`): the manifest with files, providers, target, evidence level and linked document ids. `ArtifactClass` is the seven-class total-artifact protocol (native, portfolio, hybrid, parametric, exploration, continuation, diagnostic; `ArtifactClass::ALL`), with stable string tokens that round-trip.
- `required_paths_for_class`: package contents per class — code-bearing classes ship a Cargo crate plus the four metadata documents; exploration and diagnostic artifacts are metadata-only. `required_artifact_paths` stays the native-class alias.
- `SourceMap`, `SourceMapEntry` (`emath.source-map`): byte-range + `source_package` shape.
- `PlanRecord`, `OperationRecord` (`emath.resolution-plan`): provider-free Phase 1 mirror of the GIR plan.
- `EvidenceBundleRecord` (`emath.evidence-bundle`): claims, artifact paths, reproduction steps.
- `StagedFile`, `Staging`: staging with per-file bootstrap content ids and derived artifact id (`stage`).
- `JsonWriter`/`JsonObject`, `JsonValue`, `parse_json_document`, `manifest_files_declared`: the single std-only deterministic JSON writer and its parse-back reader.
- `GeneratedCrateSourceMap`, `GeneratedCrateSourceMapEntry` (`emath.generated-crate-source-map`): world-codegen provenance map, distinct from the durable source map.
- `ArtifactError`: typed failure (variant list below).

## Invariants

- The four durable schemas are emitted by one deterministic std-only JSON writer (two-space indent, `BTreeMap` file order); serde is forbidden.
- `manifest_identity` is the single artifact identity: a deterministic hash of the manifest body excluding self-referential `artifact_id` and the manifest's own content-id entry.
- Publish is atomic: verified pre- and post-write under a temporary sibling directory, renamed into place, never overwriting a verified existing artifact.
- Symlinks, absolute staged paths and `..` traversal are refused; symlinks cannot smuggle files in or out.
- The generated-crate source map must never share an id with the durable artifact source map; other schemas are refused on parse-back (E-EVID-108 class shape refusal).
- Resolution monotonicity: adding providers or enlarging budgets must never destroy an artifact class that was previously reachable (regression pinned in `emath-plan` planner tests).

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

- `lib.rs` `artifact_class_tests`: seven-class token round-trip, per-class package inventories, manifest schema/version pin.
- Workspace-level integration suites live in `tests/emath-artifact` (publish durability, schema lanes, checker identity, battery seed).

## No-claim boundaries

- Artifact identity is the bootstrap FNV-1a fingerprint, not a release cryptographic identity.
- The checker is independent and only recomputes documented identities; no certification power beyond content-identity verification.
