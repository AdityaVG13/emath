# CONTRACT.md

## Purpose and layer

Deterministic JSON for host documents: the std-only streaming writer
(re-exported from `emath-core`) and its parse-back reader. Users: the CLI,
the TUI run directory, the WASM payloads, and exec-ir scratch checkpoints.
Layer: core.

## Public types and semantics

- `JsonWriter`, `JsonObject`: re-exported from `emath-core`.
- `JsonValue`: `Str`, `Num` (kept verbatim), `Bool`, `Null`, `Obj`
  (insertion order preserved), `Arr`, with typed field accessors
  (`field`, `string_field`, `int_field`).
- `parse_json_document(text) -> Result<JsonValue, JsonError>`.
- `JsonError`: a malformed document or a field of the wrong shape.

## Invariants

- The reader accepts the writer's grammar (an RFC 8259 subset: objects,
  arrays, strings, integers, booleans, `null`) and refuses anything else.
- Trailing content after the document is a refusal.
- serde is forbidden; the crate is std-only.

## Error model

`JsonError(String)` with an actionable detail. Callers usually render it
with `to_string()`.

## Determinism class

Deterministic: identical input text produces an identical value tree.

## Cancellation behavior

Not applicable. Bounded synchronous parsing.

## Unsafe boundary

None. `#![forbid(unsafe_code)]`.

## Feature flags

None.

## Conformance tests

Exercised through its consumers: `tests/emath-cli` (`api_modules`,
`catalog`) and `tests/emath-wasm` (`wasm_ops`).

## No-claim boundaries

- Not a general JSON validator: number tokens are kept verbatim and only
  checked when a caller asks for an integer.
