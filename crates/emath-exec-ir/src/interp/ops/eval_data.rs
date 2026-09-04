//! Data-shaped op evaluation: consts, text, reports, series, sets, records, rationals, inputs, capability, intervals.

use super::*;

pub(super) fn eval_data_op(
    op: &EmirOp,
    registers: &[Value],
    inputs: &[Value],
    state: &[Value],
    name: &'static str,
) -> Result<Value, EvalFault> {
    match *op {
        EmirOp::ConstF64(bits) => Ok(Value::F64(f64::from_bits(bits))),
        EmirOp::ConstI64(value) => Ok(Value::I64(value)),
        // Canonical digits are an emitter invariant; a parse failure is
        // an internal fault, never a silent zero.
        EmirOp::ConstBigInt(ref digits) => Ok(Value::BigInt(
            emath_rt::UBig::parse_decimal(digits.as_str()).map_err(|detail| {
                EvalFault::Arithmetic {
                    op: "const-bigint",
                    detail,
                }
            })?,
        )),
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
        EmirOp::RatConstruct { num, den } => {
            let num = i128_of(registers, num, name)?;
            let den = i128_of(registers, den, name)?;
            rat_canonicalize(num, den, name)
        }
        EmirOp::RatAdd(left, right) => {
            let (left_num, left_den) = rat_parts(registers, left, name)?;
            let (right_num, right_den) = rat_parts(registers, right, name)?;
            // a/b + c/d = (a*d + c*b) / (b*d); every intermediate is
            // checked — overflow is a typed refusal, never a silent wrap.
            let num = left_num
                .checked_mul(right_den)
                .and_then(|left_term| {
                    right_num
                        .checked_mul(left_den)
                        .and_then(|right_term| left_term.checked_add(right_term))
                })
                .ok_or(EvalFault::Arithmetic {
                    op: name,
                    detail: "rational addition overflow (i128)",
                })?;
            let den = left_den
                .checked_mul(right_den)
                .ok_or(EvalFault::Arithmetic {
                    op: name,
                    detail: "rational addition overflow (i128)",
                })?;
            rat_canonicalize(num, den, name)
        }
        EmirOp::RatNorm(value) => {
            let (num, den) = rat_parts(registers, value, name)?;
            rat_canonicalize(num, den, name)
        }
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
        // The program artifact is an ordinary value: evaluating the
        // literal produces the carrier; it interprets nothing itself.
        EmirOp::ProgramLiteral(ref program) => Ok(Value::Program(program.clone())),
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
        _ => unreachable!("eval_data_op routed a non-matching op"),
    }
}
