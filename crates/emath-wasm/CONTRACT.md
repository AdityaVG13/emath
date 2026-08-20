# emath-wasm

## Purpose and layer

- Host-side WASM engine for the in-browser emath demo.
- Compiles the real compiler pipeline (`emath-syntax` → `emath-sema` → `emath-ir` → `emath-rust-backend`) to `wasm32-unknown-unknown`.
- Exposes a tiny hand-rolled C ABI (`em_alloc` / `em_free` / `em_run`). No wasm-bindgen, no serde, no filesystem.

## Public types and semantics

- Safe host API: `run_op(op, payload) -> String` returns one JSON object.
- ABI version constant: `ABI_VERSION = 1`.
- C ABI (`ffi`):
  - `em_alloc(len) -> ptr`: allocate `len` bytes of linear memory.
  - `em_free(ptr, len)`: reclaim a prior `em_alloc` (or `em_run` response).
  - `em_run(op_ptr, op_len, payload_ptr, payload_len) -> u64`: dispatch; packed as `(ptr as u64) << 32 | (len as u64)`.
- Ops: `version`, `examples`, `check`, `plan`, `mig`, `generate`, `format`, `run`, `inputs`.
- Playground wrap (this crate only): if the payload source is not already a
  declaration (after leading whitespace/comments, first content line does
  not start with `emath `), it is wrapped as `emath function Pane` (
  assignment lines become `definitions:`, a lone final expression is bound
  as `result`, and free identifiers become untyped `inputs:`; Float64 via
  `N-TYPE-001`). No `tests:` section is synthesized. This is not legal
  `.emath` outside the pane; `emath-syntax` / `emath-sema` are unchanged
  at the parse layer.
- When wrapping happens, `check` / `plan` / `mig` / `generate` / `run` /
  `inputs` include `"desugared_source"` (the wrapped text). The field is
  omitted when the source was already a declaration.
- `run` payload is either raw source (backward compatible) or a JSON
  envelope `{"source":"…","given":{"x":4.0}}`. Detection: payload trims
  to `{` and parses as JSON with a `source` key; otherwise the bytes are
  raw source. `given` is optional.
- When `given` is present, every declaration gets a synthetic worked run
  `{"name":"_pane","given":{…},"computed":true}` (or a typed refusal
  naming a missing input) **in addition to** the source's own example
  tests. A declaration with no tests and every input bound (zero inputs
  = trivially bound) also emits `_pane` without an envelope.
- `inputs` returns `{ok, diagnostics, declarations:[{declaration, inputs:[{name, type, defaulted}]}]}`.
  `defaulted` is true when admission emitted `N-TYPE-001` for that name.

## Invariants

- Dispatch never panics across the ABI. The sema pipeline returns diagnostics; `ok:false` is reserved for invalid UTF-8 and unknown ops (and backend generate failure).
- `check` / `plan` / `generate` / `format` / `run` keep `ok:true` when the pipeline ran, even if the source has diagnostics.
- `run` uses `check` then the Tier-0 EMIR interpreter (`emath-exec-ir`). Source errors return diagnostics only (no report). Successful admission returns `tier: interpreted-strict-f64` plus a `RunReport`.
- Per-test JSON: an asserted example emits `"expect_passed": true|false`. A worked example (`expect` omitted) or a synthetic `_pane` run emits `"computed": true` and omits `expect_passed`. Summary is `{tests, passed, failed, refused, computed}`. A missing envelope binding is `refusal: "lowering-refused"` with `reason` containing `missing input \`name\``.
- All unsafe is confined to `ffi`. Each block documents its pointer/length pairing.
- `em_alloc(len)` produces an exclusive region of capacity `len` (length 0 until written). `em_free(ptr, len)` must pair the same `ptr`/`len` exactly once.
- `em_run` reads `op` and `payload` as UTF-8 slices the caller owns, writes the JSON response through `em_alloc`, and returns that allocation. The JS caller copies, then `em_free`s.

## Error model

- `{"ok":false,"error":"..."}`: invalid UTF-8, unknown op, or generate backend refusal.
- `{"ok":true,"diagnostics":[...]}`: the pipeline ran. Diagnostics are not a dispatch failure.
- Diagnostic objects: `severity` (`error`|`warning`), `code`, `message`, `start`, `end`.

## Determinism class

- Deterministic: same `(op, payload)` bytes produce byte-identical JSON.
- Field order is fixed per op. Arrays follow pipeline order (declarations, requests) or `BTreeMap` key order (generated files).
- MIG `canonical` / `identity` are the SIR intent-graph encodings (span-free).
- `run` is the same class as generated Rust (Tier 1): arithmetic/comparisons are bit-exact IEEE-754 binary64; transcendentals (`sin`/`cos`/`tan`/`tanh`/`exp`/`ln`/`powf`/`atan2`) follow platform libm.

## Cancellation behavior

- Not applicable. Synchronous, single-threaded dispatch. The wasm runtime provides no cancellation surface.

## Unsafe boundary

- Crate lint is `deny(unsafe_code)` (workspace `forbid` is overridden only here).
- `ffi` is the single `allow(unsafe_code)` leaf.
- Invariants:
  1. `em_alloc` returns either `0` (`len == 0`) or a pointer from `Vec::with_capacity(len)` whose backing store is leaked via `mem::forget`. Nothing else aliases that allocation until `em_free`.
  2. `em_free(ptr, len)` reconstructs `Vec::from_raw_parts(ptr, 0, len)` and drops it. `ptr`/`len` must match a live `em_alloc` (or the `em_run` response allocation). Double-free and mismatched length are undefined.
  3. `em_run` may read `[op_ptr, op_ptr+op_len)` and `[payload_ptr, payload_ptr+payload_len)` only as bytes the caller initialized. Those regions must be valid, non-overlapping with the response allocation, and not freed until `em_run` returns.

## Feature flags

- None.

## Conformance tests

- Native unit tests in `src/lib.rs`: version shape; check on hello-square; check on a known-bad source; MIG canonical stability; generate files; unknown-op refusal; JSON escaping; `run` on hello-square (pass), affine scorer with constructor+state, failing expect, error-source diagnostics, expect-less worked example (`given x = 4` → `y = 16.0`, `computed: 1`), head-args `square(x: Float64) -> Float64` (`given x = 4` → `square = 16.0`, free-fn generate), constant-only `TwentyOne` (`y = 3 * 7` → `y = 21.0`), bare `y = x * x` (admits, `N-TYPE-001`, `desugared_source`), bare `a = 2` / `b = a * a` (computes `b = 4` with no `tests:` section), `run` envelope `given x = 5` on Square (`y = 25`, `_pane`), and envelope missing `x` (typed refusal).

## No-claim boundaries

- Not a sandbox: the wasm runtime is assumed; this crate does not isolate the host.
- No filesystem, network, or clock. `generate` is in-memory only (`emath-rust-backend`); it does not invoke `emath-build` or `cargo`.
- No claim that generated Rust is compiled or executed in the browser. `run` is the Tier-0 interpreter (strict-f64), not the compiled crate.
- No claim that every language example admits. Curated embeds are the
  language/examples sources that check with zero errors
  (`00_hello_square`, `00_stateful_affine_scorer`,
  `11_parametric_unknown_operator`) plus one intentional diagnostics demo.
  `01_cache_policy` and `02_tensor_program` exist but do not admit on this
  pipeline, so they are not embedded.
- Bare-expression wrap is playground-only. A `.emath` file without an
  `emath …:` header is still refused by the CLI / `emath-syntax` parser.
