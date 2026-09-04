# emath-core

## Purpose and layer
Tier 0 identity, diagnostics and canonical structural primitives. Provider-free,
std only. This crate does not decide which mathematical features exist, how they
are labelled, or where they apply. Those decisions belong to authored language
capsules and their generated images.

The obsolete static compiler-capability catalog is no longer linked or exported.
`src/capabilities.rs` was deleted on 2026-09-04 with user authorization
(user text: "yeah delete those", referring to the retained-unreferenced
residue list); it was never module-declared and is recoverable from git
history.

## Public types and semantics

- `ContentId`: content identity over bytes, produced by content hashing.
- `SourceId`, `MeaningId`, `EvidenceId`, `ViewId`, `RecipeId`, `ArtifactId`,
  `SnapshotId`, `PackId`: versioned, domain-separated SHA-256 identities.
  Their canonical wire form is `emath:<domain>:v1:<64 lowercase hex digits>`.
  `from_bytes` hashes a domain-framed payload; `FromStr` verifies wire shape.
- `IdentityParseError`: malformed durable identity with expected prefix.
- `FileId`, `QualifiedName`, `SchemaId`: ID and naming primitives for files,
  qualified names and schema identities.
- `FeatureId`: durable, unversioned, opaque dotted identifier. Core validates
  and stores its canonical spelling; it does not assign feature meaning. The
  legacy `authority`, `class`, `path_segments`, and `require_class` accessors
  remain temporarily public only for unmigrated external callers.
- `SemanticHash`, `DistributionHash`, `OperationalHash`: canonical,
  field-sorted SHA-256 envelopes with disjoint domain frames. Semantic envelopes
  reject operational metadata; operational envelopes reject semantic metadata.
- `LegacyIdMapping`: explicit tagged bridge from bootstrap/FNV identifiers to a
  `FeatureId`; an untagged legacy value is never reinterpreted.
- `Span`: source span for diagnostics and tree nodes.
- `Diagnostic`, `Diagnostics`, `Severity`: stable diagnostic envelope with
  code, message, primary span, notes and help (not exhaustive, see modules).
- `SourceParser`: kernel parser seam injected at runtime.
- `SourceFile`, `SourceStore`: source buffer and store types.
- Re-exported helpers: `bootstrap_content_id`, `content_id_of_str`,
  `fnv1a64_bytes`, `register_source_parser`, `source_parser`.
- Generic boundary modules: `diagnostic`, `hash`, `id`, `limits`, `parse`,
  `source`, `span`, `text`, and `tree`.
- Obsolete domain implementations are no longer linked; their dead source
  files (`geometry.rs`, `linprog.rs`, `measure.rs`, `optimization.rs`,
  `signal.rs`, `codata.rs`, `game_theory.rs`) were deleted on 2026-09-04
  with user authorization (never module-declared; recoverable from git
  history). The narrow root
  kernel surface consists of `KernelSpecialFn`, `KernelDomainRefusal`,
  `evaluate_special_kernel`, `kernel_mean`, `kernel_median`, `kernel_quantile`,
  `kernel_variance`, and the generic deterministic seed values `Seed`,
  `StreamPath`, and `local_stream_seed`.
- `sigfigs`, `special`, and `units` remain available only as retained generic
  presentation/representation plumbing re-exported at the crate root (for
  example `count_sig_figs`, `PrecisionLedger`, `Quantity`, `UnitTable`,
  `seed_table`). Their visibility is compatibility residue, not authority for
  admission, labels, or applicability: admission and meaning must come from the
  installed language image.

## Invariants

- Canonical primitives are the shared identity and boundary types for the
  workspace.
- Content IDs are deterministic over bytes via the bootstrap hash.
- Durable IDs hash `prefix || NUL || payload`, preventing equal payloads in
  different identity domains from sharing an identity.
- Feature IDs are opaque canonical keys rather than semantic dispatch values.
  Numeric suffixes and version/edition/semver fields are rejected; exact meaning
  is supplied by authored capsule content and identified by `SemanticHash`.
- Hash envelopes length-frame sorted field names and bytes. Equal payloads in
  semantic, distribution, and operational domains produce different hashes.
- Source types depend only on core identity/diagnostic types.

## Error model

Stable diagnostics through `Diagnostics`: `Diagnostic::error` / `warning`
carry a stable `&'static str` code, message and primary span; notes and help
are chained via `with_note` / `with_help`. No panic on user input.

## Determinism class

Content identity and hashing are deterministic and byte-comparable by design.
Durable IDs use std-only SHA-256 (FIPS 180-4).

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. Crate root is `#![forbid(unsafe_code)]`.

## Feature flags

None.

## Conformance tests

`tests/emath-core/tests/feature_identity.rs` covers FeatureID grammar and class
matching, canonical vectors, semantic mutation, three-domain separation,
metadata contamination refusal, and tagged legacy-mapping round trips. The
remaining 21 host-authority test files (units, sigfigs, numtheory, geometry,
statistics, stochastic, version, and kin) tested domain authority that is now
capsule-mirrored or privatized; they no longer compiled against the contracted
nucleus and were deleted with explicit user authorization on 2026-09-04.

## No-claim boundaries

Content identity and FNV-1a hashing are content-addressing primitives, not a
cryptographic or release identity. No authentication or integrity guarantee.
Durable IDs provide cryptographic content identity, not signatures,
authentication, semantic equivalence, or evidence authority. Historical domain
modules do not authorize a language feature merely because a Rust API exists;
admission and meaning must come from the installed language image.
