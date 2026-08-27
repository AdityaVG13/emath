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
pub(crate) struct PreparedSource<'a> {
    pub source: Cow<'a, str>,
    pub is_wrapped: bool,
}

impl<'a> PreparedSource<'a> {
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
#[must_use]
pub(crate) fn prepare_source<'a>(raw: &'a str) -> PreparedSource<'a> {
    let expansion = expand_scratch(raw);
    if expansion.rewritten && !expansion.diagnostics.has_errors() {
        PreparedSource {
            source: Cow::Owned(expansion.expanded),
            is_wrapped: true,
        }
    } else {
        PreparedSource {
            source: Cow::Borrowed(raw),
            is_wrapped: false,
        }
    }
}
