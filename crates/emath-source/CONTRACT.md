# emath-source

## Purpose and layer

Tier 0 stable public import surface for source files, line maps and
human-readable diagnostic rendering. The underlying types live in
`emath-core`; this crate re-exports them so downstream annotations keep
compiling unchanged against `emath_source::*`.

## Public types and semantics

- `SourceFile`: re-export of `emath_core::SourceFile` (same type).
- `SourceStore`: re-export of `emath_core::SourceStore` (same type).

Both resolve to the identical type as the `emath_core` paths. This crate
defines no additional public types.

## Invariants

- `emath_source::SourceFile` / `SourceStore` are the same types as
  `emath_core::SourceFile` / `SourceStore`; paths are interchangeable.

## Error model

No errors emitted. This crate only re-exports types and declares no
diagnostic-producing surface of its own.

## Determinism class

No stronger guarantee documented beyond what `emath-core` provides for the
re-exported source types.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. Crate root is `#![forbid(unsafe_code)]`.

## Feature flags

None.

## Conformance tests

No `tests/` directory present and no inline test evidence in the single-file
crate. Conformance is inherited through the underlying `emath-core` types.

## No-claim boundaries

No additional no-claim boundaries documented. Line-mapping and rendering
behavior is owned by `emath-core`, not this re-export crate.
