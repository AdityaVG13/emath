//! The admission pass: syntax → typed neutral SIR with stable
//! diagnostics and a source-to-SIR trace.

use emath_core::{Diagnostics, Span};
use emath_ir::{ExprNode, TypeId, TypeNode};
use std::collections::BTreeSet;

mod declaration;
use declaration::admit_constructor_declaration;
mod attributes;
mod sections_meta;
pub use sections_meta::{check_tree, check_tree_at};

pub const E_DUPLICATE_FIELD: &str = "E-NAME-020";
pub const E_UNKNOWN_VARIABLE: &str = "E-TYPE-002";
pub const E_UNKNOWN_FUNCTION: &str = "E-TYPE-003";
pub const E_UNSUPPORTED_TYPE: &str = "E-TYPE-010";

/// Constructor-layer sections. Any other section is `E-SEC-101`.
const PHASE1_SECTIONS: &[&str] = &[
    "parameters",
    "representation",
    "invariants",
    "inputs",
    "outputs",
    "definitions",
    "question",
    "using",
    "answer",
    "budget",
    "tests",
    "exports",
];

/// Folds a declaration name for confusable-collision detection; names that
/// fold alike are refused as `E-NAME-024`.
#[must_use]
pub fn confusable_fold(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        out.push(match ch {
            // Cyrillic lowercase lookalikes.
            '\u{0430}' | '\u{03B1}' => 'a', // а, α
            '\u{0435}' => 'e',              // е
            '\u{043A}' | '\u{03BA}' => 'k', // к, κ
            '\u{043C}' | '\u{03BC}' => 'm', // м, μ
            '\u{043D}' => 'h',              // н
            '\u{043E}' | '\u{03BF}' => 'o', // о, ο
            '\u{0440}' | '\u{03C1}' => 'p', // р, ρ
            '\u{0441}' => 'c',              // с
            '\u{0442}' | '\u{03C4}' => 't', // т, τ
            '\u{0443}' => 'y',              // у
            '\u{0445}' | '\u{03C7}' => 'x', // х, χ
            '\u{0455}' => 's',              // ѕ
            '\u{0456}' | '\u{03B9}' => 'i', // і, ι
            '\u{0458}' => 'j',              // ј
            // Cyrillic uppercase lookalikes.
            '\u{0410}' => 'A',              // А
            '\u{0415}' => 'E',              // Е
            '\u{041A}' | '\u{039A}' => 'K', // К, Κ
            '\u{041C}' | '\u{039C}' => 'M', // М, Μ
            '\u{041D}' => 'H',              // Н
            '\u{041E}' | '\u{039F}' => 'O', // О, Ο
            '\u{0420}' | '\u{03A1}' => 'P', // Р, Ρ
            '\u{0421}' => 'C',              // С
            '\u{0422}' | '\u{03A4}' => 'T', // Т, Τ
            '\u{0423}' => 'Y',              // У
            '\u{0425}' | '\u{03A7}' => 'X', // Х, Χ
            '\u{0405}' => 'S',              // Ѕ
            '\u{0406}' | '\u{0399}' => 'I', // І, Ι
            '\u{0408}' => 'J',              // Ј
            // Greek lowercase lookalikes.
            '\u{03BD}' => 'v', // ν
            // Greek uppercase lookalikes.
            '\u{039D}' => 'N', // Ν
            other => other,
        });
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceEntry {
    pub phase: String,
    pub detail: String,
    pub span: Option<Span>,
}

#[derive(Clone, Debug, Default)]
pub struct SemanticTrace {
    pub entries: Vec<TraceEntry>,
}

impl SemanticTrace {
    pub fn record(&mut self, phase: &str, detail: impl Into<String>, span: Option<Span>) {
        self.entries.push(TraceEntry {
            phase: phase.to_string(),
            detail: detail.into(),
            span,
        });
    }
}

#[derive(Clone, Debug)]
pub struct CheckResult {
    /// Syntax consumed by this check, retained even when semantic admission
    /// refuses it. This is not an execution or translation permission.
    pub syntax: Option<emath_core::tree::SyntaxTree>,
    /// Parser diagnostics from the same session input.
    pub parse_diagnostics: Diagnostics,
    pub package: emath_ir::SemanticPackage,
    pub diagnostics: Diagnostics,
    pub trace: SemanticTrace,
    /// Effective per-declaration units-profile table, in
    /// source order; empty when no `@units_profile` attribute exists.
    pub units_profiles: Vec<(String, String)>,
    /// The exact module admitted by this result, with imports already resolved.
    /// Absent on any parse or semantic error. Execution starts with fresh state.
    pub prepared: Option<emath_exec_ir::constructor_layer::PreparedModule>,
}

struct Admitter {
    diagnostics: Diagnostics,
    trace: Vec<TraceEntry>,
    exprs: Vec<(ExprNode, Span)>,
    types: Vec<TypeNode>,
    host_types: BTreeSet<String>,
}

impl Admitter {
    fn new() -> Self {
        Self {
            diagnostics: Diagnostics::new(),
            trace: Vec::new(),
            exprs: Vec::new(),
            types: Vec::new(),
            host_types: BTreeSet::new(),
        }
    }

    fn type_id(&mut self, node: TypeNode) -> TypeId {
        self.types.push(node);
        TypeId(u32::try_from(self.types.len() - 1).unwrap_or(u32::MAX))
    }

    fn error(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics.error(code, message, span);
    }
}
