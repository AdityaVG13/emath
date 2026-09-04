//! The Phase 1 admission pass: syntax → typed neutral SIR with stable
//! diagnostics and a source-to-SIR trace.

use emath_core::tree::{Expr, ExprKind, Stmt, UnaryOp as SynUnOp};
use emath_core::{Diagnostics, QualifiedName, Span};
use emath_ir::{
    BinaryOp, EventDecl, ExprId, ExprNode, Literal, ModelResidual, TransitionDecl, TypeId, TypeNode,
};
use std::collections::{BTreeMap, BTreeSet};

mod declaration;
use declaration::admit_declaration;
mod attributes;
mod expr_helpers;
use expr_helpers::*;
mod infer;
use infer::*;
mod equations;
mod lowering;
mod sections;
mod sections_meta;
mod types;
pub use sections_meta::check_tree;

pub const E_DUPLICATE_FIELD: &str = "E-NAME-020";
pub const E_UNKNOWN_VARIABLE: &str = "E-TYPE-002";
pub const E_UNKNOWN_FUNCTION: &str = "E-TYPE-003";
pub const E_UNSUPPORTED_TYPE: &str = "E-TYPE-010";

/// Sections the Phase 1 admission pass consumes; any other section is
/// refused with `E-SEC-101` instead of being silently dropped.
const PHASE1_SECTIONS: &[&str] = &[
    "inputs",
    "outputs",
    "state",
    "algebraic",
    "definitions",
    "equations",
    "equation",
    "constructors",
    "goals",
    "exports",
    "tests",
    "compile",
    "about",
    "evidence",
    "assumptions",
    "domain",
    "provenance",
    "citations",
    // L3 optional worked-example section: rows are data, never admission
    // tickets.
    "examples",
    "host",
    "constraints",
    "invariant",
    // Migration cards: `from:` states what moved, `rules:`
    // classifies each change. Admitted sections, not new keywords.
    "from",
    "rules",
    // Hybrid events (ch7): `events:` declares the
    // discrete event surface of a stateful declaration. Admitted
    // section, not a new keyword; the `transitions:` rules and the
    // event-triggering simulation are the named next slices (the
    // `on <trigger>:` rule suite does not parse yet — parser lane).
    "events",
    // Hybrid transitions (ch7, transitions slice):
    // `transitions:` maps a declared event to re-assignments of
    // input/state slots. Admitted section, not a new keyword;
    // `on <Event>:` rules are structurally validated here and
    // wired into execution by the runner lane.
    "transitions",
    // Measured evidence (04 §5.2):
    // `observations:` rows are read-only instrument data (`obs <name>
    // [: type] = <data>`), distinct from `definitions:`. Admitted
    // section; the §5.3 observation-vs-prediction comparison and the
    // Series<T in unit> value-generics are the named next slices.
    "observations",
    // Proof outlines (B13 + 05 §7.2):
    // `proofs:` holds obligation outlines as DATA (assumption / lemma
    // / check / qed steps; an outline ends with qed). Proofs are
    // additive authority, never admission tickets; no ProofChecker
    // runs in the thin slice (the checker contract is the named
    // follow-up).
    "proofs",
    // Declarative figures (05 §7.4): the
    // `figures:` section NAME + payload grammar slot is RESERVED so
    // kind schemas can require/allow it. Data-only plot specs, no
    // callbacks, no behavior — determinism is preserved by tying
    // sampling to the budgets/continuation machinery from day one.
    // The payload grammar is the named follow-up: rows inside refuse
    // (declaration.rs) naming the design forks instead of the generic
    // roster error.
    "figures",
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
    /// Effective per-declaration units-profile table (04 §6.1), in
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
    /// Constraint expression IDs from `constraints:` section, for penalty method.
    constraints: Vec<ExprId>,
    /// Causalized implicit residuals admitted from `equations:` (unknowns
    /// solved by Newton at each time step; see `crate::ModelResidual`).
    residuals: Vec<ModelResidual>,
    /// Hybrid event rules admitted from `events:` payload suites
    /// (ch7, event-execution slice). Bare event
    /// declarations without a payload suite contribute nothing.
    events: Vec<EventDecl>,
    /// Transition rules admitted from `transitions:` `on <Event>:`
    /// suites (ch7, transitions slice). Each rule
    /// attaches deterministic re-assignments to a declared event;
    /// action values may reference the event's captured parameters.
    transitions: Vec<TransitionDecl>,
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
    /// Declared capability cells visible to this declaration: the
    /// canonical/bare match keys, the cell's index in the package
    /// capability arena, and its declared output type text. The GENERIC
    /// declared-capability call data — a call resolving here lowers to
    /// `ExprNode::Apply` (the emitter's ApplyCapability path), never a
    /// new builtin name or domain keyword.
    capability_cells: Vec<CapabilityCallBinding>,
    /// Sibling `emath function` declarations callable from lowering time
    /// function DATA for the generic declared-call seam's
    /// inline path — no new AST node, no registry entry.
    sibling_functions: BTreeMap<String, SiblingFunction>,
    /// Inline-substitution cycle guard: the stack of callee names
    /// currently being inlined.
    inline_stack: Vec<String>,
    /// Binder names shadowing same-named definitions during the
    /// current `inline_defs` walk. A reference inside a binder body to
    /// a name that a binder rebinds must stay a `Variable` (it reads
    /// the binder local at runtime), never the shadowed definition.
    inline_shadows: Vec<String>,
}

#[derive(Clone)]
pub(super) struct CapabilityCallBinding {
    pub(super) key: String,
    pub(super) capability: u32,
    pub(super) inputs: Vec<String>,
    pub(super) output: Option<String>,
    pub(super) arity: Option<usize>,
    pub(super) diagnostic: Option<String>,
}

/// One sibling `emath function` callable from lowering time: parameter
/// names with their inferred types, the output binding name, and the
/// cloned `definitions:` statements.
#[derive(Clone)]
pub(super) struct SiblingFunction {
    pub(super) params: Vec<(String, Infer)>,
    pub(super) output_name: String,
    pub(super) definitions: Vec<Stmt>,
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
            constraints: Vec::new(),
            residuals: Vec::new(),
            events: Vec::new(),
            transitions: Vec::new(),
            rate_placeholders: BTreeMap::new(),
            exprs: Vec::new(),
            types: Vec::new(),
            host_types: BTreeSet::new(),
            index_locals: BTreeMap::new(),
            in_claim_context: false,
            capability_cells: Vec::new(),
            sibling_functions: BTreeMap::new(),
            inline_stack: Vec::new(),
            inline_shadows: Vec::new(),
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

    /// Recursively inline definition references so downstream consumers
    /// (e.g. forward-mode autodiff) see the actual computation.
    fn inline_defs(&mut self, expr_id: ExprId) -> ExprId {
        let Some((node, span)) = self.exprs.get(expr_id.0 as usize).cloned() else {
            return expr_id;
        };
        match node {
            ExprNode::Variable(name) => {
                // A binder-local of the same name shadows the definition:
                // the reference reads the binder local at runtime.
                if !self.inline_shadows.iter().any(|shadow| shadow == &name.0) {
                    if let Some((def_id, _)) = self.definitions.get(&name.0) {
                        return self.inline_defs(*def_id);
                    }
                }
                expr_id
            }
            ExprNode::Literal(_) => expr_id,
            ExprNode::Unary { operation, value } => {
                let value = self.inline_defs(value);
                self.push_expr(ExprNode::Unary { operation, value }, span)
            }
            ExprNode::Binary {
                operation,
                left,
                right,
            } => {
                let left = self.inline_defs(left);
                let right = self.inline_defs(right);
                self.push_expr(
                    ExprNode::Binary {
                        operation,
                        left,
                        right,
                    },
                    span,
                )
            }
            ExprNode::Call {
                function,
                arguments,
            } => {
                let arguments: Vec<_> =
                    arguments.into_iter().map(|a| self.inline_defs(a)).collect();
                self.push_expr(
                    ExprNode::Call {
                        function,
                        arguments,
                    },
                    span,
                )
            }
            ExprNode::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.inline_defs(condition);
                let then_value = self.inline_defs(then_value);
                let else_value = self.inline_defs(else_value);
                self.push_expr(
                    ExprNode::If {
                        condition,
                        then_value,
                        else_value,
                    },
                    span,
                )
            }
            ExprNode::Index { value, indices } => {
                let value = self.inline_defs(value);
                let indices: Vec<_> = indices.into_iter().map(|i| self.inline_defs(i)).collect();
                self.push_expr(ExprNode::Index { value, indices }, span)
            }
            ExprNode::Vector(els) => {
                let els: Vec<_> = els.into_iter().map(|e| self.inline_defs(e)).collect();
                self.push_expr(ExprNode::Vector(els), span)
            }
            ExprNode::Matrix(rows) => {
                let rows: Vec<Vec<_>> = rows
                    .into_iter()
                    .map(|row| row.into_iter().map(|e| self.inline_defs(e)).collect())
                    .collect();
                self.push_expr(ExprNode::Matrix(rows), span)
            }
            ExprNode::Tensor { shape, elements } => {
                let elements: Vec<_> = elements.into_iter().map(|e| self.inline_defs(e)).collect();
                self.push_expr(ExprNode::Tensor { shape, elements }, span)
            }
            ExprNode::Differentiate { body, var } => {
                let body = self.inline_defs(body);
                self.push_expr(ExprNode::Differentiate { body, var }, span)
            }
            // Slice, Record — keep as-is (rare in derivative bodies).
            // Binder DOMAINS must be inlined like any other expression:
            // a variable-range binder in a callee body (`product k in
            // 1..=n`) carries the renamed parameter (`n#f`) in its domain
            // vector, and only this pass resolves it to the caller's
            // argument. The BODY and its nested expressions are walked
            // too (emath-87ls0): a sibling callee's renamed parameter
            // (`y#f`) referenced inside a fold guard otherwise survives
            // into the spliced tree and the runner refuses it as an
            // unknown input. Binder names shadow same-named definitions
            // for the body walk, so a binder variable is never captured
            // by the substitution.
            ExprNode::Binder {
                kind,
                mut variables,
                body,
            } => {
                for variable in variables.iter_mut() {
                    variable.domain = self.inline_defs(variable.domain);
                }
                let shadowed = variables
                    .iter()
                    .map(|variable| variable.name.clone())
                    .collect::<Vec<_>>();
                self.inline_shadows.extend(shadowed.iter().cloned());
                let body = self.inline_defs(body);
                self.inline_shadows
                    .truncate(self.inline_shadows.len() - shadowed.len());
                self.push_expr(
                    ExprNode::Binder {
                        kind,
                        variables,
                        body,
                    },
                    span,
                )
            }
            _ => expr_id,
        }
    }

    /// Add penalty terms for each constraint to the optimization body:
    /// `max(0, b - a)^2` / `max(0, a - b)^2` / `(a - b)^2` for >= / <= / ==.
    fn add_constraint_penalties(&mut self, body: ExprId, span: Span) -> ExprId {
        if self.constraints.is_empty() {
            return body;
        }
        // Quadratic exterior penalty. The optimizer is Newton on ∇L = 0,
        // so a large weight is stable (no GD learning-rate restriction).
        // For min x²+y² s.t. x+y≥1 the equilibrium is x=y=w/(1+2w);
        // w=1000 sits at ≈0.49975 (constraint gap ≈5e-4).
        const PENALTY_WEIGHT: f64 = 1000.0;
        let weight_id = self.push_expr(
            ExprNode::Literal(Literal::FloatBits(PENALTY_WEIGHT.to_bits())),
            span,
        );
        let mut result = body;
        for &constraint_id in &self.constraints.clone() {
            let Some(penalty) = self.constraint_penalty(constraint_id, span) else {
                continue;
            };
            let weighted = self.push_expr(
                ExprNode::Binary {
                    operation: BinaryOp::StrictFloatMul,
                    left: weight_id,
                    right: penalty,
                },
                span,
            );
            result = self.push_expr(
                ExprNode::Binary {
                    operation: BinaryOp::StrictFloatAdd,
                    left: result,
                    right: weighted,
                },
                span,
            );
        }
        if result != body {
            self.record(
                "sema",
                format!(
                    "added {} constraint penalty term(s) to optimization body",
                    self.constraints.len()
                ),
                span,
            );
        }
        result
    }

    /// Build a penalty expression for a single constraint.
    /// Returns None for non-comparison constraints (e.g. NotEqual).
    fn constraint_penalty(&mut self, constraint_id: ExprId, span: Span) -> Option<ExprId> {
        let (node, _) = self.exprs.get(constraint_id.0 as usize)?.clone();
        let ExprNode::Binary {
            operation,
            left,
            right,
        } = node
        else {
            return None;
        };
        let zero = self.push_expr(
            ExprNode::Literal(Literal::FloatBits(0.0f64.to_bits())),
            span,
        );
        let two = self.push_expr(
            ExprNode::Literal(Literal::FloatBits(2.0f64.to_bits())),
            span,
        );
        // violation = amount by which constraint is violated (>= 0 when violated)
        let violation = match operation {
            BinaryOp::GreaterEqual | BinaryOp::Greater => {
                // a >= b violated when a < b → violation = b - a
                self.push_expr(
                    ExprNode::Binary {
                        operation: BinaryOp::StrictFloatSub,
                        left: right,
                        right: left,
                    },
                    span,
                )
            }
            BinaryOp::LessEqual | BinaryOp::Less => {
                // a <= b violated when a > b → violation = a - b
                self.push_expr(
                    ExprNode::Binary {
                        operation: BinaryOp::StrictFloatSub,
                        left,
                        right,
                    },
                    span,
                )
            }
            BinaryOp::Equal => {
                // a == b → violation = a - b (squared, so sign doesn't matter)
                self.push_expr(
                    ExprNode::Binary {
                        operation: BinaryOp::StrictFloatSub,
                        left,
                        right,
                    },
                    span,
                )
            }
            _ => return None,
        };
        // For inequalities: clamp violation at 0 with max(0, violation)
        let clamped = if matches!(operation, BinaryOp::Equal) {
            violation
        } else {
            self.push_expr(
                ExprNode::Call {
                    function: QualifiedName("max".to_string()),
                    arguments: vec![zero, violation],
                },
                span,
            )
        };
        // penalty = clamped^2
        Some(self.push_expr(
            ExprNode::Call {
                function: QualifiedName("pow".to_string()),
                arguments: vec![clamped, two],
            },
            span,
        ))
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
