//! Bounded compilation: source, token, nesting and diagnostic limits.
//!
//! All limits are advisory ceilings that the parser enforces with
//! `E-SYN-1xx` / `E-RES-1xx` diagnostics instead of panicking.

/// Maximum accepted source size in bytes (1 MiB).
pub const DEFAULT_MAX_SOURCE_BYTES: usize = 1 << 20;
/// Maximum number of tokens lexed per source file.
pub const DEFAULT_MAX_TOKENS: usize = 65_536;
/// Maximum nesting depth for suites/delimiters/expressions.
pub const DEFAULT_MAX_NESTING: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_source_bytes: usize,
    pub max_tokens: usize,
    pub max_nesting: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_source_bytes: DEFAULT_MAX_SOURCE_BYTES,
            max_tokens: DEFAULT_MAX_TOKENS,
            max_nesting: DEFAULT_MAX_NESTING,
        }
    }
}

impl Limits {
    pub fn check_source(&self, bytes: usize) -> Result<(), usize> {
        if bytes <= self.max_source_bytes {
            Ok(())
        } else {
            Err(self.max_source_bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_reject_oversized_source() {
        let limits = Limits::default();
        assert!(limits.check_source(limits.max_source_bytes).is_ok());
        assert!(limits.check_source(limits.max_source_bytes + 1).is_err());
    }
}
