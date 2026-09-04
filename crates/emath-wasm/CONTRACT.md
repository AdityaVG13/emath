# emath-wasm

## Purpose and layer

- Host-side WASM engine for the in-browser emath demo.
- Compiles the real compiler pipeline (`emath-syntax` -> `emath-sema` -> `emath-ir` -> `emath-rust-backend`) to `wasm32-unknown-unknown`.
- Exposes a tiny hand-rolled C ABI (`em_alloc` / `em_free` / `em_run` / `em_init`). No wasm-bindgen, no serde, no filesystem.
- Depends on `emath-artifact` (JSON document parse/serialize) and `emath-exec-ir` (Tier-0 interpreter behind the `run` op) in addition to the pipeline crates.

## Public types and semantics

- Safe host API: `run_op(op, payload) -> String` returns one JSON object.
- ABI version constant: `ABI_VERSION = 1`.
- C ABI (`ffi`):
  - `em_init()`: initialize runtime environment and clean panic hook.
  - `em_alloc(len) -> ptr`: allocate `len` bytes of linear memory.
  - `em_free(ptr, len)`: reclaim a prior `em_alloc` (or `em_run` response).
  - `em_run(op_ptr, op_len, payload_ptr, payload_len) -> u64`: dispatch; packed as `(ptr as u64) << 32 | (len as u64)`.
- Ops: `version`, `examples`, `check`, `plan`, `mig`, `generate`, `format`, `run`, `inputs`.
- Playground wrap delegates to official `emath_syntax::expand_scratch` (L0/L1
  scratch and L2 named shorthand). Bare panes become `emath function Scratch`
  with inferred `inputs:` / `definitions:` (Float64 via `N-TYPE-001`). Language
  keywords and binder variables are not free inputs. No `tests:` section is
  synthesized unless the source wrote `example`. The same rewrite is applied
  by `emath-syntax::parse` and `emath expand`; this is legal `.emath`.
- When wrapping happens, `check` / `plan` / `mig` / `generate` / `run` /
  `inputs` include `"desugared_source"` (the wrapped text). The field is
  omitted when the source was already a declaration.
- `run` payload is either raw source (backward compatible) or a JSON
  envelope `{"source":"…","given":{"x":4.0}}`. Detection: payload trims
  to `{` and parses as JSON with a `source` key; otherwise the bytes are
  raw source. `given` is optional.
- When `given` is present, every declaration gets a synthetic worked run
  `{"name":"_pane","given":{…},"computed":true}` (or, when an input
  binding is missing, a labeled symbolic run) **in addition to** the
  source's own example tests. A declaration with no tests and every
  input bound (zero inputs = trivially bound) also emits `_pane` without
  an envelope.
- `inputs` returns `{ok, admitted, diagnostics, declarations:[{declaration, inputs:[{name, type, defaulted}]}]}`.
  `defaulted` is true when admission emitted `N-TYPE-001` for that name.

## Invariants

- Dispatch never panics across the ABI. The sema pipeline returns diagnostics; `ok:false` is reserved for invalid UTF-8, unknown ops, backend generate failure, and a `run` envelope whose `given` is present but not a JSON object of finite numbers / vectors / matrices. JSON numbers and numeric strings are both accepted; `NaN` / `Infinity` / non-numeric strings are refused (they must not silently drop the binding). Duplicate `source` / `given` keys on the envelope, and duplicate names inside `given` (or `shape`/`data` on a tensor), are `ok:false`; they must not first-win or last-win.
- `check` / `plan` / `mig` / `generate` / `format` / `run` / `inputs` keep `ok:true` when the pipeline ran, even if the source has diagnostics.
- Those ops also emit `admitted: true` iff diagnostics have no errors. `ok` is not admission; untrusted pane text is never implied admitted by `ok`.
- `run` uses `check` then the Tier-0 EMIR interpreter (`emath-exec-ir`). Source errors return diagnostics only (no report). Successful admission returns `tier: interpreted-strict-f64` plus a `RunReport`. Vector / matrix / tensor values are serialized (tensors as `{shape, data}`), not dropped.
- Per-test JSON: an asserted example emits `"expect_passed": true|false`. A worked example (`expect` omitted) or a synthetic `_pane` run emits `"computed": true` and omits `expect_passed`. Summary is `{tests, passed, failed, refused, computed}`. A missing envelope binding is a labeled symbolic run (`TestVerdict::Symbolic`, label `symbolic-only`, or `hole-open` when no symbolic form exists; counted in the runner's `summary.symbolic`), not `lowering-refused` — `lowering-refused` survives only for genuinely impossible lowering. Note: the wasm JSON summary does not yet surface `summary.symbolic`.
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
- MIG `canonical` / `identity` are the SIR intent-graph encodings (span-free).
- `run` is the same class as generated Rust (Tier 1): arithmetic/comparisons are bit-exact IEEE-754 binary64; transcendentals (`sin`/`cos`/`tan`/`tanh`/`exp`/`ln`/`powf`/`atan2`) follow platform libm.

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

- Native unit tests in `src/lib.rs`: `version_op_shape`; `check_hello_square_admits`; `check_bad_source_surfaces_code`; `mig_canonical_contains_goal_and_is_stable`; `generate_hello_square_files`; `unknown_op_refuses`; `json_escaping_survives_quotes_backslashes_newlines`; `curated_non_demo_examples_admit` (every curated embed admits with zero errors); `run` on hello-square (pass), affine scorer with constructor+state (`run_affine_scorer_constructor_state`), `run_failing_expect_counts_failed`, `run_error_source_surfaces_diagnostics`, expect-less worked example (`run_worked_example_computes_without_expect`), head-args `square(x: Float64) -> Float64` (`run_head_args_square_computes_sixteen`, `generate_head_args_square_emits_free_function`), constant-only `TwentyOne` (`run_twenty_one_constant_only`), bare `y = x * x` (`check_bare_square_desugars_and_admits` with `N-TYPE-001` + `desugared_source`), bare `a = 2` / `b = a * a` (`run_bare_constants_computes_without_tests_section`), `run` envelope `given x = 5` on Square (`run_envelope_given_square_computes`, `_pane`), envelope missing `x` (typed refusal `run_envelope_missing_binding_refuses`), malformed `given` numbers (`run_envelope_malformed_given_number_refuses`), and the newer capability lanes: vector/factorial/range-sum/forall-exists/integral/autodiff/solve/optimize/constrained-opt (`run_vector_given_computes`, `run_factorial_inclusive_computes`, `run_range_sum_computes`, `run_forall_exists_computes`, `run_integral_computes`, `run_autodiff_computes`, `run_solve_computes`, `run_optimize_computes`, `run_constrained_opt_computes`, `run_tensor_face_serializes_matrix`, `run_finite_sum_is_fifteen`) plus `parity_transcendentals_bit_exact` (Tier-1 arithmetic parity with the generated Rust class).

## No-claim boundaries

- Not a sandbox: the wasm runtime is assumed; this crate does not isolate the host.
- No filesystem, network, or clock. `generate` is in-memory only (`emath-rust-backend`); it does not invoke `emath-build` or `cargo`.
- No claim that generated Rust is compiled or executed in the browser. `run` is the Tier-0 interpreter (strict-f64), not the compiled crate.
- No claim that every language example admits. The 14 curated embeds are
  `hello-square`, `stateful-affine-scorer`, `sum-one-to-five`, `tensor-face`,
  `vector-given`, `factorial`, `range-sum`, `forall-exists`, `integral`,
  `autodiff`, `solve`, `optimize`, `constrained-opt`, plus the intentional
  Tutorial 6 diagnostics demo; `curated_non_demo_examples_admit` proves each
  non-demo embed admits with zero errors. `cache-policy` and `tensor-program`
  do not admit on this pipeline, so they are not embedded.
- Bare-expression wrap is playground-only. A `.emath` file without an
  `emath …:` header is still refused by the CLI / `emath-syntax` parser.
