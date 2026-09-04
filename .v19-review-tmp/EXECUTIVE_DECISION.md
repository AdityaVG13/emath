# Executive Decision

## Decision

Adopt a **spec-first, feature-scoped, generated-projection language system**.

Agents do not implement wave prose. They implement accepted Feature Capsules and prove conformance.

## Current versus target order

Current pattern:

```text
implement compiler
→ test
→ update prose
→ update grammar
→ update examples
→ update capability table
```

Target pattern:

```text
register FeatureID or Spec Hole
→ classify authority and identity impact
→ author Feature Capsule and ELP delta
→ author positive, negative, mutation, and migration cases
→ compile capsule into skeleton and Impact Closure
→ implement reference semantics or provider adapter
→ generate projections
→ pass targeted and closure gates
→ transition authority
→ emit Change Receipt
```

## Why no single existing file is enough

- EBNF does not define exactness, evidence, worlds, artifacts, or migration.
- Prose cannot deterministically generate implementation.
- Tests sample behavior but do not define unsampled semantics.
- Rust code is not a stable public language contract.
- A capability matrix collapses several independent status axes.
- Object packs distribute meaning but do not alone define authoring, lowering, and conformance.

The Feature Capsule joins all of them while keeping the compiler modular.

## Success

An independent implementation consumes the same Language Image and Conformance Corpus and produces
the same canonical semantics, labels, diagnostics, world applicability, results, and migrations
without sharing the original Rust implementation.
