//! The admission pass: syntax → typed neutral SIR with stable
//! diagnostics and a source-to-SIR trace.

use emath_core::tree::{Expr, ExprKind, UnaryOp as SynUnOp};
use emath_core::{Diagnostics, Span};
use emath_ir::{ExprId, ExprNode, TypeId, TypeNode};
use std::collections::{BTreeMap, BTreeSet};

mod declaration;
use declaration::admit_constructor_declaration;
mod attributes;
pub(crate) mod expr_helpers;
use expr_helpers::parse_float_constant;
mod infer;
use infer::Infer;
mod equations;
mod lowering;
mod sections;
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
    pub package: emath_ir::SemanticPackage,
    pub diagnostics: Diagnostics,
    pub trace: SemanticTrace,
    /// Effective per-declaration units-profile table, in
    /// source order; empty when no `@units_profile` attribute exists.
    pub units_profiles: Vec<(String, String)>,
}

struct Admitter {
    diagnostics: Diagnostics,
    trace: Vec<TraceEntry>,
    params: BTreeMap<String, Infer>,
    inputs: BTreeMap<String, Infer>,
    states: BTreeMap<String, Infer>,
    definitions: BTreeMap<String, (ExprId, Infer)>,
    /// Synthetic inputs `__rate_<state>` that replace `der(state)` inside
    /// residual expressions; inferred like the state field they derive.
    rate_placeholders: BTreeMap<String, Infer>,
    exprs: Vec<(ExprNode, Span)>,
    types: Vec<TypeNode>,
    host_types: BTreeSet<String>,
    /// Finite binder locals (`sum i in 0..n`). Looked up before inputs.
    index_locals: BTreeMap<String, i64>,
    /// When true, claim expressions (limit, series, asymp) are admitted
    /// as Bool(true) instead of erroring. Set during require/invariant
    /// lowering; false during definitions lowering.
    in_claim_context: bool,
    /// Declared capability cells visible to this declaration. The
    /// constructor lane admits no cells (always empty), but the goal
    /// surface still consults it and refuses with the pinned
    /// "not executable in the loaded Language Image" diagnostics.
    capability_cells: Vec<CapabilityCallBinding>,
}

/// A capability cell consult entry. The constructor lane admits no cells,
/// so only the consult keys are ever read; the shape fields exist for the
/// consult sites' pinned refusal paths.
#[derive(Clone)]
pub(super) struct CapabilityCallBinding {
    pub(super) key: String,
    pub(super) capability: u32,
    pub(super) kernel: Option<String>,
}

impl Admitter {
    fn new() -> Self {
        Self {
            diagnostics: Diagnostics::new(),
            trace: Vec::new(),
            params: BTreeMap::new(),
            inputs: BTreeMap::new(),
            states: BTreeMap::new(),
            definitions: BTreeMap::new(),
            rate_placeholders: BTreeMap::new(),
            exprs: Vec::new(),
            types: Vec::new(),
            host_types: BTreeSet::new(),
            index_locals: BTreeMap::new(),
            in_claim_context: false,
            capability_cells: Vec::new(),
        }
    }

    fn type_id(&mut self, node: TypeNode) -> TypeId {
        self.types.push(node);
        TypeId(u32::try_from(self.types.len() - 1).unwrap_or(u32::MAX))
    }

    fn push_expr(&mut self, node: ExprNode, span: Span) -> ExprId {
        self.exprs.push((node, span));
        ExprId(u32::try_from(self.exprs.len() - 1).unwrap_or(u32::MAX))
    }

    fn record(&mut self, phase: &str, detail: impl Into<String>, span: Span) {
        self.trace.push(TraceEntry {
            phase: phase.to_string(),
            detail: detail.into(),
            span: Some(span),
        });
    }

    fn error(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics.error(code, message, span);
        if code == "E-UNIT-101" {
            self.diagnostics.attach_pedagogy(emath_core::Pedagogy {
                understood: "the operands have different quantity kinds (Duration vs Information)"
                    .into(),
                unknown: "how those kinds could be added".into(),
                why: "addition is only admitted when dimensions match".into(),
                smallest_repair: "convert one operand to the other's unit, or do not add them"
                    .into(),
                alternatives: vec![
                    "convert Duration to a common scale".into(),
                    "keep the quantities in separate fields".into(),
                ],
                example: Some("1 s + 1 s is admitted; 1 s + 1 MiB is E-UNIT-101".into()),
                deeper_concept: Some("quantity kinds are not numbers".into()),
                authority_consequence: Some("a unit refusal does not grant numeric meaning".into()),
                library_link: Some("language/reference/types-units-shapes-and-domains.md".into()),
            });
        }
    }

    fn note(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics.note(code, message, span);
    }

    fn warning(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics.warning(code, message, span);
    }

    fn lookup(&self, name: &str) -> Option<Infer> {
        if let Some(value) = self.index_locals.get(name) {
            return Some(if *value < 0 { Infer::Int } else { Infer::Nat });
        }
        if let Some(infer) = self.rate_placeholders.get(name) {
            return Some(infer.clone());
        }
        if let Some(infer) = self.params.get(name) {
            return Some(infer.clone());
        }
        if let Some(infer) = self.inputs.get(name) {
            return Some(infer.clone());
        }
        if let Some(stripped) = name.strip_prefix("state.") {
            return self.states.get(stripped).cloned();
        }
        if let Some((_, infer)) = self.definitions.get(name) {
            return Some(infer.clone());
        }
        self.states.get(name).cloned()
    }
}

pub(super) fn expr_number(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::Int(text) | ExprKind::Float(text) => parse_float_constant(text),
        ExprKind::Unary {
            op: SynUnOp::Neg,
            value,
        } => expr_number(value).map(|value| -value),
        _ => None,
    }
}
