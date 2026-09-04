# emath Language

This directory is the user-facing and executable source of truth for the language.
Authored Feature Capsules define factual language meaning; human prose explains
motivation and use; generated views project the locked Language Image.

| Path | Purpose |
|---|---|
| [`QUICKSTART.md`](QUICKSTART.md) | Build and run a first program |
| [`CAPABILITY.md`](CAPABILITY.md) | Current parse, world-coverage, and label matrix |
| [`reference/`](reference/README.md) | Normative syntax and semantics |
| [`grammar/`](grammar/README.md) | Machine-readable surface grammar |
| [`examples/`](examples/README.md) | Runnable teaching programs (indexed)
| [`templates/`](templates/README.md) | Project and declaration scaffolds |
| [`stdlib/`](stdlib/README.md) | Standard-library and provider contracts |
| [`NAMING.md`](NAMING.md) | Naming and diagnostic conventions |
| `spec/` | Authored executable Feature Capsules; edit these to change language facts |
| `conformance/` | Positive, negative, and mutation cases bound to FeatureIDs |
| `generated/` | Locked factual projections; never edit by hand |
| `language.lock` | Active/candidate Language Image and per-feature authority |

## One-path feature authoring

1. Select the exact catalog ID and stable FeatureID.
2. Author the capsule and typed Meaning Spine edges under `spec/`.
3. Add positive, negative, and mutation conformance cases.
4. Regenerate the Language Image, runtime tables, and factual views.
5. Run dual conformance when a legacy implementation exists.
6. Publish a candidate receipt; switch only the named FeatureID after every
   stable-language gate passes.

Generated feature index, diagnostics, provider/world coverage, source links,
and gap radar carry `@generated` headers and a distribution lock. Cataloged and
proposed capsules appear as gaps, never as “computes today.” Editorial chapters
remain manual but every factual claim links to a FeatureID and generated status.

Run `cargo xtask generate-language` after changing an authored capsule. The
command deterministically rebuilds `language.lock` and these files under
`generated/`: `language.image`, `source-map.lock`, `runtime-tables.lock`,
`feature-index.md`, `diagnostics.md`, `coverage.md`, and `gap-radar.md`. They are
one distribution and must be reviewed and checked in together; never edit an
individual projection.

Before `check`, `plan`, `planner`, `build`, `eval`, `sweep`, `run`, `test`, or
another semantic command executes, the CLI locates `language/` from the source
path and then the working-directory ancestry. It rebuilds the distribution from
`spec/` and verifies every checked-in byte, source link, runtime table, reference
view, authority row, and capsule-active hole constraint. Missing or stale files
refuse with `E-LANG-IMAGE` before source admission. For example:

```console
$ emath check language/examples/intro/add-exact.emath --json
```

Worked capsules: [`examples/intro/feature-capsules.emath`](examples/intro/feature-capsules.emath).
Template: [`templates/feature-capsule.emath`](templates/feature-capsule.emath).
Rust owns only generic validation, neutral IR/image/VM, providers, and output
backends; it does not own domain meaning.

## Worked source boundaries

Exact addition starts at
[`spec/capabilities/core/add.emath`](spec/capabilities/core/add.emath), with its
user program in [`examples/intro/add-exact.emath`](examples/intro/add-exact.emath).
The finite sum candidate starts at
[`spec/binders/core/sum.emath`](spec/binders/core/sum.emath), with its user
program in [`examples/intro/sum-first-n.emath`](examples/intro/sum-first-n.emath).
Both capsule files own meaning, identity, edges, conformance, authority target,
and docs facts. Generated tables/views point back to them. Rust owns only the
generic parser, validator, finite fold, exact arithmetic, VM, image, and output
mechanisms.

A language-only addition using those mechanisms changes no Rust source. A
feature needing an optimized Rust binding still defines meaning and authority
in its capsule first. The gate refuses missing IDs/negative or mutation cases,
hidden holes, stale generated material, direct feature-name compiler branches,
and golden updates made only to silence a failing implementation.

## Catalog-to-capsule selection

Start from [`templates/catalog-to-capsule.emath`](templates/catalog-to-capsule.emath).
Every descendant names exact catalog IDs and FeatureIDs, target class, current
status, direct edges, required worlds/theories/instances, reference/provider
disposition, effects/exactness/evidence ceiling, positive/negative/mutation and
migration cases, projections, owner, implementation mechanism, and authority
rollback. Link overlapping Beads/contracts instead of duplicating them.

Classify implementation as capsule-only, `.emath` reference semantics, generic
Rust mechanism, optimized Rust binding, world/provider binding, or
documentation/accounting. A coherent family may share one mechanism only while
each ID remains independently accounted. Never infer category-wide support from
one sample, cite a closed task without contract evidence, promote a heuristic,
or mark a catalog record live before conformance and authority receipts.

## Measurable Rust boundary

Allowed Rust work is generic: Stage-0 forms and limits, FeatureID/canonical
hashing, capsule/image validation, typed graph closures, generic VM and
specialization, World/provider ABI, artifact emission, and optimized kernels
bound through FeatureIDs. Forbidden nucleus growth includes feature-name
parser/sema/backend branches, domain-named stable op variants, handwritten
active tables, provider-native stable-IR types, unsafe generated code, or
optional provider/storage/search stacks becoming mandatory.

The contraction gate measures those boundaries while language breadth grows in
`language/spec/`. Capsule semantic edits must change image/projection hashes;
presentation-only edits preserve meaning identity. A failed boundary check
blocks stable publication and leaves legacy authority available for rollback.

### Final authority and no-claim boundary

For a named mathematical feature, the authored capsule under `language/spec/`
is the source of truth. `language.lock` selects its authority state, and the
locked Language Image plus generated projections are byte-for-byte derived
views. Reference prose explains that source; Rust contracts describe mechanism.
Neither prose, a generated table, a Rust enum name, a kernel implementation, nor
a passing execution may create or upgrade feature authority.

The Rust nucleus may parse bounded Stage-0 forms, validate and load images, move
typed values through neutral IR/graphs/VM control, enforce budgets and faults,
bind verified FeatureIDs to domain-neutral kernels/providers, and emit
artifacts. A kernel returns a value or fault. It may not decide applicability,
world, exactness, evidence, authority, or a result/claim label; those decisions
come from verified capsule/image data. Reusable arithmetic code is therefore an
allowed mechanism, not an independent statement that a named feature exists or
that a mathematical proposition is true.

The current whole-nucleus ratchet, derived from all authored capsule FeatureIDs,
has exact forbidden residue `[feature dispatch: 58, stable math IR variants: 79,
active handwritten registries: 1, kernel claim authority: 0, public semantic
modules: 26]`. Zero is asserted only for kernel claim authority. Every non-zero
category remains migration work; the gate reports each file, line, and subject
and may be ratcheted only downward.

Wave-16 capability cells (finite fields, probability, analysis, geometry)
are indexed in [`stdlib/README.md`](stdlib/README.md); the probability
trio (`std.probability.markov`, `std.probability.montecarlo`,
`std.probability.bayes`) computes today.

When reference prose and grammar differ, the reference is normative.
`CAPABILITY.md` states what each documented form does today and which
worlds can run it.

Start with the quickstart, then the first four examples. Use the
reference for exact syntax, semantics, and diagnostics.

Nothing you write is refused at the door: everything enters the
language, and every answer comes back labeled with what it means
(`exact`, `approximate(±bound)`, `symbolic-only`, `hole-open`, `fault`).
Where a capability cannot compute something yet, the docs say so explicitly:
the response is a routed diagnosis pointing at the world
that can, never a silent guess. The governing doctrine lives in
[`../implementation/VISION.md`](../implementation/VISION.md).
