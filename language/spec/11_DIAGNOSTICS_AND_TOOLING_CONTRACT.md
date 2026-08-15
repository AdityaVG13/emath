# Diagnostics and Tooling Contract

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

## Taxonomy

```text
E-SYN syntax/layout
E-PKG package/resolution
E-NAME name/visibility
E-KIND schema/lowering
E-TYPE type/refinement
E-UNIT units/dimensions
E-SHAPE shapes/layout
E-DOM domain/branch
E-CTOR constructor/invariant
E-GOAL request/planning
E-PROV provider/adapter
E-EVID evidence/certificate
E-CODEGEN backend/artifact
E-RES resource/cancellation
```

Codes are not reused for different meanings.

## Error recovery

IDE parsing can recover and continue; build semantic admission fails if an error affects the requested artifact. Warnings are policy-upgradable to errors.

## Formatter

The formatter is idempotent, edition-aware and preserves comments. Formatting does not require providers or execute user code.

## LSP

The LSP exposes typed hover, go-to-definition, semantic references, diagnostics, code actions, goal/provider inspection, plan preview, evidence links and generated-Rust/source-map navigation.

## CLI

Core commands are specified in `docs/CLI_REFERENCE.md` and include `new`, `fmt`, `check`, `explain`, `plan`, `build`, `run`, `test`, `bench`, `verify`, `inspect`, `diff`, `doctor`, `vendor` and fork/provider tooling.
