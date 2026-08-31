//! Deterministic Unicode text identity.

use unicode_normalization::UnicodeNormalization;

/// Return Unicode NFC. Text literals are normalized before semantic
/// identity is computed, so canonically equivalent spellings share one
/// meaning.
#[must_use]
pub fn normalize_nfc(value: &str) -> String {
    value.nfc().collect()
}
