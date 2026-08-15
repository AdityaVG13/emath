//! eMath core: identity, spans, stable diagnostics, limits, content identity.
//!
//! Tier 0 of the canonical crate map. Std only, no provider concepts.

#![forbid(unsafe_code)]

pub mod diagnostic;
pub mod hash;
pub mod id;
pub mod limits;
pub mod span;

pub use diagnostic::{Diagnostic, Diagnostics, Severity};
pub use hash::{bootstrap_content_id, content_id_of_str, fnv1a64_bytes};
pub use id::{ContentId, FileId, QualifiedName, SchemaId};
pub use span::Span;
