# emath-syntax

## Purpose and layer

Tier 1 bootstrap syntax crate: layout lexer and recursive-descent parser.
Provider-free. Implements the kernel `SourceParser` seam so `emath-sema` can
admit without depending on this crate. The syntax tree is owned by
`emath-core` (`emath_core::tree`).

## Public types and semantics

- `parse(text, file, limits) -> (SyntaxTree, Diagnostics)`: parse with bounds.
- `parse_str(text) -> (SyntaxTree, Diagnostics)`: parse with default limits.
- `parse_lossless(text, file, limits) -> LosslessParse`: parse retaining
  comments and spans for formatting round-trips.
- `format_lossless(&LosslessParse) -> String`: canonical, idempotent,
  comment-preserving formatting.
- `LosslessParse`: `{ tree, diagnostics, comments }`.
- `SyntaxParser`: process-wide default `SourceParser` unit.
- `install_source_parser()`: idempotently installs `SyntaxParser`; hosts must
  call once per process before first parse.
- Modules: `formatter`, `genesis`, `lexer`, `parser`, `token`, `tree`.

## Invariants

- No panics on arbitrary UTF-8 input.
- Exact spans on every token and node.
- Indentation enforcement, duplicate-section checks, precedence handling.
- Bounded source/token/nesting limits.
- Recovery at statement boundaries.
- Formatting is idempotent, comment-preserving and parse-stable.
- `genesis` parses `emath custom` world declarations (G0 only).

## Error model

Stable diagnostics through `emath_core::Diagnostics`, returned alongside the
tree. No panic on malformed input.

## Determinism class

Deterministic. Tokenization and parsing are pure over source bytes; lossless
formatting is canonical and idempotent.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. Crate root is `#![forbid(unsafe_code)]`.

## Feature flags

None.

## Conformance tests

No `tests/` directory present. Inline `#[cfg(test)]` unit tests live in the
`src` modules (not enumerated).

## No-claim boundaries

Parser is a bootstrap implementation and is replaceable by the Phase 4
lossless parser. The G1 world/forest stage (bounded parse forest plus
signature inference) lives in `emath-genesis`; this crate carries no
emath-genesis dependency and makes no claim on it.
