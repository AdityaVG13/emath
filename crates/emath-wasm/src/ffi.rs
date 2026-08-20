//! C ABI leaf for the wasm engine.
//!
//! # Safety
//!
//! This is the only \`unsafe\` module in the crate. Every \`unsafe\` block is
//! a documented pointer/length pairing with the JS host:
//!
//! 1. [\`em_alloc\`] returns either \`0\` (\`len == 0\`) or the start of a
//!    \`Vec<u8>\` allocation with **capacity == \`len\` exactly** (site-1
//!    proof: [\`Vec::with_capacity\`] stores the requested capacity verbatim
//!    (RawVec has no amortized-growth path at allocation time), so
//!    \`capacity()\` equals \`len\`; pinned by \`test_em_alloc_capacity_exact\`)
//!    and **length = 0**, leaked via [\`std::mem::forget\`]. The host owns
//!    that region until [\`em_free\`].
//! 2. [\`em_free\`] reconstructs that \`Vec\` with \`from_raw_parts(ptr, 0, len)\`
//!    and drops it. \`ptr\`/\`len\` must match a live allocation from
//!    [\`em_alloc\`] (including the JSON buffer returned by [\`em_run\`]).
//!    Double-free or a mismatched length is undefined.
//! 3. [\`em_run\`] reads \`[op_ptr, op_ptr + op_len)\` and
//!    \`[payload_ptr, payload_ptr + payload_len)\` as UTF-8. Those slices
//!    must be valid, initialized, and not freed for the duration of the
//!    call. The packed return value names a fresh [\`em_alloc\`] region
//!    the host must copy and then free.
//!
//! # Why \`#![allow(unsafe_code)]\` stays (edition 2024)
//!
//! The four exported entry points carry \`#[unsafe(no_mangle)]\`. In edition
//! 2024 an unsafe attribute is itself governed by the \`unsafe_code\` lint,
//! so nested under \`lib.rs\`'s \`#![deny(unsafe_code)]\` this module needs
//! the allowance for those attributes alone, before any \`unsafe\` block. The
//! allowance cannot be dropped without redesigning symbol export (e.g. a
//! generated shim crate or linker script), which the web host protocol does
//! not warrant: every \`unsafe\` block below is a numbered, proof-obligation
//! site, and \`lib.rs\` confines all of them to this leaf module. Comment
//! only, no code change.

#![allow(unsafe_code)]
#![allow(clippy::cast_possible_truncation)]

use std::ptr;
use std::slice;
use std::sync::Once;

use crate::run_op;

static INIT_PANIC_HOOK: Once = Once::new();

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
        let mut buf = Vec::<u8>::with_capacity(len as usize);
        let capacity = buf.capacity();
        let ptr = buf.as_mut_ptr();
        std::mem::forget(buf);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut map = ALLOCATIONS.lock().unwrap();
        map.insert(id, (ptr as usize, capacity));
        (id, capacity)
    }

    pub fn free(ptr: u32, len: u32) {
        if ptr == 0 {
            return;
        }
        let mut map = ALLOCATIONS.lock().unwrap();
        if let Some((raw_addr, _cap)) = map.remove(&ptr) {
            unsafe {
                let _ = Vec::from_raw_parts(raw_addr as *mut u8, 0, len as usize);
            }
        }
    }

    pub fn resolve(ptr: u32) -> *const u8 {
        if ptr == 0 {
            return std::ptr::null();
        }
        let map = ALLOCATIONS.lock().unwrap();
        map.get(&ptr).map_or(std::ptr::null(), |&(addr, _)| addr as *const u8)
    }
}

/// Allocate \`len\` bytes of linear memory and return the pointer.
///
/// A zero length returns \`0\`. The host writes into the region, then
/// either passes it to [\`em_run\`] or frees it with [\`em_free\`].
///
/// # Pointer width
/// - On \`wasm32\`, linear memory pointers are 32-bit, so \`u32\` matches
///   \`usize\` losslessly.
/// - On 64-bit host builds (unit tests), an internal table routes 32-bit
///   IDs to native pointers to avoid 64-to-32-bit truncation.
#[unsafe(no_mangle)]
pub extern "C" fn em_alloc(len: u32) -> u32 {
    alloc_region(len).0
}

/// Build an exact-size region; returns \`(address, capacity)\`.
///
/// Private seam so \`#[cfg(test)]\` can assert the exact-capacity invariant
/// (site 1) through the real production construction path.
fn alloc_region(len: u32) -> (u32, usize) {
    if len == 0 {
        return (0, 0);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut buf = Vec::<u8>::with_capacity(len as usize);
        let capacity = buf.capacity();
        debug_assert_eq!(capacity, len as usize, "exact-capacity invariant (ffi site 1)");
        let ptr = buf.as_mut_ptr();
        std::mem::forget(buf);
        (ptr as u32, capacity)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        host_alloc::alloc(len)
    }
}

/// Reclaim a region previously returned by [\`em_alloc\`] or [\`em_run\`].
///
/// # Safety (host contract)
///
/// \`ptr\` and \`len\` must be a live pair from [\`em_alloc\`]. \`ptr == 0\` is a
/// no-op (the \`len == 0\` allocation).
#[unsafe(no_mangle)]
pub extern "C" fn em_free(ptr: u32, len: u32) {
    if ptr == 0 {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, 0, len as usize);
    }
    #[cfg(not(target_arch = "wasm32"))]
    host_alloc::free(ptr, len);
}

/// Dispatch \`op\` / \`payload\` and return a packed JSON allocation.
///
/// The \`u64\` is \`(ptr as u64) << 32 | (len as u64)\`. The host copies
/// \`[ptr, ptr + len)\` then calls [\`em_free\`]\`(ptr, len)\`.
///
/// # Safety (host contract)
///
/// \`op_ptr\`/\`op_len\` and \`payload_ptr\`/\`payload_len\` must name valid
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
        let resolved = host_alloc::resolve(ptr);
        if resolved.is_null() {
            ptr as usize as *const u8
        } else {
            resolved
        }
    };
    let bytes = unsafe { slice::from_raw_parts(raw_ptr, len as usize) };
    std::str::from_utf8(bytes).map_err(|_| "invalid UTF-8 input")
}

fn pack_json(json: &str) -> u64 {
    let bytes = json.as_bytes();
    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    let copy_len = bytes.len().min(len as usize);
    let ptr = em_alloc(len);
    if copy_len != 0 && ptr != 0 {
        #[cfg(target_arch = "wasm32")]
        let dst = ptr as *mut u8;
        #[cfg(not(target_arch = "wasm32"))]
        let dst = host_alloc::resolve(ptr) as *mut u8;

        if !dst.is_null() {
            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), dst, copy_len);
            }
        }
    }
    (u64::from(ptr) << 32) | u64::from(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_em_init() {
        em_init();
    }

    #[test]
    fn test_em_free_zero() {
        em_free(0, 0);
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
    fn test_read_utf8_zero_copy() {
        let text = "let x = 42;";
        let len = text.len() as u32;
        let (ptr, capacity) = alloc_region(len);
        assert_eq!(capacity, len as usize);
        #[cfg(target_arch = "wasm32")]
        let dst = ptr as *mut u8;
        #[cfg(not(target_arch = "wasm32"))]
        let dst = host_alloc::resolve(ptr) as *mut u8;
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
