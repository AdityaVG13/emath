# Background

UB-exorcist skill-loop (`2026-08-24-emath-1`, Passes 1–13) confirmed emath is a **pure-safe-with-wasm-FFI-leaf** crate: 41/42 crates `#![forbid(unsafe_code)]`; the only production `unsafe` lives in `crates/emath-wasm/src/ffi.rs` (plus JS peer `web/app.js` and `crates/emath-wasm/CONTRACT.md`).

Passes 8–13 found **zero** new bead-worthy issues (no transmute/MaybeUninit/get_unchecked/Pin/manual Send-Sync/library-trait UB feeders). All open soundness work collapses into this **one** aggregate bead so we do not create bead bloat for 1-line SAFETY comments.

Audit artifacts (local, gitignored): `.ub-exorcism/2026-08-24-emath-1/phase4_unified_findings.md` and `phase2_findings_*.md`.

## Why this serves the project

Stage 2 (WASM interactive surface) depends on an honest FFI. Host/dev/test currently use `panic=unwind` while exporting bare `extern "C"` — a Rustonomicon-class unwind-across-FFI hazard. LIVE map capacity coupling and host lock order are latent allocator/alias bugs. Fixing these as one coordinated change keeps JS + CONTRACT + Rust invariants aligned.

## Technical Approach

### P0 — ABI / allocator fences

1. **Panic × `extern "C"` (P7-F001/F002):** Choose one:
   - `[profile.dev]` / `[profile.test] panic = "abort"` for the wasm crate / workspace, **or**
   - Wrap `em_init` / `em_alloc` / `em_free` / `em_run` in `catch_unwind` with documented abort-or-structured-error policy.
   Align `CONTRACT.md` “never panics across the ABI” with the chosen fence.
2. **LIVE capacity coupling (P4-F001 + docs):** On wasm mint, store `buf.capacity() as u32` in `LIVE_ALLOCS` (not request `len`); free with stored capacity; fix module/CONTRACT wording that still says `Vec::with_capacity` while code uses `vec![0u8; n]`.

### P1 — Host peer hardening

3. **In-flight LIVE reclaim (P7-F003)** if catch_unwind is chosen; else document abort-only instance death.
4. **JS alloc/free hygiene (P7-F004/F005):** Move `em_alloc` under `try`; per-`em_free` try/catch; require `em_free` export in `finally`.
5. **Host shim provenance (P5-F001):** Stop `usize` round-trip in `ALLOCATIONS` — store `NonNull<u8>` / `*mut u8`.
6. **Alias window (P5-F002):** Copy-at-ABI or seal/`'static` policy for `read_utf8` borrows across `run_op`/`pack_json`.
7. **JS ptr unpack (P3-F002):** Unsigned normalize for `em_run` packed `(ptr<<32)|len`; CONTRACT document `ptr ∈ [1, 2^31)` (or widen ABI).
8. **Host free lock order (P6-F001):** Hold `LIVE_ALLOCS` + `ALLOCATIONS` through reclaim (mirror mint path).

### Verify (owned residuals — tests, not extra beads)

9. **G1 adversarial `read_utf8`:** foreign / freed / `len > mint_cap` → clean Err, no UB (native first; wasm/Miri when toolchain allows).
10. **Optional G2:** generation-tagged handles for remint+stale-free — product call only.

### Explicit non-goals

- Generated-artifact `Send`/`Sync` / dispatcher → **`emath-gap-concurrency-contract-qmm2`** (do not expand here).
- Installing Miri without operator approval; when available, add a targeted `cargo miri test -p emath-wasm` lane as regression.
- Workspace forbid-seal chores on test packages.

## Success Criteria

- Host/dev/test cannot unwind across `em_*` ABI (abort profile and/or catch_unwind + CONTRACT truth).
- `LIVE_ALLOCS` stores and frees by **capacity**, with unit test that would fail if request-len were stored under a future `with_capacity` mint.
- JS host never orphans an `em_alloc` on throw; free path holds both maps through reclaim.
- Host `ALLOCATIONS` does not round-trip pointers through `usize` alone.
- Adversarial `read_utf8` cases refuse without invoking UB.
- `br`-tracked AC + targeted tests (no full workspace suite).

## Test Plan

- Unit (emath-wasm): capacity-store free; mismatched free len; double-free; foreign ptr; adversarial read lens; optional remint tags.
- Integration / web: JS try/finally reclaim; unsigned unpack smoke for high ptr (or documented window).
- When miri installed for pinned nightly: `cargo +nightly-YYYY-MM-DD miri test -p emath-wasm` (targeted).
- Regression: CONTRACT excerpt reviewed in same PR as fence choice.

## Considerations

- Product wasm32 + release already use `panic=abort` — primary hole is host/dev/test.
- Do not “fix” docs to `with_capacity` without capacity store — that reintroduces free-size holes.
- Soft-related: `emath-gap-concurrency-contract-qmm2` for generated crates; this bead is the wasm leaf only.
- Audit run id `2026-08-24-emath-1`; miri was degraded (component missing for `nightly-2026-08-04`).

## Provenance

Aggregated from UB-exorcist Passes 1–13 (QuietFrog). Sources: `phase4_unified_findings.md`, `phase2_findings_{ffi_contracts,uninit_allocator,aliasing_provenance,data_races,panic_safety}.md`.
