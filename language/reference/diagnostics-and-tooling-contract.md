# Chapter 12: Diagnostics and Tooling Contract

## Diagnostic structure

```text
stable code
severity
primary span
message
related spans
constraint/provenance trace
fix suggestions
help/reference link
machine fields
```

Implemented today: `emath explain E-LAW-001` prints a Cayley-table witness from the finite law checker. The explanation is schema `emath.diagnostic.explanation v1` and must pass `tutor-check/v1` (no synthesized green claim without a receipt). `E-UNIT-101` carries structured pedagogy: understood, unknown, why, smallest repair, alternatives, example, deeper concept, authority consequence, library link.

## Taxonomy

```text
E-SYN syntax/layout
E-PKG package/resolution
E-NAME name/visibility
E-KIND schema/lowering
E-TYPE type/refinement
E-UNIT units/dimensions
E-SHAPE shapes/layout
E-SYM symbolic algebra
E-DOM domain/branch
E-CTOR constructor/invariant
E-GOAL request/planning
E-PROV provider/adapter
E-EVID evidence/certificate
E-CODEGEN backend/artifact
E-RES resource/cancellation
```

Law admission uses `E-LAW-002` for missing or empty required law metadata.
Finite categorical admission uses `E-KIND-027` for malformed schemas or
references and `E-LAW-003` when exhaustive checking finds a concrete law or
morphism-preservation counterexample.
Native symbolic admission uses `E-SYM-001` through `E-SYM-004` for malformed
rules, bounded exact-arithmetic failures, unsupported claims, and false
authority labels.
`E-EVID-115` refuses evidence levels outside `E0` through `E5`.
`E-PKG-052` refuses unresolved curated law-package imports.
`E-PKG-053` refuses an unknown symbol or alias in the embedded law-package
slice.
`E-SYN-152` refuses malformed binding provenance, including unknown keys,
kinds, and fields not admitted by the selected closed variant.
`E-NAME-028` refuses provenance attached to an unknown binding.

Codes are not reused for different meanings.

## Error recovery

IDE parsing can recover and continue; build semantic admission fails if an error affects the requested artifact. Warnings are policy-upgradable to errors.

## Formatter

The formatter is idempotent, edition-aware and preserves comments. Formatting does not require providers or execute user code.

## LSP

The LSP exposes typed hover, go-to-definition, semantic references, diagnostics, code actions, goal/provider inspection, plan preview, evidence links and generated-Rust/source-map navigation.

## CLI

Core commands include `new`, `fmt`, `check`, `expand`, `explain`, `plan`, `build`, `run`, `test`, `bench`, `verify`, `inspect`, `diff`, `doctor`, `vendor` and fork/provider tooling.

`emath expand` prints the contracted form of L0/L1 scratch and L2 named shorthand (inferred defaults, labeled solve candidates, durable hole objects). `emath exactness [--raise units]`, `emath freeze`, `emath why inference:N`, and `emath assumptions` expose the meaning budget. `emath freeze` always emits schema `emath.freeze.lock.v1` (stdout marker, `--json` `lock` field, and a sidecar next to `--out`). Hidden desugaring is `E-SYN-144`; mixing scratch with an `emath` header is `E-SYN-141`; conflicting example types are `E-SYN-142`; a bodyless L2 name is `E-SYN-143`; a non-expression scratch line is `E-SYN-145`; unlabeled solve defaults that hide alternatives are `E-SYN-146`; claiming exactness with open holes is `E-SYN-147`; an unknown intent verb is `E-SYN-148`; an L2 explicit signature that does not match the inferred body is `E-SYN-149`; an L2 unknown callee that would need a hole to have a domain is `E-SYN-150`; an unlabeled unique numeric `solve` root is `E-SYN-151`. Refusals carry understood / missing / smallest fix / library help.

`emath explain <file> --json` emits `PlanInspection::to_json` under schema `emath.plan-explanation v1` (policy, candidates, exclusions, selected plan, checks, budget, artifact class). `emath explain E-LAW-001` uses `emath.diagnostic.explanation v1` and must pass `tutor-check/v1`. Every finite-checker law refutation carries a `RenderedWitness` with the exact counterexample tuple.

`emath explain <file> --provenance` renders a deterministic binding-to-root
provenance DAG. With `--json`, the schema is
`emath.provenance-explanation.v1`. `Assumed` and `Unstated` are printed
without authority promotion.
