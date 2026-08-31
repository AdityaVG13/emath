# emath-macro CONTRACT

## Purpose and layer
- Procedural macro convenience crate (CRATE_MAP non-exhaustive layer marker; corrected name, formerly `emath-macros`).
- `emath! { "... .emath source ..." }` lowers an inline source literal to the same compiler path as `.emath` text. The macro parses its input as tokens (never concatenates strings), validates it is a single string literal and valid source, and expands to `::emath_build::builder::MacroExpansion::from_literals(source, identity)`, which hosts pass to `emath_build::builder::build_from_source`.
- Thin shim: parsing/lowering logic lives in `emath-build`'s `builder` module (a normal crate) so it is unit-testable; this crate is proc-macro only.

## Public types and semantics
- Single proc-macro entry: `#[proc_macro] pub fn emath(input: TokenStream) -> TokenStream`.

## Invariants
- Input must be a single string literal of valid `.emath` source; malformed input (non-literal, unescaped quotes, or source that does not parse) fails compilation with a typed `E-CODEGEN-011` message and generates nothing.
- The expansion embeds the literal source into the host binary. `.emath` source is treated as code: untrusted input must never enter `emath!`.
- The macro performs no I/O and touches no files.
- Token text is parsed, never concatenated, so arbitrary input cannot inject tokens.

## Error model
- Compile-time errors only: `::core::compile_error!` with a stable `E-CODEGEN-011` code. No runtime errors are emitted.

## Determinism class
- Deterministic at compile time: expansion output depends only on the input token stream; `identity` is the content hash the expansion references.

## Cancellation behavior
- Not applicable: proc-macro crate with no runtime and no cancellation surface.

## Unsafe boundary
- None: crate declares `#![forbid(unsafe_code)]`; the workspace lint forbids it.

## Feature flags
- None: no `[features]` in Cargo.toml (crate is `proc-macro = true`).

## Conformance tests
- None: no `tests/` directory and no `#[cfg(test)]` module on disk; correctness is exercised through downstream compile-time use via `emath-namespace` (naming contract) and `emath-build`'s `builder` parsing tests.

## No-claim boundaries
- This crate only expands; it does not parse, admit, or build. Those responsibilities live entirely in `emath-build` (`builder` module: parse; build), whose contracts govern them.
- The expansion's naming follows the language-spec naming contract enforced upstream; this crate adds no naming machinery of its own.
