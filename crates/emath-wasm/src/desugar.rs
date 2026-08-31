//! Playground wrap: official L0/L1 scratch expansion from `emath-syntax`.
//!
//! The same rewrite is applied by `emath-syntax::parse`, so the pane and the
//! CLI share one contracted declaration IR. `desugared_source` is the inspectable
//! expansion (`emath expand` on the host).

use std::borrow::Cow;

use emath_syntax::expand_scratch;

/// Source after official scratch expansion, plus the visible desugared text
/// when wrapping happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedSource<'a> {
    /// Source text after scratch-expansion, ready for the parser.
    pub source: Cow<'a, str>,
    /// True when bare pane text was wrapped in a scratch declaration.
    pub is_wrapped: bool,
}

impl<'a> PreparedSource<'a> {
    /// Borrow the prepared text as a `&str` slice when no wrapping occurred.
    #[inline]
    #[must_use]
    pub fn desugared(&self) -> Option<&str> {
        if self.is_wrapped {
            Some(self.source.as_ref())
        } else {
            None
        }
    }
}

/// Wrap bare pane text when the first content line is not a declaration header.
///
/// Public pipeline seam: embedders and tests compile the same prepared source
/// the op layer sees, so diagnostics refer to identical text.
#[must_use]
pub fn prepare_source<'a>(raw: &'a str) -> PreparedSource<'a> {
    let expansion = expand_scratch(raw);
    let parsed = expansion.parse_source(raw);
    if std::ptr::eq(parsed, raw) {
        PreparedSource {
            source: Cow::Borrowed(raw),
            is_wrapped: false,
        }
    } else {
        PreparedSource {
            source: Cow::Owned(parsed.to_string()),
            is_wrapped: true,
        }
    }
}
