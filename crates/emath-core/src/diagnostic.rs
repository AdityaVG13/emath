//! Stable diagnostics with typed codes (see the diagnostics contract in
//! `language/reference/diagnostics-and-tooling-contract.md`).

use crate::span::Span;
use std::fmt;

/// Severity of a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// One diagnostic. `code` is a stable identifier such as `E-TYPE-002`; codes
/// are never repurposed and message text may improve without changing meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub primary: Span,
    pub related: Vec<(Span, String)>,
    pub help: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn error(code: &'static str, message: impl Into<String>, primary: Span) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            primary,
            related: Vec::new(),
            help: None,
        }
    }

    #[must_use]
    pub fn warning(code: &'static str, message: impl Into<String>, primary: Span) -> Self {
        Self {
            code,
            severity: Severity::Warning,
            message: message.into(),
            primary,
            related: Vec::new(),
            help: None,
        }
    }

    #[must_use]
    pub fn with_note(mut self, span: Span, note: impl Into<String>) -> Self {
        self.related.push((span, note.into()));
        self
    }

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}\u{2009}{} (file {}, bytes {}..{})",
            self.code, self.message, self.primary.file.0, self.primary.start, self.primary.end
        )
    }
}

/// Bounded diagnostic sink. The parser and semantic passes never panic.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
    max: usize,
}

impl Diagnostics {
    /// Default maximum number of retained diagnostics.
    pub const DEFAULT_MAX: usize = 256;

    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            max: Self::DEFAULT_MAX,
        }
    }

    pub fn set_max(&mut self, max: usize) {
        self.max = max;
        self.items.truncate(max);
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        if self.items.len() < self.max {
            self.items.push(diagnostic);
        }
    }

    pub fn error(&mut self, code: &'static str, message: impl Into<String>, primary: Span) {
        self.push(Diagnostic::error(code, message, primary));
    }
    pub fn warning(&mut self, code: &'static str, message: impl Into<String>, primary: Span) {
        self.push(Diagnostic::warning(code, message, primary));
    }

    pub fn extend_from(&mut self, other: &Diagnostics) {
        for item in &other.items {
            self.push(item.clone());
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter().filter(|d| d.severity == Severity::Error)
    }

    #[must_use]
    pub fn items(&self) -> &[Diagnostic] {
        &self.items
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

impl FromIterator<Diagnostic> for Diagnostics {
    fn from_iter<T: IntoIterator<Item = Diagnostic>>(iter: T) -> Self {
        let mut out = Self::new();
        for item in iter {
            out.push(item);
        }
        out
    }
}
