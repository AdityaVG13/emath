//! C ABI leaf for the wasm engine.
//!
//! # Safety
//!
//! This is the only `unsafe` module in the crate. Every `unsafe` block is
//! a documented pointer/length pairing with the JS host:
//!
//! 1. [`em_alloc`] returns either `0` (`len == 0`) or the start of a
//!    `Vec<u8>` allocation with **capacity = `len`** and **length = 0**,
//!    leaked via [`std::mem::forget`]. The host owns that region until
//!    [`em_free`].
//! 2. [`em_free`] reconstructs that `Vec` with `from_raw_parts(ptr, 0, len)`
//!    and drops it. `ptr`/`len` must match a live allocation from
//!    [`em_alloc`] (including the JSON buffer returned by [`em_run`]).
//!    Double-free or a mismatched length is undefined.
//! 3. [`em_run`] reads `[op_ptr, op_ptr + op_len)` and
//!    `[payload_ptr, payload_ptr + payload_len)` as UTF-8. Those slices
//!    must be valid, initialized, and not freed for the duration of the
//!    call. The packed return value names a fresh [`em_alloc`] region
//!    the host must copy and then free.

#![allow(unsafe_code)]
#![allow(clippy::cast_possible_truncation)]

use std::ptr;
use std::slice;

use crate::run_op;

/// Allocate `len` bytes of linear memory and return the pointer.
///
/// A zero length returns `0`. The host writes into the region, then
/// either passes it to [`em_run`] or frees it with [`em_free`].
#[unsafe(no_mangle)]
pub extern "C" fn em_alloc(len: u32) -> u32 {
    if len == 0 {
        return 0;
    }
    let mut buf = Vec::<u8>::with_capacity(len as usize);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr as u32
}

/// Reclaim a region previously returned by [`em_alloc`] or [`em_run`].
///
/// # Safety (host contract)
///
/// `ptr` and `len` must be a live pair from [`em_alloc`]. `ptr == 0` is a
/// no-op (the `len == 0` allocation).
#[unsafe(no_mangle)]
pub extern "C" fn em_free(ptr: u32, len: u32) {
    if ptr == 0 {
        return;
    }
    // SAFETY: host contract (module docs invariant 2): `ptr` is the
    // exclusive allocation from `em_alloc(len)`, capacity `len`, length 0.
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, 0, len as usize);
    }
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
    let op = match read_utf8(op_ptr, op_len) {
        Ok(op) => op,
        Err(error) => return pack_json(&crate::error_json(&error)),
    };
    let payload = match read_utf8(payload_ptr, payload_len) {
        Ok(payload) => payload,
        Err(error) => return pack_json(&crate::error_json(&error)),
    };
    pack_json(&run_op(&op, &payload))
}

fn read_utf8(ptr: u32, len: u32) -> Result<String, String> {
    if len == 0 {
        return Ok(String::new());
    }
    if ptr == 0 {
        return Err("invalid UTF-8 input".to_string());
    }
    // SAFETY: host contract (module docs invariant 3): `[ptr, ptr+len)` is
    // a valid initialized region the caller owns for this call.
    let bytes = unsafe { slice::from_raw_parts(ptr as *const u8, len as usize) };
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| "invalid UTF-8 input".to_string())
}

fn pack_json(json: &str) -> u64 {
    let bytes = json.as_bytes();
    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    let ptr = em_alloc(len);
    if len != 0 && ptr != 0 {
        // SAFETY: `em_alloc(len)` just returned an exclusive region of
        // capacity `len`; `bytes.len()` fits in that region.
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        }
    }
    (u64::from(ptr) << 32) | u64::from(len)
}
