//! Constructor semantic representation.

use crate::ids::{ExprId, TypeId};
use emath_core::Span;
use std::collections::BTreeMap;

/// Constructor authority: parameters, preconditions, assignments,
/// postconditions, defaults, error type and construction failure
/// variants ( subset).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constructor {
    pub name: String,
    pub parameters: Vec<Field>,
    pub preconditions: Vec<ExprId>,
    pub assignments: BTreeMap<String, ExprId>,
    pub postconditions: Vec<ExprId>,
    /// Default values for parameters that may be omitted at call sites.
    pub defaults: BTreeMap<String, ExprId>,
    pub error_type: Option<TypeId>,
    pub is_public: bool,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: TypeId,
    pub visibility: Visibility,
    pub source: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Package,
    Private,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestCase {
    pub name: String,
    pub given: BTreeMap<String, ExprId>,
    pub expect: ExprId,
    pub source: Span,
}
