//! ffi tests migrated from the in-crate `#[cfg(test)]` module.

use emath_wasm::ffi::*;

/// Test seam: current count of minted-and-owed live allocations
/// (`allow(dead_code)`: shared `LIVE_ALLOCS` makes counts reliable only
/// from a harness that controls the full set).
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
#[allow(unsafe_code)]
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
