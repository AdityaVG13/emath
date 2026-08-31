# Chapter 11: Canonicalization, Identity and Serialization

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

Readers recompute hashes and validate cross-references. A mismatched child identity, source map, plan or evidence reference refuses the artifact; it is not repaired silently.
