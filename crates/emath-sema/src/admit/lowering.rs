//! Expression lowering: lowers parsed `.emath` expressions into typed
//! EMIR expression nodes with stable inference.

use emath_core::QualifiedName;
use emath_core::tree::{
    BinaryOp as SynBinOp, BinderKind, DerivativeKind, Expr, ExprKind, UnaryOp as SynUnOp,
};
use emath_exec_ir::BuiltinId;
use emath_ir::{
    BinaryOp, CapabilityId, DistributionKind, ExprId, ExprNode, Extent, Literal, TypeNode, UnitDim,
    UnitFamily, lookup_unit,
};

mod helpers;

use super::Admitter;
use super::equations::*;
use super::expr_helpers::*;
use super::infer::*;
use super::sections::{integer_range, restore_index_local};
use super::{E_UNKNOWN_FUNCTION, E_UNKNOWN_VARIABLE, E_UNSUPPORTED_TYPE};
use crate::recognition::expr_text;

/// Typed refusal: a tolerance-less `≈` edge. An approximation without a
/// declared tolerance is never admitted as if it were exact (04 §6.4).
const E_APPROX_TOL: &str = "E-APPROX-TOL";

/// Strip namespace prefixes (`math::`, `linalg::`, `pde::`, `coding::`,
/// legacy `core::math::`) from builtin function names to a bare form.
fn normalize_builtin(name: &str) -> String {
    for prefix in &[
        "math::",
        "linalg::",
        "pde::",
        "coding::",
        "logic::",
        "core::math::",
        "core::pde::",
        "core::logic::",
    ] {
        if let Some(bare) = name.strip_prefix(prefix) {
            return bare.to_string();
        }
    }
    name.to_string()
}

/// The declared-output type text of a capability cell → the call's
/// inferred type. The mapping reads the cell declaration's OWN contract
/// (the small closed set of Phase-1 type spellings); anything outside
/// it is opaque — never a silently-assumed scalar.
fn capability_result_infer(output: Option<&str>) -> Infer {
    match output.map(str::trim) {
        Some("Float64") | Some("F64") => Infer::F64,
        Some("Bool") => Infer::Bool,
        Some("Int") => Infer::Int,
        Some("Nat") => Infer::Nat,
        Some(text) if text.starts_with("Vector") => Infer::Vector { extent: None },
        Some(text) if text.starts_with("Matrix") => Infer::Matrix {
            rows: None,
            cols: None,
        },
        _ => Infer::Opaque,
    }
}

fn csv_series_column_name(header: &str) -> String {
    let header = header.trim();
    header
        .rfind('(')
        .filter(|_| header.ends_with(')'))
        .map_or(header, |unit_start| &header[..unit_start])
        .trim()
        .to_string()
}

/// RFC-4180-style CSV field splitter yielding one owned cell per field.
///
/// Outside quotes: a `,` ends a field; a `"` enters a quoted section. Per RFC
/// a `"` should only open a field, but this splitter is deliberately tolerant
/// and treats any `"` outside a quoted section as opening one. Inside quotes:
/// `""` is an escaped literal quote; a lone `"` closes the quoted section;
/// commas and whitespace pass through verbatim. Fields end at `,` or end of
/// line.
///
/// This is NOT a general CSV framework:
/// - Embedded NEWLINES inside a quoted field are NOT supported (no-claim); a
///   quoted field cannot span lines.
/// - A field that ends while still inside an open quote is MALFORMED. It is
///   still returned and the second element of the tuple is set; pass 5 adds a
///   typed refusal on top of this `malformed` signal.
fn split_csv_fields(line: &str) -> (Vec<String>, bool) {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut malformed = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    // `""` inside a quoted field is an escaped literal quote.
                    field.push('"');
                    chars.next();
                } else {
                    // Lone `"` closes the quoted section.
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
        } else if ch == ',' {
            fields.push(std::mem::take(&mut field));
        } else if ch == '"' {
            // Tolerant: any `"` outside a quoted section opens one.
            in_quotes = true;
        } else {
            field.push(ch);
        }
    }
    fields.push(field);
    if in_quotes {
        // Field never closed by a `"`: caller sees a `malformed` row/header.
        malformed = true;
    }
    (fields, malformed)
}

fn interpolation_paths(template: &str) -> Vec<&str> {
    let mut paths = Vec::new();
    let bytes = template.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'{' {
            if bytes.get(index + 1) == Some(&b'{') {
                index += 2;
                continue;
            }
            if let Some(relative_end) = template[index + 1..].find('}') {
                let end = index + 1 + relative_end;
                let field = template[index + 1..end]
                    .split_once(':')
                    .map_or(&template[index + 1..end], |(path, _)| path);
                paths.push(field);
                index = end + 1;
                continue;
            }
        }
        index += 1;
    }
    paths
}

fn graph_tuple_parts(items: &[Expr]) -> Option<(&[Expr], &[Expr])> {
    if items.len() != 2 {
        return None;
    }
    let (ExprKind::List(nodes), ExprKind::List(edges)) = (&items[0].kind, &items[1].kind) else {
        return None;
    };
    if edges
        .iter()
        .all(|edge| matches!(&edge.kind, ExprKind::List(parts) if parts.len() == 4))
    {
        Some((nodes, edges))
    } else {
        None
    }
}

/// A numeric literal with an optional unary sign — `1`, `-1.0`, `+2`,
/// and nested sign chains (`--1.0` folds to `1.0`). The recursive fold
/// over `Unary { Neg | Pos }` of `Int`/`Float` spellings admits signed
/// graph edge weights (`0 -[-1.0]-> 1`) and signed node labels through
/// the SAME literal helper, without graph-specific parser branches.
/// Non-literal forms (paths, calls, arithmetic, measured quotes) still
/// yield `None` and refuse `E-TYPE-012` at the call site.
fn signed_numeric_literal(expr: &Expr) -> Option<f64> {
    fn raw(expr: &Expr) -> Option<(f64, bool)> {
        match &expr.kind {
            ExprKind::Int(text) | ExprKind::Float(text) => {
                parse_float_constant(text).map(|value| (value, false))
            }
            ExprKind::Unary {
                op: SynUnOp::Neg,
                value,
            } => raw(value).map(|(value, neg)| (value, !neg)),
            ExprKind::Unary {
                op: SynUnOp::Pos,
                value,
            } => raw(value),
            _ => None,
        }
    }
    raw(expr).and_then(|(value, neg)| {
        let value = if neg { -value } else { value };
        value.is_finite().then_some(value)
    })
}

/// Unit families must agree inside one composed unit spelling
/// (`kg*m^2/s^2` is fine; `m*ft` is a typed refusal).
fn combine_unit_families(
    admitter: &mut Admitter,
    left: UnitFamily,
    right: UnitFamily,
    expr: &Expr,
) -> Option<UnitFamily> {
    if left == right {
        Some(left)
    } else {
        admitter.error(
            "E-UNIT-101",
            format!("unit family mismatch in unit comparison: {left:?} vs {right:?}"),
            expr.source,
        );
        None
    }
}

impl super::Admitter {
    /// True when an `E-NAME-023` refusal for exactly this declared
    /// output is already on record (message `output `<name>` has no
    /// definition`). The consequent `E-TYPE-002` "unknown variable" at
    /// later use sites is suppressed for such names (emath-2bwk):
    /// the empty `definitions:` block is the single root refusal, and
    /// repeating the same missing name at every use site buries it in
    /// noise. The message pair is pinned by the regression
    /// `empty_definitions_refuses_once_and_preserves_independent_errors`
    /// in tests/emath-sema/tests/session.rs, so a message edit that
    /// breaks the pairing fails that test.
    fn undefined_output_already_refused(&self, name: &str) -> bool {
        let expected = format!("output `{name}` has no definition");
        self.diagnostics
            .errors()
            .any(|d| d.code == "E-NAME-023" && d.message == expected)
    }

    /// 04 §5.4: resolve one series pair element to its SI scalar. Only
    /// data literals admit: a quantity (`2.5 mg/L`) scales to SI; a bare
    /// numeric is dimensionless. Anything else (variables, calls,
    /// expressions) is not a datum — the series is not executable in
    /// this slice, so it must not smuggle in computation either.
    fn series_pair_scalar(&mut self, expr: &Expr) -> Option<f64> {
        match &expr.kind {
            ExprKind::Quantity { value, unit } => {
                let Some(magnitude) = parse_quantity_magnitude(value) else {
                    self.error(
                        "E-UNIT-105",
                        "series pair elements must be numeric quantity literals",
                        expr.source,
                    );
                    return None;
                };
                let factors = unit.flatten();
                let mut scale = 1.0_f64;
                let mut offset = 0.0_f64;
                for (name, power) in &factors {
                    match lookup_unit(name) {
                        Ok(looked_up) => {
                            if looked_up.is_affine() && (*power != 1 || factors.len() != 1) {
                                self.error(
                                    "E-UNIT-102",
                                    format!(
                                        "affine unit misuse: `{}` cannot appear in a compound or powered series element",
                                        looked_up.name
                                    ),
                                    expr.source,
                                );
                                return None;
                            }
                            scale *= looked_up.scale.powi(*power);
                            if *power == 1 {
                                offset = looked_up.offset;
                            }
                        }
                        Err(error) => {
                            self.error(error.code, error.message, expr.source);
                            return None;
                        }
                    }
                }
                let si = (magnitude + offset) * scale;
                if si.is_finite() {
                    Some(si)
                } else {
                    self.error(
                        "E-TYPE-011",
                        "non-finite series element refused under the selected numeric model",
                        expr.source,
                    );
                    None
                }
            }
            ExprKind::Float(text) => match text.parse::<f64>() {
                Ok(value) if value.is_finite() => Some(value),
                _ => {
                    self.error(
                        "E-TYPE-011",
                        "non-finite series element refused under the selected numeric model",
                        expr.source,
                    );
                    None
                }
            },
            _ => {
                self.error(
                    "E-SYN-101",
                    "series pair elements are data literals (`2.5 mg/L`, `0.1 s`) — no expressions or references inside the series",
                    expr.source,
                );
                None
            }
        }
    }

    pub(super) fn lower_expr(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        match &expr.kind {
            ExprKind::Int(text) => {
                let id = self.push_expr(
                    ExprNode::Literal(Literal::Integer(text.clone())),
                    expr.source,
                );
                let infer = if text.starts_with('-') {
                    Infer::Int
                } else {
                    Infer::Nat
                };
                Some((id, infer))
            }
            ExprKind::Float(text) => {
                let value = parse_float_constant(text);
                match value {
                    Some(value) if value.is_finite() => {
                        self.record(
                            "sema",
                            format!("constant `{text}` → strict f64"),
                            expr.source,
                        );
                        let id = self.push_expr(
                            ExprNode::Literal(Literal::FloatBits(value.to_bits())),
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    _ => {
                        self.error(
                            "E-TYPE-011",
                            format!("non-finite constant `{text}` refused under strict-f64 policy"),
                            expr.source,
                        );
                        None
                    }
                }
            }
            ExprKind::Bool(value) => {
                let id = self.push_expr(ExprNode::Literal(Literal::Bool(*value)), expr.source);
                Some((id, Infer::Bool))
            }
            ExprKind::Str(template) => {
                let template = emath_core::normalize_nfc(template);
                let literal = self.push_expr(
                    ExprNode::Literal(Literal::Text(template.clone())),
                    expr.source,
                );
                let paths = interpolation_paths(&template);
                if paths.is_empty() {
                    return Some((literal, Infer::Text));
                }
                let mut arguments = Vec::with_capacity(paths.len() + 1);
                arguments.push(literal);
                for path in paths {
                    let value = Expr {
                        kind: ExprKind::Path {
                            segments: path.split('.').map(str::to_string).collect(),
                            generics: None,
                        },
                        source: expr.source,
                    };
                    let (id, _) = self.lower_expr(&value)?;
                    arguments.push(id);
                }
                let id = self.push_expr(
                    ExprNode::Call {
                        function: QualifiedName("__format_text".to_string()),
                        arguments,
                    },
                    expr.source,
                );
                Some((id, Infer::Text))
            }
            ExprKind::Measured {
                value,
                uncertainty,
                uncertainty_digits,
                distribution,
            } => {
                // Measurement literal (spec 04 section 1.5). Phase 1 lowers
                // the central value to strict f64; the uncertainty and the
                // Unstated provenance are recorded loudly, never silently
                // merged into the value (a measured value used as exact is
                // the same lie of omission in reverse). Full Measured<T>
                // propagation is the Phase 2 wave (bead considerations).
                let Some(central) = parse_float_constant(value) else {
                    self.error(
                        "E-MEAS-001",
                        format!("measurement literal value `{value}` is not a valid number"),
                        expr.source,
                    );
                    return None;
                };
                let spread = if uncertainty_digits.is_empty() {
                    parse_float_constant(uncertainty)
                } else {
                    measured_digits_uncertainty(value, uncertainty_digits)
                };
                let Some(std_uncertainty) = spread.filter(|s| s.is_finite() && *s >= 0.0) else {
                    self.error(
                        "E-MEAS-001",
                        format!(
                            "measurement literal `{value}` has an invalid uncertainty `{uncertainty}{uncertainty_digits}`"
                        ),
                        expr.source,
                    );
                    return None;
                };
                let kind = match distribution.as_deref() {
                    None | Some("normal") => DistributionKind::Normal,
                    Some("uniform") => DistributionKind::Uniform,
                    Some("lognormal") => DistributionKind::Lognormal,
                    Some(other) => {
                        self.error(
                            "E-MEAS-002",
                            format!(
                                "unknown distribution tag `~ {other}` (normal | uniform | lognormal)"
                            ),
                            expr.source,
                        );
                        return None;
                    }
                };
                self.record(
                    "sema",
                    format!(
                        "measurement `{value} ± {std_uncertainty:e}` ({kind:?}, provenance Unstated) recorded; central value lowers strict"
                    ),
                    expr.source,
                );
                self.warning(
                    "E-MEAS-003",
                    format!(
                        "measurement literal `{value} ± {std_uncertainty:e}` is used as a strict value; the uncertainty is recorded (provenance: Unstated) but not propagated in Phase 1"
                    ),
                    expr.source,
                );
                let id = self.push_expr(
                    ExprNode::Literal(Literal::FloatBits(central.to_bits())),
                    expr.source,
                );
                Some((id, Infer::F64))
            }
            ExprKind::WithSeriesPolicy {
                value,
                interpolation,
                extrapolation,
            } => {
                // 04 §5.4 (emath-r3-timeseries-1nsa slice 1): a series
                // literal is pure data — `[(<time>, <value>), ...]` of
                // quantity literals, SI-scaled — plus its DECLARED
                // interpretation policy. Interpolation has no silent
                // default (the mode changes every downstream number);
                // extrapolation defaults to `refuse`. Evaluation is the
                // named next slice: admitting a series never claims it
                // interpolates or extrapolates.
                let ExprKind::List(items) = &value.kind else {
                    self.error(
                        "E-SYN-101",
                        "a series value is `[(<time quantity>, <value quantity>), ...]` — a list of pairs",
                        value.source,
                    );
                    return None;
                };
                if items.is_empty() {
                    self.error(
                        "E-SYN-101",
                        "a series needs at least one `(time, value)` pair",
                        value.source,
                    );
                    return None;
                }
                let mut points: Vec<(f64, f64)> = Vec::with_capacity(items.len());
                for item in items {
                    let ExprKind::Tuple(pair) = &item.kind else {
                        self.error(
                            "E-SYN-101",
                            "series rows are `(<time quantity>, <value quantity>)` pairs",
                            item.source,
                        );
                        return None;
                    };
                    if pair.len() != 2 {
                        self.error(
                            "E-SYN-101",
                            "series rows are exactly `(<time quantity>, <value quantity>)`",
                            item.source,
                        );
                        return None;
                    }
                    let (Some(time), Some(val)) = (
                        self.series_pair_scalar(&pair[0]),
                        self.series_pair_scalar(&pair[1]),
                    ) else {
                        return None;
                    };
                    points.push((time, val));
                }
                for window in points.windows(2) {
                    if window[0].0 >= window[1].0 {
                        self.error(
                            "E-SYN-101",
                            "series time axis must be strictly increasing — every interpolation mode orders the support by time",
                            value.source,
                        );
                        return None;
                    }
                }
                let Some(interpolation) = interpolation else {
                    self.error(
                        "E-SYN-101",
                        "declare `with interpolation: previous|linear|nearest|pwc|monotone_cubic` on the series — the mode changes every downstream number and is never guessed",
                        expr.source,
                    );
                    return None;
                };
                let extrapolation =
                    extrapolation.unwrap_or(emath_core::tree::SeriesExtrapolation::Refuse);
                let id = self.push_expr(
                    ExprNode::Series {
                        points,
                        interpolation: interpolation.spelling().to_string(),
                        extrapolation: extrapolation.spelling().to_string(),
                    },
                    expr.source,
                );
                Some((id, Infer::Series))
            }
            ExprKind::Quantity { value, unit } => {
                let Some(magnitude) = parse_quantity_magnitude(value) else {
                    self.error(
                        "E-UNIT-105",
                        "quantity value must be a numeric literal",
                        expr.source,
                    );
                    return None;
                };
                // Flatten compound units to (name, power) pairs; combine
                // dimensions and SI scale so `1 km + 1 m` is 1001 m.
                let factors = unit.flatten();
                let mut combined_dims = UnitDim::one();
                let mut combined_family = UnitFamily::Si;
                let mut combined_scale = 1.0_f64;
                let mut combined_offset = 0.0_f64;
                let mut unit_label = String::new();
                for (name, power) in &factors {
                    match lookup_unit(name) {
                        Ok(looked_up) => {
                            if looked_up.is_affine() && (*power != 1 || factors.len() != 1) {
                                self.error(
                                    "E-UNIT-102",
                                    format!(
                                        "affine unit misuse: `{}` cannot appear in a compound or powered unit",
                                        looked_up.name
                                    ),
                                    expr.source,
                                );
                                return None;
                            }
                            if !unit_label.is_empty() && looked_up.family != combined_family {
                                self.error(
                                    "E-UNIT-101",
                                    format!(
                                        "dimension mismatch: cannot combine `{}` ({}) with `{}` ({})",
                                        unit_label,
                                        combined_family.as_str(),
                                        looked_up.name,
                                        looked_up.family.as_str()
                                    ),
                                    expr.source,
                                );
                                return None;
                            }
                            if *power >= 0 {
                                combined_dims = combined_dims.mul(looked_up.dims.pow(*power));
                            } else {
                                combined_dims = combined_dims.div(looked_up.dims.pow(-*power));
                            }
                            combined_family = looked_up.family;
                            combined_scale *= looked_up.scale.powi(*power);
                            if factors.len() == 1 && *power == 1 {
                                combined_offset = looked_up.offset;
                            }
                            if !unit_label.is_empty() {
                                unit_label.push('*');
                            }
                            unit_label.push_str(&looked_up.name);
                        }
                        Err(error) => {
                            self.error(error.code, error.message, expr.source);
                            return None;
                        }
                    }
                }
                let si = (magnitude + combined_offset) * combined_scale;
                if si.is_finite() {
                    self.record(
                        "sema",
                        format!(
                            "quantity `{} {unit_label}` → SI {si} dims {}",
                            super::super::recognition::expr_text(value),
                            combined_dims.render()
                        ),
                        expr.source,
                    );
                    let id = self.push_expr(
                        ExprNode::Literal(Literal::FloatBits(si.to_bits())),
                        expr.source,
                    );
                    Some((
                        id,
                        Infer::from_dims_affine(
                            combined_dims,
                            combined_family,
                            combined_offset != 0.0,
                        ),
                    ))
                } else {
                    self.error(
                        "E-TYPE-011",
                        format!(
                            "non-finite quantity `{} {unit_label}` refused under the selected numeric model",
                            super::super::recognition::expr_text(value)
                        ),
                        expr.source,
                    );
                    None
                }
            }
            ExprKind::Path { segments, .. } => {
                let name = segments.join(".");
                if segments.len() == 1 {
                    if let Some(value) = self.index_locals.get(&name).copied() {
                        let id = self.push_expr(
                            ExprNode::Literal(Literal::Integer(value.to_string())),
                            expr.source,
                        );
                        let infer = if value < 0 { Infer::Int } else { Infer::Nat };
                        return Some((id, infer));
                    }
                }
                if let Some(infer) = self.lookup(&name) {
                    let ir_name = state_variable_name(self, segments, &name);
                    let id =
                        self.push_expr(ExprNode::Variable(QualifiedName(ir_name)), expr.source);
                    return Some((id, infer));
                }
                if segments.len() >= 2 {
                    if matches!(self.lookup(&segments[0]), Some(Infer::Opaque)) {
                        self.record(
                            "sema",
                            format!("host field `{name}` deferred to the host boundary"),
                            expr.source,
                        );
                        let id =
                            self.push_expr(ExprNode::Variable(QualifiedName(name)), expr.source);
                        return Some((id, Infer::HostDeferred));
                    }
                }
                if segments.len() == 1 {
                    if let Ok(unit) = lookup_unit(&segments[0]) {
                        let si = unit.to_si(1.0);
                        self.record(
                            "sema",
                            format!("unit literal `{}` → SI {si} ({})", segments[0], unit.name),
                            expr.source,
                        );
                        let id = self.push_expr(
                            ExprNode::Literal(Literal::FloatBits(si.to_bits())),
                            expr.source,
                        );
                        return Some((id, Infer::from_unit(&unit)));
                    }
                    // B14: `i` is the imaginary unit (0 + 1i). It is a
                    // named constant, not a reserved keyword — only
                    // recognized when not shadowed by an input/definition.
                    if segments[0] == "Hole" {
                        self.note(
                            "N-HOLE-001",
                            "open hole; meaning stays open and is not claimed exact",
                            expr.source,
                        );
                        let id = self.push_expr(
                            ExprNode::Variable(QualifiedName("Hole".to_string())),
                            expr.source,
                        );
                        return Some((id, Infer::HostDeferred));
                    }
                    if segments[0] == "i" {
                        self.record("sema", "imaginary unit `i` → Complex(0, 1)", expr.source);
                        let id = self.push_expr(
                            ExprNode::Literal(Literal::Complex {
                                re_bits: 0.0_f64.to_bits(),
                                im_bits: 1.0_f64.to_bits(),
                            }),
                            expr.source,
                        );
                        return Some((id, Infer::Complex));
                    }
                }
                if self.undefined_output_already_refused(&name) {
                    // Cascade suppression (emath-2bwk): an `E-NAME-023`
                    // ("output `<name>` has no definition") is already on
                    // record for this declared output — the empty
                    // `definitions:` block already refused at the root.
                    // Repeating "unknown variable" at every later use site
                    // is consequent noise, not a second root cause.
                    // Suppressed here because this use is not resolvable
                    // AT ALL — the name is a declared output with no
                    // definition; if it had one, `lookup` above would have
                    // found it.
                    return None;
                }
                self.error(
                    E_UNKNOWN_VARIABLE,
                    format!("unknown variable `{name}`"),
                    expr.source,
                );
                None
            }
            ExprKind::Call { function, args } => {
                let ExprKind::Path { segments, .. } = &function.kind else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "callable must be a plain path in the Phase 1 subset",
                        function.source,
                    );
                    return None;
                };
                // Join with `::` so namespaced builtins normalize exactly
                // like the exec emitter's `core::math::` stripping (a `.`
                // join made `core::math::pow` an unknown function despite
                // the documented "namespace::name" spelling).
                let name = segments.join("::");
                let name = normalize_builtin(&name);
                // Function duals of `+ - * /` and unary `-`. Notation
                // targets (`core::math::add`) and qualified calls must
                // compute the same IR as the operators, matching `pow`/`^`.
                if matches!(name.as_str(), "add" | "sub" | "mul")
                    || (name == "div" && args.len() == 2)
                {
                    if args.len() != 2 {
                        self.error(
                            "E-TYPE-012",
                            format!("`{name}` expects 2 arguments, found {}", args.len()),
                            expr.source,
                        );
                        return None;
                    }
                    let op = match name.as_str() {
                        "add" => SynBinOp::Add,
                        "sub" => SynBinOp::Sub,
                        "mul" => SynBinOp::Mul,
                        "div" => SynBinOp::Div,
                        _ => unreachable!(),
                    };
                    let synthetic = Expr {
                        kind: ExprKind::Binary {
                            op,
                            left: Box::new(args[0].clone()),
                            right: Box::new(args[1].clone()),
                        },
                        source: expr.source,
                    };
                    return self.lower_expr(&synthetic);
                }
                if name == "neg" {
                    if args.len() != 1 {
                        self.error(
                            "E-TYPE-012",
                            format!("`neg` expects 1 argument, found {}", args.len()),
                            expr.source,
                        );
                        return None;
                    }
                    let synthetic = Expr {
                        kind: ExprKind::Unary {
                            op: SynUnOp::Neg,
                            value: Box::new(args[0].clone()),
                        },
                        source: expr.source,
                    };
                    return self.lower_expr(&synthetic);
                }
                if matches!(name.as_str(), "sum" | "product") {
                    if args.len() != 1 {
                        self.error(
                            "E-TYPE-012",
                            format!("`{name}` expects 1 argument, found {}", args.len()),
                            expr.source,
                        );
                        return None;
                    }
                    return self.lower_reduction(expr, &name, &args[0]);
                }
                if matches!(name.as_str(), "nfc" | "text_length") {
                    if args.len() != 1 {
                        self.error(
                            "E-TYPE-012",
                            format!("`{name}` expects 1 text argument, found {}", args.len()),
                            expr.source,
                        );
                        return None;
                    }
                    let (argument, infer) = self.lower_expr(&args[0])?;
                    if infer != Infer::Text {
                        self.error(
                            "E-TYPE-012",
                            format!("`{name}` expects Text"),
                            args[0].source,
                        );
                        return None;
                    }
                    let id = self.push_expr(
                        ExprNode::Call {
                            function: QualifiedName(name.clone()),
                            arguments: vec![argument],
                        },
                        expr.source,
                    );
                    return Some((
                        id,
                        if name == "text_length" {
                            Infer::Nat
                        } else {
                            Infer::Text
                        },
                    ));
                }
                if name == "section" {
                    if args.len() != 2 {
                        self.error(
                            "E-TYPE-012",
                            format!(
                                "`section` expects heading and body Text, found {}",
                                args.len()
                            ),
                            expr.source,
                        );
                        return None;
                    }
                    let (heading, heading_infer) = self.lower_expr(&args[0])?;
                    let (body, body_infer) = self.lower_expr(&args[1])?;
                    if heading_infer != Infer::Text || body_infer != Infer::Text {
                        self.error(
                            "E-TYPE-012",
                            "`section` heading and body must be Text",
                            expr.source,
                        );
                        return None;
                    }
                    let id = self.push_expr(
                        ExprNode::Call {
                            function: QualifiedName(name),
                            arguments: vec![heading, body],
                        },
                        expr.source,
                    );
                    return Some((id, Infer::Record("core::report::Section".to_string())));
                }
                if name == "document" {
                    if args.len() != 2 {
                        self.error(
                            "E-TYPE-012",
                            format!(
                                "`document` expects title Text and one Section, found {}",
                                args.len()
                            ),
                            expr.source,
                        );
                        return None;
                    }
                    let (title, title_infer) = self.lower_expr(&args[0])?;
                    let (section, section_infer) = self.lower_expr(&args[1])?;
                    if title_infer != Infer::Text
                        || section_infer != Infer::Record("core::report::Section".to_string())
                    {
                        self.error(
                            "E-TYPE-012",
                            "`document` expects a Text title and a report Section",
                            expr.source,
                        );
                        return None;
                    }
                    let id = self.push_expr(
                        ExprNode::Call {
                            function: QualifiedName(name),
                            arguments: vec![title, section],
                        },
                        expr.source,
                    );
                    return Some((id, Infer::Record("core::report::Document".to_string())));
                }
                if matches!(name.as_str(), "render_markdown" | "render_latex") {
                    if args.len() != 1 {
                        self.error(
                            "E-TYPE-012",
                            format!("`{name}` expects 1 Document, found {}", args.len()),
                            expr.source,
                        );
                        return None;
                    }
                    let (document, infer) = self.lower_expr(&args[0])?;
                    if infer != Infer::Record("core::report::Document".to_string()) {
                        self.error(
                            "E-TYPE-012",
                            format!("`{name}` expects a report Document"),
                            args[0].source,
                        );
                        return None;
                    }
                    let id = self.push_expr(
                        ExprNode::Call {
                            function: QualifiedName(name),
                            arguments: vec![document],
                        },
                        expr.source,
                    );
                    return Some((id, Infer::Text));
                }
                if name == "series_from_csv" {
                    if args.len() != 5 {
                        self.error(
                            "E-TYPE-012",
                            format!(
                                "`series_from_csv` expects CSV text, time column, value column, interpolation, and extrapolation; found {} arguments",
                                args.len()
                            ),
                            expr.source,
                        );
                        return None;
                    }
                    let Some(strings) = args
                        .iter()
                        .map(|argument| match &argument.kind {
                            ExprKind::Str(value) => Some(value.as_str()),
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>()
                    else {
                        self.error(
                            "E-SERIES-CSV",
                            "`series_from_csv` arguments must be pure string literals",
                            expr.source,
                        );
                        return None;
                    };
                    let [csv, time_column, value_column, interpolation, extrapolation] =
                        strings.as_slice()
                    else {
                        unreachable!("arity checked above")
                    };
                    if !matches!(
                        *interpolation,
                        "previous" | "linear" | "nearest" | "pwc" | "monotone_cubic"
                    ) {
                        self.error(
                            "E-SERIES-CSV",
                            format!("unknown interpolation policy `{interpolation}`"),
                            args[3].source,
                        );
                        return None;
                    }
                    if !matches!(*extrapolation, "refuse" | "clamp" | "extend") {
                        self.error(
                            "E-SERIES-CSV",
                            format!("unknown extrapolation policy `{extrapolation}`"),
                            args[4].source,
                        );
                        return None;
                    }
                    // A `\u{FEFF}` byte-order mark is a file-prefix convention
                    // some CSV exports emit before the header (or on its own
                    // line). U+FEFF is NOT `char::is_whitespace`, so a BOM-only
                    // line would otherwise survive the blank filter and become
                    // a bogus one-cell header. Stripping a single leading BOM
                    // per line is idempotent and normalizes both a BOM-prefixed
                    // header and a BOM-on-own-line away; `lines()` already
                    // handles `\r\n` endings. Blank filtering stays AFTER the
                    // strip so a BOM-only line is dropped.
                    let mut lines = csv
                        .lines()
                        .map(|line| line.strip_prefix('\u{FEFF}').unwrap_or(line))
                        .filter(|line| !line.trim().is_empty());
                    let Some(header_line) = lines.next() else {
                        self.error(
                            "E-SERIES-CSV",
                            "CSV input has no header row",
                            args[0].source,
                        );
                        return None;
                    };
                    // Keep BOTH the normalized column name (unit suffix stripped,
                    // what `csv_series_column_name` returns) and the exact raw
                    // header cell. A request may designate a column by either
                    // form; it resolves iff EXACTLY ONE candidate matches across
                    // both namespaces (a normalized name and a distinct raw cell
                    // both matching is still ambiguity, never a guess).
                    let (raw_header, header_malformed) = split_csv_fields(header_line);
                    let header: Vec<String> =
                        raw_header.iter().map(|cell| csv_series_column_name(cell)).collect();
                    // A dangling/unbalanced quote in the header corrupts column
                    // resolution, so report it before any matching (deterministic:
                    // E-CSV-006 wins over E-CSV-001/002/003/004).
                    if header_malformed {
                        self.error(
                            "E-CSV-006",
                            "CSV header has an unclosed double-quote; fix or re-quote the header row",
                            args[0].source,
                        );
                        return None;
                    }
                    // Returns `(single_match_index, match_count)`; the caller
                    // distinguishes missing (0 matches -> E-CSV-00X) from
                    // ambiguous (>=2 matches -> E-CSV-00X).
                    let matching_column = |wanted: &str| -> (Option<usize>, usize) {
                        let indices: Vec<usize> = (0..header.len())
                            .filter(|&index| {
                                header[index] == wanted || raw_header[index].trim() == wanted
                            })
                            .collect();
                        if indices.len() == 1 {
                            (Some(indices[0]), indices.len())
                        } else {
                            (None, indices.len())
                        }
                    };
                    let (time_index, time_count) = matching_column(time_column);
                    let time_index = match time_count {
                        0 => {
                            self.error(
                                "E-CSV-001",
                                format!(
                                    "time column `{time_column}` is missing; available columns: {}",
                                    header.join(", ")
                                ),
                                args[1].source,
                            );
                            return None;
                        }
                        1 => time_index.expect("one match yields an index"),
                        _ => {
                            self.error(
                                "E-CSV-002",
                                format!(
                                    "time column `{time_column}` is ambiguous: {time_count} columns match; available columns: {}",
                                    header.join(", ")
                                ),
                                args[1].source,
                            );
                            return None;
                        }
                    };
                    let (value_index, value_count) = matching_column(value_column);
                    let value_index = match value_count {
                        0 => {
                            self.error(
                                "E-CSV-003",
                                format!(
                                    "value column `{value_column}` is missing; available columns: {}",
                                    header.join(", ")
                                ),
                                args[2].source,
                            );
                            return None;
                        }
                        1 => value_index.expect("one match yields an index"),
                        _ => {
                            self.error(
                                "E-CSV-004",
                                format!(
                                    "value column `{value_column}` is ambiguous: {value_count} columns match; available columns: {}",
                                    header.join(", ")
                                ),
                                args[2].source,
                            );
                            return None;
                        }
                    };
                    let mut points = Vec::new();
                    for (row_index, line) in lines.enumerate() {
                        // Field split first, trim AFTER unquoting: a quoted
                        // ` " 1,5 " ` unquotes to ` 1,5 ` then trims to `1,5`
                        // (deterministic; spaces inside quotes are not
                        // significant for numeric cells), while an unquoted
                        // ` 0.0 , 1.0 ` still trims to `0.0, 1.0`.
                        let (line_fields, row_malformed) = split_csv_fields(line);
                        // Malformed quotes checked BEFORE raggedness: an
                        // unclosed quote corrupts escaping AND can shift the
                        // cell count, and the quote defect is the root cause.
                        // Deterministic: E-CSV-006 wins over E-CSV-005 per row.
                        if row_malformed {
                            self.error(
                                "E-CSV-006",
                                format!(
                                    "CSV row {} has an unclosed double-quote; fix or re-quote the row",
                                    row_index + 2
                                ),
                                args[0].source,
                            );
                            return None;
                        }
                        if line_fields.len() != header.len() {
                            self.error(
                                "E-CSV-005",
                                format!(
                                    "CSV row {} has {} cells, expected {}",
                                    row_index + 2,
                                    line_fields.len(),
                                    header.len()
                                ),
                                args[0].source,
                            );
                            return None;
                        }
                        // Only the two projected cells are parsed; each is
                        // trimmed here (after unquoting) rather than copying
                        // every cell of the row into owned Strings first.
                        let parse_cell = |index: usize| {
                            line_fields[index]
                                .trim()
                                .parse::<f64>()
                                .ok()
                                .filter(|value| value.is_finite())
                        };
                        let (Some(time), Some(value)) =
                            (parse_cell(time_index), parse_cell(value_index))
                        else {
                            self.error(
                                "E-CSV-008",
                                format!(
                                    "CSV row {} selected columns must contain finite numbers",
                                    row_index + 2
                                ),
                                args[0].source,
                            );
                            return None;
                        };
                        if points.last().is_some_and(|(previous, _)| time <= *previous) {
                            self.error(
                                "E-CSV-009",
                                format!(
                                    "CSV time column `{time_column}` is nonincreasing: row {} time {time} is not strictly after the previous row's time",
                                    row_index + 2
                                ),
                                args[0].source,
                            );
                            return None;
                        }
                        points.push((time, value));
                    }
                    if points.is_empty() {
                        self.error(
                            "E-CSV-007",
                            "CSV series has no data rows",
                            args[0].source,
                        );
                        return None;
                    }
                    let id = self.push_expr(
                        ExprNode::Series {
                            points,
                            interpolation: (*interpolation).to_string(),
                            extrapolation: (*extrapolation).to_string(),
                        },
                        expr.source,
                    );
                    return Some((id, Infer::Series));
                }
                // Generic declared/mounted-capability call path — the
                // shared builtin-miss seam: a call whose name resolves
                // to a capability cell of this package lowers to
                // `ExprNode::Apply`, the exec emitter's existing
                // ApplyCapability application path. No new builtin
                // name, no domain keyword, no parser branch: the cell's
                // identity and result type are its own declaration's
                // data. Unknown names still refuse below (the typed
                // unknown-function diagnostic). The hot path (no cells
                // in the package) pays zero allocations.
                if !self.capability_cells.is_empty() {
                    let dotted = name
                        .contains("::")
                        .then(|| name.replace("::", "."));
                    if let Some((_key, capability_index, output)) = self
                        .capability_cells
                        .iter()
                        .find(|(key, _, _)| *key == name || dotted.as_deref() == Some(key))
                        .cloned()
                    {
                    let mut arguments = Vec::with_capacity(args.len());
                    for argument in args {
                        let (argument, _) = self.lower_expr(argument)?;
                        arguments.push(argument);
                    }
                    let id = self.push_expr(
                        ExprNode::Apply {
                            capability: CapabilityId(capability_index),
                            arguments,
                        },
                        expr.source,
                    );
                    return Some((id, capability_result_infer(output.as_deref())));
                    }
                }
                let arity: Option<usize> = match name.as_str() {
                    s if BuiltinId::from_name(s).is_some() => {
                        Some(BuiltinId::from_name(s).unwrap().arity())
                    }
                    // Option/Result constructors, predicates, unwrap-or,
                    // and error projection (emath-option-result-graph-field-aj8d).
                    "option_none" => Some(0),
                    "option_some"
                    | "option_is_some"
                    | "result_ok"
                    | "result_err"
                    | "result_is_ok"
                    | "result_error_of" => Some(1),
                    "option_unwrap_or" | "result_unwrap_or" => Some(2),
                    "is_finite"
                    | "norm"
                    | "transpose"
                    | "length"
                    | "mean"
                    | "factorial"
                    | "grad"
                    | "not"
                    | "poisson_sine"
                    | "eigvals"
                    | "eigvecs"
                    | "singular_values"
                    | "svd_factors"
                    | "sparse_triplets"
                    | "poles_stable"
                    | "out_degrees"
                    | "graph_laplacian"
                    | "graph_symmetrize"
                    | "pareto_front"
                    | "lu"
                    | "qr"
                    | "gamma"
                    | "gamma_error_bound"
                    | "erf"
                    | "erf_error_bound"
                    | "zeta"
                    | "zeta_error_bound"
                    | "lambert_w0"
                    | "lambert_w0_error_bound"
                    | "elliptic_k"
                    | "elliptic_k_error_bound"
                    | "elliptic_e"
                    | "elliptic_e_error_bound" => Some(1),
                    "pow"
                    | "dot"
                    | "laplacian"
                    | "laplacian_neumann"
                    | "laplacian_2d"
                    | "laplacian_2d_neumann"
                    | "gradient"
                    | "gradient_2d_x"
                    | "gradient_2d_y"
                    | "gradient_3d_x"
                    | "gradient_3d_y"
                    | "gradient_3d_z"
                    | "div_1d"
                    | "mod_inv" | "field_inv" | "int_rem"
                    | "hamming_distance"
                    | "series_at"
                    | "coefficient"
                    | "beta"
                    | "beta_error_bound" => Some(2),
                    "solve_iterative"
                    | "bellman_ford"
                    | "sparse_from_triplets"
                    | "poly_add"
                    | "poly_mul"
                    | "poly_eval"
                    | "reachability"
                    | "bfs_order"
                    | "shortest_distances"
                    | "solve_linear"
                    | "outer_product" => Some(2),
                    "lerp"
                    | "clamp"
                    | "congruence"
                    | "poly_eval_mod"
                    | "rs_encode"
                    | "transfer_eval"
                    | "dc_gain"
                    | "lp_minimize"
                    | "generating_function"
                    | "convolution"
                    | "elliptic_pi"
                    | "elliptic_pi_error_bound" => Some(3),
                    "normal_sample" | "uniform_sample" | "bernoulli_sample" => {
                        if !matches!(args.len(), 3 | 4) {
                            self.error(
                                "E-TYPE-012",
                                format!(
                                    "`{name}` expects (params, seed, draws[, stream]), found {} arguments",
                                    args.len()
                                ),
                                expr.source,
                            );
                            return None;
                        }
                        Some(args.len())
                    }
                    "normal_density" | "uniform_density" | "bernoulli_pmf" => Some(2),
                    // Certified intervals (8pjn): constructor and intersection.
                    "interval" | "intersect" => Some(2),
                    "laplacian_dirichlet" => Some(4),
                    "laplacian_3d" | "laplacian_3d_neumann" => {
                        if !matches!(args.len(), 2 | 4) {
                            self.error(
                                "E-TYPE-012",
                                format!(
                                    "`{name}` expects (tensor, spacing) or (tensor, dx, dy, dz), found {} arguments",
                                    args.len()
                                ),
                                expr.source,
                            );
                            return None;
                        }
                        Some(args.len())
                    }
                    "div_2d" => {
                        if !matches!(args.len(), 3 | 4) {
                            self.error(
                                "E-TYPE-012",
                                format!(
                                    "`div_2d` expects (vx, vy, spacing) or (vx, vy, dx, dy), found {} arguments",
                                    args.len()
                                ),
                                expr.source,
                            );
                            return None;
                        }
                        Some(args.len())
                    }
                    "div" | "div_3d" => {
                        if !matches!(args.len(), 4 | 6) {
                            self.error(
                                "E-TYPE-012",
                                format!(
                                    "`{name}` expects (vx, vy, vz, spacing) or (vx, vy, vz, dx, dy, dz), found {} arguments",
                                    args.len()
                                ),
                                expr.source,
                            );
                            return None;
                        }
                        Some(args.len())
                    }
                    "einsum" => {
                        // einsum(subscripts, tensor1, ...) — variable arity, min 2.
                        if args.len() < 2 {
                            self.error(
                                "E-TYPE-012",
                                "`einsum` expects at least 2 arguments (subscripts + tensors)",
                                expr.source,
                            );
                            return None;
                        }
                        // First arg must be a string literal.
                        if !matches!(&args[0].kind, ExprKind::Str(_)) {
                            self.error(
                                "E-TYPE-012",
                                "`einsum` first argument must be a string literal",
                                args[0].source,
                            );
                            return None;
                        }
                        // Lower as Einsum op.
                        return self.lower_einsum(expr, &name, &args);
                    }
                    _ => {
                        self.error(
                            E_UNKNOWN_FUNCTION,
                            format!(
                                "unknown function `{name}` (Phase 1 builtins — math: add, sub, mul, div, neg, exp, ln, log, sqrt, sin, cos, tan, tanh, abs, floor, ceil, round, sign, log2, log10, sinh, cosh, atan, cbrt, recip, fract, min, max, atan2, pow, mod, hypot, is_finite, factorial, mod_inv, int_rem, congruence; autodiff: grad; linalg: norm, transpose, dot, length, einsum; pde: laplacian, laplacian_neumann, laplacian_dirichlet, laplacian_2d, laplacian_2d_neumann, laplacian_3d, laplacian_3d_neumann, gradient, gradient_2d_x, gradient_2d_y, gradient_3d_x, gradient_3d_y, gradient_3d_z, div_1d, div_2d, div/div_3d; coding: poly_eval_mod, rs_encode, hamming_distance; logic: not. Use bare names or namespace::name)"
                            ),
                            function.source,
                        );
                        return None;
                    }
                };
                if arity != Some(args.len()) {
                    self.error(
                        "E-TYPE-012",
                        format!(
                            "`{name}` expects {arity:?} argument(s), found {}",
                            args.len()
                        ),
                        expr.source,
                    );
                    return None;
                }
                match name.as_str() {
                    "series_at" => {
                        let (series_id, series_infer) = self.lower_expr(&args[0])?;
                        if !matches!(series_infer, Infer::Series | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`series_at` expects a Series as its first argument",
                                args[0].source,
                            );
                            return None;
                        }
                        let (time_id, time_infer) = self.lower_expr(&args[1])?;
                        if !matches!(
                            time_infer,
                            Infer::F64
                                | Infer::Nat
                                | Infer::Int
                                | Infer::Unit { .. }
                                | Infer::HostDeferred
                        ) {
                            self.error(
                                "E-TYPE-012",
                                "`series_at` expects a numeric time coordinate",
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![series_id, time_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "norm" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        if !matches!(arg_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`norm` expects a Vector argument",
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![arg_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "poisson_sine" => {
                        let (load_id, load_infer) = self.lower_expr(&args[0])?;
                        let extent = match load_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`poisson_sine` expects a Vector interior load",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![load_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "eigvals" | "singular_values" => {
                        let (matrix_id, matrix_infer) = self.lower_expr(&args[0])?;
                        let extent = match matrix_infer {
                            Infer::Matrix { rows, cols } => {
                                if name == "eigvals" && rows != cols {
                                    self.error(
                                        "E-SHAPE-005",
                                        "`eigvals` expects a square matrix",
                                        args[0].source,
                                    );
                                    return None;
                                }
                                if name == "eigvals" { rows } else { None }
                            }
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Matrix argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![matrix_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "eigvecs" | "svd_factors" => {
                        let (matrix_id, matrix_infer) = self.lower_expr(&args[0])?;
                        let (rows, cols) = match matrix_infer {
                            Infer::Matrix { rows, cols } => (rows, cols),
                            Infer::HostDeferred => (None, None),
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Matrix argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![matrix_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Matrix { rows, cols }))
                    }
                    "solve_iterative" => {
                        let (matrix_id, matrix_infer) = self.lower_expr(&args[0])?;
                        let rows = match matrix_infer {
                            Infer::Matrix { rows, cols } if rows == cols => rows,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`solve_iterative` expects a square Matrix first argument",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (rhs_id, rhs_infer) = self.lower_expr(&args[1])?;
                        let extent = match rhs_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`solve_iterative` expects a Vector right-hand side",
                                    args[1].source,
                                );
                                return None;
                            }
                        };
                        if rows.is_some() && extent.is_some() && rows != extent {
                            self.error(
                                "E-SHAPE-005",
                                "`solve_iterative` matrix and vector extents must agree",
                                expr.source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![matrix_id, rhs_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "bellman_ford" => {
                        let (matrix_id, matrix_infer) = self.lower_expr(&args[0])?;
                        let extent = match matrix_infer {
                            Infer::Matrix { rows, cols } if rows == cols => rows,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`bellman_ford` expects a square Matrix adjacency",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (source_id, source_infer) = self.lower_expr(&args[1])?;
                        if !matches!(
                            source_infer,
                            Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                        ) {
                            self.error(
                                "E-TYPE-012",
                                "`bellman_ford` source must be scalar",
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![matrix_id, source_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "sparse_triplets" => {
                        let (matrix_id, matrix_infer) = self.lower_expr(&args[0])?;
                        if !matches!(matrix_infer, Infer::Matrix { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`sparse_triplets` expects a Matrix adjacency",
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![matrix_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent: None }))
                    }
                    "sparse_from_triplets" => {
                        let (n_id, n_infer) = self.lower_expr(&args[0])?;
                        if !matches!(
                            n_infer,
                            Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                        ) {
                            self.error(
                                "E-TYPE-012",
                                "`sparse_from_triplets` vertex count must be scalar",
                                args[0].source,
                            );
                            return None;
                        }
                        let (triplets_id, triplets_infer) = self.lower_expr(&args[1])?;
                        if !matches!(triplets_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`sparse_from_triplets` expects a triplet Vector",
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![n_id, triplets_id],
                            },
                            expr.source,
                        );
                        Some((
                            id,
                            Infer::Matrix {
                                rows: None,
                                cols: None,
                            },
                        ))
                    }
                    "reachability" | "bfs_order" | "shortest_distances" => {
                        let (matrix_id, matrix_infer) = self.lower_expr(&args[0])?;
                        let extent = match matrix_infer {
                            Infer::Matrix { rows, cols } if rows == cols => rows,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a square Matrix adjacency"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (source_id, source_infer) = self.lower_expr(&args[1])?;
                        if !matches!(
                            source_infer,
                            Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                        ) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` source must be scalar"),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![matrix_id, source_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "out_degrees" | "graph_laplacian" | "graph_symmetrize" => {
                        let (matrix_id, matrix_infer) = self.lower_expr(&args[0])?;
                        let (rows, cols) = match matrix_infer {
                            Infer::Matrix { rows, cols } => (rows, cols),
                            Infer::HostDeferred => (None, None),
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Matrix adjacency"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![matrix_id],
                            },
                            expr.source,
                        );
                        if name == "out_degrees" {
                            Some((id, Infer::Vector { extent: rows }))
                        } else {
                            Some((id, Infer::Matrix { rows, cols }))
                        }
                    }
                    "lp_minimize" => {
                        let mut lowered = Vec::with_capacity(3);
                        for (index, argument) in args.iter().enumerate() {
                            let (id, infer) = self.lower_expr(argument)?;
                            let admitted = if index == 0 {
                                matches!(infer, Infer::Matrix { .. } | Infer::HostDeferred)
                            } else {
                                matches!(infer, Infer::Vector { .. } | Infer::HostDeferred)
                            };
                            if !admitted {
                                self.error(
                                    "E-TYPE-012",
                                    "`lp_minimize` expects (Matrix, Vector, Vector)",
                                    argument.source,
                                );
                                return None;
                            }
                            lowered.push(id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: lowered,
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent: None }))
                    }
                    "pareto_front" => {
                        let (points_id, points_infer) = self.lower_expr(&args[0])?;
                        if !matches!(points_infer, Infer::Matrix { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`pareto_front` expects a Matrix of objective points",
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![points_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent: None }))
                    }
                    "solve_linear" => {
                        let (matrix_id, matrix_infer) = self.lower_expr(&args[0])?;
                        let rows = match matrix_infer {
                            Infer::Matrix { rows, cols } if rows == cols => rows,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`solve_linear` expects a square Matrix",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (rhs_id, rhs_infer) = self.lower_expr(&args[1])?;
                        let extent = match rhs_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`solve_linear` expects a Vector right-hand side",
                                    args[1].source,
                                );
                                return None;
                            }
                        };
                        if rows.is_some() && extent.is_some() && rows != extent {
                            self.error(
                                "E-SHAPE-005",
                                "`solve_linear` matrix and vector extents must agree",
                                expr.source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![matrix_id, rhs_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "lu" | "qr" => {
                        let (matrix_id, matrix_infer) = self.lower_expr(&args[0])?;
                        let (rows, cols) = match matrix_infer {
                            Infer::Matrix { rows, cols } => (rows, cols),
                            Infer::HostDeferred => (None, None),
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Matrix"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![matrix_id],
                            },
                            expr.source,
                        );
                        Some((
                            id,
                            Infer::Matrix {
                                rows: None,
                                cols: cols.or(rows),
                            },
                        ))
                    }
                    "outer_product" => {
                        let mut lowered = Vec::with_capacity(2);
                        let mut extents = Vec::with_capacity(2);
                        for argument in args {
                            let (id, infer) = self.lower_expr(argument)?;
                            let extent = match infer {
                                Infer::Vector { extent } => extent,
                                Infer::HostDeferred => None,
                                _ => {
                                    self.error(
                                        "E-TYPE-012",
                                        "`outer_product` expects two Vectors",
                                        argument.source,
                                    );
                                    return None;
                                }
                            };
                            lowered.push(id);
                            extents.push(extent);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: lowered,
                            },
                            expr.source,
                        );
                        Some((
                            id,
                            Infer::Matrix {
                                rows: extents[0].clone(),
                                cols: extents[1].clone(),
                            },
                        ))
                    }
                    "transfer_eval" => {
                        let mut lowered = Vec::with_capacity(3);
                        for (index, argument) in args.iter().enumerate() {
                            let (id, infer) = self.lower_expr(argument)?;
                            let admitted = if index < 2 {
                                matches!(infer, Infer::Vector { .. } | Infer::HostDeferred)
                            } else {
                                matches!(
                                    infer,
                                    Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                                )
                            };
                            if !admitted {
                                self.error(
                                    "E-TYPE-012",
                                    "`transfer_eval` expects (Vector, Vector, scalar)",
                                    argument.source,
                                );
                                return None;
                            }
                            lowered.push(id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: lowered,
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "dc_gain" => {
                        let mut lowered = Vec::with_capacity(3);
                        for (index, argument) in args.iter().enumerate() {
                            let (id, infer) = self.lower_expr(argument)?;
                            let admitted = if index == 0 {
                                matches!(infer, Infer::Matrix { .. } | Infer::HostDeferred)
                            } else {
                                matches!(infer, Infer::Vector { .. } | Infer::HostDeferred)
                            };
                            if !admitted {
                                self.error(
                                    "E-TYPE-012",
                                    "`dc_gain` expects (Matrix, Vector, Vector)",
                                    argument.source,
                                );
                                return None;
                            }
                            lowered.push(id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: lowered,
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "poles_stable" => {
                        let (den_id, den_infer) = self.lower_expr(&args[0])?;
                        if !matches!(den_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`poles_stable` expects a denominator Vector",
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![den_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Bool))
                    }
                    "gamma"
                    | "gamma_error_bound"
                    | "beta"
                    | "beta_error_bound"
                    | "erf"
                    | "erf_error_bound"
                    | "zeta"
                    | "zeta_error_bound"
                    | "lambert_w0"
                    | "lambert_w0_error_bound"
                    | "elliptic_k"
                    | "elliptic_k_error_bound"
                    | "elliptic_e"
                    | "elliptic_e_error_bound"
                    | "elliptic_pi"
                    | "elliptic_pi_error_bound" => {
                        let mut lowered = Vec::with_capacity(args.len());
                        for argument in args {
                            let (id, infer) = self.lower_expr(argument)?;
                            if !matches!(
                                infer,
                                Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                            ) {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` arguments must be real scalars"),
                                    argument.source,
                                );
                                return None;
                            }
                            lowered.push(id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: lowered,
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "poly_add" | "poly_mul" => {
                        let mut lowered = Vec::with_capacity(2);
                        let mut extent = None;
                        for argument in args {
                            let (id, infer) = self.lower_expr(argument)?;
                            match infer {
                                Infer::Vector { extent: arg_extent } => {
                                    if extent.is_none() {
                                        extent = arg_extent;
                                    }
                                }
                                Infer::HostDeferred => {}
                                _ => {
                                    self.error(
                                        "E-TYPE-012",
                                        format!("`{name}` expects coefficient Vectors"),
                                        argument.source,
                                    );
                                    return None;
                                }
                            }
                            lowered.push(id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: lowered,
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "poly_eval" => {
                        let (coefficients_id, coefficients_infer) = self.lower_expr(&args[0])?;
                        if !matches!(
                            coefficients_infer,
                            Infer::Vector { .. } | Infer::HostDeferred
                        ) {
                            self.error(
                                "E-TYPE-012",
                                "`poly_eval` expects a coefficient Vector",
                                args[0].source,
                            );
                            return None;
                        }
                        let (point_id, point_infer) = self.lower_expr(&args[1])?;
                        if !matches!(
                            point_infer,
                            Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                        ) {
                            self.error(
                                "E-TYPE-012",
                                "`poly_eval` point must be scalar",
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![coefficients_id, point_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "generating_function" => {
                        let mut lowered = Vec::with_capacity(3);
                        for (index, argument) in args.iter().enumerate() {
                            let (id, infer) = self.lower_expr(argument)?;
                            let valid = if index < 2 {
                                matches!(infer, Infer::Vector { .. } | Infer::HostDeferred)
                            } else {
                                matches!(
                                    infer,
                                    Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                                )
                            };
                            if !valid {
                                self.error(
                                    "E-TYPE-012",
                                    "`generating_function` expects (initial Vector, recurrence Vector, budget)",
                                    argument.source,
                                );
                                return None;
                            }
                            lowered.push(id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: lowered,
                            },
                            expr.source,
                        );
                        Some((id, Infer::Sequence))
                    }
                    "coefficient" => {
                        let (sequence, sequence_infer) = self.lower_expr(&args[0])?;
                        let (index, index_infer) = self.lower_expr(&args[1])?;
                        if sequence_infer != Infer::Sequence
                            || !matches!(
                                index_infer,
                                Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                            )
                        {
                            self.error(
                                "E-TYPE-012",
                                "`coefficient` expects (Sequence, nonnegative integer index)",
                                expr.source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Index {
                                value: sequence,
                                indices: vec![index],
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "convolution" => {
                        let (left, left_infer) = self.lower_expr(&args[0])?;
                        let (right, right_infer) = self.lower_expr(&args[1])?;
                        let (count, count_infer) = self.lower_expr(&args[2])?;
                        if left_infer != Infer::Sequence
                            || right_infer != Infer::Sequence
                            || !matches!(
                                count_infer,
                                Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                            )
                        {
                            self.error(
                                "E-TYPE-012",
                                "`convolution` expects (Sequence, Sequence, coefficient count)",
                                expr.source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![left, right, count],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent: None }))
                    }
                    "normal_sample" | "uniform_sample" | "bernoulli_sample" => {
                        let (params_id, params_infer) = self.lower_expr(&args[0])?;
                        if !matches!(params_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a Vector parameter list"),
                                args[0].source,
                            );
                            return None;
                        }
                        let (seed_id, seed_infer) = self.lower_expr(&args[1])?;
                        let (draws_id, draws_infer) = self.lower_expr(&args[2])?;
                        if !matches!(seed_infer, Infer::F64 | Infer::HostDeferred)
                            || !matches!(draws_infer, Infer::F64 | Infer::HostDeferred)
                        {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` seed and draw count must be Float64"),
                                expr.source,
                            );
                            return None;
                        }
                        let mut arguments = vec![params_id, seed_id, draws_id];
                        if let Some(stream) = args.get(3) {
                            let (stream_id, stream_infer) = self.lower_expr(stream)?;
                            if !matches!(stream_infer, Infer::Text) {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` stream path must be Text"),
                                    stream.source,
                                );
                                return None;
                            }
                            arguments.push(stream_id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments,
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent: None }))
                    }
                    "normal_density" | "uniform_density" | "bernoulli_pmf" => {
                        let (params_id, params_infer) = self.lower_expr(&args[0])?;
                        if !matches!(params_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a Vector parameter list"),
                                args[0].source,
                            );
                            return None;
                        }
                        let (point_id, point_infer) = self.lower_expr(&args[1])?;
                        if !matches!(point_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` evaluation point must be Float64"),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![params_id, point_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "laplacian" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`laplacian` expects a Vector first argument",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian` expects a Float64 cell width (dx) as the second argument",
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "laplacian_neumann" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`laplacian_neumann` expects a Vector first argument",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_neumann` expects a Float64 cell width (dx) as the second argument",
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "laplacian_dirichlet" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`laplacian_dirichlet` expects a Vector first argument",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_dirichlet` expects a Float64 cell width (dx) as the second argument",
                                args[1].source,
                            );
                            return None;
                        }
                        let (g_left_id, g_left_infer) = self.lower_expr(&args[2])?;
                        if !matches!(g_left_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_dirichlet` expects a Float64 left boundary value as the third argument",
                                args[2].source,
                            );
                            return None;
                        }
                        let (g_right_id, g_right_infer) = self.lower_expr(&args[3])?;
                        if !matches!(g_right_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_dirichlet` expects a Float64 right boundary value as the fourth argument",
                                args[3].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id, g_left_id, g_right_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "laplacian_2d" | "laplacian_2d_neumann" => {
                        let (mat_id, mat_infer) = self.lower_expr(&args[0])?;
                        let (rows, cols) = match mat_infer {
                            Infer::Matrix { rows, cols } => (rows, cols),
                            Infer::HostDeferred => (None, None),
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Matrix first argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a Float64 cell width (dx) as the second argument"),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![mat_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Matrix { rows, cols }))
                    }
                    "gradient" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Vector first argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a Float64 cell width (dx) as the second argument"),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "gradient_2d_x" | "gradient_2d_y" => {
                        let (mat_id, mat_infer) = self.lower_expr(&args[0])?;
                        let (rows, cols) = match mat_infer {
                            Infer::Matrix { rows, cols } => (rows, cols),
                            Infer::HostDeferred => (None, None),
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Matrix first argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a Float64 cell width (dx) as the second argument"),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![mat_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Matrix { rows, cols }))
                    }
                    "laplacian_3d"
                    | "laplacian_3d_neumann"
                    | "gradient_3d_x"
                    | "gradient_3d_y"
                    | "gradient_3d_z" => {
                        let (tensor_id, tensor_infer) = self.lower_expr(&args[0])?;
                        let shape = match tensor_infer {
                            Infer::Tensor { shape } if shape.len() == 3 => shape,
                            Infer::HostDeferred => Vec::new(),
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a rank-3 Tensor first argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let mut arguments = vec![tensor_id];
                        for spacing in &args[1..] {
                            let (spacing_id, spacing_infer) = self.lower_expr(spacing)?;
                            if !matches!(spacing_infer, Infer::F64 | Infer::HostDeferred) {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` spacing arguments must be Float64"),
                                    spacing.source,
                                );
                                return None;
                            }
                            arguments.push(spacing_id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments,
                            },
                            expr.source,
                        );
                        Some((id, Infer::Tensor { shape }))
                    }
                    "div_1d" => {
                        let (field_id, field_infer) = self.lower_expr(&args[0])?;
                        let extent = match field_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`div_1d` expects a Vector first argument",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`div_1d` spacing must be Float64",
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![field_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "div_2d" => {
                        let mut shape = None;
                        let mut arguments = Vec::with_capacity(args.len());
                        for field in &args[..2] {
                            let (field_id, field_infer) = self.lower_expr(field)?;
                            let found = match field_infer {
                                Infer::Matrix { rows, cols } => Some((rows, cols)),
                                Infer::HostDeferred => None,
                                _ => {
                                    self.error(
                                        "E-TYPE-012",
                                        "`div_2d` field arguments must be Matrix values",
                                        field.source,
                                    );
                                    return None;
                                }
                            };
                            if let (Some(expected), Some(found)) = (&shape, &found) {
                                if expected != found {
                                    self.error(
                                        "E-SHAPE-005",
                                        "`div_2d` field arguments must have equal shapes",
                                        field.source,
                                    );
                                    return None;
                                }
                            } else if found.is_some() {
                                shape = found;
                            }
                            arguments.push(field_id);
                        }
                        for spacing in &args[2..] {
                            let (spacing_id, spacing_infer) = self.lower_expr(spacing)?;
                            if !matches!(spacing_infer, Infer::F64 | Infer::HostDeferred) {
                                self.error(
                                    "E-TYPE-012",
                                    "`div_2d` spacing arguments must be Float64",
                                    spacing.source,
                                );
                                return None;
                            }
                            arguments.push(spacing_id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments,
                            },
                            expr.source,
                        );
                        let (rows, cols) = shape.unwrap_or((None, None));
                        Some((id, Infer::Matrix { rows, cols }))
                    }
                    "div" | "div_3d" => {
                        let mut shape: Option<Vec<Extent>> = None;
                        let mut arguments = Vec::with_capacity(args.len());
                        for field in &args[..3] {
                            let (field_id, field_infer) = self.lower_expr(field)?;
                            let found = match field_infer {
                                Infer::Tensor { shape } if shape.len() == 3 => Some(shape),
                                Infer::HostDeferred => None,
                                _ => {
                                    self.error(
                                        "E-TYPE-012",
                                        format!(
                                            "`{name}` field arguments must be rank-3 Tensor values"
                                        ),
                                        field.source,
                                    );
                                    return None;
                                }
                            };
                            if let (Some(expected), Some(found)) = (&shape, &found) {
                                if expected != found {
                                    self.error(
                                        "E-SHAPE-005",
                                        format!("`{name}` field arguments must have equal shapes"),
                                        field.source,
                                    );
                                    return None;
                                }
                            } else if found.is_some() {
                                shape = found;
                            }
                            arguments.push(field_id);
                        }
                        for spacing in &args[3..] {
                            let (spacing_id, spacing_infer) = self.lower_expr(spacing)?;
                            if !matches!(spacing_infer, Infer::F64 | Infer::HostDeferred) {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` spacing arguments must be Float64"),
                                    spacing.source,
                                );
                                return None;
                            }
                            arguments.push(spacing_id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments,
                            },
                            expr.source,
                        );
                        Some((
                            id,
                            Infer::Tensor {
                                shape: shape.unwrap_or_default(),
                            },
                        ))
                    }
                    "transpose" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        match arg_infer {
                            Infer::Matrix { rows, cols } => {
                                let id = self.push_expr(
                                    ExprNode::Call {
                                        function: QualifiedName(name.clone()),
                                        arguments: vec![arg_id],
                                    },
                                    expr.source,
                                );
                                Some((
                                    id,
                                    Infer::Matrix {
                                        rows: cols,
                                        cols: rows,
                                    },
                                ))
                            }
                            Infer::HostDeferred => {
                                let id = self.push_expr(
                                    ExprNode::Call {
                                        function: QualifiedName(name.clone()),
                                        arguments: vec![arg_id],
                                    },
                                    expr.source,
                                );
                                Some((
                                    id,
                                    Infer::Matrix {
                                        rows: None,
                                        cols: None,
                                    },
                                ))
                            }
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`transpose` expects a Matrix argument",
                                    args[0].source,
                                );
                                None
                            }
                        }
                    }
                    "length" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        if !matches!(arg_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`length` expects a Vector argument",
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![arg_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "dot" => {
                        let (l_id, l_infer) = self.lower_expr(&args[0])?;
                        let (r_id, r_infer) = self.lower_expr(&args[1])?;
                        match (&l_infer, &r_infer) {
                            (Infer::Vector { extent: e1 }, Infer::Vector { extent: e2 }) => {
                                if let (Some(ext1), Some(ext2)) = (e1, e2) {
                                    if ext1 != ext2 {
                                        self.error(
                                            "E-SHAPE-002",
                                            format!("dimension mismatch in dot product: {ext1:?} vs {ext2:?}"),
                                            expr.source,
                                        );
                                        return None;
                                    }
                                }
                                let id = self.push_expr(
                                    ExprNode::Binary {
                                        operation: emath_ir::BinaryOp::VectorDot,
                                        left: l_id,
                                        right: r_id,
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::F64))
                            }
                            (Infer::HostDeferred, _) | (_, Infer::HostDeferred) => {
                                let id = self.push_expr(
                                    ExprNode::Binary {
                                        operation: emath_ir::BinaryOp::VectorDot,
                                        left: l_id,
                                        right: r_id,
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::F64))
                            }
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`dot` expects two Vector arguments",
                                    expr.source,
                                );
                                None
                            }
                        }
                    }
                    "mean" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        if !matches!(arg_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`mean` expects a Vector argument",
                                args[0].source,
                            );
                            return None;
                        }
                        // mean = sum(arg) / length(arg), reusing the known-shape fold and len.
                        let (sum_id, _) = self.lower_reduction(expr, "sum", &args[0])?;
                        let length_id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName("length".to_string()),
                                arguments: vec![arg_id],
                            },
                            expr.source,
                        );
                        let id = self.push_expr(
                            ExprNode::Binary {
                                operation: emath_ir::BinaryOp::StrictFloatDiv,
                                left: sum_id,
                                right: length_id,
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "abs" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        match arg_infer {
                            Infer::F64
                            | Infer::Nat
                            | Infer::Int
                            | Infer::Complex
                            | Infer::HostDeferred => {
                                let id = self.push_expr(
                                    ExprNode::Call {
                                        function: QualifiedName("abs".to_string()),
                                        arguments: vec![arg_id],
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::F64))
                            }
                            Infer::Vector {
                                extent: Some(Extent::Fixed(n)),
                            } => {
                                let mut elems = Vec::with_capacity(n);
                                for i in 0..n {
                                    let idx = self.push_expr(
                                        ExprNode::Literal(Literal::Integer(i.to_string())),
                                        expr.source,
                                    );
                                    let term = self.push_expr(
                                        ExprNode::Index {
                                            value: arg_id,
                                            indices: vec![idx],
                                        },
                                        expr.source,
                                    );
                                    let abs_term = self.push_expr(
                                        ExprNode::Call {
                                            function: QualifiedName("abs".to_string()),
                                            arguments: vec![term],
                                        },
                                        expr.source,
                                    );
                                    elems.push(abs_term);
                                }
                                let id = self.push_expr(ExprNode::Vector(elems), expr.source);
                                Some((
                                    id,
                                    Infer::Vector {
                                        extent: Some(Extent::Fixed(n)),
                                    },
                                ))
                            }
                            Infer::Vector { extent: None } => {
                                self.error(
                                    "E-TYPE-012",
                                    "`abs` on a vector needs a known size",
                                    args[0].source,
                                );
                                None
                            }
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`abs` expects a scalar or vector argument",
                                    args[0].source,
                                );
                                None
                            }
                        }
                    }
                    "factorial" => {
                        let (arg_id, _) = self.lower_expr(&args[0])?;
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![arg_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Int))
                    }
                    "not" => {
                        // `core::logic::not` as a callable: the bool
                        // complement used by `notation` targets (for
                        // example `notation prefix 80 "¬" =>
                        // core::logic::not`). Lowered as EmirOp::Not by
                        // the exec emitter; operand must be Bool.
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        if !matches!(arg_infer, Infer::Bool) {
                            self.error(
                                "E-TYPE-012",
                                "argument to `not` must be Boolean",
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![arg_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Bool))
                    }
                    "grad" => {
                        // Reverse-mode AD: grad(expr) computes the gradient
                        // of a scalar expression w.r.t. all declaration
                        // inputs.  Returns Vector[N] where N = input count.
                        let (body_id, body_infer) = self.lower_expr(&args[0])?;
                        if !is_numeric_element(&body_infer) {
                            self.error(
                                "E-TYPE-012",
                                "`grad` expects a scalar numeric expression",
                                args[0].source,
                            );
                            return None;
                        }
                        let n = self.inputs.len();
                        if n == 0 {
                            self.error(
                                "E-TYPE-012",
                                "`grad` requires at least one input to differentiate",
                                expr.source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![body_id],
                            },
                            expr.source,
                        );
                        Some((
                            id,
                            Infer::Vector {
                                extent: Some(Extent::Fixed(n)),
                            },
                        ))
                    }
                    "mod_inv" => {
                        let (a_id, _) = self.lower_expr(&args[0])?;
                        let (m_id, _) = self.lower_expr(&args[1])?;
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![a_id, m_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Int))
                    }
                    "congruence" => {
                        let (a_id, _) = self.lower_expr(&args[0])?;
                        let (b_id, _) = self.lower_expr(&args[1])?;
                        let (m_id, _) = self.lower_expr(&args[2])?;
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![a_id, b_id, m_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Bool))
                    }
                    "poly_eval_mod" => {
                        let (c_id, _) = self.lower_expr(&args[0])?;
                        let (x_id, _) = self.lower_expr(&args[1])?;
                        let (p_id, _) = self.lower_expr(&args[2])?;
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![c_id, x_id, p_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Int))
                    }
                    "rs_encode" => {
                        let (c_id, _) = self.lower_expr(&args[0])?;
                        let (n_id, _) = self.lower_expr(&args[1])?;
                        let (p_id, _) = self.lower_expr(&args[2])?;
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![c_id, n_id, p_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent: None }))
                    }
                    "hamming_distance" => {
                        let (a_id, _) = self.lower_expr(&args[0])?;
                        let (b_id, _) = self.lower_expr(&args[1])?;
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![a_id, b_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Int))
                    }
                    // ── Option/Result + prime-field call surface ──
                    // (emath-option-result-graph-field-aj8d). Names here
                    // lower through the same generic ExprNode::Call path as
                    // the graph arms; term_compile/emitter already register
                    // them, so only sema admission + return Infer live here.
                    "option_some" | "result_ok" | "result_err" => {
                        let (payload_id, payload_infer) = self.lower_expr(&args[0])?;
                        if !matches!(
                            payload_infer,
                            Infer::F64
                                | Infer::Nat
                                | Infer::Int
                                | Infer::Bool
                                | Infer::Complex
                                | Infer::HostDeferred
                                | Infer::OptionCarrier
                                | Infer::ResultCarrier
                        ) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` payload must be a concrete scalar value or a nested Option/Result carrier"),
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![payload_id],
                            },
                            expr.source,
                        );
                        let result = if name == "option_some" {
                            Infer::OptionCarrier
                        } else {
                            Infer::ResultCarrier
                        };
                        Some((id, result))
                    }
                    "option_none" => {
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: Vec::new(),
                            },
                            expr.source,
                        );
                        Some((id, Infer::OptionCarrier))
                    }
                    "option_is_some" | "result_is_ok" => {
                        let (carrier_id, carrier_infer) = self.lower_expr(&args[0])?;
                        let admitted = match name.as_str() {
                            "option_is_some" => carrier_infer == Infer::OptionCarrier,
                            _ => carrier_infer == Infer::ResultCarrier,
                        };
                        if !admitted && carrier_infer != Infer::HostDeferred {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a {} carrier", if matches!(name.as_str(), "option_is_some") { "Option" } else { "Result" }),
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![carrier_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Bool))
                    }
                    "option_unwrap_or" | "result_unwrap_or" => {
                        let (carrier_id, carrier_infer) = self.lower_expr(&args[0])?;
                        let carrier_ok = match name.as_str() {
                            "option_unwrap_or" => {
                                carrier_infer == Infer::OptionCarrier
                                    || carrier_infer == Infer::HostDeferred
                            }
                            _ => {
                                carrier_infer == Infer::ResultCarrier
                                    || carrier_infer == Infer::HostDeferred
                            }
                        };
                        if !carrier_ok {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects an Option/Result carrier as its first argument"),
                                args[0].source,
                            );
                            return None;
                        }
                        let (default_id, default_infer) = self.lower_expr(&args[1])?;
                        // Kind-matched default: the default's carrier, if
                        // any, must be the SAME kind as the unwrapped
                        // carrier (Option default for an Option, Result
                        // default for a Result). A foreign carrier default
                        // is a typed type-confusion, matching the term
                        // layer's kind-specific guard.
                        let same_kind_default = match name.as_str() {
                            "option_unwrap_or" => {
                                default_infer == Infer::OptionCarrier
                                    || default_infer == Infer::HostDeferred
                            }
                            _ => {
                                default_infer == Infer::ResultCarrier
                                    || default_infer == Infer::HostDeferred
                            }
                        };
                        if !matches!(
                            default_infer,
                            Infer::F64
                                | Infer::Nat
                                | Infer::Int
                                | Infer::Bool
                                | Infer::Complex
                                | Infer::HostDeferred
                        ) && !same_kind_default
                        {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` default must be a concrete scalar value or a {} carrier (kind-matched)", if matches!(name.as_str(), "option_unwrap_or") { "Option" } else { "Result" }),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![carrier_id, default_id],
                            },
                            expr.source,
                        );
                        // A same-kind carrier default (like the option_none
                        // used to extract a nested payload) yields a
                        // carrier — the default's own Infer, type-honest.
                        Some((id, default_infer))
                    }
                    "result_error_of" => {
                        let (carrier_id, carrier_infer) = self.lower_expr(&args[0])?;
                        if carrier_infer != Infer::ResultCarrier
                            && carrier_infer != Infer::HostDeferred
                        {
                            self.error(
                                "E-TYPE-012",
                                "`result_error_of` expects a Result carrier",
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![carrier_id],
                            },
                            expr.source,
                        );
                        // Error-as-option: Err(x) → Some(x), Ok(_) → none.
                        Some((id, Infer::OptionCarrier))
                    }
                    "field_inv" | "mod_inv" => {
                        let (a_id, a_infer) = self.lower_expr(&args[0])?;
                        let (p_id, p_infer) = self.lower_expr(&args[1])?;
                        if !matches!(
                            a_infer,
                            Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                        ) || !matches!(
                            p_infer,
                            Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                        ) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects (a, modulus) scalar operands"),
                                expr.source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![a_id, p_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Int))
                    }
                    "int_rem" => {
                        let (a_id, a_infer) = self.lower_expr(&args[0])?;
                        let (m_id, m_infer) = self.lower_expr(&args[1])?;
                        if !matches!(
                            a_infer,
                            Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                        ) || !matches!(
                            m_infer,
                            Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                        ) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects (a, modulus) scalar operands"),
                                expr.source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![a_id, m_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Int))
                    }
                    _ => {
                        let mut lowered = Vec::new();
                        let mut saw_complex = false;
                        for arg in args {
                            let (id, infer) = self.lower_expr(arg)?;
                            if !matches!(
                                infer,
                                Infer::F64
                                    | Infer::Nat
                                    | Infer::Int
                                    | Infer::Complex
                                    | Infer::HostDeferred
                            ) {
                                self.error(
                                    "E-TYPE-012",
                                    format!("argument to `{name}` must be numeric"),
                                    arg.source,
                                );
                                return None;
                            }
                            if matches!(infer, Infer::Complex) {
                                if !matches!(
                                    name.as_str(),
                                    "sqrt"
                                        | "ln"
                                        | "log"
                                        | "exp"
                                        | "log10"
                                        | "log2"
                                        | "abs"
                                        | "recip"
                                ) {
                                    self.error(
                                        "E-TYPE-012",
                                        format!("`{name}` is not admitted on Complex"),
                                        arg.source,
                                    );
                                    return None;
                                }
                                saw_complex = true;
                            }
                            lowered.push(id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: lowered,
                            },
                            expr.source,
                        );
                        let result = if name == "is_finite" {
                            Infer::Bool
                        } else if name == "abs" && saw_complex {
                            Infer::F64
                        } else if saw_complex {
                            Infer::Complex
                        } else {
                            Infer::F64
                        };
                        Some((id, result))
                    }
                }
            }
            ExprKind::Unary { op, value } => {
                let (id, infer) = self.lower_expr(value)?;
                match (op, &infer) {
                    (
                        SynUnOp::Neg,
                        Infer::F64
                        | Infer::Nat
                        | Infer::Int
                        | Infer::Complex
                        | Infer::Unit { .. }
                        | Infer::HostDeferred,
                    ) => {
                        self.record("sema", "negate → strict negate", expr.source);
                        let result = if matches!(infer, Infer::Nat) {
                            Infer::Int
                        } else {
                            infer
                        };
                        Some((
                            self.push_expr(
                                ExprNode::Unary {
                                    operation: emath_ir::UnaryOp::Negate,
                                    value: id,
                                },
                                expr.source,
                            ),
                            result,
                        ))
                    }
                    (
                        SynUnOp::Pos,
                        Infer::F64
                        | Infer::Nat
                        | Infer::Int
                        | Infer::Complex
                        | Infer::Unit { .. }
                        | Infer::HostDeferred,
                    ) => Some((id, infer)),
                    (SynUnOp::Not, Infer::Bool) => Some((
                        self.push_expr(
                            ExprNode::Unary {
                                operation: emath_ir::UnaryOp::Not,
                                value: id,
                            },
                            expr.source,
                        ),
                        Infer::Bool,
                    )),
                    _ => {
                        self.error(
                            "E-TYPE-012",
                            "unary operator applied to an incompatible value",
                            expr.source,
                        );
                        None
                    }
                }
            }
            ExprKind::Binary { op, left, right } => {
                // Unit queries compute (bead emath-unit-query-computes-8e8c,
                // 04 §1.4): `unit of E == spelling`, `unit of a == unit of b`
                // and the `!=` forms are compile-time comparisons over the
                // static unit layer — evaluated at admission, never pushed
                // as runtime arithmetic.
                if matches!(op, SynBinOp::Eq | SynBinOp::Ne)
                    && (matches!(left.kind, ExprKind::UnitQuery { .. })
                        || matches!(right.kind, ExprKind::UnitQuery { .. }))
                {
                    return self.lower_unit_query_comparison(*op, left, right);
                }
                let (l, l_infer) = self.lower_expr(left)?;
                let (r, r_infer) = self.lower_expr(right)?;
                let arithmetic = |admitter: &mut Admitter,
                                  operation: emath_ir::BinaryOp,
                                  expr: &Expr,
                                  l: ExprId,
                                  r: ExprId,
                                  result: Infer| {
                    Some((
                        admitter.push_expr(
                            ExprNode::Binary {
                                operation,
                                left: l,
                                right: r,
                            },
                            expr.source,
                        ),
                        result,
                    ))
                };
                match op {
                    SynBinOp::Add => match (&l_infer, &r_infer) {
                        (Infer::Vector { extent: ext_l }, Infer::Vector { extent: ext_r }) => {
                            if let (Some(l_e), Some(r_e)) = (ext_l, ext_r) {
                                if l_e != r_e {
                                    self.error(
                                            "E-SHAPE-005",
                                            format!("dimension mismatch in vector addition: {l_e:?} vs {r_e:?}"),
                                            expr.source,
                                        );
                                    return None;
                                }
                            }
                            let res_extent = ext_l.clone().or_else(|| ext_r.clone());
                            self.record("sema", "vector add", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::VectorAdd,
                                expr,
                                l,
                                r,
                                Infer::Vector { extent: res_extent },
                            )
                        }
                        (
                            Infer::Matrix { rows: r1, cols: c1 },
                            Infer::Matrix { rows: r2, cols: c2 },
                        ) => {
                            if let (Some(r1_e), Some(r2_e)) = (r1, r2) {
                                if r1_e != r2_e {
                                    self.error(
                                        "E-SHAPE-005",
                                        "matrix row dimension mismatch in addition",
                                        expr.source,
                                    );
                                    return None;
                                }
                            }
                            if let (Some(c1_e), Some(c2_e)) = (c1, c2) {
                                if c1_e != c2_e {
                                    self.error(
                                        "E-SHAPE-005",
                                        "matrix col dimension mismatch in addition",
                                        expr.source,
                                    );
                                    return None;
                                }
                            }
                            self.record("sema", "matrix add", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::MatrixAdd,
                                expr,
                                l,
                                r,
                                Infer::Matrix {
                                    rows: r1.clone().or_else(|| r2.clone()),
                                    cols: c1.clone().or_else(|| c2.clone()),
                                },
                            )
                        }
                        (
                            Infer::Tensor { shape: left_shape },
                            Infer::Tensor { shape: right_shape },
                        ) => {
                            let shape =
                                broadcast_tensor_shapes(self, left_shape, right_shape, expr)?;
                            self.record("sema", "tensor add", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::TensorAdd,
                                expr,
                                l,
                                r,
                                Infer::Tensor { shape },
                            )
                        }
                        _ => {
                            self.record("sema", "add → strict f64 add", expr.source);
                            let result = combine_numeric(
                                &l_infer,
                                &r_infer,
                                NumericCombine::Add,
                                expr,
                                self,
                            )?;
                            arithmetic(self, emath_ir::BinaryOp::StrictFloatAdd, expr, l, r, result)
                        }
                    },
                    SynBinOp::Sub => match (&l_infer, &r_infer) {
                        (Infer::Vector { extent: ext_l }, Infer::Vector { extent: ext_r }) => {
                            if let (Some(l_e), Some(r_e)) = (ext_l, ext_r) {
                                if l_e != r_e {
                                    self.error(
                                            "E-SHAPE-005",
                                            format!("dimension mismatch in vector subtraction: {l_e:?} vs {r_e:?}"),
                                            expr.source,
                                        );
                                    return None;
                                }
                            }
                            let res_extent = ext_l.clone().or_else(|| ext_r.clone());
                            self.record("sema", "vector subtract", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::VectorSub,
                                expr,
                                l,
                                r,
                                Infer::Vector { extent: res_extent },
                            )
                        }
                        (
                            Infer::Matrix { rows: r1, cols: c1 },
                            Infer::Matrix { rows: r2, cols: c2 },
                        ) => {
                            if let (Some(r1_e), Some(r2_e)) = (r1, r2) {
                                if r1_e != r2_e {
                                    self.error(
                                        "E-SHAPE-005",
                                        "matrix row dimension mismatch in subtraction",
                                        expr.source,
                                    );
                                    return None;
                                }
                            }
                            if let (Some(c1_e), Some(c2_e)) = (c1, c2) {
                                if c1_e != c2_e {
                                    self.error(
                                        "E-SHAPE-005",
                                        "matrix col dimension mismatch in subtraction",
                                        expr.source,
                                    );
                                    return None;
                                }
                            }
                            self.record("sema", "matrix subtract", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::MatrixSub,
                                expr,
                                l,
                                r,
                                Infer::Matrix {
                                    rows: r1.clone().or_else(|| r2.clone()),
                                    cols: c1.clone().or_else(|| c2.clone()),
                                },
                            )
                        }
                        (
                            Infer::Tensor { shape: left_shape },
                            Infer::Tensor { shape: right_shape },
                        ) => {
                            let shape =
                                broadcast_tensor_shapes(self, left_shape, right_shape, expr)?;
                            self.record("sema", "tensor subtract", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::TensorSub,
                                expr,
                                l,
                                r,
                                Infer::Tensor { shape },
                            )
                        }
                        _ => {
                            self.record("sema", "subtract → strict f64 subtract", expr.source);
                            let result = combine_numeric(
                                &l_infer,
                                &r_infer,
                                NumericCombine::Sub,
                                expr,
                                self,
                            )?;
                            arithmetic(self, emath_ir::BinaryOp::StrictFloatSub, expr, l, r, result)
                        }
                    },
                    SynBinOp::Mul => match (&l_infer, &r_infer) {
                        (Infer::Vector { extent }, Infer::F64 | Infer::HostDeferred) => {
                            self.record("sema", "vector scale", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::VectorScale,
                                expr,
                                l,
                                r,
                                Infer::Vector {
                                    extent: extent.clone(),
                                },
                            )
                        }
                        (Infer::F64 | Infer::HostDeferred, Infer::Vector { extent }) => {
                            self.record("sema", "vector scale", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::VectorScale,
                                expr,
                                r,
                                l,
                                Infer::Vector {
                                    extent: extent.clone(),
                                },
                            )
                        }
                        (Infer::Matrix { rows, cols }, Infer::F64 | Infer::HostDeferred) => {
                            self.record("sema", "matrix scale", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::MatrixScale,
                                expr,
                                l,
                                r,
                                Infer::Matrix {
                                    rows: rows.clone(),
                                    cols: cols.clone(),
                                },
                            )
                        }
                        (Infer::F64 | Infer::HostDeferred, Infer::Matrix { rows, cols }) => {
                            self.record("sema", "matrix scale", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::MatrixScale,
                                expr,
                                r,
                                l,
                                Infer::Matrix {
                                    rows: rows.clone(),
                                    cols: cols.clone(),
                                },
                            )
                        }
                        (Infer::Tensor { shape }, Infer::F64 | Infer::HostDeferred) => {
                            self.record("sema", "tensor scale", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::TensorScale,
                                expr,
                                l,
                                r,
                                Infer::Tensor {
                                    shape: shape.clone(),
                                },
                            )
                        }
                        (Infer::F64 | Infer::HostDeferred, Infer::Tensor { shape }) => {
                            self.record("sema", "tensor scale", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::TensorScale,
                                expr,
                                r,
                                l,
                                Infer::Tensor {
                                    shape: shape.clone(),
                                },
                            )
                        }
                        (Infer::Matrix { rows, cols }, Infer::Vector { extent }) => {
                            if let (Some(c_e), Some(v_e)) = (cols, extent) {
                                if c_e != v_e {
                                    self.error(
                                            "E-SHAPE-002",
                                            format!("dimension mismatch in matrix-vector multiplication: matrix columns {c_e:?} != vector length {v_e:?}"),
                                            expr.source,
                                        );
                                    return None;
                                }
                            }
                            self.record("sema", "matrix mul vector", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::MatrixMulVector,
                                expr,
                                l,
                                r,
                                Infer::Vector {
                                    extent: rows.clone(),
                                },
                            )
                        }
                        (
                            Infer::Matrix { rows: r1, cols: c1 },
                            Infer::Matrix { rows: r2, cols: c2 },
                        ) => {
                            if let (Some(c1_e), Some(r2_e)) = (c1, r2) {
                                if c1_e != r2_e {
                                    self.error(
                                            "E-SHAPE-002",
                                            format!("dimension mismatch in matrix multiplication: left columns {c1_e:?} != right rows {r2_e:?}"),
                                            expr.source,
                                        );
                                    return None;
                                }
                            }
                            self.record("sema", "matrix mul matrix", expr.source);
                            arithmetic(
                                self,
                                emath_ir::BinaryOp::MatrixMulMatrix,
                                expr,
                                l,
                                r,
                                Infer::Matrix {
                                    rows: r1.clone(),
                                    cols: c2.clone(),
                                },
                            )
                        }
                        _ => {
                            self.record("sema", "multiply → strict f64 multiply", expr.source);
                            let result = combine_numeric(
                                &l_infer,
                                &r_infer,
                                NumericCombine::Mul,
                                expr,
                                self,
                            )?;
                            arithmetic(self, emath_ir::BinaryOp::StrictFloatMul, expr, l, r, result)
                        }
                    },
                    SynBinOp::Div => {
                        self.record("sema", "divide → strict f64 divide", expr.source);
                        let result =
                            combine_numeric(&l_infer, &r_infer, NumericCombine::Div, expr, self)?;
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatDiv, expr, l, r, result)
                    }
                    SynBinOp::Pow => {
                        self.record("sema", "power → strict f64 powf", expr.source);
                        if !matches!(
                            (&l_infer, &r_infer),
                            (
                                Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred,
                                Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                            )
                        ) {
                            self.error(
                                "E-TYPE-012",
                                "operator `^` requires dimensionless numeric operands",
                                expr.source,
                            );
                            return None;
                        }
                        arithmetic(
                            self,
                            emath_ir::BinaryOp::StrictFloatPow,
                            expr,
                            l,
                            r,
                            Infer::F64,
                        )
                    }
                    SynBinOp::Eq
                    | SynBinOp::Ne
                    | SynBinOp::Lt
                    | SynBinOp::Le
                    | SynBinOp::Gt
                    | SynBinOp::Ge => {
                        let operation = match op {
                            SynBinOp::Eq => emath_ir::BinaryOp::Equal,
                            SynBinOp::Ne => emath_ir::BinaryOp::NotEqual,
                            SynBinOp::Lt => emath_ir::BinaryOp::Less,
                            SynBinOp::Le => emath_ir::BinaryOp::LessEqual,
                            SynBinOp::Gt => emath_ir::BinaryOp::Greater,
                            _ => emath_ir::BinaryOp::GreaterEqual,
                        };
                        if matches!(
                            op,
                            SynBinOp::Lt | SynBinOp::Le | SynBinOp::Gt | SynBinOp::Ge
                        ) && !comparable_numeric(&l_infer, &r_infer)
                        {
                            self.error(
                                "E-UNIT-101",
                                "ordered comparisons require dimensionally compatible numeric operands",
                                expr.source,
                            );
                            return None;
                        }
                        Some((
                            self.push_expr(
                                ExprNode::Binary {
                                    operation,
                                    left: l,
                                    right: r,
                                },
                                expr.source,
                            ),
                            Infer::Bool,
                        ))
                    }
                    SynBinOp::And | SynBinOp::Or | SynBinOp::Imply | SynBinOp::Iff => {
                        if !matches!(l_infer, Infer::Bool) || !matches!(r_infer, Infer::Bool) {
                            self.error(
                                "E-TYPE-012",
                                "boolean operators require Boolean operands",
                                expr.source,
                            );
                            return None;
                        }
                        Some((
                            self.push_expr(
                                ExprNode::Binary {
                                    operation: match op {
                                        SynBinOp::And => emath_ir::BinaryOp::And,
                                        SynBinOp::Or => emath_ir::BinaryOp::Or,
                                        SynBinOp::Imply => emath_ir::BinaryOp::Imply,
                                        SynBinOp::Iff => emath_ir::BinaryOp::Iff,
                                        _ => unreachable!(),
                                    },
                                    left: l,
                                    right: r,
                                },
                                expr.source,
                            ),
                            Infer::Bool,
                        ))
                    }
                    SynBinOp::In => {
                        let Infer::Set(element) = &r_infer else {
                            self.error(
                                "E-TYPE-012",
                                "membership (`in`) requires a finite set on the right",
                                expr.source,
                            );
                            return None;
                        };
                        if **element != l_infer {
                            self.error(
                                "E-TYPE-012",
                                format!(
                                    "membership element has type {l_infer}, but the set contains {element}"
                                ),
                                expr.source,
                            );
                            return None;
                        }
                        Some((
                            self.push_expr(
                                ExprNode::Binary {
                                    operation: emath_ir::BinaryOp::SetContains,
                                    left: l,
                                    right: r,
                                },
                                expr.source,
                            ),
                            Infer::Bool,
                        ))
                    }
                    SynBinOp::Asymp => {
                        if self.in_claim_context {
                            // Admit as a stated claim: Bool(true).
                            self.record(
                                "sema",
                                "asymptotic equivalence (`~~`) claim admitted (not computationally verified)",
                                expr.source,
                            );
                            let id =
                                self.push_expr(ExprNode::Literal(Literal::Bool(true)), expr.source);
                            return Some((id, Infer::Bool));
                        }
                        self.error(
                            E_UNSUPPORTED_TYPE,
                            "asymptotic equivalence (`~~`) is a claim, not a computation; \
                             use it in `require` or `invariant`",
                            expr.source,
                        );
                        return None;
                    }
                }
            }
            ExprKind::Approx {
                left,
                right,
                tolerance,
            } => {
                let (left_id, left_infer) = self.lower_expr(left)?;
                let (right_id, right_infer) = self.lower_expr(right)?;
                if !self.in_claim_context {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "approximation (`≈`) is a claim, not a computation; \
                         use it in `require` or `invariant`",
                        expr.source,
                    );
                    return None;
                }
                let Some(tolerance) = tolerance else {
                    self.error(
                        E_APPROX_TOL,
                        "bare `≈` has no declared tolerance; an approximation without a \
                         tolerance is never admitted as exact — declare one \
                         (`within rtol=…, atol=…`)",
                        expr.source,
                    );
                    return None;
                };
                let mut declared: Vec<String> = Vec::new();
                if tolerance.rtol.is_some() {
                    declared.push("rtol".to_string());
                }
                if tolerance.atol.is_some() {
                    declared.push("atol".to_string());
                }
                self.record(
                    "sema",
                    format!(
                        "≈ approximation claim admitted with declared tolerance ({}) — \
                         authority degraded through the ≈ edge, never recovered upward, \
                         not computationally exact",
                        declared.join(", ")
                    ),
                    expr.source,
                );
                combine_numeric(&left_infer, &right_infer, NumericCombine::Sub, expr, self)?;
                let zero = |admitter: &mut Self| {
                    admitter.push_expr(
                        ExprNode::Literal(Literal::FloatBits(0.0_f64.to_bits())),
                        expr.source,
                    )
                };
                let lower_tolerance =
                    |admitter: &mut Self, value: Option<&Expr>, label: &str| -> Option<ExprId> {
                        let Some(value) = value else {
                            return Some(zero(admitter));
                        };
                        let (id, infer) = admitter.lower_expr(value)?;
                        if !matches!(
                            infer,
                            Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                        ) {
                            admitter.error(
                                "E-TYPE-012",
                                format!("approximation {label} must be dimensionless numeric"),
                                value.source,
                            );
                            return None;
                        }
                        Some(id)
                    };
                let rtol = lower_tolerance(self, tolerance.rtol.as_ref(), "rtol")?;
                let atol = lower_tolerance(self, tolerance.atol.as_ref(), "atol")?;
                let difference = self.push_expr(
                    ExprNode::Binary {
                        operation: BinaryOp::StrictFloatSub,
                        left: left_id,
                        right: right_id,
                    },
                    expr.source,
                );
                let absolute_difference = self.push_expr(
                    ExprNode::Call {
                        function: QualifiedName::single("abs"),
                        arguments: vec![difference],
                    },
                    expr.source,
                );
                let absolute_reference = self.push_expr(
                    ExprNode::Call {
                        function: QualifiedName::single("abs"),
                        arguments: vec![right_id],
                    },
                    expr.source,
                );
                let relative = self.push_expr(
                    ExprNode::Binary {
                        operation: BinaryOp::StrictFloatMul,
                        left: rtol,
                        right: absolute_reference,
                    },
                    expr.source,
                );
                let threshold = self.push_expr(
                    ExprNode::Binary {
                        operation: BinaryOp::StrictFloatAdd,
                        left: atol,
                        right: relative,
                    },
                    expr.source,
                );
                let id = self.push_expr(
                    ExprNode::Binary {
                        operation: BinaryOp::LessEqual,
                        left: absolute_difference,
                        right: threshold,
                    },
                    expr.source,
                );
                Some((id, Infer::Bool))
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let (cond, cond_infer) = self.lower_expr(condition)?;
                if !matches!(cond_infer, Infer::Bool) {
                    self.error(
                        "E-TYPE-012",
                        "`if` condition must be Boolean",
                        condition.source,
                    );
                    return None;
                }
                let (then_id, then_infer) = self.lower_expr(then_value)?;
                let (else_id, else_infer) = self.lower_expr(else_value)?;
                if then_infer != else_infer {
                    self.error(
                        "E-TYPE-012",
                        "`if` branches must have the same type",
                        expr.source,
                    );
                    return None;
                }
                Some((
                    self.push_expr(
                        ExprNode::If {
                            condition: cond,
                            then_value: then_id,
                            else_value: else_id,
                        },
                        expr.source,
                    ),
                    then_infer,
                ))
            }
            ExprKind::Cases {
                subject: _,
                arms,
                else_arm,
            } => {
                // U1: Lower `cases: | c1 => e1 | c2 => e2 | else => e3`
                // to nested `If { c1, e1, If { c2, e2, e3 } }`.
                // The subject is for readability only (arm conditions
                // are full expressions, not pattern matches).
                let (mut current_else, result_infer) = self.lower_expr(else_arm)?;
                for (cond, value) in arms.iter().rev() {
                    let (cond_id, cond_infer) = self.lower_expr(cond)?;
                    if !matches!(cond_infer, Infer::Bool) {
                        self.error(
                            "E-TYPE-012",
                            "cases arm condition must be Boolean",
                            cond.source,
                        );
                        return None;
                    }
                    let (val_id, val_infer) = self.lower_expr(value)?;
                    if val_infer != result_infer {
                        self.error(
                            "E-TYPE-012",
                            "cases arms must have the same type",
                            expr.source,
                        );
                        return None;
                    }
                    current_else = self.push_expr(
                        ExprNode::If {
                            condition: cond_id,
                            then_value: val_id,
                            else_value: current_else,
                        },
                        expr.source,
                    );
                }
                Some((current_else, result_infer))
            }
            ExprKind::List(items) => self.lower_list_literal(expr, items),
            ExprKind::Table { headers, rows } => self.lower_table_literal(expr, headers, rows),
            ExprKind::Tuple(items) if graph_tuple_parts(items).is_some() => {
                self.lower_graph_tuple(expr, items)
            }
            ExprKind::Set(items) => {
                let mut elements = Vec::with_capacity(items.len());
                let mut element_infer = None;
                for item in items {
                    let (id, infer) = self.lower_expr(item)?;
                    if let Some(expected) = &element_infer {
                        if expected != &infer {
                            self.error(
                                "E-TYPE-012",
                                "set literal elements must have one type",
                                item.source,
                            );
                            return None;
                        }
                    } else {
                        element_infer = Some(infer);
                    }
                    elements.push(id);
                }
                let element_infer = element_infer.unwrap_or(Infer::F64);
                let id = self.push_expr(
                    ExprNode::Set {
                        guards: vec![None; elements.len()],
                        elements,
                    },
                    expr.source,
                );
                Some((id, Infer::Set(Box::new(element_infer))))
            }
            ExprKind::SetComprehension {
                element,
                var,
                domain,
                guard,
            } => {
                let Some((start, end)) = integer_range(domain) else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "set comprehensions require a finite literal integer range",
                        domain.source,
                    );
                    return None;
                };
                if end < start || end - start > 10_000 {
                    self.error(
                        "E-DOM-002",
                        "set comprehension range must be ordered and at most 10000 elements",
                        domain.source,
                    );
                    return None;
                }
                let previous = self.index_locals.insert(var.clone(), start);
                let mut elements = Vec::with_capacity((end - start) as usize);
                let mut guards = Vec::with_capacity((end - start) as usize);
                let mut element_infer = None;
                for value in start..end {
                    self.index_locals.insert(var.clone(), value);
                    let Some((element_id, infer)) = self.lower_expr(element) else {
                        restore_index_local(&mut self.index_locals, var, previous);
                        return None;
                    };
                    if let Some(expected) = &element_infer {
                        if expected != &infer {
                            self.error(
                                "E-TYPE-012",
                                "set comprehension elements must have one type",
                                element.source,
                            );
                            restore_index_local(&mut self.index_locals, var, previous);
                            return None;
                        }
                    } else {
                        element_infer = Some(infer);
                    }
                    let guard_id = if let Some(guard) = guard {
                        let Some((id, infer)) = self.lower_expr(guard) else {
                            restore_index_local(&mut self.index_locals, var, previous);
                            return None;
                        };
                        if infer != Infer::Bool {
                            self.error(
                                "E-TYPE-012",
                                "set comprehension guard must be Boolean",
                                guard.source,
                            );
                            restore_index_local(&mut self.index_locals, var, previous);
                            return None;
                        }
                        Some(id)
                    } else {
                        None
                    };
                    elements.push(element_id);
                    guards.push(guard_id);
                }
                restore_index_local(&mut self.index_locals, var, previous);
                let id = self.push_expr(ExprNode::Set { elements, guards }, expr.source);
                Some((
                    id,
                    Infer::Set(Box::new(element_infer.unwrap_or(Infer::F64))),
                ))
            }
            ExprKind::Record { type_path, fields } => {
                let mut lowered = std::collections::BTreeMap::new();
                for (name, value) in fields {
                    let (id, _) = self.lower_expr(value)?;
                    lowered.insert(name.clone(), id);
                }
                let name = QualifiedName(type_path.join("::"));
                let ty = self.type_id(TypeNode::Record(name.clone()));
                let id = self.push_expr(
                    ExprNode::Record {
                        ty,
                        fields: lowered,
                    },
                    expr.source,
                );
                Some((id, Infer::Record(name.0)))
            }
            ExprKind::Index { value, indices } => self.lower_index(expr, value, indices),
            ExprKind::Binder {
                kind,
                binders,
                body,
                guard,
            } => {
                // Series in claim context: admit as Bool(true).
                if *kind == BinderKind::Series && self.in_claim_context {
                    self.record(
                        "sema",
                        "series convergence claim admitted (not computationally verified)",
                        expr.source,
                    );
                    let id = self.push_expr(ExprNode::Literal(Literal::Bool(true)), expr.source);
                    return Some((id, Infer::Bool));
                }
                self.lower_finite_binder(expr, *kind, binders, body, guard.as_deref())
            }
            ExprKind::Derivative { kind, holding, .. } => {
                // Partial without `holding` is a MeaningHole: autodiff wrt
                // one input would silently hold every other input fixed.
                if *kind == DerivativeKind::Partial {
                    if holding.is_empty() {
                        self.error(
                            E_UNSUPPORTED_TYPE,
                            "partial derivative requires an explicit `holding` set \
                             (e.g. `partial(H) wrt T holding p`); the compiler will not \
                             guess which variables are held fixed",
                            expr.source,
                        );
                        return None;
                    }
                    for held in holding {
                        let Some(segments) = path_segments(held) else {
                            self.error(
                                E_UNSUPPORTED_TYPE,
                                "holding variable must be a plain name",
                                held.source,
                            );
                            return None;
                        };
                        let held_name = &segments[0];
                        if self.lookup(held_name).is_none() {
                            self.error(
                                E_UNKNOWN_VARIABLE,
                                format!("unknown holding variable `{held_name}`"),
                                held.source,
                            );
                            return None;
                        }
                    }
                }
                // The parser may produce nested Derivative nodes:
                // `derivative x wrt y` becomes Derivative(Derivative(x)) wrt y.
                // Unwrap to get the inner value and the wrt clause.
                let Some((value, wrt)) = unwrap_derivative(expr) else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative could not be unwrapped",
                        expr.source,
                    );
                    return None;
                };
                let Some(vars) = wrt else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative requires `wrt` clause: derivative(expr) wrt var",
                        expr.source,
                    );
                    return None;
                };
                if vars.len() != 1 {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative wrt supports a single variable in Phase 1",
                        expr.source,
                    );
                    return None;
                }
                let Some(segments) = path_segments(&vars[0]) else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative variable must be a plain name",
                        expr.source,
                    );
                    return None;
                };
                let var_name = segments[0].clone();
                if !self.inputs.contains_key(&var_name) {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        format!("derivative variable `{var_name}` must be an input"),
                        expr.source,
                    );
                    return None;
                }
                // Lower the value expression, then inline definition
                // references so the EMIR dual-number evaluator sees the
                // full computation chain.
                let (body_id, body_infer) = match self.lower_expr(value) {
                    Some(result) => result,
                    None => return None,
                };
                if !is_numeric_element(&body_infer) {
                    self.error(
                        "E-TYPE-012",
                        "derivative body must be numeric",
                        value.source,
                    );
                    return None;
                }
                let inlined = self.inline_defs(body_id);
                let id = self.push_expr(
                    ExprNode::Differentiate {
                        body: inlined,
                        var: var_name.clone(),
                    },
                    expr.source,
                );
                self.record(
                    "sema",
                    format!("derivative wrt {var_name} → forward-mode autodiff"),
                    expr.source,
                );
                Some((id, Infer::F64))
            }
            ExprKind::Solve { value, wrt } => {
                let Some(vars) = wrt.as_deref() else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "solve requires `wrt` clause: solve(expr) wrt var",
                        expr.source,
                    );
                    return None;
                };
                if vars.len() != 1 {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "solve wrt supports a single variable in Phase 1",
                        expr.source,
                    );
                    return None;
                }
                let Some(segments) = path_segments(&vars[0]) else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "solve variable must be a plain name",
                        expr.source,
                    );
                    return None;
                };
                let var_name = segments[0].clone();
                if !self.inputs.contains_key(&var_name) {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        format!("solve variable `{var_name}` must be an input"),
                        expr.source,
                    );
                    return None;
                }
                let (body_id, body_infer) = match self.lower_expr(value) {
                    Some(result) => result,
                    None => return None,
                };
                if !is_numeric_element(&body_infer) {
                    self.error("E-TYPE-012", "solve body must be numeric", value.source);
                    return None;
                }
                let inlined = self.inline_defs(body_id);
                let id = self.push_expr(
                    ExprNode::Solve {
                        body: inlined,
                        var: var_name.clone(),
                    },
                    expr.source,
                );
                self.record(
                    "sema",
                    format!("solve wrt {var_name} → Newton's method root-finding"),
                    expr.source,
                );
                Some((id, Infer::F64))
            }
            ExprKind::Optimize {
                value,
                wrt,
                maximize,
            } => {
                let Some(vars) = wrt.as_deref() else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "minimize/maximize requires `wrt` clause: minimize(expr) wrt var",
                        expr.source,
                    );
                    return None;
                };
                if vars.is_empty() {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "minimize/maximize requires at least one `wrt` variable",
                        expr.source,
                    );
                    return None;
                }
                let mut var_names = Vec::with_capacity(vars.len());
                for var in vars {
                    let Some(segments) = path_segments(var) else {
                        self.error(
                            E_UNSUPPORTED_TYPE,
                            "optimization variable must be a plain name",
                            var.source,
                        );
                        return None;
                    };
                    let name = segments[0].clone();
                    if !self.inputs.contains_key(&name) {
                        self.error(
                            E_UNSUPPORTED_TYPE,
                            format!("optimization variable `{name}` must be an input"),
                            var.source,
                        );
                        return None;
                    }
                    var_names.push(name);
                }
                let (body_id, body_infer) = match self.lower_expr(value) {
                    Some(result) => result,
                    None => return None,
                };
                if !is_numeric_element(&body_infer) {
                    self.error(
                        "E-TYPE-012",
                        "optimization body must be numeric",
                        value.source,
                    );
                    return None;
                }
                let inlined = self.inline_defs(body_id);
                let body_with_penalty = self.add_constraint_penalties(inlined, expr.source);
                let id = self.push_expr(
                    ExprNode::Optimize {
                        body: body_with_penalty,
                        vars: var_names.clone(),
                        maximize: *maximize,
                    },
                    expr.source,
                );
                let direction = if *maximize { "maximize" } else { "minimize" };
                self.record(
                    "sema",
                    format!(
                        "{direction} wrt {} → Newton stationarity (∇f = 0)",
                        var_names.join(", ")
                    ),
                    expr.source,
                );
                Some((id, Infer::F64))
            }
            ExprKind::Limit {
                var,
                target,
                direction,
                body,
            } => {
                if self.in_claim_context {
                    // Admit as a stated claim: Bool(true), not verified.
                    self.record(
                        "sema",
                        format!("limit {var} -> claim admitted (not computationally verified)"),
                        expr.source,
                    );
                    let _ = (target, direction, body);
                    let id = self.push_expr(ExprNode::Literal(Literal::Bool(true)), expr.source);
                    return Some((id, Infer::Bool));
                }
                let dir = match direction {
                    emath_core::tree::LimitDirection::TwoSided => "",
                    emath_core::tree::LimitDirection::FromAbove => "+",
                    emath_core::tree::LimitDirection::FromBelow => "-",
                };
                self.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "`limit {var} -> {dir}` is a claim, not a computation; \
                         use `sample_limit` for numerical evaluation or place in `require`/`invariant`"
                    ),
                    expr.source,
                );
                let _ = (target, body);
                None
            }
            ExprKind::SampleLimit {
                var,
                target,
                direction,
                body,
            } => {
                // Lower as a SampleLimit node: the body is compiled as a
                // sub-program with the limit variable as an input.
                let dir_bits = match direction {
                    emath_core::tree::LimitDirection::TwoSided => 0.0_f64,
                    emath_core::tree::LimitDirection::FromAbove => 1.0_f64,
                    emath_core::tree::LimitDirection::FromBelow => -1.0_f64,
                };
                let (target_id, _) = self.lower_expr(target)?;
                let dir_id = self.push_expr(
                    ExprNode::Literal(Literal::FloatBits(dir_bits.to_bits())),
                    expr.source,
                );
                // Register the limit variable as a temporary input so the
                // body can reference it.
                let prev = self.inputs.insert(var.clone(), Infer::F64);
                let (body_id, body_infer) = self.lower_expr(body)?;
                if let Some(p) = prev {
                    self.inputs.insert(var.clone(), p);
                } else {
                    self.inputs.remove(var);
                }
                if !is_numeric_element(&body_infer) {
                    self.error(
                        "E-TYPE-012",
                        "sample_limit body must be numeric",
                        body.source,
                    );
                    return None;
                }
                let id = self.push_expr(
                    ExprNode::SampleLimit {
                        body: body_id,
                        var: var.clone(),
                        target: target_id,
                        direction: dir_id,
                    },
                    expr.source,
                );
                self.record(
                    "sema",
                    format!("sample_limit {var} → numerical limit approximation"),
                    expr.source,
                );
                Some((id, Infer::F64))
            }
            ExprKind::UnitQuery { kind, .. } => {
                let query = match kind {
                    emath_core::tree::UnitQueryKind::Unit => "unit of",
                    emath_core::tree::UnitQueryKind::Dimension => "dimension of",
                };
                self.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "`{query}` is a compile-time query: it parses, but Phase 1 does not evaluate it"
                    ),
                    expr.source,
                );
                None
            }
            other => {
                self.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "expression form `{}` is outside the Phase 1 strict-f64 subset",
                        expr_form_name(other)
                    ),
                    expr.source,
                );
                None
            }
        }
    }

    fn lower_graph_tuple(&mut self, expr: &Expr, items: &[Expr]) -> Option<(ExprId, Infer)> {
        let (nodes, edges) = graph_tuple_parts(items)?;
        let nodes = nodes
            .iter()
            .map(signed_numeric_literal)
            .collect::<Option<Vec<_>>>()
            .or_else(|| {
                self.error(
                    "E-TYPE-012",
                    "graph nodes must be finite numeric literals",
                    expr.source,
                );
                None
            })?;
        if nodes.is_empty() {
            self.error("E-TYPE-012", "a graph needs at least one node", expr.source);
            return None;
        }
        let mut adjacency = vec![vec![0.0_f64; nodes.len()]; nodes.len()];
        for edge in edges {
            let ExprKind::List(parts) = &edge.kind else {
                return None;
            };
            let values = parts
                .iter()
                .map(signed_numeric_literal)
                .collect::<Option<Vec<_>>>()
                .or_else(|| {
                    self.error(
                        "E-TYPE-012",
                        "graph edge endpoints, weights, and direction must be finite literals",
                        edge.source,
                    );
                    None
                })?;
            let Some(from) = nodes.iter().position(|node| *node == values[0]) else {
                self.error(
                    "E-TYPE-012",
                    "graph edge names an unknown source node",
                    edge.source,
                );
                return None;
            };
            let Some(to) = nodes.iter().position(|node| *node == values[1]) else {
                self.error(
                    "E-TYPE-012",
                    "graph edge names an unknown target node",
                    edge.source,
                );
                return None;
            };
            adjacency[from][to] = values[2];
            if values[3] == 0.0 {
                adjacency[to][from] = values[2];
            }
        }
        let rows = adjacency
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|value| {
                        self.push_expr(
                            ExprNode::Literal(Literal::FloatBits(value.to_bits())),
                            expr.source,
                        )
                    })
                    .collect()
            })
            .collect();
        let id = self.push_expr(ExprNode::Matrix(rows), expr.source);
        Some((
            id,
            Infer::Matrix {
                rows: Some(Extent::Fixed(nodes.len())),
                cols: Some(Extent::Fixed(nodes.len())),
            },
        ))
    }

    /// Compile-time unit comparison (bead emath-unit-query-computes-8e8c):
    /// `unit of E == spelling`, `dimension of E == spelling`, query-to-query
    /// forms, and their `!=` negations. Both sides resolve to a static
    /// (dimension vector, family); the equality is computed at admission.
    /// A held comparison admits as a constant Bool with a receipt naming
    /// the computed units; a failed comparison is the typed refusal
    /// `E-UNIT-101` (or `E-UNIT-104` for an unresolvable spelling), never
    /// a silently-true claim. A bare `unit of E` outside a comparison is
    /// unchanged: still a named refuse (`E-TYPE-010`) — a unit is not a
    /// Phase-1 value.
    fn lower_unit_query_comparison(
        &mut self,
        op: SynBinOp,
        left: &Expr,
        right: &Expr,
    ) -> Option<(ExprId, Infer)> {
        let (left_dims, left_family, left_label) = self.static_unit_of(left)?;
        let (right_dims, right_family, right_label) = self.static_unit_of(right)?;
        let equal = left_dims == right_dims && left_family == right_family;
        if equal != matches!(op, SynBinOp::Eq) {
            self.error(
                "E-UNIT-101",
                format!(
                    "unit query computed false: `{}` has {} but `{}` has {}; \
                     the comparison does not hold",
                    expr_text(left),
                    left_label,
                    expr_text(right),
                    right_label,
                ),
                left.source.cover(right.source),
            );
            return None;
        }
        self.record(
            "sema",
            format!(
                "unit query computed: `unit of` comparison {} {} {} ({} vs {}); \
                 units are compile-time data, admitted",
                expr_text(left),
                if matches!(op, SynBinOp::Eq) {
                    "=="
                } else {
                    "!="
                },
                expr_text(right),
                left_label,
                right_label,
            ),
            left.source.cover(right.source),
        );
        let id = self.push_expr(
            ExprNode::Literal(Literal::Bool(true)),
            left.source.cover(right.source),
        );
        Some((id, Infer::Bool))
    }

    /// Static unit of an expression in a unit comparison: a `unit of` /
    /// `dimension of` query (the inner expression's inferred unit), a unit
    /// spelling (`m`, `m^2`, `kg*m^2/s^2`), a quantity literal, or an
    /// arithmetic composition of spellings.
    fn static_unit_of(&mut self, expr: &Expr) -> Option<(UnitDim, UnitFamily, String)> {
        match &expr.kind {
            ExprKind::UnitQuery { expr, .. } => {
                let (_, infer) = self.lower_expr(expr)?;
                match infer {
                    Infer::Unit { dims, family, .. } => Some((dims, family, expr_text(expr))),
                    // Dimensionless numeric: a bare Float64/Int input or a
                    // fully cancelled expression.
                    Infer::F64 | Infer::Nat | Infer::Int => {
                        Some((UnitDim::one(), UnitFamily::Si, expr_text(expr)))
                    }
                    other => {
                        self.error(
                            "E-TYPE-010",
                            format!(
                                "`unit of` requires a unit-carrying operand, found {:?}",
                                other
                            ),
                            expr.source,
                        );
                        None
                    }
                }
            }
            ExprKind::Path { segments, .. } if segments.len() == 1 => {
                let name = &segments[0];
                match lookup_unit(name) {
                    Ok(unit) => Some((unit.dims, unit.family, name.clone())),
                    Err(_) => {
                        self.error(
                            "E-UNIT-104",
                            format!("unknown unit `{name}` in unit comparison"),
                            expr.source,
                        );
                        None
                    }
                }
            }
            ExprKind::Binary {
                op: SynBinOp::Mul,
                left,
                right,
            } => {
                let (ld, lf, ll) = self.static_unit_of(left)?;
                let (rd, rf, rl) = self.static_unit_of(right)?;
                Some((
                    ld.mul(rd),
                    combine_unit_families(self, lf, rf, expr)?,
                    format!("{ll}*{rl}"),
                ))
            }
            ExprKind::Binary {
                op: SynBinOp::Div,
                left,
                right,
            } => {
                let (ld, lf, ll) = self.static_unit_of(left)?;
                let (rd, rf, rl) = self.static_unit_of(right)?;
                Some((
                    ld.div(rd),
                    combine_unit_families(self, lf, rf, expr)?,
                    format!("{ll}/{rl}"),
                ))
            }
            ExprKind::Binary {
                op: SynBinOp::Pow,
                left,
                right,
            } => {
                let (ld, lf, ll) = self.static_unit_of(left)?;
                let exponent = match &right.kind {
                    ExprKind::Int(text) | ExprKind::Float(text) => text.parse::<i32>().ok(),
                    _ => None,
                };
                let Some(exponent) = exponent else {
                    self.error(
                        "E-TYPE-010",
                        "unit power must be an integer literal",
                        right.source,
                    );
                    return None;
                };
                Some((ld.pow(exponent), lf, format!("{ll}^{exponent}")))
            }
            ExprKind::Quantity { unit, .. } => {
                // A quantity literal on one side: compare against its unit.
                let mut dims = UnitDim::one();
                let mut family = UnitFamily::Si;
                let mut label = String::new();
                for (name, power) in unit.flatten() {
                    let Ok(looked_up) = lookup_unit(&name) else {
                        self.error(
                            "E-UNIT-104",
                            format!("unknown unit `{name}` in unit comparison"),
                            expr.source,
                        );
                        return None;
                    };
                    let factor_dims = looked_up.dims.pow(power);
                    dims = if power >= 0 {
                        dims.mul(factor_dims)
                    } else {
                        dims.div(factor_dims)
                    };
                    family = looked_up.family;
                    if label.is_empty() {
                        label = name;
                    }
                }
                Some((dims, family, label))
            }
            other => {
                self.error(
                    "E-TYPE-010",
                    format!(
                        "unit comparison requires a unit spelling or unit query on each side, found {}",
                        expr_form_name(other)
                    ),
                    expr.source,
                );
                None
            }
        }
    }

    pub(super) fn lower_requirement(&mut self, expr: &Expr) -> Option<ExprId> {
        // Claim expressions (limit, series, asymp) are admitted as stated
        // claims in require/invariant. They produce Bool(true) — the claim
        // is recorded but not computationally verified in Phase 1.
        let prev_claim = self.in_claim_context;
        self.in_claim_context = true;
        let result = self.lower_expr(expr);
        self.in_claim_context = prev_claim;
        let (id, infer) = result?;
        if !matches!(infer, Infer::Bool) {
            self.error(
                "E-CTOR-032",
                "`require` must be a Boolean expression",
                expr.source,
            );
            return None;
        }
        Some(id)
    }

    /// Lower an `einsum("subscripts", A, B, ...)` call.
    /// The subscripts string is carried as the first Call argument
    /// (a Literal::Text). The emitter extracts it and emits EmirOp::Einsum.
    fn lower_einsum(&mut self, expr: &Expr, name: &str, args: &[Expr]) -> Option<(ExprId, Infer)> {
        use emath_ir::ExprNode;

        // Subscript strings are contraction labels, not Phase-1 values.
        // Push Literal::Text directly so general string lowering (E-TYPE-010)
        // cannot refuse the documented `einsum("ik,kj->ij", A, B)` form.
        let subscripts = if let ExprKind::Str(s) = &args[0].kind {
            s.clone()
        } else {
            self.error(
                "E-TYPE-012",
                "`einsum` first argument must be a string literal",
                args[0].source,
            );
            return None;
        };
        let mut arg_ids = Vec::with_capacity(args.len());
        arg_ids.push(self.push_expr(
            ExprNode::Literal(Literal::Text(subscripts.clone())),
            args[0].source,
        ));
        for arg in &args[1..] {
            let (id, _) = self.lower_expr(arg)?;
            arg_ids.push(id);
        }

        let strip = |t: &str| t.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        let output_spec = if let Some((_, rhs)) = subscripts.split_once("->") {
            strip(rhs)
        } else {
            // Implicit mode: unique free indices, alphabetical (numpy).
            let inputs: Vec<String> = subscripts.split(',').map(strip).collect();
            let mut counts: std::collections::HashMap<char, usize> =
                std::collections::HashMap::new();
            for spec in &inputs {
                for c in spec.chars() {
                    *counts.entry(c).or_insert(0) += 1;
                }
            }
            let mut output: Vec<char> = counts
                .into_iter()
                .filter(|&(_, n)| n == 1)
                .map(|(c, _)| c)
                .collect();
            output.sort_unstable();
            output.into_iter().collect()
        };

        let infer = match output_spec.len() {
            0 => Infer::F64,
            1 => Infer::Vector { extent: None },
            2 => Infer::Matrix {
                rows: None,
                cols: None,
            },
            _ => Infer::HostDeferred,
        };

        let id = self.push_expr(
            ExprNode::Call {
                function: QualifiedName(name.to_string()),
                arguments: arg_ids,
            },
            expr.source,
        );
        Some((id, infer))
    }
}
