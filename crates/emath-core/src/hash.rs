//! Bootstrap content identity.
//!
//! FNV-1a 64-bit. This is a bootstrap fingerprint for deterministic
//! artifact/source identity during Phase 1. Per AGENTS.md it is NOT a release
//! cryptographic identity and must be replaced before stable publication.

use crate::id::ContentId;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64-bit fingerprint over `bytes`.
#[must_use]
pub fn fnv1a64_bytes(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Bootstrap content identity over arbitrary bytes.
#[must_use]
pub fn bootstrap_content_id(bytes: &[u8]) -> ContentId {
    ContentId(format!("fnv1a64:{:016x}", fnv1a64_bytes(bytes)))
}

/// Bootstrap content identity over a string.
#[must_use]
pub fn content_id_of_str(text: &str) -> ContentId {
    bootstrap_content_id(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_id_is_stable_and_sensitive() {
        assert_eq!(
            bootstrap_content_id(b"emath"),
            bootstrap_content_id(b"emath")
        );
        assert_ne!(
            bootstrap_content_id(b"emath"),
            bootstrap_content_id(b"eMath")
        );
        assert_ne!(
            bootstrap_content_id(b"emath"),
            bootstrap_content_id(b"emath ")
        );
    }

    #[test]
    fn fnv_known_vector() {
        // FNV-1a 64 of empty input is the offset basis.
        assert_eq!(fnv1a64_bytes(b""), FNV_OFFSET);
    }
}
