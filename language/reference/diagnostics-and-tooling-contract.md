# Diagnostics and Tooling

## Diagnostic structure

A diagnostic has a stable code, severity, primary span, message, related spans, semantic trace, suggested repair, reference link, and machine-readable fields. Codes are not reused for different meanings.

| Prefix | Area |
|---|---|
| `E-SYN` | syntax and layout |
| `E-PKG` | packages and resolution |
| `E-NAME` | names and visibility |
| `E-KIND` | declaration schemas |
| `E-TYPE` | types and refinements |
| `E-UNIT` | units and dimensions |
| `E-SHAPE` | shapes and layout |
| `E-SYM` | symbolic computation |
| `E-DOM` | domains and branches |
| `E-CTOR` | constructors and invariants |
| `E-GOAL` | goals and planning |
| `E-PROV` | providers and adapters |
| `E-EVID` | evidence and certificates |
| `E-CODEGEN` | generated artifacts |
| `E-RES` | resources and cancellation |

`emath explain <code>` renders the structured explanation when available. A checker cannot report success without its required witness or receipt.

## Non-negotiable refusals

The language never:

1. infers units from variable names;
2. treats measurement, fitted, and plain values as the same type;
3. admits approximate equality without an explicit tolerance;
4. resolves a glyph collision by precedence luck;
5. allows a profile to weaken a core correctness check;
6. hides an interpretation, numeric policy, provider, or desugaring choice;
7. inserts implicit coercions that lose exactness;
8. resolves functions by argument type;
9. performs ambient I/O, randomness, network access, or mutation;
10. treats an equation as an imperative assignment;
11. accepts confusable identifiers in one namespace;
12. interprets juxtaposition such as `2x` as multiplication.

These rules are typed refusals and cannot be disabled by a profile.

## Common repairs

- `E-UNIT-101`: declare compatible dimensions or convert explicitly.
- `E-UNIT-104`: correct or import the unit spelling.
- `E-TYPE-012`: use an operator whose carrier and arity match.
- `E-SHAPE-*`: correct rank, extents, or index bounds.
- `E-NAME-022`: rename or remove a duplicate declaration.
- `E-NOTATION-AMBIG`: remove one notation pack or qualify the operation.
- `E-SYN-110`: add the required total `else` or catch-all arm.
- `E-APPROX-TOL`: add `within rtol=..., atol=...`.
- `E-OBS-WRITE`: give the prediction a different name from the observation.
- `E-OBS-HASH`: restore the declared data or update its provenance and identity.
- `E-MIG-AMBIGUOUS-SITE`: choose one of the receipted semantic migration candidates explicitly.
- `E-MIG-RULE-INVALID`: register the rule with a stable ID and explicit respell or semantic proof class.

Diagnostics intended for teaching should state what was understood, what is missing, the smallest repair, and any authority consequence.

## Error recovery

Editor parsing may recover after an error to provide more diagnostics. Semantic admission fails when an error affects the requested artifact. Warnings may be promoted by policy.

A refused construct produces one root refusal, never a cascade. When a section head demands an indented block and the block is empty, the parser refuses that section once (`E-SYN-112`) and continues with the sections that follow; it does not re-refuse the same head or misparse later heads (`E-SYN-101`). A downstream diagnostic that only restates an already-recorded root is suppressed as consequent noise: after `E-NAME-023` ("output `name` has no definition") has fired, later uses of that same output do not also fire `E-TYPE-002` ("unknown variable"). Independent errors elsewhere in the file stay visible.

## Formatter

`emath fmt` is idempotent, edition-aware, and comment-preserving. It does not invoke providers or execute user code. Formatter-backed migrations must preserve semantic identity when classified as presentation-only. Semantic corrections instead record checked before/after `MeaningId` values; ambiguous corrections refuse with candidates.

## CLI inspection

The main inspection commands are:

```text
emath check <file>
emath expand <file>
emath exactness <file>
emath freeze <file>
emath why <file> inference:N
emath assumptions <file>
emath explain <file-or-code>
emath plan <file>
emath run <file>
emath simulate <file>
```

`expand` shows inferred sections and desugaring. `exactness`, `freeze`, `why`, and `assumptions` expose the meaning budget and open choices. `freeze` writes the deterministic `emath.freeze.lock.v1` contract. Hidden desugaring is `E-SYN-144`.

`emath explain <file> --show-defaults` prints each effective default, its source, and its explicit override spelling. JSON output uses the same deterministic order.

`emath explain <file> --provenance` renders the binding-to-source provenance graph. `Assumed` and `Unstated` remain visible and do not promote authority.

`emath check <file> --verify-data` verifies every declared `InstrumentRun` SHA-256. Plain `check` does not read external data files.

## Generated and provider diagnostics

Generated Rust retains semantic error categories while adding backend context. Provider failures must preserve provider identity, requested capability, budget state, cancellation disposition, and whether a partial result was discarded.

## Coverage

`emath coverage --emit json` produces a deterministic capability ledger over mathematical domains and facets. A claimed support level must cite an existing artifact. `--check <ledger>` compares regenerated output byte-for-byte and refuses drift.
