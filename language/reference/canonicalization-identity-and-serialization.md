# Chapter 11: Canonicalization, Identity and Serialization
## FeatureID constitution

A `FeatureID` is the permanent concept name used by Feature Capsules and
cross-package semantic edges:

```text
<authority>.<class>.<path>
std.capability.math.add
acme-labs.field_pack.tensor.linear_algebra
```

Every segment is NFC-normalized lowercase ASCII. A segment starts with `a`–`z`
and continues with lowercase letters, digits, `_`, or `-`. There is at least one
path segment after the authority and primary class. The class segment must equal
the capsule's primary class. Uppercase, non-ASCII, non-NFC, empty segments, and
numeric suffixes such as `@2` are refused rather than normalized or rewritten.

FeatureIDs are deliberately unversioned. A meaning-breaking change retains the
stable concept name, changes its canonical semantic hash, and names the change
with an explicit typed migration and authority receipt. Aliases such as `+` are
presentation data and never become FeatureIDs.

Legacy FNV/bootstrap identifiers remain usable only in a tagged mapping:

```text
legacy_id=fnv1a64:4a3f78ce09b18d21
feature_id=std.capability.math.add
```

The tag is part of the mapping. Readers reject a bare legacy value instead of
guessing its identity domain.

## Feature hash envelopes

Feature material has three non-substitutable SHA-256 identities:

- **semantic** — meaning-affecting capsule fields and semantic edges;
- **distribution** — exact Language Image/package bytes required to reproduce
  loading;
- **operational** — repository commits, binaries, measurements, filesystem
  paths, timestamps, agents, and receipts.

Canonical envelopes sort unique lowercase `snake_case` field names and
length-frame every name and byte value. Version, edition, major, minor, and
patch fields are forbidden. Operational fields cannot enter a semantic
envelope; semantic fields cannot enter an operational envelope. Domain framing
ensures identical payload bytes hash differently in all three domains.

Presentation-only aliases and operational receipts do not change semantic
identity. Removing or changing a semantic field does. Substituting a
distribution hash where a semantic hash is required is a typed parse failure,
not an implicit conversion.
### Minimal executable vector

The executable conformance vector in
`tests/emath-core/tests/feature_identity.rs` constructs the capsule identity
without introducing the capsule schema owned by the next constitution slice:

```rust
let id = FeatureId::from_str("std.capability.math.add")?;
id.require_class("capability")?;
let meaning = SemanticHash::new(&[
    CanonicalField::new("feature_id", id.as_str().as_bytes())?,
    CanonicalField::new("class", b"capability")?,
    CanonicalField::new("semantics", b"checked-add")?,
])?;
```

The surface alias `+` belongs only to distribution/presentation material. A
meaning change such as replacing checked addition with wrapping addition keeps
`std.capability.math.add`, produces a different `SemanticHash`, and requires an
explicit migration record from the old hash to the new hash. The legacy mapping
above is explicitly labeled **legacy** and does not assert such a meaning
migration.

## Meaning Spine

Feature Capsules and generated resources share the feature/resource projection
of the Mathematical Intent Graph (MIG). It has exactly twelve edge kinds:

| Edge | Direction and use | Cycle policy |
|---|---|---|
| `depends_on` | feature → feature; direct semantic/build prerequisite | forbidden |
| `implements` | feature → `ir://`; neutral IR implementation | forbidden |
| `defines` | feature → feature; source establishes meaning | forbidden |
| `uses` | feature → feature/`ir://`; build input | forbidden |
| `requires_world` | feature → world FeatureID | forbidden |
| `provided_by` | feature → provider FeatureID | forbidden |
| `emits` | feature → artifact FeatureID/`ir://` | allowed |
| `documents` | feature → `doc://` | allowed |
| `conforms_to` | feature → `test://` | allowed |
| `migrates_from` | new feature → prior feature | forbidden; receipt required at publication |
| `replaces` | replacement → prior feature | forbidden; history retained |
| `projects_to` | feature → `ir://`/`doc://` generated view | allowed |

External resources use only `ir://`, `test://`, or `doc://` with normalized
relative paths. Filesystem paths and unknown schemes are not graph endpoints.
Duplicate semantic edges, unresolved targets, illegal endpoint combinations,
and forbidden dependency/migration cycles are typed refusals with witnesses.

Canonical ordering is endpoint then edge-kind then target. Deterministic graph
queries provide direct dependencies, transitive build dependencies, reverse
impact (including generated tables/reference views/conformance), migration
reachability, conformance requirements, and minimum agent context. The latter
contains the capsule, direct prerequisites, owner contract, hazards,
conformance, and migration constraints—not an unrelated repository crawl.

Use an existing edge above; inventing a synonym creates an incompatible graph
and is refused. Before changing a FeatureID, inspect reverse impact and update
every returned generated projection and consumer.

## Language Image and lock

The Language Image is the immutable compilation of authored Feature Capsules
and the Meaning Spine. It extends the existing independently loadable image
partitions rather than introducing a second image system:

- `language.capsules` — canonical FeatureID/class/maturity/semantic-hash rows;
- `language.spine` — canonical typed resources and edges;
- `language.tables` — generated runtime-consumer data;
- `language.sources` — FeatureID → authored `.emath` source;
- `language.authority` — per-feature authority state;
- `language.lock` — stable lock schema, semantic hash, distribution hash, and
  prior image hashes retained for rollback.

`emath.language-image` and `emath.language-lock` are stable schema names. They
have no version, edition, or compatibility-range field. The semantic hash covers
capsule meaning and typed edges. The distribution hash covers exact partition
material needed for loading. Repository commits, binaries, measurements,
filesystem paths, timestamps, and receipts belong only to an optional
operational envelope.

Loading validates every partition and the lock before returning an image. It
refuses an unknown schema/hash domain, duplicate FeatureID, missing source map,
stale lock, changed capsule semantics under an old hash, unresolved graph
resource, or operational metadata in semantic bytes. Generated Rust and
reference pages consume `language.tables` and `language.sources`; they are
projections, never authority and never hand-edited.

To trace a generated entry, locate its FeatureID in `language.tables`, read the
matching `language.sources` row, and edit that authored capsule. Rebuild into a
new distribution hash. Do not replace the previous hash: locks retain it for
migration replay and feature-scoped rollback.

### Independent first-cutover reader

Stable publication also runs a deliberately bounded reader in
`tests/emath-exec-ir/tests/independent_language_reader.rs`. It does not call the
production capsule decoder, Language Image loader, parser, or evaluator. It
independently validates canonical FeatureID/hash/ordering rows, typed edges and
authority for the exact Int/add corpus, uses checked integer addition, and
recognizes Float→Int exactness loss.

Sharing is limited to Rust's standard integer/string operations; the reader
does not reuse product interpretation code. It detects byte reorder, stale or
substituted hashes, changed FeatureIDs/edges/authority, result `999`, overflow
wrapping, and diagnostic drift. Future cutovers extend only their pinned corpus
and expected records; they do not turn this into a second compiler.

## Source canonicalization

Source formatting is not the canonical semantic encoding. A package hash may include canonical source bytes for provenance, while semantic identity is derived from typed IR.

## Semantic identity

Every declaration binds:

```text
language edition
kind identity/version
qualified name
generic parameters
semantic sections after defaults/lowering
types/units/shapes/domains
constructors and invariants
definitions/equations/state
goals/evidence/compile policy where identity-relevant
```

Admitted packages expose a `MeaningID` with wire form
`emath:meaning:v1:<sha256>`. The preimage starts with
`emath.meaning.canonical.v1` and uses length-framed canonical SIR.

MeaningID ignores whitespace, comments, formatting, spans, declaration/local
names, hygienic binder names, notation aliases after admission,
non-authoritative prose, tests, evidence attachments, and host bindings.
It includes admitted expression/type/unit/shape structure, numeric policy,
goal requirements, unresolved-meaning state, and sorted dependency
MeaningIDs. Changing `x * x` to `x * x + 1` changes MeaningID.

MeaningID does not claim arbitrary formulas are mathematically equivalent.
Such claims require an explicit evidenced relation.

Binding provenance is semantic artifact data, so it participates in canonical
package/content identity. Replacing `Citation(reference="doi:a")` with
`Citation(reference="doi:b")` changes the package identity even when the
number and formula are unchanged. Provenance does not enter `MeaningID`:
the same admitted formula retains one mathematical identity while its
source-bearing artifacts remain distinguishable.

The capsule's executable reference slots (`reference_params`,
`reference_signature`, `reference_body`) are semantic material: changing
them changes the semantic hash and the declaration's `MeaningID`, and the
canonical term in `reference_body` is compiled into `language.reference`.
The Feature-Capsule row `reference: "authored"` is different data: a
reference-disposition claim for accounting and the gap radar. It carries
no executable meaning and cannot make a metadata-only capsule run. This
distinction is the executable reference boundary; its execution contract
is documented in the standard-library constitution and the Rust interop
chapter.

## Canonical encoding

The encoding is versioned, length-framed and deterministic. Maps/sets use declared canonical order. Floating values encode exact bit patterns or exact literal forms according to semantic type. NaN normalization policy is explicit.

## Serialization surfaces

- canonical binary for identities and internal artifacts;
- canonical JSON for manifests/evidence interchange;
- canonical `.emath` formatter for source;
- provider-specific transport behind adapters.

## Unknown fields

Envelope formats may retain unknown fields for forwarding only when their identity and authority semantics are defined. Core semantic records reject unknown fields by default.

## Tamper behavior

Readers recompute hashes and validate cross-references. A mismatched child identity, source map, plan or evidence reference fails the artifact's verification; it is not repaired silently.
