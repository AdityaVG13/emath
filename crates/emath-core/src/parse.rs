//! Source-parser seam: the parse contract lives in the kernel; the `.emath`
//! parser implementation ships in `emath-syntax`.
//!
//! Semantic admission (`emath-sema`) depends on core/ir only and reaches
//! the parser through this seam: a host installs the default
//! [`SourceParser`] once per process (`emath_syntax::install_source_parser`).
//! Until a parser is installed, session parse operations return a typed
//! refusal (E-SYN-120) instead of failing silently.

use crate::Edition;
use crate::diagnostic::Diagnostics;
use crate::id::FileId;
use crate::limits::Limits;
use crate::tree::SyntaxTree;

/// Contract implemented by a source-language parser.
pub trait SourceParser: Send + Sync {
    /// Parse in-memory `.emath` source into a syntax tree.
    fn parse(
        &self,
        text: &str,
        file: FileId,
        limits: &Limits,
        edition: Edition,
    ) -> (SyntaxTree, Diagnostics);
}

static DEFAULT_PARSER: std::sync::OnceLock<&'static dyn SourceParser> = std::sync::OnceLock::new();

/// Install the process-wide default source parser. Idempotent: the first
/// registration wins and later calls are ignored.
pub fn register_source_parser(parser: &'static dyn SourceParser) {
    let _ = DEFAULT_PARSER.set(parser);
}

/// The installed default parser, if any.
#[must_use]
pub fn source_parser() -> Option<&'static dyn SourceParser> {
    DEFAULT_PARSER.get().copied()
}
