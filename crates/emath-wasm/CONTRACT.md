# emath-wasm

## Purpose and layer

- Host-side WASM engine for the in-browser emath demo.
- Compiles the real compiler pipeline (`emath-syntax` -> `emath-sema` -> `emath-ir` -> `emath-rust-backend`) to `wasm32-unknown-unknown`.
- Exposes a tiny hand-rolled C ABI (`em_alloc` / `em_free` / `em_run` / `em_init`). No wasm-bindgen, no serde, no filesystem.
- Depends on `emath-artifact` (JSON document parse/serialize) and `emath-exec-ir` (constructor VM and constructor EMIR behind `run` / `generate`) in addition to the pipeline crates.

## Public types and semantics

- Safe host API: `run_op(op, payload) -> String` returns one JSON object.
- ABI version constant: `ABI_VERSION = 1`.
- C ABI (`ffi`):
  - `em_init()`: initialize runtime environment and clean panic hook.
  - `em_alloc(len) -> ptr`: allocate `len` bytes of linear memory.
  - `em_free(ptr, len)`: reclaim a prior `em_alloc` (or `em_run` response).
  - `em_run(op_ptr, op_len, payload_ptr, payload_len) -> u64`: dispatch; packed as `(ptr as u64) << 32 | (len as u64)`.
- Ops: `version`, `examples`, `check`, `plan`, `mig`, `generate`, `format`, `run`, `inputs`, `solve_candidates`.
- `plan`, `mig`, and `solve_candidates` refuse `E-KIND-GONE`. There is no
  `goals:` planner and no leftover solve-world menu.
- Bare pane wrap is off. `prepare_source` passes text through. Scratch
  expansion remains leftover in `emath-syntax`; this crate does not wrap
  untrusted panes into inferred functions.
- `run` payload is either raw source or a JSON envelope
  `{"source":"…","given":{"x":4.0}}`. Detection: payload trims to `{`
  and parses as JSON with a `source` key; otherwise the bytes are raw
  source. `given` is optional.
- `run` admits constructor declarations and evaluates them on the
  constructor VM (`evaluate_tree`). When `given` is present, the first
  `emath function` is also evaluated with those bindings. A missing
  required input is `ok:false` with `missing input \`name\``. Unknown
  sections such as `goals:` / `compile:` refuse `E-SEC-101`.
- Successful `run` / `generate` / `inputs` use schema
  `emath.constructor.v1`. `generate` emits constructor EMIR Rust
  (`pub fn entry`), not SIR/`goals:` crates.
- `inputs` reads `inputs:` from the syntax tree, not hollow SIR.

## Invariants

- Dispatch never panics across the ABI. The sema pipeline returns diagnostics; `ok:false` is reserved for invalid UTF-8, unknown ops, backend generate failure, and a `run` envelope whose `given` is present but not a JSON object of finite numbers / vectors / matrices. JSON numbers and numeric strings are both accepted; `NaN` / `Infinity` / non-numeric strings are refused (they must not silently drop the binding). Duplicate `source` / `given` keys on the envelope, and duplicate names inside `given` (or `shape`/`data` on a tensor), are `ok:false`; they must not first-win or last-win.
- `check` / `plan` / `mig` / `generate` / `format` / `run` / `inputs` keep `ok:true` when the pipeline ran, even if the source has diagnostics.
- Those ops also emit `admitted: true` iff diagnostics have no errors. `ok` is not admission; untrusted pane text is never implied admitted by `ok`.
- `run` uses constructor admission and the constructor VM. Parse errors
  return diagnostics. Successful runs return `emath.constructor.v1` with
  `bindings`, authored `tests`, and `uses`. There is no `tier` /
  `_pane` / `expect_passed` SIR report.
- All unsafe is confined to `ffi`. Each block documents its pointer/length pairing.
- `em_alloc(len)` produces an exclusive region of `len` initialized bytes (`vec![0u8; len]`; capacity == len). `em_free(ptr, len)` must pair a live `ptr` exactly once; reconstruction uses the capacity stored at mint time, so a mismatched host `len` cannot induce allocator UB (still a contract violation to lie about `len`).
- `em_run` reads `op` and `payload` as UTF-8 slices the caller owns, writes the JSON response through `em_alloc`, and returns that allocation. The JS caller copies, then `em_free`s. An oversized response (`len > u32::MAX`) or a failed response alloc returns the empty pack `0`.

## Error model

- `{"ok":false,"error":"..."}`: invalid UTF-8, unknown op, generate backend refusal, or malformed `run` `given` (not an object, or a value that is not a finite Float64 / vector / matrix).
- `{"ok":true,"admitted":false,"diagnostics":[...]}`: the pipeline ran. Diagnostics are not a dispatch failure. `admitted` is false until the source is error-free.
- Diagnostic objects: `severity` (`error`|`warning`), `code`, `message`, `start`, `end`.

## Determinism class

- Deterministic: same `(op, payload)` bytes produce byte-identical JSON.
- Field order is fixed per op. Arrays follow pipeline order (declarations, requests) or `BTreeMap` key order (generated files).
- `plan` / `mig` refuse; they do not emit SIR intent-graph encodings.
- Constructor `run` uses the constructor scalar carriers. There is no
  image transcendental recipe catalog.

## Cancellation behavior

- Not applicable. Synchronous, single-threaded dispatch. The wasm runtime provides no cancellation surface.

## Unsafe boundary

- Crate lint is `deny(unsafe_code)` (workspace `forbid` is overridden only here).
- `ffi` is the single `allow(unsafe_code)` leaf.
- Invariants:
  1. `em_alloc` returns either `0` (`len == 0`) or a pointer from `vec![0u8; len]` (exact capacity; not `Vec::with_capacity`, which may over-allocate) whose backing store is leaked via `mem::forget`. Nothing else aliases that allocation until `em_free`.
  2. `em_free(ptr, len)` reconstructs `Vec::from_raw_parts(ptr, 0, stored_cap)` and drops it. `ptr` must be a live `em_alloc` (or the `em_run` response allocation). Double-free / foreign `ptr` are provable no-ops via `LIVE_ALLOCS`. Mismatched host `len` is ignored for drop sizing (stored capacity wins).
  3. `em_run` may read `[op_ptr, op_ptr+op_len)` and `[payload_ptr, payload_ptr+payload_len)` only as bytes the caller initialized. Those regions must be valid, non-overlapping with the response allocation, and not freed until `em_run` returns.

## Feature flags

- None.

## Conformance tests

- Native tests live in `tests/emath-wasm/tests/wasm_ops.rs`: constructor
  check/run/generate/inputs, `given` envelope, leftover `plan`/`mig`/
  `solve_candidates` refuse, `goals:` / `emath policy` refuse, curated
  constructor tutorials, and dispatch refusals.

## No-claim boundaries

- Not a sandbox: the wasm runtime is assumed; this crate does not isolate the host.
- No filesystem, network, or clock. `generate` is in-memory only (`emath-rust-backend`); it does not invoke `emath-build` or `cargo`.
- No claim that generated Rust is compiled or executed in the browser.
  `run` is the constructor VM, not a compiled crate.
- Curated `examples` are constructor tutorials 1, 2, 3, and 6. Leftover
  recipe fixtures stay on disk and are not served.
- Bare panes are not wrapped. A file without `emath object|function|query`
  is not constructor source.
