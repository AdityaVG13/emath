# emath-core

## Purpose and layer

Tier 0 identity, diagnostics and canonical primitives. Provider-free, std
only. Defines content identity, file/spans, stable diagnostics, canonical ID
and source types that all downstream crates consume.

## Public types and semantics

- `ContentId`: content identity over bytes, produced by content hashing.
- `SourceId`, `MeaningId`, `EvidenceId`, `ViewId`, `RecipeId`, `ArtifactId`,
  `SnapshotId`, `PackId`: versioned, domain-separated SHA-256 identities.
  Their canonical wire form is `emath:<domain>:v1:<64 lowercase hex digits>`.
  `from_bytes` hashes a domain-framed payload; `FromStr` verifies wire shape.
- `IdentityParseError`: malformed durable identity with expected prefix.
- `FileId`, `QualifiedName`, `SchemaId`: ID and naming primitives for files,
  qualified names and schema identities.
- `Span`: source span for diagnostics and tree nodes.
- `Diagnostic`, `Diagnostics`, `Severity`: stable diagnostic envelope with
  code, message, primary span, notes and help (not exhaustive, see modules).
- `SourceParser`: kernel parser seam injected at runtime.
- `SourceFile`, `SourceStore`: source buffer and store types.
- Re-exported helpers: `bootstrap_content_id`, `content_id_of_str`,
  `fnv1a64_bytes`, `register_source_parser`, `source_parser`.
- `CompilerCapabilities` (`compiler_capabilities()`, schema
  `emath.compiler-capabilities.v1`): self-describing capability report
  (descriptors for sections, numeric models, goals, world classes and
  deferred features).
- Modules: `capabilities`, `diagnostic`, `hash`, `id`, `limits`, `parse`,
  `source`, `span`, `tree`.

## Invariants

- Canonical primitives are the shared identity and boundary types for the
  workspace.
- Content IDs are deterministic over bytes via the bootstrap hash.
- Durable IDs hash `prefix || NUL || payload`, preventing equal payloads in
  different identity domains from sharing an identity.
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

`tests/emath-store/tests/lib.rs` covers every durable domain, one-byte source
mutation, domain separation, and strict wire parsing.

## No-claim boundaries

Content identity and FNV-1a hashing are content-addressing primitives, not a
cryptographic or release identity. No authentication or integrity guarantee.
Durable IDs provide cryptographic content identity, not signatures,
authentication, semantic equivalence, or evidence authority.
