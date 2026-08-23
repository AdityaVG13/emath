//! C ABI leaf for the wasm engine.
//!
//! # Safety
//!
//! This is the only `unsafe` module in the crate. Every `unsafe` block is
//! a documented pointer/length pairing with the JS host:
//!
//! 1. [`em_alloc`] returns either `0` (`len == 0`) or the start of a
//!    `Vec<u8>` allocation with **capacity == `len` exactly** (site-1
//!    proof: [`Vec::with_capacity`] stores the requested capacity verbatim
//!    (RawVec has no amortized-growth path at allocation time), so
//!    `capacity()` equals `len`; pinned by `test_em_alloc_capacity_exact`)
//!    and **length = 0**, leaked via [`std::mem::forget`]. The host owns
//!    that region until [`em_free`].
//! 2. [`em_free`] reconstructs that `Vec` with
//!    `from_raw_parts(ptr, 0, stored_cap)` and drops it. `ptr` must be a
//!    live allocation from [`em_alloc`] (including the JSON buffer returned
//!    by [`em_run`]). Double-free / foreign `ptr` are no-ops via
//!    `LIVE_ALLOCS`. Host `len` is ignored for drop sizing — the capacity
//!    recorded at mint time always wins (mismatched `len` cannot UB).
//! 3. [`em_run`] reads `[op_ptr, op_ptr + op_len)` and
//!    `[payload_ptr, payload_ptr + payload_len)` as UTF-8. Those slices
//!    must be valid, initialized, and not freed for the duration of the
//!    call. The packed return value names a fresh [`em_alloc`] region
//!    the host must copy and then free.
//!
//! # Why unsafe_code is allowed here (edition 2024)
//!
//! The four exported entry points carry `#[unsafe(no_mangle)]`. In edition
//! 2024 an unsafe attribute is itself governed by the `unsafe_code` lint.
//! `lib.rs` provides the allowance on this leaf module: `#[allow(unsafe_code)]`
//! on its `pub mod ffi` item (which overrides `lib.rs`'s crate-level
//! `#![deny(unsafe_code)]` for this child) covers the unsafe attributes and
//! the `unsafe` blocks below. No inner attribute is needed here. The
//! allowance cannot be dropped without redesigning symbol export (e.g. a
//! generated shim crate or linker script), which the web host protocol does
//! not warrant: every `unsafe` block below is a numbered, proof-obligation
//! site, and `lib.rs` confines all of them to this leaf module. Comment
//! only, no code change.

#![allow(clippy::cast_possible_truncation)]

use std::collections::HashMap;
use std::ptr;
use std::slice;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::Once;

use crate::run_op;

static INIT_PANIC_HOOK: Once = Once::new();

/// Live-ownership map of every address/id this module has minted via
/// `em_alloc` (or the host-alloc shim) and has not yet reclaimed via
/// `em_free`. Values are the exact capacity recorded at mint time.
///
/// This is the load-bearing, locally-enforced half of the `em_free`
/// soundness invariant: a raw address only reaches `Vec::from_raw_parts`
/// if it is (a) minted by this module and (b) still owed. Foreign pointers
/// and double-frees of addresses with no current membership are rejected as
/// provable no-ops before any dereference, without trusting the host ABI
/// pledge. Reconstruction always uses the **stored capacity**, so a host
/// that passes a mismatched `len` cannot induce allocator UB — the caller's
/// `len` is ignored for the drop size (still a contract violation to lie,
/// but no longer undefined). The guard is generation-blind (address-keyed):
/// a stale pointer whose address a later `em_alloc` reused is
/// indistinguishable from that new mint and passes membership, so a stray
/// double-free straddling an address-reusing `em_alloc` can consume the new
/// mint's single free pass. That sequence requires the host to already be
/// violating the exactly-once contract, so it stays a documented ABI
/// violation, not a guard-guaranteed no-op.
///
/// Single-threaded wasm linear memory, so the `Mutex` is uncontended; the
/// poisoning policy (`unwrap_or_else(Into::into_inner)`) mirrors
/// `INIT_PANIC_HOOK`'s style and keeps a poisoned map usable after a panic.
static LIVE_ALLOCS: LazyLock<Mutex<HashMap<u32, u32>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn live_allocs_lock() -> std::sync::MutexGuard<'static, HashMap<u32, u32>> {
    LIVE_ALLOCS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Install a panic hook that formats panic info cleanly.
pub fn install_panic_hook() {
    INIT_PANIC_HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
                *s
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.as_str()
            } else {
                "panic payload not a string"
            };
            let location = info.location().map_or_else(
                || "unknown location".to_string(),
                |l| format!("{}:{}:{}", l.file(), l.line(), l.column()),
            );
            eprintln!("emath panic at {location}: {payload}");
        }));
    });
}

/// Optional initialization entry point for the WASM module.
///
/// Installs the clean panic hook. Safe to call multiple times.
#[unsafe(no_mangle)]
pub extern "C" fn em_init() {
    install_panic_hook();
}

#[cfg(not(target_arch = "wasm32"))]
mod host_alloc {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    static ALLOCATIONS: Mutex<BTreeMap<u32, (usize, usize)>> = Mutex::new(BTreeMap::new());
    static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

    pub fn alloc(len: u32) -> (u32, usize) {
        if len == 0 {
            return (0, 0);
        }
        // `vec![0u8; len]` guarantees capacity == len exactly (RawVec from a
        // sized repeat has no amortized-growth path), so the paired `free`
        // reconstructs with the stored capacity and the drop sizes correctly
        // (site 1).
        let mut buf = vec![0u8; len as usize];
        let capacity = buf.capacity();
        debug_assert_eq!(capacity, len as usize, "exact-capacity invariant (ffi host shim)");
        let ptr = buf.as_mut_ptr();
        std::mem::forget(buf);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // Hold LIVE_ALLOCS then ALLOCATIONS together (same order as em_free →
        // host free) so a concurrent em_free cannot observe membership in
        // LIVE_ALLOCS while the addr table still lacks the id — that window
        // would remove the guard entry and miss the free, leaking the buffer.
        let mut live = super::live_allocs_lock();
        let mut map = ALLOCATIONS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        live.insert(id, len);
        map.insert(id, (ptr as usize, capacity));
        (id, capacity)
    }

    /// Reclaim a host-shim allocation. `expected_len` is the caller-reported
    /// size; reconstruction always uses the capacity stored at mint time.
    pub fn free(ptr: u32, expected_len: u32) {
        if ptr == 0 {
            return;
        }
        let mut map = ALLOCATIONS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((raw_addr, cap)) = map.remove(&ptr) {
            let _ = expected_len; // host report ignored; drop uses mint capacity
            unsafe {
                let _ = Vec::from_raw_parts(raw_addr as *mut u8, 0, cap);
            }
        }
    }

    pub fn resolve(ptr: u32) -> *const u8 {
        if ptr == 0 {
            return std::ptr::null();
        }
        let map = ALLOCATIONS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.get(&ptr).map_or(std::ptr::null(), |&(addr, _)| addr as *const u8)
    }
}

/// Allocate `len` bytes of linear memory and return the pointer.
///
/// A zero length returns `0`. The host writes into the region, then
/// either passes it to [`em_run`] or frees it with [`em_free`].
///
/// Leak-until-`em_free` is the transfer protocol (`mem::forget` inside):
/// the region stays alive until the host frees it, and the `LIVE_ALLOCS`
/// entry dies with the process on a `wasm` abort (`panic = abort`), so
/// there is no cross-invocation accumulation beyond the host's own
/// forgetting.
///
/// # Pointer width
/// - On `wasm32`, linear memory pointers are 32-bit, so `u32` matches
///   `usize` losslessly.
/// - On 64-bit host builds (unit tests), an internal table routes 32-bit
///   IDs to native pointers to avoid 64-to-32-bit truncation.
#[unsafe(no_mangle)]
pub extern "C" fn em_alloc(len: u32) -> u32 {
    alloc_region(len).0
}

/// Build an exact-size region; returns `(address, capacity)`.
///
/// Private seam so `#[cfg(test)]` can assert the exact-capacity invariant
/// (site 1) through the real production construction path.
fn alloc_region(len: u32) -> (u32, usize) {
    if len == 0 {
        return (0, 0);
    }
    #[cfg(target_arch = "wasm32")]
    {
        // `vec![0; len]` builds from `RawVec` with capacity exactly `len`
        // (site 1). This is the load-bearing half of the `em_free`
        // reconstruction contract: `from_raw_parts(ptr, 0, len)` sets cap ==
        // len, and the drop's `free` size only matches the allocator's if the
        // original capacity is exactly len. A sized repeat cannot
        // over-allocate, so the capacity coupling is sound.
        let mut buf = vec![0u8; len as usize];
        let capacity = buf.capacity();
        debug_assert_eq!(capacity, len as usize, "exact-capacity invariant (ffi site 1)");
        let ptr = buf.as_mut_ptr();
        // Invariant, no `unsafe` block here (safe calls, unsafe-adjacent):
        // `mem::forget(buf)` leaks the allocation until `em_free`. Leak-until-
        // em_free is the protocol: the host's JS `finally` blocks always free
        // every address this returns (module invariant 2), and `len == 0`
        // already returned `0` above so nothing is leaked for the null case.
        // `ptr as u32` truncation is lossless on wasm32 (address == usize ==
        // 32-bit); `vec![0; len]` keeps capacity exactly len so `em_free`'s
        // from_raw_parts reconstructs an identical Vec (site 1).
        std::mem::forget(buf);
        // Minted-and-owed: register address → capacity so em_free accepts it
        // exactly once and reconstructs with the mint-time size. Never
        // registers the len==0 null case (returned above).
        live_allocs_lock().insert(ptr as u32, len);
        (ptr as u32, capacity)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        host_alloc::alloc(len)
    }
}

/// Reclaim a region previously returned by [`em_alloc`] or [`em_run`].
///
/// # Safety (host contract)
///
/// `ptr` must be a live address from [`em_alloc`]. `ptr == 0` is a no-op
/// (the `len == 0` allocation). The host should pass the original `len`;
/// reconstruction uses the capacity stored at mint time, so a mismatched
/// `len` is ignored for allocator sizing (no UB) rather than trusted.
#[unsafe(no_mangle)]
pub extern "C" fn em_free(ptr: u32, len: u32) {
    if ptr == 0 {
        return;
    }
    // Guard (step 0): accept only addresses this module minted and still
    // owes. A foreign pointer, a double-free of an address with no current
    // membership, or a stale pointer whose address was NOT re-minted fails
    // the membership check and returns here as a provable no-op — before any
    // dereference. Generation-blind by design: a stale pointer whose address
    // a later `em_alloc` reused passes membership and is handled as that mint
    // (the sequence requires the host to already be violating the exactly-
    // once contract, so it stays a documented ABI violation, not a guard-
    // guaranteed no-op). So the `Vec::from_raw_parts` block below runs only
    // on addresses with a current membership record, and at most once per
    // mint (a stray second call for the same address is a no-op unless a
    // re-mint intervened). Capacity comes from the map value, never from the
    // host's `len` argument.
    let Some(cap) = live_allocs_lock().remove(&ptr) else {
        return;
    };
    let _ = len; // host report ignored; drop uses mint capacity `cap`
    // SAFETY: `Vec::from_raw_parts(ptr, 0, cap)` reconstructs ownership of a
    // Vec the caller previously leaked via `mem::forget` in `alloc_region`.
    //
    // (0) Invariant locally enforced by LIVE_ALLOCS membership (single-
    //     threaded wasm; Mutex uncontended): this block runs only on
    //     addresses this module minted and still owes, exactly-once.
    //     Foreign/double/stale pointers never reach this block.
    // (1) Valid ownership: `ptr` is a live mint from `em_alloc`, still
    //     leaked and not double-freed (guard, step 0). Reconstructing it
    //     here transfers that responsibility back to the Vec, whose drop
    //     now frees it.
    // (2) Capacity coupling: `cap` is the exact capacity recorded when
    //     `vec![0; len]` minted this region (sized repeat ⇒ capacity ==
    //     requested len). `from_raw_parts(ptr, 0, cap)` matches the
    //     allocator footprint; the host's `len` argument is not used for
    //     the drop size, so a mismatched report cannot induce UB.
    // (3) len == 0 handled above: `ptr == 0` returns early as a no-op, so
    //     the zero-length (null) allocation is never reconstructed here.
    // (4) `u32 -> usize` widens losslessly on wasm32; `_ =` deliberately
    //     drops the Vec by binding.
    // Residual: if the guard and reality disagree (allocator swap, memory
    //     corruption) such that a registered `ptr`'s backing no longer
    //     matches the stored capacity, `from_raw_parts` can panic/UB. The
    //     guard enforces minted-and-owed + recorded capacity, not the
    //     allocator's physical layout against external corruption.
    // Enforced by: LIVE_ALLOCS membership + stored capacity (clauses 0–2)
    // and `alloc_region`'s exact-capacity construction. Failure = host
    // feeding a foreign/double pair or memory corruption, not a library bug.
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, 0, cap as usize);
    }
    #[cfg(not(target_arch = "wasm32"))]
    host_alloc::free(ptr, cap);
}

/// Dispatch `op` / `payload` and return a packed JSON allocation.
///
/// The `u64` is `(ptr as u64) << 32 | (len as u64)`. The host copies
/// `[ptr, ptr + len)` then calls [`em_free`]`(ptr, len)`.
///
/// # Safety (host contract)
///
/// `op_ptr`/`op_len` and `payload_ptr`/`payload_len` must name valid
/// initialized byte regions in linear memory (module docs invariant 3).
#[unsafe(no_mangle)]
pub extern "C" fn em_run(op_ptr: u32, op_len: u32, payload_ptr: u32, payload_len: u32) -> u64 {
    install_panic_hook();
    let op = match read_utf8(op_ptr, op_len) {
        Ok(op) => op,
        Err(error) => return pack_json(&crate::error_json(error)),
    };
    let payload = match read_utf8(payload_ptr, payload_len) {
        Ok(payload) => payload,
        Err(error) => return pack_json(&crate::error_json(error)),
    };
    pack_json(&run_op(op, payload))
}

fn read_utf8<'a>(ptr: u32, len: u32) -> Result<&'a str, &'static str> {
    if len == 0 {
        return Ok("");
    }
    if ptr == 0 {
        return Err("invalid UTF-8 input");
    }
    if ptr >= (1_u32 << 31) || len > (1_u32 << 30) {
        return Err("invalid UTF-8 input");
    }
    #[cfg(target_arch = "wasm32")]
    let raw_ptr = ptr as *const u8;
    #[cfg(not(target_arch = "wasm32"))]
    let raw_ptr = {
        // Host builds mint opaque IDs, not native addresses. An unresolved
        // id must not be cast to a pointer — that would be dangling UB.
        let resolved = host_alloc::resolve(ptr);
        if resolved.is_null() {
            return Err("invalid UTF-8 input");
        }
        resolved
    };
    // SAFETY: `slice::from_raw_parts(raw_ptr, len)` is sound only if the
    // whole `[raw_ptr, raw_ptr + len)` range is in-bounds, aligned, valid,
    // and initialized for the `'a` the slice is borrowed for.
    //
    // (1) Host-initialized region: `len == 0` returned above; otherwise the
    //     host wrote `len` bytes at the `em_alloc`-returned address, which
    //     this module guarantees has capacity >= len (module invariant 3).
    // (2) Alignment/validity: `*const u8` is always aligned, and the region
    //     is caller-initialized (clause 1), so the bytes are valid for the
    //     call duration only (`'a` is the function's anonymous lifetime).
    // (3) Guards precede construction: `ptr == 0`, `ptr >= 1 << 31`, and
    //     `len > 1 GiB` all reject via the `Err` path before this block, so
    //     the slice is never built over a null/oversize range.
    // (4) u32 -> usize bounds: widens losslessly on wasm32 (both 32-bit) and
    //     on 64-bit hosts; `ptr >= 1<<31` keeps the wasm32 window under the
    //     mid-point of a max 4 GiB linear heap, so `raw_ptr + len` stays
    //     addressable (len capped at 1 GiB).
    // Enforced by: the host ABI contract (clauses 1-2) and the guards above
    // (clauses 3-4). Failure = a host feeding a non-owned/null/oversize
    // pointer, an ABI violation, not a library bug.
    let bytes = unsafe { slice::from_raw_parts(raw_ptr, len as usize) };
    std::str::from_utf8(bytes).map_err(|_| "invalid UTF-8 input")
}

fn pack_json(json: &str) -> u64 {
    let bytes = json.as_bytes();
    // Refuse rather than silently truncate a >u32::MAX response into a
    // corrupt (ptr,len) pack the host would treat as complete JSON.
    let Ok(len) = u32::try_from(bytes.len()) else {
        return 0;
    };
    let copy_len = bytes.len();
    let ptr = em_alloc(len);
    // Non-empty response must mint a real region. A zero ptr with nonzero
    // len would be an invalid pack (host would read from linear-memory
    // base). Recurse once through the error path only when the primary
    // alloc failed — error JSON is tiny and cannot itself OOM in practice;
    // if it did, return the empty pack rather than loop.
    if copy_len != 0 && ptr == 0 {
        if json.starts_with("{\"ok\": false") {
            return 0;
        }
        return pack_json(&crate::error_json("wasm response allocation failed"));
    }
    if copy_len != 0 && ptr != 0 {
        #[cfg(target_arch = "wasm32")]
        let dst = ptr as *mut u8;
        #[cfg(not(target_arch = "wasm32"))]
        let dst = host_alloc::resolve(ptr).cast_mut();

        if dst.is_null() {
            em_free(ptr, len);
            if json.starts_with("{\"ok\": false") {
                return 0;
            }
            return pack_json(&crate::error_json("wasm response allocation failed"));
        }
        // SAFETY: `ptr::copy_nonoverlapping(bytes.as_ptr(), dst, copy_len)`
        // requires `copy_len` writable bytes at `dst`, readable bytes at
        // the source, and the two ranges non-overlapping.
        //
        // (1) Fresh dst: `dst` is a region just returned by
        //     `em_alloc(len)` (or its host-table alias) with capacity
        //     >= copy_len == bytes.len(); it is not otherwise referenced
        //     and is exclusively owned by this write.
        // (2) Non-overlap by construction: `dst` is freshly allocated and
        //     never aliases the static `bytes` input buffer; the only
        //     degenerate case is empty JSON (`copy_len == 0`), which the
        //     enclosing `copy_len != 0 && ptr != 0` guard skips.
        // (3) Readable source / writable dst: `bytes` borrows the call's
        //     `&str`; `dst` is free, writable slack the host expects to be
        //     overwritten (module invariant round-trip).
        // (4) Length bound: `u32::try_from(bytes.len())` above returns 0
        //     (no pack) when the response exceeds u32::MAX, so this copy
        //     only runs when `len == bytes.len()` fits in `u32` and
        //     `copy_len == bytes.len()`.
        // Enforced by: `alloc_region`'s exact-capacity contract (clause 1)
        // and the fallible length check above (clause 4). Failure = a host
        // that overwrote the fresh region between `em_alloc` and this copy,
        // an ABI violation.
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), dst, copy_len);
        }
    }
    (u64::from(ptr) << 32) | u64::from(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test seam: current count of minted-and-owed live allocations.
    ///
    /// `allow(dead_code)`: this seam is public observation surface for
    /// debugging/regression scripts, not every test must call it. Safe under
    /// membership-based tests because `LIVE_ALLOCS` is shared across parallel
    /// test threads, so absolute counts are only reliable from a harness that
    /// controls the full set.
    #[allow(dead_code)]
    fn live_alloc_count() -> usize {
        live_allocs_lock().len()
    }

    #[test]
    fn test_em_init() {
        em_init();
    }

    #[test]
    fn test_em_free_zero() {
        em_free(0, 0);
        // ptr == 0 never registers (len == 0 mint path returns 0 unregistered).
        assert!(!live_allocs_lock().contains_key(&0));
    }

    #[test]
    fn test_em_alloc_zero_not_registered() {
        assert_eq!(em_alloc(0), 0);
        assert!(
            !live_allocs_lock().contains_key(&0),
            "null never mints a guard entry"
        );
    }

    #[test]
    fn test_em_free_double_free_noop() {
        let p = em_alloc(64);
        assert_ne!(p, 0);
        assert_eq!(
            live_allocs_lock().get(&p).copied(),
            Some(64),
            "alloc mints ptr→capacity"
        );

        em_free(p, 64);
        assert!(
            !live_allocs_lock().contains_key(&p),
            "first free reclaims the entry"
        );

        // Second free of the same pair is a provable no-op: no re-deref, no
        // panic. `p` is our own unique handle (monotonic id), so membership
        // checks are race-free even alongside parallel tests.
        em_free(p, 64);
        assert!(
            !live_allocs_lock().contains_key(&p),
            "double free leaves no residue"
        );
    }

    #[test]
    fn test_em_free_unminted_ptr_noop() {
        // A value this module never minted (arbitrary high handle). Guard
        // rejects it before any dereference; must not crash. The handle is
        // not ours and not reachable by concurrent ids, so the containment
        // check is stable.
        let unminted = 0xDEAD_BEEF_u32;
        assert!(!live_allocs_lock().contains_key(&unminted));
        em_free(unminted, 16);
        assert!(!live_allocs_lock().contains_key(&unminted));
    }

    #[test]
    fn test_em_free_mismatched_len_uses_stored_capacity() {
        // Host lies about len; reconstruction must still use mint capacity
        // (no allocator UB). Drop sizing reads the map value, not `len`.
        let p = em_alloc(32);
        assert_ne!(p, 0);
        assert_eq!(live_allocs_lock().get(&p).copied(), Some(32));
        em_free(p, 9999);
        assert!(
            !live_allocs_lock().contains_key(&p),
            "mismatched len still reclaims via stored capacity"
        );
        em_free(p, 32); // double-free remains a no-op
    }

    #[test]
    fn test_read_utf8_unresolved_host_id_rejected() {
        // On the host shim, IDs are opaque. An unminted id must not be
        // interpreted as a native pointer.
        assert_eq!(read_utf8(0xDEAD_BEEF, 4), Err("invalid UTF-8 input"));
    }

    #[test]
    fn test_read_utf8_empty_and_null() {
        assert_eq!(read_utf8(0, 0), Ok(""));
        assert_eq!(read_utf8(0, 10), Err("invalid UTF-8 input"));
    }

    #[test]
    fn test_em_alloc_capacity_exact() {
        for len in [0, 1, 7, 8, 64, 4096, 65536, 1 << 20] {
            let (ptr, capacity) = alloc_region(len);
            assert_eq!(capacity, len as usize, "exact capacity for len={len}");
            if len == 0 {
                assert_eq!(ptr, 0, "zero length yields null");
            } else {
                assert_ne!(ptr, 0, "non-null address for len={len}");
                em_free(ptr, len);
            }
        }
    }

    #[test]
    fn test_em_alloc_free_roundtrip_repeated() {
        for i in 1..=256_u32 {
            let size = i.wrapping_mul(37) % 4096 + 1;
            let ptr = em_alloc(size);
            assert_ne!(ptr, 0);
            em_free(ptr, size);
        }
        assert_eq!(em_alloc(0), 0);
        em_free(0, 0);
    }

    #[test]
    fn test_em_alloc_stability_thousand() {
        // alloc -> free -> alloc repeatedly at varied sizes: catches any
        // capacity-coupling or leak-accounting drift across many cycles
        // (site 1). Non-zero handles every time; no crash, no double-free.
        for i in 0..1000_u32 {
            let size = (i.wrapping_mul(101) % 8192) + 1;
            let a = em_alloc(size);
            assert_ne!(a, 0, "non-null handle at iter {i}");
            em_free(a, size);
            let b = em_alloc(size);
            assert_ne!(b, 0, "re-alloc non-null at iter {i}");
            em_free(b, size);
        }
    }

    #[test]
    fn test_read_utf8_zero_copy() {
        let text = "let x = 42;";
        let len = text.len() as u32;
        let (ptr, capacity) = alloc_region(len);
        assert_eq!(capacity, len as usize);
        #[cfg(target_arch = "wasm32")]
        let dst = ptr as *mut u8;
        #[cfg(not(target_arch = "wasm32"))]
        let dst = host_alloc::resolve(ptr).cast_mut();
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), dst, text.len());
        }
        let got = read_utf8(ptr, len).expect("valid UTF-8");
        assert_eq!(got, text);
        em_free(ptr, len);
    }

    #[test]
    fn test_read_utf8_bounds_rejected() {
        let two_gib = 1_u32 << 31;
        let over_one_gib = (1_u32 << 30) + 1;
        assert_eq!(read_utf8(two_gib, 4), Err("invalid UTF-8 input"));
        assert_eq!(read_utf8(16, over_one_gib), Err("invalid UTF-8 input"));
        assert_eq!(read_utf8(two_gib, over_one_gib), Err("invalid UTF-8 input"));
    }
}
