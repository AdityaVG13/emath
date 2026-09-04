//! Macro expansion for build scripts.

use super::*;

/// Expansion of the `emath!` proc macro: the parsed source literal plus its
/// deterministic identity. Parsing lives here (a normal crate) so it is
/// unit-testable; the proc-macro crate is a thin shim over it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroExpansion {
    /// Parsed `.emath` source text.
    pub source: String,
    /// FNV-1a64 identity of the source.
    pub identity: String,
}

impl MacroExpansion {
    /// Used by the `emath!` proc macro to reconstruct an expansion from
    /// emitted literals (compile-time constant path).
    #[must_use]
    pub fn from_literals(source: &'static str, identity: &'static str) -> Self {
        Self {
            source: source.to_string(),
            identity: identity.to_string(),
        }
    }
}

/// Macro expansion failure (input must be a single string literal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroError {
    /// Stable code (`E-CODEGEN-011`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Parses a proc-macro token stream into a source literal. Token text is
/// parsed (never concatenated), so arbitrary input cannot inject tokens.
pub fn macro_expand(input: &str) -> Result<MacroExpansion, MacroError> {
    let input = input.trim();
    if input.starts_with('"') && input.ends_with('"') && input.len() >= 2 {
        let inner = &input[1..input.len() - 1];
        if inner.contains('"') {
            return Err(MacroError {
                code: "E-CODEGEN-011",
                message: "unescaped quotes are not supported in emath! literals".into(),
            });
        }
        let identity = emath_core::content_id_of_str(inner).0;
        Ok(MacroExpansion {
            source: inner.to_string(),
            identity,
        })
    } else {
        Err(MacroError {
            code: "E-CODEGEN-011",
            message: "`emath!` requires a single string literal of `.emath` source".into(),
        })
    }
}

/// Builds an artifact from in-memory `.emath` source (the runtime half of
/// the `emath!` macro expansion); the exact `build_text` compiler path.
pub fn build_from_source(
    name: &str,
    source: &str,
    target_dir: impl AsRef<std::path::Path>,
) -> Result<crate::BuildReport, crate::BuildError> {
    crate::build_text(name, source, target_dir, crate::BuildOptions::default())
}
