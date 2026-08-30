//! Interpreter for [`EmirProgram`]. Typed registers (`f64` / `i64` /
//! `bool` / vectors / ...); type confusion is a typed fault, never a
//! silent coercion. I64 add/sub/mul/neg stay exact (overflow is a fault).
//! Mixed I64×F64 arithmetic widens to f64; mixed comparisons are exact
//! (not a 2^53 widening round). Same-kind F64 comparisons are IEEE-754;
//! transcendentals follow platform libm (same caveat as generated Rust),
//! and domain obligations are assumptions, not runtime checks.

use crate::{
    BuiltinId, CellClass, EdgePolicy, EmirOp, EmirProgram, EmirValue, EvalBudget, FoldCombine,
};

mod dual;
mod helpers;
mod reverse;
mod value;

use dual::evaluate_dual;
use helpers::*;
use reverse::evaluate_reverse;
pub use value::{EvalFault, Value, format_f64};

/// Evaluate `program` in one forward pass; slots are indexed by
/// [`EmirOp::LoadInput`] / [`EmirOp::LoadState`], missing slots are
/// faults, IEEE-754 exceptions are not. `And`/`Or` evaluate both operands
/// (registers already materialized), matching the Rust backend.
pub fn evaluate(
    program: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
) -> Result<Value, EvalFault> {
    evaluate_with_budget(program, inputs, state, EvalBudget::default())
}

/// [`evaluate`] under an explicit [`EvalBudget`]: resource exhaustion is
/// a typed refusal (`EvalFault::BudgetExhausted`) — never partial
/// authority, never a silently truncated value.
pub fn evaluate_with_budget(
    program: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    let mut registers: Vec<Value> = Vec::with_capacity(program.ops.len());
    let mut applications: u32 = 0;
    for (step, (op, _)) in program.ops.iter().enumerate() {
        let executed = u32::try_from(step).unwrap_or(u32::MAX);
        if executed >= budget.max_steps {
            return Err(EvalFault::BudgetExhausted { executed });
        }
        if matches!(op, EmirOp::ApplyCapability { .. }) {
            applications = applications.saturating_add(1);
            if applications > budget.max_capability_applications {
                return Err(EvalFault::BudgetExhausted { executed });
            }
        }
        let value = eval_op(op, &registers, inputs, state)?;
        registers.push(value);
    }
    register(&registers, program.result).cloned()
}

/// Convenience for scalar-only programs (existing tests and given maps).
pub fn evaluate_f64(
    program: &EmirProgram,
    inputs: &[f64],
    state: &[f64],
) -> Result<Value, EvalFault> {
    let inputs: Vec<Value> = inputs.iter().copied().map(Value::F64).collect();
    let state: Vec<Value> = state.iter().copied().map(Value::F64).collect();
    evaluate(program, &inputs, &state)
}

fn format_text(template: &str, arguments: &[Value]) -> String {
    let mut output = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    let mut argument = 0usize;
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                output.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                output.push('}');
            }
            '{' => {
                let mut field = String::new();
                for next in chars.by_ref() {
                    if next == '}' {
                        break;
                    }
                    field.push(next);
                }
                if let Some(value) = arguments.get(argument) {
                    let precision = field
                        .split_once(':')
                        .and_then(|(_, spec)| spec.strip_prefix('.'))
                        .and_then(|spec| spec.strip_suffix('f'))
                        .and_then(|digits| digits.parse::<usize>().ok());
                    match (value, precision) {
                        (Value::F64(number), Some(precision)) => {
                            output.push_str(&format!("{number:.precision$}"));
                        }
                        (Value::I64(number), Some(precision)) => {
                            output.push_str(&format!("{:.precision$}", *number as f64));
                        }
                        _ => output.push_str(&value.to_string()),
                    }
                }
                argument += 1;
            }
            _ => output.push(ch),
        }
    }
    output
}

fn report_parts(document: &Value) -> Option<(&str, &str, &str)> {
    let Value::Record { type_name, fields } = document else {
        return None;
    };
    if type_name != "core::report::Document" {
        return None;
    }
    let Value::Text(title) = fields.get("title")? else {
        return None;
    };
    let Value::Record { type_name, fields } = fields.get("section")? else {
        return None;
    };
    if type_name != "core::report::Section" {
        return None;
    }
    let Value::Text(heading) = fields.get("heading")? else {
        return None;
    };
    let Value::Text(body) = fields.get("body")? else {
        return None;
    };
    Some((title, heading, body))
}

fn latex_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\textbackslash{}"),
            '{' => escaped.push_str("\\{"),
            '}' => escaped.push_str("\\}"),
            '#' | '$' | '%' | '&' | '_' => {
                escaped.push('\\');
                escaped.push(character);
            }
            '^' => escaped.push_str("\\textasciicircum{}"),
            '~' => escaped.push_str("\\textasciitilde{}"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn sample_series(
    points: &[(f64, f64)],
    interpolation: &str,
    extrapolation: &str,
    time: f64,
) -> Result<f64, EvalFault> {
    let Some(&(start, start_value)) = points.first() else {
        return Err(EvalFault::Arithmetic {
            op: "series-sample",
            detail: "series has no support points",
        });
    };
    let &(end, end_value) = points.last().expect("nonempty checked");
    let after_end = time > end;
    if time < start || after_end {
        match extrapolation {
            "refuse" => {
                return Err(EvalFault::SeriesOutOfSupport {
                    time_bits: time.to_bits(),
                    start_bits: start.to_bits(),
                    end_bits: end.to_bits(),
                });
            }
            "clamp" => return Ok(if time < start { start_value } else { end_value }),
            "extend" => {
                // `extend` continues the OUTER interval's interpolation.
                // Step (previous/pwc) and nearest modes never form an
                // outer segment past the last sample: the value beyond
                // the end is the last sample itself (the endpoint is
                // always evaluated, never the step below it). Linear and
                // monotone_cubic fall through to their outer-interval
                // continuation below. (emath-uooxi)
                if after_end {
                    match interpolation {
                        "previous" | "pwc" | "nearest" => return Ok(end_value),
                        _ => {}
                    }
                }
            }
            _ => {
                return Err(EvalFault::Arithmetic {
                    op: "series-sample",
                    detail: "unknown extrapolation policy",
                });
            }
        }
    }
    if points.len() == 1 || time == end {
        return Ok(end_value);
    }
    let index = if time <= start {
        0
    } else if time >= end {
        points.len() - 2
    } else {
        points
            .windows(2)
            .position(|window| time >= window[0].0 && time < window[1].0)
            .expect("strictly increasing support brackets interior time")
    };
    let (left_time, left_value) = points[index];
    let (right_time, right_value) = points[index + 1];
    let alpha = (time - left_time) / (right_time - left_time);
    match interpolation {
        "previous" | "pwc" => Ok(left_value),
        "nearest" => Ok(if alpha < 0.5 { left_value } else { right_value }),
        "linear" => Ok(left_value + alpha * (right_value - left_value)),
        "monotone_cubic" => {
            let secant = (right_value - left_value) / (right_time - left_time);
            let left_slope = if index == 0 {
                secant
            } else {
                let prior = (left_value - points[index - 1].1) / (left_time - points[index - 1].0);
                if prior.signum() == secant.signum() {
                    0.5 * (prior + secant)
                } else {
                    0.0
                }
            };
            let right_slope = if index + 2 == points.len() {
                secant
            } else {
                let next = (points[index + 2].1 - right_value) / (points[index + 2].0 - right_time);
                if next.signum() == secant.signum() {
                    0.5 * (secant + next)
                } else {
                    0.0
                }
            };
            let h = right_time - left_time;
            let a2 = alpha * alpha;
            let a3 = a2 * alpha;
            Ok((2.0 * a3 - 3.0 * a2 + 1.0) * left_value
                + (a3 - 2.0 * a2 + alpha) * h * left_slope
                + (-2.0 * a3 + 3.0 * a2) * right_value
                + (a3 - a2) * h * right_slope)
        }
        _ => Err(EvalFault::Arithmetic {
            op: "series-sample",
            detail: "unknown interpolation policy",
        }),
    }
}

/// Capability-cell application at the VM seam (fjxh.6 + fjxh.5). Cells
/// with local reference semantics evaluate in the interp world by
/// executing the cell's COMPILED reference bytecode — dispatched from the
/// data registry (`term_compile::std_cell_registry`), never a per-op
/// match arm. Contract guards are cell data run in declared order before
/// the body. Everything else is an outstanding provider call: the typed
/// continuation hole, never a silent identity and never partial authority.
fn apply_capability_cell(
    capability: &str,
    class: CellClass,
    args: &[(u32, Value)],
) -> Result<Value, EvalFault> {
    if class != CellClass::Pure {
        return Err(EvalFault::ProviderCallRequired {
            capability: capability.to_string(),
            args: args.len(),
        });
    }
    let Some(cell) = crate::term_compile::std_cell_registry().get(capability) else {
        // Compiled-data miss: fall back to the immutable native-kernel
        // registry — the shared builtin-miss seam. Kernel-backed pure
        // cells resolve here with the SAME arity/refusal discipline as
        // the compiled path, no new EmirOp and no domain switch; the
        // handler owns its guards (strict-f64, shape) and its refusal
        // text flows through verbatim. Unknown names keep the exact
        // pre-existing refusal below.
        if let Some(kernel) = crate::native_kernel::native_kernel(capability) {
            if args.len() != kernel.arity {
                return Err(EvalFault::Arithmetic {
                    op: "apply-capability",
                    detail: "capability argument count does not match the cell contract",
                });
            }
            let values: Vec<Value> = args.iter().map(|(_, value)| value.clone()).collect();
            let values: Vec<Value> = args.iter().map(|(_, value)| value.clone()).collect();
            let value =
                (kernel.handler)(&values).map_err(|code| EvalFault::CapabilityRefused {
                    capability: capability.to_string(),
                    code,
                })?;
            return Ok(value);
        }
        return Err(EvalFault::Arithmetic {
            op: "apply-capability",
            detail: "no local reference semantics for this pure cell",
        });
    };
    if args.len() != cell.params.len() {
        return Err(EvalFault::Arithmetic {
            op: "apply-capability",
            detail: "capability argument count does not match the cell contract",
        });
    }
    // Data-driven guards (empty / non-finite under the strict-f64 finite
    // policy): a violation is the capability layer's typed refusal, and a
    // non-vector argument is a typed confusion — never a coercion. One
    // shared implementation with the specializer's residual entry.
    let values: Vec<Value> = args.iter().map(|(_, value)| value.clone()).collect();
    crate::term_compile::run_guards(capability, &cell.guards, &values)?;
    let inputs: Vec<Value> = args.iter().map(|(_, value)| value.clone()).collect();
    let value = evaluate_with_budget(&cell.program, &inputs, &[], EvalBudget::default())?;
    // Post-body zero certificate (cell DATA, rymw): when the cell
    // declares an exact-zero result contract, one nonzero residual is
    // a typed refusal naming the first violating index and its exact
    // residual — never a silent value. Vector results name the first
    // violating index; scalar results (counts, sums) refuse with the
    // residual directly. Other result carriers are the cell author's
    // contract, not this guard.
    if let Some(crate::term_compile::ResultGuard::AllZero { code }) = cell.result_guard {
        match &value {
            Value::Vector(entries) => {
                for (index, residual) in entries.iter().enumerate() {
                    if *residual != 0.0 {
                        return Err(EvalFault::CapabilityRefused {
                            capability: capability.to_string(),
                            code: format!("{code}(element {index}, residual {residual})"),
                        });
                    }
                }
            }
            Value::F64(residual) if *residual != 0.0 => {
                return Err(EvalFault::CapabilityRefused {
                    capability: capability.to_string(),
                    code: format!("{code}(residual {residual})"),
                });
            }
            _ => {}
        }
    }
    Ok(value)
}

/// Interval value at an interval-op boundary. Anything else is a typed
/// confusion — an interval never silently widens to a scalar or back.
pub(super) fn interval_of(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
) -> Result<(f64, f64), EvalFault> {
    match register(registers, value)? {
        Value::Interval { lo, hi } => Ok((*lo, *hi)),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

/// Deterministic bracket discovery + bisection fallback for
/// `EmirOp::Solve` (emath-9bj1, Track A3). When Newton is unreliable
/// (vanished derivative or non-finite residual/step), probe a fixed
/// geometric grid alternating both sides of the seed (×8 per level,
/// 48 levels — every parameter deterministic), then bisect the first
/// sign-changing bracket with a fixed 120-iteration budget. Returns
/// `Some(root)` only when the bisection midpoint reaches
/// `|f| < tolerance`; `None` means no bracket or no convergent
/// midpoint, so the caller refuses rather than invent a root. Every
/// loop has a fixed count: the fallback cannot hang.
fn solve_bracket_fallback(
    body: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
    var_index: u16,
    seed: f64,
    tolerance: f64,
    name: &'static str,
) -> Result<Option<f64>, EvalFault> {
    let mut probe_inputs = inputs.to_vec();
    let mut evaluate_at = |x: f64| -> Result<f64, EvalFault> {
        probe_inputs[var_index as usize] = Value::F64(x);
        let dual = evaluate_dual(body, &probe_inputs, state, var_index, name)?;
        Ok(dual.primal)
    };
    let mut prev_x = seed;
    let mut prev_f = evaluate_at(seed)?;
    if !prev_f.is_finite() {
        // A non-finite residual at the seed never blocks the scan: it
        // only means "no previous sign" for the next finite probe.
        prev_f = f64::NAN;
    }
    let mut h = 1e-4_f64;
    const GROWTH: f64 = 8.0;
    for _ in 0..48 {
        for side in [1.0_f64, -1.0_f64] {
            let candidate = seed + side * h;
            let f = evaluate_at(candidate)?;
            if !f.is_finite() {
                continue;
            }
            if f == 0.0 {
                return Ok(Some(candidate));
            }
            if prev_f.is_finite() && (prev_f < 0.0) != (f < 0.0) {
                let (mut lo, mut hi) = (prev_x, candidate);
                let mut f_lo = prev_f;
                for _ in 0..120 {
                    let mid = 0.5 * (lo + hi);
                    let f_mid = evaluate_at(mid)?;
                    if !f_mid.is_finite() {
                        return Ok(None);
                    }
                    if f_mid == 0.0 || f_mid.abs() < tolerance {
                        return Ok(Some(mid));
                    }
                    if (f_lo < 0.0) != (f_mid < 0.0) {
                        hi = mid;
                    } else {
                        lo = mid;
                        f_lo = f_mid;
                    }
                }
                return Ok(None);
            }
            prev_x = candidate;
            prev_f = f;
        }
        h *= GROWTH;
    }
    Ok(None)
}

pub(super) fn eval_op(
    op: &EmirOp,
    registers: &[Value],
    inputs: &[Value],
    state: &[Value],
) -> Result<Value, EvalFault> {
    let name = op.name();
    match *op {
        EmirOp::ConstF64(bits) => Ok(Value::F64(f64::from_bits(bits))),
        EmirOp::ConstI64(value) => Ok(Value::I64(value)),
        EmirOp::ConstText(ref value) => Ok(Value::Text(value.clone())),
        EmirOp::FormatText {
            ref template,
            ref arguments,
        } => {
            let values = arguments
                .iter()
                .map(|argument| register(registers, *argument).cloned())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Text(format_text(template, &values)))
        }
        EmirOp::TextLength(text) => match register(registers, text)? {
            Value::Text(value) => Ok(Value::I64(value.chars().count() as i64)),
            _ => Err(EvalFault::TypeConfusion {
                register: text.0,
                op: name,
            }),
        },
        EmirOp::TextNfc(text) => match register(registers, text)? {
            Value::Text(value) => Ok(Value::Text(emath_core::normalize_nfc(value))),
            _ => Err(EvalFault::TypeConfusion {
                register: text.0,
                op: name,
            }),
        },
        EmirOp::SpecialFunction {
            function,
            ref arguments,
            error_bound,
        } => {
            let arguments = arguments
                .iter()
                .map(|argument| f64_of(registers, *argument, name))
                .collect::<Result<Vec<_>, _>>()?;
            let evaluated =
                emath_core::special::evaluate_strict(function, &arguments).map_err(|refusal| {
                    use emath_core::special::DomainRefusal;
                    let code = match refusal {
                        DomainRefusal::Pole { .. } => "E-SPECIAL-POLE",
                        DomainRefusal::OutsideCarrier { .. } => "E-SPECIAL-DOMAIN",
                        DomainRefusal::NotImplemented { .. } => "E-SPECIAL-NOT-IMPLEMENTED",
                        DomainRefusal::Arity { .. } => "E-SPECIAL-ARITY",
                    };
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: code.to_string(),
                    }
                })?;
            Ok(Value::F64(if error_bound {
                evaluated.error_bound
            } else {
                evaluated.value
            }))
        }
        EmirOp::ReportSection { heading, body } => {
            let (Value::Text(heading), Value::Text(body)) =
                (register(registers, heading)?, register(registers, body)?)
            else {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "report section requires text heading and body",
                });
            };
            Ok(Value::Record {
                type_name: "core::report::Section".to_string(),
                fields: std::collections::BTreeMap::from([
                    ("body".to_string(), Value::Text(body.clone())),
                    ("heading".to_string(), Value::Text(heading.clone())),
                ]),
            })
        }
        EmirOp::ReportDocument { title, section } => {
            let Value::Text(title) = register(registers, title)? else {
                return Err(EvalFault::TypeConfusion {
                    register: title.0,
                    op: name,
                });
            };
            let section_register = section;
            let section = register(registers, section_register)?;
            if !matches!(
                section,
                Value::Record { type_name, .. } if type_name == "core::report::Section"
            ) {
                return Err(EvalFault::TypeConfusion {
                    register: section_register.0,
                    op: name,
                });
            }
            Ok(Value::Record {
                type_name: "core::report::Document".to_string(),
                fields: std::collections::BTreeMap::from([
                    ("section".to_string(), section.clone()),
                    ("title".to_string(), Value::Text(title.clone())),
                ]),
            })
        }
        EmirOp::ReportMarkdown(document) => match report_parts(register(registers, document)?) {
            Some((title, heading, body)) => Ok(Value::Text(format!(
                "# {title}\n\n## {heading}\n\n{body}\n"
            ))),
            None => Err(EvalFault::TypeConfusion {
                register: document.0,
                op: name,
            }),
        },
        EmirOp::ReportLatex(document) => match report_parts(register(registers, document)?) {
            Some((title, heading, body)) => Ok(Value::Text(format!(
                "\\section{{{}}}\n\\subsection{{{}}}\n{}\n",
                latex_escape(title),
                latex_escape(heading),
                latex_escape(body)
            ))),
            None => Err(EvalFault::TypeConfusion {
                register: document.0,
                op: name,
            }),
        },
        EmirOp::SeriesCreate {
            ref points,
            ref interpolation,
            ref extrapolation,
        } => Ok(Value::Series {
            points: points.clone(),
            interpolation: interpolation.clone(),
            extrapolation: extrapolation.clone(),
        }),
        EmirOp::SeriesSample { series, time } => {
            let time = f64_of(registers, time, name)?;
            match register(registers, series)? {
                Value::Series {
                    points,
                    interpolation,
                    extrapolation,
                } => Ok(Value::F64(sample_series(
                    points,
                    interpolation,
                    extrapolation,
                    time,
                )?)),
                _ => Err(EvalFault::TypeConfusion {
                    register: series.0,
                    op: name,
                }),
            }
        }
        EmirOp::SetCreate {
            ref elements,
            ref guards,
        } => {
            if elements.len() != guards.len() {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "set guard count does not match element count",
                });
            }
            let mut values = Vec::new();
            for (element, guard) in elements.iter().zip(guards) {
                let include = match guard {
                    Some(guard) => match register(registers, *guard)? {
                        Value::Bool(value) => *value,
                        _ => {
                            return Err(EvalFault::TypeConfusion {
                                register: guard.0,
                                op: name,
                            });
                        }
                    },
                    None => true,
                };
                if include {
                    let value = register(registers, *element)?.clone();
                    if !values.iter().any(|existing| existing == &value) {
                        values.push(value);
                    }
                }
            }
            Ok(Value::Set(values))
        }
        EmirOp::SetContains { element, set } => {
            let element = register(registers, element)?;
            match register(registers, set)? {
                Value::Set(values) => Ok(Value::Bool(values.iter().any(|value| value == element))),
                _ => Err(EvalFault::TypeConfusion {
                    register: set.0,
                    op: name,
                }),
            }
        }
        EmirOp::RecordCreate {
            ref type_name,
            ref fields,
        } => {
            let mut values = std::collections::BTreeMap::new();
            for (field, value) in fields {
                values.insert(field.clone(), register(registers, *value)?.clone());
            }
            Ok(Value::Record {
                type_name: type_name.clone(),
                fields: values,
            })
        }
        EmirOp::ConstComplex(re, im) => Ok(Value::Complex { re, im }),
        EmirOp::ConstBool(value) => Ok(Value::Bool(value)),
        EmirOp::LoadInput(index) => inputs
            .get(usize::from(index))
            .cloned()
            .ok_or(EvalFault::MissingInput(index)),
        EmirOp::LoadState(index) => state
            .get(usize::from(index))
            .cloned()
            .ok_or(EvalFault::MissingState(index)),
        EmirOp::ApplyCapability {
            ref capability,
            class,
            ref args,
        } => {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                let register_index = arg.0;
                let value = register(registers, *arg)?;
                values.push((register_index, value.clone()));
            }
            apply_capability_cell(capability, class, &values)
        }
        EmirOp::VectorMap { builtin, source } => {
            let vector = vector_of(registers, source, name)?;
            Ok(Value::Vector(
                vector.iter().map(|&x| builtin.eval_unary(x)).collect(),
            ))
        }
        EmirOp::VectorMapScalar { op, vector, scalar } => {
            let values = vector_of(registers, vector, name)?;
            let scalar = match register(registers, scalar)? {
                Value::F64(scalar) => *scalar,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: scalar.0,
                        op: name,
                    });
                }
            };
            Ok(Value::Vector(
                values.iter().map(|&x| op.eval(x, scalar)).collect(),
            ))
        }
        EmirOp::VectorReduce { reduce, source } => {
            let vector = vector_of(registers, source, name)?;
            if vector.is_empty() {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "reduce of empty vector",
                });
            }
            Ok(Value::F64(reduce.eval(vector)))
        }
        EmirOp::VectorAllFinite(source) => {
            let vector = vector_of(registers, source, name)?;
            Ok(Value::Bool(vector.iter().all(|x| x.is_finite())))
        }
        EmirOp::IntervalCreate(lo_reg, hi_reg) => {
            let lo = f64_of(registers, lo_reg, name)?;
            let hi = f64_of(registers, hi_reg, name)?;
            if !lo.is_finite() || !hi.is_finite() {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "non-finite interval bound",
                });
            }
            if lo > hi {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "ill-formed interval: lower bound exceeds upper",
                });
            }
            Ok(Value::Interval { lo, hi })
        }
        EmirOp::IntervalIntersect(left, right) => {
            let (alo, ahi) = interval_of(registers, left, name)?;
            let (blo, bhi) = interval_of(registers, right, name)?;
            let lo = alo.max(blo);
            let hi = ahi.min(bhi);
            if lo > hi {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "empty interval intersection",
                });
            }
            Ok(Value::Interval { lo, hi })
        }
        EmirOp::F64Add(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Interval { .. }, _) | (_, Value::Interval { .. }) => {
                    let (alo, ahi) = interval_of(registers, left, name)?;
                    let (blo, bhi) = interval_of(registers, right, name)?;
                    Ok(Value::Interval {
                        lo: alo + blo,
                        hi: ahi + bhi,
                    })
                }
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    Ok(Value::Complex {
                        re: lr + rr,
                        im: li + ri,
                    })
                }
                (Value::I64(a), Value::I64(b)) => i64_checked(*a, *b, name, i64::checked_add),
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? + f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Sub(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Interval { .. }, _) | (_, Value::Interval { .. }) => {
                    let (alo, ahi) = interval_of(registers, left, name)?;
                    let (blo, bhi) = interval_of(registers, right, name)?;
                    Ok(Value::Interval {
                        lo: alo - bhi,
                        hi: ahi - blo,
                    })
                }
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    Ok(Value::Complex {
                        re: lr - rr,
                        im: li - ri,
                    })
                }
                (Value::I64(a), Value::I64(b)) => i64_checked(*a, *b, name, i64::checked_sub),
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? - f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Mul(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Interval { .. }, _) | (_, Value::Interval { .. }) => {
                    let (alo, ahi) = interval_of(registers, left, name)?;
                    let (blo, bhi) = interval_of(registers, right, name)?;
                    // Certified propagation: the result bounds enclose the
                    // product over every pair of points in the operands.
                    let products = [alo * blo, alo * bhi, ahi * blo, ahi * bhi];
                    Ok(Value::Interval {
                        lo: products.iter().cloned().fold(products[0], f64::min),
                        hi: products.iter().cloned().fold(products[0], f64::max),
                    })
                }
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    // (a + bi)(c + di) = (ac - bd) + (ad + bc)i
                    Ok(Value::Complex {
                        re: lr * rr - li * ri,
                        im: lr * ri + li * rr,
                    })
                }
                (Value::I64(a), Value::I64(b)) => i64_checked(*a, *b, name, i64::checked_mul),
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? * f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Div(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Interval { .. }, _) | (_, Value::Interval { .. }) => {
                    let (alo, ahi) = interval_of(registers, left, name)?;
                    let (blo, bhi) = interval_of(registers, right, name)?;
                    // Zero-CONTAINING divisor: typed refusal, never a
                    // silently widened interval (8pjn negative control).
                    if blo <= 0.0 && 0.0 <= bhi {
                        return Err(EvalFault::Arithmetic {
                            op: name,
                            detail: "interval divisor contains zero",
                        });
                    }
                    // 1/b on a zero-free interval: bounds flip.
                    let products = [
                        alo * (1.0 / bhi),
                        alo * (1.0 / blo),
                        ahi * (1.0 / bhi),
                        ahi * (1.0 / blo),
                    ];
                    Ok(Value::Interval {
                        lo: products.iter().cloned().fold(products[0], f64::min),
                        hi: products.iter().cloned().fold(products[0], f64::max),
                    })
                }
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    let denom = rr * rr + ri * ri;
                    // (a + bi) / (c + di) = ((ac + bd) + (bc - ad)i) / (c² + d²)
                    Ok(Value::Complex {
                        re: (lr * rr + li * ri) / denom,
                        im: (li * rr - lr * ri) / denom,
                    })
                }
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? / f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Pow(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.powf(f64_of(registers, right, name)?),
        )),
        EmirOp::Neg(value) => match register(registers, value)? {
            Value::Complex { re, im } => Ok(Value::Complex { re: -*re, im: -*im }),
            Value::I64(n) => n
                .checked_neg()
                .map(Value::I64)
                .ok_or(EvalFault::Arithmetic {
                    op: name,
                    detail: "i64 overflow",
                }),
            _ => Ok(Value::F64(-f64_of(registers, value, name)?)),
        },
        EmirOp::UnaryBuiltin(id, value) => match register(registers, value)? {
            Value::Complex { re, im } => eval_complex_unary(id, *re, *im, value.0, name),
            _ => Ok(Value::F64(id.eval_unary(f64_of(registers, value, name)?))),
        },
        EmirOp::BinaryBuiltin(id, left, right) => {
            let l = f64_of(registers, left, name)?;
            let r = f64_of(registers, right, name)?;
            Ok(Value::F64(id.eval_binary(l, r)))
        }
        EmirOp::Lt(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_lt(), |a, b| a < b)
        }
        EmirOp::Le(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_le(), |a, b| a <= b)
        }
        EmirOp::Gt(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_gt(), |a, b| a > b)
        }
        EmirOp::Ge(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_ge(), |a, b| a >= b)
        }
        EmirOp::Eq(left, right) => eq_ne(registers, left, right, name, true),
        EmirOp::Ne(left, right) => eq_ne(registers, left, right, name, false),
        EmirOp::And(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? && bool_of(registers, right, name)?,
        )),
        EmirOp::Or(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? || bool_of(registers, right, name)?,
        )),
        EmirOp::Imply(left, right) => Ok(Value::Bool(
            !bool_of(registers, left, name)? || bool_of(registers, right, name)?,
        )),
        EmirOp::Iff(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? == bool_of(registers, right, name)?,
        )),
        EmirOp::Not(value) => Ok(Value::Bool(!bool_of(registers, value, name)?)),
        EmirOp::IsFinite(value) => Ok(Value::Bool(f64_of(registers, value, name)?.is_finite())),
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => {
            if bool_of(registers, condition, name)? {
                register(registers, then_value).cloned()
            } else {
                register(registers, else_value).cloned()
            }
        }
        EmirOp::VectorCreate(ref elements) => {
            let mut vec = Vec::with_capacity(elements.len());
            for &elem in elements {
                vec.push(f64_of(registers, elem, name)?);
            }
            Ok(Value::Vector(vec))
        }
        EmirOp::MatrixCreate {
            rows,
            cols,
            ref elements,
        } => {
            let expected = rows.checked_mul(cols).ok_or(EvalFault::Arithmetic {
                op: name,
                detail: "matrix size overflow",
            })?;
            if elements.len() != expected {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "matrix element count does not match rows*cols",
                });
            }
            let mut data = Vec::with_capacity(elements.len());
            for &elem in elements {
                data.push(f64_of(registers, elem, name)?);
            }
            Ok(Value::Matrix { rows, cols, data })
        }
        EmirOp::VectorIndex { vector, index } => {
            let vec = vector_of(registers, vector, name)?;
            let raw = f64_of(registers, index, name)?;
            emath_rt::vec_index_checked(vec, raw)
                .map(Value::F64)
                .map_err(|err| map_index_error(name, err))
        }
        EmirOp::MatrixIndex { matrix, row, col } => {
            let (r_count, c_count, data) = matrix_of(registers, matrix, name)?;
            let raw_r = f64_of(registers, row, name)?;
            let raw_c = f64_of(registers, col, name)?;
            emath_rt::tensor_index_checked(&[r_count, c_count], data, &[raw_r, raw_c])
                .map(Value::F64)
                .map_err(|err| map_index_error(name, err))
        }
        EmirOp::VectorAdd(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            require_equal_len(v1.len(), v2.len(), name, "vector length mismatch")?;
            Ok(Value::Vector(emath_rt::vec_add(v1, v2)))
        }
        EmirOp::VectorSub(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            require_equal_len(v1.len(), v2.len(), name, "vector length mismatch")?;
            Ok(Value::Vector(emath_rt::vec_sub(v1, v2)))
        }
        EmirOp::VectorScale(left, right) => {
            // Canonical operand order from admission: (vector, scalar).
            // Still accept (scalar, vector) so older EMIR stays evaluable.
            match (register(registers, left)?, register(registers, right)?) {
                (Value::Vector(v), Value::F64(s)) | (Value::F64(s), Value::Vector(v)) => {
                    Ok(Value::Vector(emath_rt::vec_scale(v, *s)))
                }
                _ => Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                }),
            }
        }
        EmirOp::VectorDot(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            require_equal_len(v1.len(), v2.len(), name, "vector length mismatch")?;
            Ok(Value::F64(emath_rt::vec_dot(v1, v2)))
        }
        EmirOp::VectorNorm(value) => {
            let v = vector_of(registers, value, name)?;
            Ok(Value::F64(emath_rt::vec_norm(v)))
        }
        EmirOp::VectorLength(value) => {
            let v = vector_of(registers, value, name)?;
            Ok(Value::F64(v.len() as f64))
        }
        EmirOp::Stencil1d {
            input,
            ref weights,
            center,
            edge,
        } => {
            let v = vector_of(registers, input, name)?;
            let edge = match edge {
                EdgePolicy::Clamp => emath_rt::EdgePolicy::Clamp,
                EdgePolicy::Neumann => emath_rt::EdgePolicy::Neumann,
                EdgePolicy::OneSided => emath_rt::EdgePolicy::OneSided,
                EdgePolicy::Dirichlet { left, right } => {
                    emath_rt::EdgePolicy::Dirichlet { left, right }
                }
            };
            Ok(Value::Vector(emath_rt::stencil_1d(
                v,
                weights,
                center as i64,
                edge,
            )))
        }
        EmirOp::Stencil2d {
            input,
            ref weights,
            center,
            edge,
        } => {
            let (rows, cols, data) = matrix_of(registers, input, name)?;
            if matches!(edge, EdgePolicy::Dirichlet { .. }) {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "2D Dirichlet boundary is not yet supported; use Clamp, Neumann, or OneSided",
                });
            }
            let edge = match edge {
                EdgePolicy::Clamp => emath_rt::EdgePolicy::Clamp,
                EdgePolicy::Neumann => emath_rt::EdgePolicy::Neumann,
                EdgePolicy::OneSided => emath_rt::EdgePolicy::OneSided,
                EdgePolicy::Dirichlet { .. } => unreachable!("checked above"),
            };
            let nested = rows_of(data, cols);
            let w9: &[f64; 9] =
                weights
                    .as_slice()
                    .try_into()
                    .map_err(|_| EvalFault::Arithmetic {
                        op: name,
                        detail: "2D stencil weights must have length 9",
                    })?;
            let out = emath_rt::stencil_2d(&nested, w9, (center.0 as i64, center.1 as i64), edge);
            Ok(Value::Matrix {
                rows,
                cols,
                data: flatten_rows(&out),
            })
        }
        EmirOp::Stencil3d {
            input,
            ref weights,
            center,
            edge,
        } => {
            let (shape, data) = tensor_of(registers, input, name)?;
            let w27: &[f64; 27] =
                weights
                    .as_slice()
                    .try_into()
                    .map_err(|_| EvalFault::Arithmetic {
                        op: name,
                        detail: "3D stencil weights must have length 27",
                    })?;
            let edge = match edge {
                EdgePolicy::Clamp => emath_rt::EdgePolicy::Clamp,
                EdgePolicy::Neumann => emath_rt::EdgePolicy::Neumann,
                EdgePolicy::OneSided => emath_rt::EdgePolicy::OneSided,
                EdgePolicy::Dirichlet { left, right } => {
                    emath_rt::EdgePolicy::Dirichlet { left, right }
                }
            };
            emath_rt::stencil_3d_slices_checked(
                shape,
                data,
                w27,
                (center.0 as i64, center.1 as i64, center.2 as i64),
                edge,
            )
            .map(|out| Value::Tensor {
                shape: out.shape,
                data: out.data,
            })
            .map_err(|detail| EvalFault::Arithmetic { op: name, detail })
        }
        EmirOp::MatrixAdd(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (r2, c2, d2) = matrix_of(registers, right, name)?;
            require_same_matrix_shape(r1, c1, r2, c2, name)?;
            let a = rows_of(d1, c1);
            let b = rows_of(d2, c2);
            let data = flatten_rows(&emath_rt::mat_add(&a, &b));
            Ok(Value::Matrix {
                rows: r1,
                cols: c1,
                data,
            })
        }
        EmirOp::MatrixSub(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (r2, c2, d2) = matrix_of(registers, right, name)?;
            require_same_matrix_shape(r1, c1, r2, c2, name)?;
            let a = rows_of(d1, c1);
            let b = rows_of(d2, c2);
            let data = flatten_rows(&emath_rt::mat_sub(&a, &b));
            Ok(Value::Matrix {
                rows: r1,
                cols: c1,
                data,
            })
        }
        EmirOp::MatrixScale(left, right) => {
            match (register(registers, left)?, register(registers, right)?) {
                (Value::Matrix { rows, cols, data }, Value::F64(s))
                | (Value::F64(s), Value::Matrix { rows, cols, data }) => {
                    let nested = rows_of(data, *cols);
                    Ok(Value::Matrix {
                        rows: *rows,
                        cols: *cols,
                        data: flatten_rows(&emath_rt::mat_scale(&nested, *s)),
                    })
                }
                _ => Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                }),
            }
        }
        EmirOp::MatrixMulVector(matrix, vector) => {
            let (_, cols, m_data) = matrix_of(registers, matrix, name)?;
            let v = vector_of(registers, vector, name)?;
            require_equal_len(v.len(), cols, name, "matrix×vector width mismatch")?;
            let nested = rows_of(m_data, cols);
            Ok(Value::Vector(emath_rt::mat_mul_vec(&nested, v)))
        }
        EmirOp::MatrixMulMatrix(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (r2, c2, d2) = matrix_of(registers, right, name)?;
            if c1 != r2 {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "matrix product inner dimensions mismatch",
                });
            }
            let a = rows_of(d1, c1);
            let b = rows_of(d2, c2);
            let data = flatten_rows(&emath_rt::mat_mul_mat(&a, &b));
            Ok(Value::Matrix {
                rows: r1,
                cols: c2,
                data,
            })
        }
        EmirOp::MatrixTranspose(value) => {
            // Flat row-major involution. Nested `Vec<Vec<f64>>` cannot
            // store a 0-column (or 0-row) extent, and `chunks_exact(0)`
            // panics, so `transpose(transpose(A))` must not go through it.
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let mut out = vec![0.0; data.len()];
            if rows > 0 && cols > 0 {
                for r in 0..rows {
                    let src = r * cols;
                    for c in 0..cols {
                        out[c * rows + r] = data[src + c];
                    }
                }
            }
            Ok(Value::Matrix {
                rows: cols,
                cols: rows,
                data: out,
            })
        }
        EmirOp::EigenSymmetric(value) => {
            // Deterministic cyclic Jacobi over the matrix's dense
            // storage; eigenvalues ASCENDING. Typed refusals for
            // non-square / non-symmetric input (E-LINALG-001/002) —
            // never a garbage spectrum.
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let (values, _vectors) =
                emath_rt::linalg::jacobi_eigen(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(values))
        }
        EmirOp::EigenVectorsSymmetric(value) => {
            // Column j is the unit eigenvector for eigenvalue j.
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let (_values, vectors) =
                emath_rt::linalg::jacobi_eigen(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            let n = vectors.len();
            let mut out = vec![0.0; n * n];
            for (j, column) in vectors.iter().enumerate() {
                for (i, entry) in column.iter().enumerate() {
                    out[i * n + j] = *entry;
                }
            }
            Ok(Value::Matrix {
                rows: n,
                cols: n,
                data: out,
            })
        }
        EmirOp::SvdSingularValues(value) => {
            // Thin rank via the symmetric AᵀA eigenproblem; DESCENDING.
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let singular =
                emath_rt::linalg::svd_singular_values(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(singular))
        }
        EmirOp::SvdFactors(value) => {
            // Packed row-major `[U; s; Vᵀ]` (width max(cols, r), zero
            // padding): rows 0..m = U, row m = s, rows m+1..m+1+r = Vᵀ.
            // The kernel returns the packed block directly.
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let packed =
                emath_rt::linalg::svd_factors_packed(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            // rank = min(rows, cols) <= cols, so the packing width is
            // cols; invert the row count to recover the rank.
            let width = cols;
            let rank = packed.len() / width - rows - 1;
            Ok(Value::Matrix {
                rows: rows + 1 + rank,
                cols: width,
                data: packed,
            })
        }
        EmirOp::CgSolve(a_value, b_value) => {
            // Conjugate gradient over A's dense storage; SPD convergence
            // is checked, and a non-converging system refuses typed
            // (E-LINALG-003) — never a silently wrong x.
            let (rows, cols, data) = matrix_of(registers, a_value, name)?;
            let b = vector_of(registers, b_value, name)?;
            let x = emath_rt::linalg::cg_solve(&data, rows, cols, &b).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(x))
        }
        EmirOp::LinearSolve(a_value, b_value) => {
            let (rows, cols, data) = matrix_of(registers, a_value, name)?;
            let b = vector_of(registers, b_value, name)?;
            let x = emath_rt::linalg::linear_solve(&data, rows, cols, b).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(x))
        }
        EmirOp::LuFactors(value) => {
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let factors = emath_rt::linalg::lu_factors(&data, rows, cols).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Matrix {
                rows: 2 * rows + 1,
                cols,
                data: factors,
            })
        }
        EmirOp::QrFactors(value) => {
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let factors = emath_rt::linalg::qr_factors(&data, rows, cols).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Matrix {
                rows: rows + cols,
                cols,
                data: factors,
            })
        }
        EmirOp::OuterProduct(left_value, right_value) => {
            let left = vector_of(registers, left_value, name)?;
            let right = vector_of(registers, right_value, name)?;
            let data = left
                .iter()
                .flat_map(|left| right.iter().map(move |right| left * right))
                .collect();
            Ok(Value::Matrix {
                rows: left.len(),
                cols: right.len(),
                data,
            })
        }
        EmirOp::GraphReachable(adj, source) => {
            // BFS reachability mask over the dense adjacency carrier;
            // vertices are indices and discovery is ascending-index —
            // deterministic by construction.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let source = graph_source_index(registers, source, name, rows)?;
            let mask =
                emath_rt::graph::reachability(&data, rows, cols, source).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(mask))
        }
        EmirOp::GraphBfsOrder(adj, source) => {
            // BFS visit order: source first, ascending-index discovery
            // (breadth-first, never depth-first, never insertion-order).
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let source = graph_source_index(registers, source, name, rows)?;
            let order = emath_rt::graph::bfs_order(&data, rows, cols, source).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(order))
        }
        EmirOp::GraphDijkstra(adj, source) => {
            // Shortest distances over nonnegative weights; unreachable
            // vertices are +Inf; a negative weight refuses typed
            // E-GRAPH-002 (Dijkstra's precondition) — never a silently
            // wrong distance set.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let source = graph_source_index(registers, source, name, rows)?;
            let distances =
                emath_rt::graph::dijkstra(&data, rows, cols, source).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(distances))
        }
        EmirOp::GraphDegreeOut(adj) => {
            // Out-degree = count of nonzero entries per row; in-degree
            // is the same op over the transposed carrier.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let degrees = emath_rt::graph::degree_out(&data, rows, cols).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(degrees))
        }
        EmirOp::GraphLaplacian(adj) => {
            // L = D − A (slice 3): the unnormalized Laplacian; the
            // spectrum composes through the EXISTING symmetric eigen
            // op (undirected carriers only — the documented fence).
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let laplacian = emath_rt::graph::laplacian(&data, rows, cols).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Matrix {
                rows,
                cols,
                data: laplacian,
            })
        }
        EmirOp::GraphSymmetrize(adj) => {
            // S = (A + Aᵀ)/2 (slice 4): the weight-preserving
            // symmetrization; the output composes through the
            // EXISTING laplacian/symmetric-eigen path.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let symmetrized = emath_rt::graph::symmetrize(&data, rows, cols).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Matrix {
                rows,
                cols,
                data: symmetrized,
            })
        }
        EmirOp::GraphBellmanFord(adj, source) => {
            // Negative-edge shortest paths (slice 5): relaxation-based;
            // a reachable negative cycle refuses E-GRAPH-005 — never
            // fabricated distances.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let source = graph_source_index(registers, source, name, rows)?;
            let distances =
                emath_rt::graph::bellman_ford(&data, rows, cols, source).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(distances))
        }
        EmirOp::OptionSome(value) => Ok(Value::Option(Some(Box::new(
            registers[value.0 as usize].clone(),
        )))),
        EmirOp::OptionNone => Ok(Value::Option(None)),
        EmirOp::OptionIsSome(option) => {
            let Value::Option(inner) = &registers[option.0 as usize] else {
                return Err(EvalFault::TypeConfusion {
                    register: option.0,
                    op: name,
                });
            };
            Ok(Value::Bool(inner.is_some()))
        }
        EmirOp::OptionUnwrapOr(option, default) => {
            let Value::Option(inner) = &registers[option.0 as usize] else {
                return Err(EvalFault::TypeConfusion {
                    register: option.0,
                    op: name,
                });
            };
            match inner {
                Some(value) => Ok((**value).clone()),
                None => Ok(registers[default.0 as usize].clone()),
            }
        }
        EmirOp::ResultOk(value) => Ok(Value::Result {
            ok: true,
            payload: Box::new(registers[value.0 as usize].clone()),
        }),
        EmirOp::ResultErr(error) => Ok(Value::Result {
            ok: false,
            payload: Box::new(registers[error.0 as usize].clone()),
        }),
        EmirOp::ResultIsOk(result) => {
            let Value::Result { ok, .. } = &registers[result.0 as usize] else {
                return Err(EvalFault::TypeConfusion {
                    register: result.0,
                    op: name,
                });
            };
            Ok(Value::Bool(*ok))
        }
        EmirOp::ResultUnwrapOr(result, default) => {
            let Value::Result { ok, payload } = &registers[result.0 as usize] else {
                return Err(EvalFault::TypeConfusion {
                    register: result.0,
                    op: name,
                });
            };
            if *ok {
                Ok((**payload).clone())
            } else {
                Ok(registers[default.0 as usize].clone())
            }
        }
        EmirOp::ResultErrorOf(result) => {
            // The error as an OPTION: Ok → None, Err → Some(error)
            // (Result errors compose with the Option ops).
            let Value::Result { ok, payload } = &registers[result.0 as usize] else {
                return Err(EvalFault::TypeConfusion {
                    register: result.0,
                    op: name,
                });
            };
            if *ok {
                Ok(Value::Option(None))
            } else {
                Ok(Value::Option(Some(payload.clone())))
            }
        }
        EmirOp::GraphSparseTriplets(adj) => {
            // Sparse COO extraction (slice 6): ascending (u, v)
            // triplets of the nonzero entries.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let triplets =
                emath_rt::graph::sparse_triplets(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(triplets))
        }
        EmirOp::IntNullspace(matrix) => {
            // Exact integer null vector (rymw): the generic primitive
            // over the dense carrier. Every entry must be an exact
            // small integer (E-NULLSPACE-001); the nullspace must be
            // exactly one-dimensional (E-NULLSPACE-002); the result is
            // the canonical primitive vector (f64-exact integers).
            let (rows, cols, data) = matrix_of(registers, matrix, name)?;
            let mut int_rows: Vec<Vec<i64>> = Vec::with_capacity(rows);
            for chunk in data.chunks(cols) {
                let mut row = Vec::with_capacity(cols);
                for &x in chunk {
                    // Exact small-integer check: integral value inside
                    // the i64 range is representable exactly in f64.
                    if x.fract() != 0.0 || x < -2f64.powi(63) || x >= 2f64.powi(63) {
                        return Err(EvalFault::Arithmetic {
                            op: name,
                            detail: "E-NULLSPACE-001: non-integral entry in integer \
                                     nullspace input",
                        });
                    }
                    row.push(x as i64);
                }
                int_rows.push(row);
            }
            let null_vector = emath_rt::primitive_int_nullvector(&int_rows).map_err(|_| {
                EvalFault::Arithmetic {
                    op: name,
                    detail: "E-NULLSPACE-001: exact-integer overflow in nullspace input",
                }
            })?;
            let Some(vector) = null_vector else {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "E-NULLSPACE-002: integer matrix has no exactly one-dimensional \
                             nullspace",
                });
            };
            Ok(Value::Vector(vector.iter().map(|&v| v as f64).collect()))
        }
        EmirOp::ExactProductDelta(p_value, q_value) => {
            // Exact integer product difference (rymw thermo): the
            // generic exact-rational equality primitive. Products run
            // over u128 with overflow refusal; entries must be exact
            // small integers. The difference is returned as f64 (exact
            // while |delta| < 2^53, guaranteed by the u128 guard).
            let exact_index = |x: f64| -> Result<u64, EvalFault> {
                if x.fract() != 0.0 || x < 0.0 || x >= 9_007_199_254_740_992.0 {
                    return Err(EvalFault::Arithmetic {
                        op: name,
                        detail: "E-EXACT-001: entries must be exact small nonnegative integers",
                    });
                }
                Ok(x as u64)
            };
            let p = vector_of(registers, p_value, name)?;
            let q = vector_of(registers, q_value, name)?;
            if p.len() != q.len() {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "E-EXACT-001: numerator and denominator vectors differ in length",
                });
            }
            let product = |values: &[f64]| -> Result<u128, EvalFault> {
                let mut acc = 1u128;
                for &x in values {
                    acc = acc
                        .checked_mul(u128::from(exact_index(x)?))
                        .ok_or(EvalFault::Arithmetic {
                            op: name,
                            detail: "E-EXACT-002: exact product overflow (use reduced K_i)",
                        })?;
                }
                Ok(acc)
            };
            let pp = product(p)?;
            let qq = product(q)?;
            // Exact compare BEFORE any cast (false-zero fix, mail 93):
            // entries are < 2^53 but products are u128, so distinct
            // exact products above 2^53 can cast to the same f64 and
            // falsely certify consistency. Compare in u128, subtract
            // the magnitude exactly, then apply the sign for the
            // diagnostic scalar.
            if pp == qq {
                return Ok(Value::F64(0.0));
            }
            let (magnitude, negative) = if pp > qq {
                (pp - qq, false)
            } else {
                (qq - pp, true)
            };
            let delta = magnitude as f64;
            Ok(Value::F64(if negative { -delta } else { delta }))
        }
        EmirOp::GraphSparseFromTriplets(n_value, triplets_value) => {
            let n = f64_of(registers, n_value, name)?;
            let triplets = vector_of(registers, triplets_value, name)?.to_vec();
            let flat = emath_rt::graph::sparse_from_triplets(n, &triplets).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            let side = (flat.len() as f64).sqrt() as usize;
            Ok(Value::Matrix {
                rows: side,
                cols: side,
                data: flat,
            })
        }
        EmirOp::LpMinimize(a_value, b_value, c_value) => {
            // Standard-form LP via Bland's-rule simplex (slice 1):
            // deterministic smallest-index pivoting; unbounded
            // objectives refuse typed (E-LP-001) — never a wrong
            // finite "optimum".
            let (m, n, a_flat) = matrix_of(registers, a_value, name)?;
            let b = vector_of(registers, b_value, name)?.to_vec();
            let c = vector_of(registers, c_value, name)?.to_vec();
            let x =
                emath_rt::optimization::lp_minimize(&a_flat, m, n, &b, &c).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(x))
        }
        EmirOp::ParetoFront(points_value) => {
            // Strict Pareto mask (slice 1): rows are objective vectors
            // (all minimized); the mask is the portfolio artifact's
            // deterministic data.
            let (rows, cols, data) = matrix_of(registers, points_value, name)?;
            let mask =
                emath_rt::optimization::pareto_front(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(mask))
        }
        EmirOp::PolyMul(a_value, b_value) => {
            // Cauchy convolution over ascending coefficients (the B28
            // compute layer); empty operand = the zero polynomial.
            let a = vector_of(registers, a_value, name)?.to_vec();
            let b = vector_of(registers, b_value, name)?.to_vec();
            let product = emath_rt::polynomial::poly_mul(&a, &b).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(product))
        }
        EmirOp::PolyEval(poly_value, point_value) => {
            // Horner evaluation (ascending coefficients); empty
            // coefficients evaluate to 0.0 (the zero polynomial).
            let coefficients = vector_of(registers, poly_value, name)?.to_vec();
            let point = f64_of(registers, point_value, name)?;
            let value = emath_rt::polynomial::poly_eval(&coefficients, point).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::F64(value))
        }
        EmirOp::SequenceGenerate {
            initial,
            recurrence,
            budget,
        } => {
            let initial = vector_of(registers, initial, name)?;
            let recurrence = vector_of(registers, recurrence, name)?;
            let budget = f64_of(registers, budget, name)?;
            let values =
                emath_rt::sequence::generate(initial, recurrence, budget).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(values))
        }
        EmirOp::SequenceConvolve { left, right, count } => {
            let left = vector_of(registers, left, name)?;
            let right = vector_of(registers, right, name)?;
            let count = f64_of(registers, count, name)?;
            let values = emath_rt::sequence::convolve(left, right, count).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(values))
        }
        EmirOp::OdeBackwardEuler(rate_value, y0_value, h_value) => {
            // Backward Euler on the scalar carrier (xx0x.3): Newton to
            // machine tolerance; typed refusals E-ODE-001/003/004.
            let rate = vector_of(registers, rate_value, name)?.to_vec();
            let y0 = f64_of(registers, y0_value, name)?;
            let h = f64_of(registers, h_value, name)?;
            let y1 = emath_rt::dynamics::ode_backward_euler(&rate, y0, h).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::F64(y1))
        }
        EmirOp::OdeVelocityVerlet(a_value, q_value, v_value, h_value) => {
            // Velocity Verlet on the separable scalar carrier
            // (xx0x.3): kick-drift-kick; typed refusals E-ODE-003/004.
            let acceleration = vector_of(registers, a_value, name)?.to_vec();
            let q0 = f64_of(registers, q_value, name)?;
            let v0 = f64_of(registers, v_value, name)?;
            let h = f64_of(registers, h_value, name)?;
            let (q1, v1) = emath_rt::dynamics::ode_velocity_verlet(&acceleration, q0, v0, h)
                .map_err(|error| EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                })?;
            Ok(Value::Vector(vec![q1, v1]))
        }
        EmirOp::PoissonDirichletSine(load_value) => {
            // Spectral Poisson on the Dirichlet unit interval
            // (xx0x.4): discrete sine diagonalization; typed refusals
            // E-PDE-001/002.
            let load = vector_of(registers, load_value, name)?.to_vec();
            let field = emath_rt::pde::poisson_dirichlet_sine(&load).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(field))
        }
        EmirOp::ControlTransferEval(num_value, den_value, x_value) => {
            // Transfer-function evaluation (zxkl thin B43): Horner
            // both sides; typed refusals E-CONTROL-001/002.
            let num = vector_of(registers, num_value, name)?.to_vec();
            let den = vector_of(registers, den_value, name)?.to_vec();
            let x = f64_of(registers, x_value, name)?;
            let value = emath_rt::control::transfer_eval(&num, &den, x).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::F64(value))
        }
        EmirOp::ControlDcGain(a_value, b_value, c_value) => {
            // State-space DC gain (zxkl thin B43): Faddeev–LeVerrier
            // characteristic polynomial + Routh–Hurwitz gate, then a
            // pivoted solve; typed refusals E-CONTROL-001..005.
            let (rows, cols, a_flat) = matrix_of(registers, a_value, name)?;
            let a = (0..rows)
                .map(|r| a_flat[r * cols..(r + 1) * cols].to_vec())
                .collect::<Vec<_>>();
            let b = vector_of(registers, b_value, name)?.to_vec();
            let c = vector_of(registers, c_value, name)?.to_vec();
            let gain = emath_rt::control::state_space_dc_gain(&a, &b, &c).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::F64(gain))
        }
        EmirOp::ControlPolesStable(den_value) => {
            // Routh–Hurwitz strict stability (zxkl thin B43); typed
            // refusals E-CONTROL-001/002/005.
            let den = vector_of(registers, den_value, name)?.to_vec();
            let stable = emath_rt::control::poles_stable(&den).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Bool(stable))
        }
        EmirOp::CategoryCheck(dom_value, cod_value, comp_value) => {
            // Finite-category law gate (88wo thin B39): certifies the
            // dense composition table; typed refusals E-CAT-001..007.
            let dom = vector_of(registers, dom_value, name)?.to_vec();
            let cod = vector_of(registers, cod_value, name)?.to_vec();
            let (rows, cols, comp_flat) = matrix_of(registers, comp_value, name)?;
            let comp = (0..rows)
                .map(|r| comp_flat[r * cols..(r + 1) * cols].to_vec())
                .collect::<Vec<_>>();
            let valid = emath_rt::category::category_check(&dom, &cod, &comp).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Bool(valid))
        }
        EmirOp::CategoryDiagramCommutative(dom_value, cod_value, comp_value, faces_value) => {
            // Diagram commutativity over face path-pairs (88wo thin
            // B39); the carrier certifies first, typed refusals
            // E-CAT-001..007.
            let dom = vector_of(registers, dom_value, name)?.to_vec();
            let cod = vector_of(registers, cod_value, name)?.to_vec();
            let (rows, cols, comp_flat) = matrix_of(registers, comp_value, name)?;
            let comp = (0..rows)
                .map(|r| comp_flat[r * cols..(r + 1) * cols].to_vec())
                .collect::<Vec<_>>();
            let faces = vector_of(registers, faces_value, name)?.to_vec();
            let mask = emath_rt::category::diagram_commutative(&dom, &cod, &comp, &faces).map_err(
                |error| EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                },
            )?;
            Ok(Value::Vector(
                mask.iter()
                    .map(|face| if *face { 1.0 } else { 0.0 })
                    .collect(),
            ))
        }
        EmirOp::ProbSample {
            kind,
            params: params_value,
            seed: seed_value,
            draws: draws_value,
            stream: stream_value,
        } => {
            // Seeded sampling from an admitted family (xx0x.5):
            // SplitMix64 stream; typed refusals E-PROB-001/002/003.
            let params = vector_of(registers, params_value, name)?.to_vec();
            let seed = f64_of(registers, seed_value, name)?;
            let draws = f64_of(registers, draws_value, name)?;
            let family = match kind {
                crate::ProbKind::Normal => emath_rt::probability::Family::Normal,
                crate::ProbKind::Uniform => emath_rt::probability::Family::Uniform,
                crate::ProbKind::Bernoulli => emath_rt::probability::Family::Bernoulli,
            };
            let stream_path = match stream_value {
                Some(value) => match register(registers, value)? {
                    Value::Text(path) => path.as_str(),
                    _ => {
                        return Err(EvalFault::TypeConfusion {
                            register: value.0,
                            op: name,
                        });
                    }
                },
                None => "",
            };
            let stream = emath_rt::probability::prob_sample_in_stream(
                family,
                &params,
                seed,
                draws,
                stream_path,
            )
            .map_err(|error| EvalFault::CapabilityRefused {
                capability: name.to_string(),
                code: error.code().to_string(),
            })?;
            Ok(Value::Vector(stream))
        }
        EmirOp::ProbDensity {
            kind,
            params: params_value,
            x: x_value,
        } => {
            // Exact density / PMF (xx0x.5): closed forms, not
            // estimates; same refusal surface as ProbSample.
            let params = vector_of(registers, params_value, name)?.to_vec();
            let x = f64_of(registers, x_value, name)?;
            let family = match kind {
                crate::ProbKind::Normal => emath_rt::probability::Family::Normal,
                crate::ProbKind::Uniform => emath_rt::probability::Family::Uniform,
                crate::ProbKind::Bernoulli => emath_rt::probability::Family::Bernoulli,
            };
            let density =
                emath_rt::probability::prob_density(family, &params, x).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::F64(density))
        }
        EmirOp::TensorCreate {
            ref shape,
            ref elements,
        } => {
            let expected = shape_product(shape).ok_or(EvalFault::Arithmetic {
                op: name,
                detail: "tensor size overflow",
            })?;
            if elements.len() != expected {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "tensor element count does not match shape product",
                });
            }
            let mut data = Vec::with_capacity(elements.len());
            for &elem in elements {
                data.push(f64_of(registers, elem, name)?);
            }
            Ok(Value::Tensor {
                shape: shape.clone(),
                data,
            })
        }
        EmirOp::TensorIndex {
            tensor,
            ref indices,
        } => {
            let (shape, data) = tensor_of(registers, tensor, name)?;
            if indices.len() != shape.len() {
                return Err(EvalFault::TypeConfusion {
                    register: tensor.0,
                    op: name,
                });
            }
            let mut raw = Vec::with_capacity(indices.len());
            for &index in indices {
                raw.push(f64_of(registers, index, name)?);
            }
            emath_rt::tensor_index_checked(shape, data, &raw)
                .map(Value::F64)
                .map_err(|err| map_index_error(name, err))
        }
        EmirOp::TensorSlice { tensor, ref axes } => {
            eval_tensor_slice(registers, tensor, axes, name)
        }
        EmirOp::TensorAdd(left, right) => {
            let (s1, d1) = tensor_of(registers, left, name)?;
            let (s2, d2) = tensor_of(registers, right, name)?;
            if s1 != s2 {
                return Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                });
            }
            Ok(Value::Tensor {
                shape: s1.to_vec(),
                data: emath_rt::tensor_add(d1, d2),
            })
        }
        EmirOp::TensorSub(left, right) => {
            let (s1, d1) = tensor_of(registers, left, name)?;
            let (s2, d2) = tensor_of(registers, right, name)?;
            if s1 != s2 {
                return Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                });
            }
            Ok(Value::Tensor {
                shape: s1.to_vec(),
                data: emath_rt::tensor_sub(d1, d2),
            })
        }
        EmirOp::TensorScale(left, right) => {
            match (register(registers, left)?, register(registers, right)?) {
                (Value::Tensor { shape, data }, Value::F64(scale))
                | (Value::F64(scale), Value::Tensor { shape, data }) => Ok(Value::Tensor {
                    shape: shape.clone(),
                    data: emath_rt::tensor_scale(data, *scale),
                }),
                _ => Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                }),
            }
        }
        EmirOp::Einsum {
            ref subscripts,
            ref inputs,
        } => eval_einsum(registers, subscripts, inputs, name),
        EmirOp::Factorial(n) => {
            let n = i64_of(registers, n, name)?;
            let result = emath_rt::factorial_checked(n)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::I64(result))
        }
        EmirOp::ModInv(a, m) => {
            let a = i64_of(registers, a, name)?;
            let m = i64_of(registers, m, name)?;
            let result = emath_rt::mod_inv_checked(a, m)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::I64(result))
        }
        EmirOp::IntRem(a, m) => {
            let a = i64_of(registers, a, name)?;
            let m = i64_of(registers, m, name)?;
            if m <= 0 {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "int-rem: modulus must be positive",
                });
            }
            // Exact Euclidean remainder; result is always non-negative and
            // in [0, m). rem_euclid(i64::MIN, -1) cannot overflow because a
            // positive modulus is enforced above (rem_euclid's docs warn the
            // -1 divisor case, which is unreachable here).
            Ok(Value::I64(a.rem_euclid(m)))
        }
        EmirOp::Congruence(a, b, m) => {
            let a = i64_of(registers, a, name)?;
            let b = i64_of(registers, b, name)?;
            let m = i64_of(registers, m, name)?;
            if m == 0 {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "cong: modulus must be non-zero",
                });
            }
            Ok(Value::Bool((a - b).rem_euclid(m) == 0))
        }
        EmirOp::PolyEvalMod(coeffs, x, p) => {
            let x = i64_of(registers, x, name)?;
            let p = i64_of(registers, p, name)?;
            let coeff_vec = match register(registers, coeffs)? {
                Value::Vector(data) => data,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: coeffs.0,
                        op: name,
                    });
                }
            };
            let result = emath_rt::poly_eval_mod_checked(coeff_vec, x, p)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::I64(result))
        }
        EmirOp::RSEncode(coeffs, n, p) => {
            let n = i64_of(registers, n, name)?;
            let p = i64_of(registers, p, name)?;
            let coeff_vec = match register(registers, coeffs)? {
                Value::Vector(data) => data.clone(),
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: coeffs.0,
                        op: name,
                    });
                }
            };
            let codeword = emath_rt::rs_encode_checked(&coeff_vec, n, p)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::Vector(codeword))
        }
        EmirOp::HammingDistance(a, b) => {
            let a_vec = match register(registers, a)? {
                Value::Vector(data) => data,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: a.0,
                        op: name,
                    });
                }
            };
            let b_vec = match register(registers, b)? {
                Value::Vector(data) => data,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: b.0,
                        op: name,
                    });
                }
            };
            let dist = emath_rt::hamming_distance_checked(a_vec, b_vec)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::I64(dist))
        }
        EmirOp::Fold {
            start,
            end,
            init,
            combine,
            loop_var_index,
            ref body,
        } => {
            // I64 bounds stay exact; F64 bounds must be finite whole numbers
            // (bare `as i64` maps NaN→0 and Inf→saturating extremes).
            let start_i = fold_bound(registers, start, name)?;
            let end_i = fold_bound(registers, end, name)?;
            match combine {
                FoldCombine::Add | FoldCombine::Mul => {
                    let mut acc_i: Option<i64> = match register(registers, init)? {
                        Value::I64(n) => Some(*n),
                        Value::F64(_) => None,
                        _ => {
                            return Err(EvalFault::TypeConfusion {
                                register: init.0,
                                op: name,
                            });
                        }
                    };
                    let mut acc_f: f64 = if acc_i.is_none() {
                        f64_of(registers, init, name)?
                    } else {
                        0.0
                    };
                    for i in start_i..end_i {
                        let mut body_inputs = inputs.to_vec();
                        let idx = usize::from(loop_var_index);
                        while body_inputs.len() <= idx {
                            body_inputs.push(Value::F64(0.0));
                        }
                        body_inputs[idx] = if acc_i.is_some() {
                            Value::I64(i)
                        } else {
                            Value::F64(i as f64)
                        };
                        match evaluate(body, &body_inputs, state)? {
                            Value::I64(term) => {
                                if let Some(ref mut acc) = acc_i {
                                    *acc = match combine {
                                        FoldCombine::Add => {
                                            acc.checked_add(term).ok_or(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "i64 overflow",
                                            })?
                                        }
                                        FoldCombine::Mul => {
                                            acc.checked_mul(term).ok_or(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "i64 overflow",
                                            })?
                                        }
                                        FoldCombine::And | FoldCombine::Or => {
                                            return Err(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "numeric fold got bool combine",
                                            });
                                        }
                                    };
                                } else {
                                    let term_f = term as f64;
                                    acc_f = match combine {
                                        FoldCombine::Add => acc_f + term_f,
                                        FoldCombine::Mul => acc_f * term_f,
                                        FoldCombine::And | FoldCombine::Or => {
                                            return Err(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "numeric fold got bool combine",
                                            });
                                        }
                                    };
                                }
                            }
                            Value::F64(term) => {
                                if let Some(acc) = acc_i.take() {
                                    acc_f = acc as f64;
                                }
                                acc_f = match combine {
                                    FoldCombine::Add => acc_f + term,
                                    FoldCombine::Mul => acc_f * term,
                                    FoldCombine::And | FoldCombine::Or => {
                                        return Err(EvalFault::Arithmetic {
                                            op: name,
                                            detail: "numeric fold got bool combine",
                                        });
                                    }
                                };
                            }
                            _ => {
                                return Err(EvalFault::TypeConfusion {
                                    register: body.result.0,
                                    op: name,
                                });
                            }
                        }
                    }
                    Ok(match acc_i {
                        Some(n) => Value::I64(n),
                        None => Value::F64(acc_f),
                    })
                }
                FoldCombine::And | FoldCombine::Or => {
                    // `bool_of` admits Bool and numeric 0/≠0; bare `f64_of`
                    // wrongly refused a Bool vacuous init for forall/exists.
                    let mut acc = bool_of(registers, init, name)?;
                    for i in start_i..end_i {
                        let mut body_inputs = inputs.to_vec();
                        let idx = usize::from(loop_var_index);
                        while body_inputs.len() <= idx {
                            body_inputs.push(Value::F64(0.0));
                        }
                        body_inputs[idx] = Value::F64(i as f64);
                        let term = match evaluate(body, &body_inputs, state)? {
                            Value::Bool(b) => b,
                            Value::F64(f) => f != 0.0,
                            _ => {
                                return Err(EvalFault::TypeConfusion {
                                    register: body.result.0,
                                    op: name,
                                });
                            }
                        };
                        acc = match combine {
                            FoldCombine::And => acc && term,
                            FoldCombine::Or => acc || term,
                            FoldCombine::Add | FoldCombine::Mul => {
                                return Err(EvalFault::Arithmetic {
                                    op: name,
                                    detail: "bool fold got numeric combine",
                                });
                            }
                        };
                    }
                    Ok(Value::Bool(acc))
                }
            }
        }
        EmirOp::Integral {
            start,
            end,
            steps,
            loop_var_index,
            ref integrand,
        } => {
            // Composite Simpson requires a positive even panel count; steps==0
            // is `/ 0.0` → Inf, and odd n is a silently wrong quadrature.
            if steps == 0 || steps % 2 != 0 {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "integral steps must be positive and even",
                });
            }
            let a = f64_of(registers, start, name)?;
            let b = f64_of(registers, end, name)?;
            let n = i64::from(steps);
            let h = (b - a) / n as f64;
            let mut acc = 0.0;
            for i in 0..=n {
                let x = a + i as f64 * h;
                let weight = if i == 0 || i == n {
                    1.0
                } else if i % 2 == 0 {
                    2.0
                } else {
                    4.0
                };
                let mut body_inputs = inputs.to_vec();
                let idx = usize::from(loop_var_index);
                while body_inputs.len() <= idx {
                    body_inputs.push(Value::F64(0.0));
                }
                body_inputs[idx] = Value::F64(x);
                match evaluate(integrand, &body_inputs, state)? {
                    Value::F64(fx) => acc += weight * fx,
                    _ => {
                        return Err(EvalFault::TypeConfusion {
                            register: integrand.result.0,
                            op: name,
                        });
                    }
                }
            }
            Ok(Value::F64(acc * h / 3.0))
        }
        EmirOp::Differentiate {
            ref body,
            var_index,
        } => {
            let dual = evaluate_dual(body, inputs, state, var_index, name)?;
            Ok(Value::F64(dual.tangent))
        }
        EmirOp::Solve {
            ref body,
            var_index,
            tolerance,
            max_iter,
        } => {
            // Newton's method: x_new = x_old - f(x) / f'(x)
            // Uses dual-number evaluation for both f and f' in one
            // pass. When Newton is unreliable — the derivative
            // vanishes, or the residual/step becomes non-finite — the
            // solver falls back to a deterministic bracket scan around
            // the seed followed by bisection (emath-9bj1, Track A3).
            // The fallback only reports a root whose residual is below
            // tolerance; no bracket (or a divergent bisection) still
            // refuses with a typed fault — never a hang, never a
            // silently invented root.
            let mut x = match inputs.get(var_index as usize).and_then(Value::as_real_f64) {
                Some(v) => v,
                None => {
                    return Err(EvalFault::TypeConfusion {
                        register: var_index as u32,
                        op: name,
                    });
                }
            };
            let seed = x;
            let mut work_inputs = inputs.to_vec();
            let mut unreliable = None;
            for _ in 0..max_iter {
                work_inputs[var_index as usize] = Value::F64(x);
                let dual = evaluate_dual(body, &work_inputs, state, var_index, name)?;
                let f = dual.primal;
                let df = dual.tangent;
                if f.abs() < tolerance {
                    return Ok(Value::F64(x));
                }
                // A vanished derivative is not convergence: Newton
                // cannot step, so returning `x` would silently invent
                // a root — fall back to bisection instead.
                if df.abs() < 1e-30 {
                    unreliable = Some("derivative vanished");
                    break;
                }
                if !f.is_finite() || !df.is_finite() {
                    unreliable = Some("nonfinite value");
                    break;
                }
                x -= f / df;
                if !x.is_finite() {
                    unreliable = Some("nonfinite step");
                    break;
                }
            }
            if let Some(reason) = unreliable {
                return match solve_bracket_fallback(
                    body,
                    &work_inputs,
                    state,
                    var_index,
                    seed,
                    tolerance,
                    name,
                )? {
                    Some(root) => Ok(Value::F64(root)),
                    None => Err(EvalFault::Arithmetic {
                        op: name,
                        detail: match reason {
                            "derivative vanished" => {
                                "solve derivative vanished before convergence"
                            }
                            _ => {
                                "solve produced a nonfinite value and found no sign-changing bracket in the deterministic scan"
                            }
                        },
                    }),
                };
            }
            // Accept a root landed by the final Newton update; otherwise
            // refuse rather than invent one (same rule as causal_newton).
            work_inputs[var_index as usize] = Value::F64(x);
            let dual = evaluate_dual(body, &work_inputs, state, var_index, name)?;
            if dual.primal.abs() < tolerance {
                return Ok(Value::F64(x));
            }
            Err(EvalFault::Arithmetic {
                op: name,
                detail: "solve did not converge within max_iter",
            })
        }
        EmirOp::Optimize {
            ref body,
            ref var_indices,
            maximize,
            learning_rate: _,
            tolerance,
            max_iter,
        } => {
            // Newton's method on ∇f = 0. A claimed min/max must be a
            // stationary point: x -= H^{-1} ∇f, with H from a
            // forward-difference of the dual gradient. Fixed-step
            // gradient descent with a small penalty weight could stop
            // at a point that was neither stationary for f nor feasible.
            if var_indices.is_empty() {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "optimize requires at least one variable",
                });
            }
            let mut work_inputs = inputs.to_vec();
            let mut x: Vec<f64> = Vec::with_capacity(var_indices.len());
            for &vi in var_indices {
                match inputs.get(vi as usize).and_then(Value::as_real_f64) {
                    Some(v) => x.push(v),
                    None => {
                        return Err(if inputs.get(vi as usize).is_none() {
                            EvalFault::MissingInput(vi)
                        } else {
                            EvalFault::TypeConfusion {
                                register: u32::from(vi),
                                op: name,
                            }
                        });
                    }
                }
            }
            const FD_EPS: f64 = 1e-8;
            for _ in 0..max_iter {
                let grads = optimize_grads(body, &mut work_inputs, state, var_indices, &x, name)?;
                let max_grad = grads.iter().fold(0.0_f64, |acc, g| acc.max(g.abs()));
                if max_grad < tolerance {
                    return Ok(Value::F64(x[0]));
                }
                let n = x.len();
                let mut hess = vec![vec![0.0_f64; n]; n];
                for j in 0..n {
                    x[j] += FD_EPS;
                    let perturbed =
                        optimize_grads(body, &mut work_inputs, state, var_indices, &x, name)?;
                    x[j] -= FD_EPS;
                    for i in 0..n {
                        hess[i][j] = (perturbed[i] - grads[i]) / FD_EPS;
                    }
                }
                let delta = dense_solve(&hess, &grads).map_err(|_| EvalFault::Arithmetic {
                    op: name,
                    detail: "optimize hessian vanished before stationarity",
                })?;
                let dot: f64 = grads.iter().zip(delta.iter()).map(|(g, d)| g * d).sum();
                // Newton on ∇f = 0 finds any stationary point. Refuse a
                // min returned as a max (or vice versa): g·(H^{-1}g) is
                // positive iff H is positive definite along g.
                if maximize {
                    if dot >= 0.0 {
                        return Err(EvalFault::Arithmetic {
                            op: name,
                            detail: "optimize hessian has the wrong curvature for maximize",
                        });
                    }
                } else if dot <= 0.0 {
                    return Err(EvalFault::Arithmetic {
                        op: name,
                        detail: "optimize hessian has the wrong curvature for minimize",
                    });
                }
                for (xi, d) in x.iter_mut().zip(delta.iter()) {
                    *xi -= d;
                }
            }
            let grads = optimize_grads(body, &mut work_inputs, state, var_indices, &x, name)?;
            let max_grad = grads.iter().fold(0.0_f64, |acc, g| acc.max(g.abs()));
            if max_grad < tolerance {
                return Ok(Value::F64(x[0]));
            }
            Err(EvalFault::Arithmetic {
                op: name,
                detail: "optimize did not converge within max_iter",
            })
        }
        EmirOp::SampleLimit {
            ref body,
            var_index,
            target,
            direction,
        } => {
            // Numerical limit approximation: sample the body at points
            // approaching the target along a geometric sequence of step
            // sizes (0.1, 0.01, ..., 1e-12). Return the last finite value
            // whose predecessor was also finite and within 1% of it
            // (convergence check). If no pair converges, return the last
            // finite sample.
            let target_val = match registers
                .get(target.0 as usize)
                .and_then(Value::as_real_f64)
            {
                Some(v) => v,
                None => {
                    return Err(EvalFault::TypeConfusion {
                        register: target.0,
                        op: name,
                    });
                }
            };
            let dir_val = match registers
                .get(direction.0 as usize)
                .and_then(Value::as_real_f64)
            {
                Some(v) => v,
                None => {
                    return Err(EvalFault::TypeConfusion {
                        register: direction.0,
                        op: name,
                    });
                }
            };
            let mut work_inputs = inputs.to_vec();
            while work_inputs.len() <= var_index as usize {
                work_inputs.push(Value::F64(0.0));
            }
            let directions: &[f64] = match dir_val as i64 {
                0 => &[1.0, -1.0], // two-sided
                1 => &[1.0],       // from above
                -1 => &[-1.0],     // from below
                _ => &[1.0, -1.0], // fallback: two-sided
            };
            let mut best = f64::NAN;
            let mut prev = f64::NAN;
            for step_exp in 1..=12u32 {
                let h = 10f64.powi(-(step_exp as i32));
                for &d in directions {
                    let x = target_val + d * h;
                    work_inputs[var_index as usize] = Value::F64(x);
                    match evaluate(body, &work_inputs, state) {
                        Ok(val) => {
                            if let Some(fx) = val.as_real_f64() {
                                if fx.is_finite() {
                                    if prev.is_finite()
                                        && (fx - prev).abs() <= fx.abs() * 0.01 + 1e-14
                                    {
                                        // Converged: successive samples agree to 1%.
                                        return Ok(Value::F64(fx));
                                    }
                                    prev = fx;
                                    best = fx;
                                }
                            }
                        }
                        _ => {} // non-finite or wrong type: skip
                    }
                }
            }
            if best.is_finite() {
                Ok(Value::F64(best))
            } else {
                Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "sample_limit produced no finite values",
                })
            }
        }
        EmirOp::ReverseMode {
            ref body,
            ref var_indices,
        } => evaluate_reverse(body, inputs, state, var_indices, name),
    }
}

fn optimize_grads(
    body: &EmirProgram,
    work_inputs: &mut [Value],
    state: &[Value],
    var_indices: &[u16],
    x: &[f64],
    name: &'static str,
) -> Result<Vec<f64>, EvalFault> {
    for (i, &vi) in var_indices.iter().enumerate() {
        work_inputs[vi as usize] = Value::F64(x[i]);
    }
    let mut grads = Vec::with_capacity(var_indices.len());
    for &vi in var_indices {
        let dual = evaluate_dual(body, work_inputs, state, vi, name)?;
        grads.push(dual.tangent);
    }
    Ok(grads)
}

/// Solve `matrix * x = rhs` by Gaussian elimination with partial pivoting.
fn dense_solve(matrix: &[Vec<f64>], rhs: &[f64]) -> Result<Vec<f64>, ()> {
    let n = rhs.len();
    if matrix.len() != n || matrix.iter().any(|row| row.len() != n) {
        return Err(());
    }
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut a = matrix.to_vec();
    let mut b = rhs.to_vec();
    for col in 0..n {
        let mut pivot = col;
        let mut best = a[col][col].abs();
        for row in (col + 1)..n {
            let candidate = a[row][col].abs();
            if candidate > best {
                best = candidate;
                pivot = row;
            }
        }
        if best < 1e-30 {
            return Err(());
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in (col + 1)..n {
            let factor = a[row][col] / a[col][col];
            if factor == 0.0 {
                continue;
            }
            for k in col..n {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut acc = b[row];
        for k in (row + 1)..n {
            acc -= a[row][k] * x[k];
        }
        x[row] = acc / a[row][row];
    }
    Ok(x)
}

fn eval_complex_unary(
    id: BuiltinId,
    re: f64,
    im: f64,
    register: u32,
    op: &'static str,
) -> Result<Value, EvalFault> {
    match id {
        BuiltinId::Sqrt => {
            let (out_re, out_im) = emath_rt::complex_sqrt(re, im);
            Ok(Value::Complex {
                re: out_re,
                im: out_im,
            })
        }
        BuiltinId::Ln => {
            let (out_re, out_im) = emath_rt::complex_ln(re, im);
            Ok(Value::Complex {
                re: out_re,
                im: out_im,
            })
        }
        BuiltinId::Exp => {
            let (out_re, out_im) = emath_rt::complex_exp(re, im);
            Ok(Value::Complex {
                re: out_re,
                im: out_im,
            })
        }
        BuiltinId::Log10 => {
            let (out_re, out_im) = emath_rt::complex_ln(re, im);
            let scale = std::f64::consts::LN_10;
            Ok(Value::Complex {
                re: out_re / scale,
                im: out_im / scale,
            })
        }
        BuiltinId::Log2 => {
            let (out_re, out_im) = emath_rt::complex_ln(re, im);
            let scale = std::f64::consts::LN_2;
            Ok(Value::Complex {
                re: out_re / scale,
                im: out_im / scale,
            })
        }
        BuiltinId::Abs => Ok(Value::F64(re.hypot(im))),
        BuiltinId::Recip => {
            let denom = re * re + im * im;
            Ok(Value::Complex {
                re: re / denom,
                im: -im / denom,
            })
        }
        _ => Err(EvalFault::TypeConfusion { register, op }),
    }
}

// --- Helper functions extracted to interp/helpers.rs ---
// --- Dual-number autodiff subsystem extracted to interp/dual.rs ---

// Extended GCD moved to crates/emath-rt/src/body.rs (mod_inv_checked).
