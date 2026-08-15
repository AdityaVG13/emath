//!: provider diagnostic mapping.
//!
//! Provider diagnostics preserve their body for debugging while mapping to
//! emath component paths and spans. A missing source-map entry is reported
//! explicitly (`E-PROV-310`), never silently dropped.

use std::collections::BTreeMap;

use emath_core::Span;

/// Diagnostic as produced by an adapter provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderDiagnostic {
    /// Provider-native code.
    pub code: String,
    /// Provider-native message.
    pub message: String,
    /// Component path the diagnostic refers to.
    pub path: String,
}

/// Mapped emath diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedDiagnostic {
    /// Stable emath code (`E-PROV-300`/`E-PROV-310`).
    pub code: &'static str,
    /// Message with provider body preserved.
    pub message: String,
    /// Original emath span when the source map has the path.
    pub span: Option<Span>,
}

/// Maps a provider diagnostic through a component-path source map.
#[must_use]
pub fn map(
    diagnostic: &ProviderDiagnostic,
    source_map: &BTreeMap<String, Span>,
) -> MappedDiagnostic {
    match source_map.get(&diagnostic.path) {
        Some(span) => MappedDiagnostic {
            code: "E-PROV-300",
            message: format!(
                "{} (provider code {} for component {})",
                diagnostic.message, diagnostic.code, diagnostic.path
            ),
            span: Some(*span),
        },
        None => MappedDiagnostic {
            code: "E-PROV-310",
            message: format!(
                "source-map loss: no emath span for component {} (provider code {})",
                diagnostic.path, diagnostic.code
            ),
            span: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_core::{FileId, Span};

    #[test]
    fn mapped_diagnostic_keeps_provider_body_and_span() {
        let mut source_map = BTreeMap::new();
        source_map.insert("mass.der_x".to_string(), Span::new(FileId(1), 10, 30));
        let mapped = map(
            &ProviderDiagnostic {
                code: "RB-42".into(),
                message: "singular system".into(),
                path: "mass.der_x".into(),
            },
            &source_map,
        );
        assert_eq!(mapped.code, "E-PROV-300");
        assert!(mapped.message.contains("RB-42"));
        assert!(mapped.message.contains("singular system"));
        assert_eq!(mapped.span, Some(Span::new(FileId(1), 10, 30)));
    }

    #[test]
    fn missing_source_map_entry_is_explicit_loss() {
        let mapped = map(
            &ProviderDiagnostic {
                code: "RB-7".into(),
                message: "unknown".into(),
                path: "ghost".into(),
            },
            &BTreeMap::new(),
        );
        assert_eq!(mapped.code, "E-PROV-310");
        assert_eq!(mapped.span, None);
    }
}
