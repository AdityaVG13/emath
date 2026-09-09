//! ffi tests migrated from the in-crate `#[cfg(test)]` module.

use emath_test_harness::Probe;
use emath_wasm::ffi::*;

#[test]
#[allow(unsafe_code)]
fn ffi() {
    let mut p = Probe::new("wasm alloc/free guards stay total and UTF-8 reads stay bounded");
    em_init();
    p.case("null", |p| {
        em_free(0, 0);
        p.demand("zero-unregistered", !live_allocs_lock().contains_key(&0), "null never registers");
        p.eq("alloc-zero", em_alloc(0), 0);
        p.demand("zero-guard", !live_allocs_lock().contains_key(&0), "zero never mints");
    });
    p.case("free", |p| {
        let a = em_alloc(64);
        p.ne("nonnull", a, 0);
        p.eq("capacity", live_allocs_lock().get(&a).copied(), Some(64));
        em_free(a, 64);
        p.demand("reclaimed", !live_allocs_lock().contains_key(&a), "first free reclaims");
        em_free(a, 64);
        p.demand("double-noop", !live_allocs_lock().contains_key(&a), "double free leaves nothing");
        let unminted = 0xDEAD_BEEF_u32;
        p.demand("unminted-absent", !live_allocs_lock().contains_key(&unminted), "unminted absent");
        em_free(unminted, 16);
        p.demand("unminted-noop", !live_allocs_lock().contains_key(&unminted), "unminted free is a no-op");
        let b = em_alloc(32);
        em_free(b, 9999);
        p.demand("mismatched", !live_allocs_lock().contains_key(&b), "stored capacity reclaims");
        em_free(b, 32);
    });
    p.case("alloc-table", |p| {
        for len in [0, 1, 7, 8, 64, 4096, 65536, 1 << 20] {
            let (ptr, capacity) = alloc_region(len);
            p.eq(format!("cap/{len}"), capacity, len as usize);
            if len == 0 {
                p.eq("null", ptr, 0);
            } else {
                p.ne(format!("nonnull/{len}"), ptr, 0);
                em_free(ptr, len);
            }
        }
        for i in 1..=256_u32 {
            let size = i.wrapping_mul(37) % 4096 + 1;
            let ptr = em_alloc(size);
            p.ne(format!("roundtrip/{i}"), ptr, 0);
            em_free(ptr, size);
        }
        p.eq("zero-again", em_alloc(0), 0);
        for i in 0..1000_u32 {
            let size = (i.wrapping_mul(101) % 8192) + 1;
            let a = em_alloc(size);
            p.ne(format!("stable-a/{i}"), a, 0);
            em_free(a, size);
            let b = em_alloc(size);
            p.ne(format!("stable-b/{i}"), b, 0);
            em_free(b, size);
        }
    });
    p.case("utf8", |p| {
        p.eq("unresolved", read_utf8(0xDEAD_BEEF, 4), Err("invalid UTF-8 input"));
        p.eq("empty", read_utf8(0, 0), Ok(""));
        p.eq("null-data", read_utf8(0, 10), Err("invalid UTF-8 input"));
        p.eq("oob1", read_utf8(1_u32 << 31, 4), Err("invalid UTF-8 input"));
        p.eq("oob2", read_utf8(16, (1_u32 << 30) + 1), Err("invalid UTF-8 input"));
        let text = "let x = 42;";
        let len = text.len() as u32;
        let (ptr, capacity) = alloc_region(len);
        p.eq("exact", capacity, len as usize);
        #[cfg(target_arch = "wasm32")]
        let dst = ptr as *mut u8;
        #[cfg(not(target_arch = "wasm32"))]
        let dst = host_alloc::resolve(ptr).cast_mut();
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), dst, text.len()) };
        p.eq("round-trip", read_utf8(ptr, len), Ok(text));
        em_free(ptr, len);
    });
    p.finish();
}
