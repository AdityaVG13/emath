//! SHA-256 compatibility wrapper for keep-gate identity dumps.

/// SHA-256 digest of `bytes` (FIPS 180-4), delegated to `emath-core`.
#[must_use]
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    emath_core::sha256_digest(bytes)
}

/// Lowercase hex of the digest.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
