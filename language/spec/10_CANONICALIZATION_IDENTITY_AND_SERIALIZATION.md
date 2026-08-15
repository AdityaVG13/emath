# Canonicalization, Identity and Serialization

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
