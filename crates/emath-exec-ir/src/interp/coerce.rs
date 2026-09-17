use super::*;

pub(super) fn value_as_f64(value: &Value, op: &'static str) -> Result<f64, EvalFault> {
    match value {
        Value::F64(value) => Ok(*value),
        Value::I64(value) => Ok(*value as f64),
        _ => Err(EvalFault::Arithmetic {
            op,
            detail: "fold carrier mismatch",
        }),
    }
}

pub(super) fn value_as_bool(value: &Value, op: &'static str) -> Result<bool, EvalFault> {
    match value {
        Value::Bool(value) => Ok(*value),
        _ => Err(EvalFault::Arithmetic {
            op,
            detail: "fold carrier mismatch",
        }),
    }
}

pub(super) fn format_text(template: &str, arguments: &[Value]) -> String {
    use std::fmt::Write;

    let mut output = String::with_capacity(template.len());
    let mut remaining = template;
    for value in arguments {
        let Some((prefix, suffix)) = remaining.split_once("{}") else {
            break;
        };
        output.push_str(prefix);
        write!(output, "{value}").expect("writing to a String cannot fail");
        remaining = suffix;
    }
    output.push_str(remaining);
    output
}

pub(super) fn sample_series(
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

pub(super) fn exact_int_of(value: &Value, op: &'static str) -> Result<emath_rt::ExactInt, EvalFault> {
    match value {
        Value::I64(n) => Ok(emath_rt::ExactInt::from(*n)),
        Value::ExactInt(n) => Ok(n.clone()),
        _ => Err(EvalFault::TypeConfusion {
            register: 0,
            op,
        }),
    }
}

pub(super) fn rat_pair(value: &Value) -> Option<(emath_rt::ExactInt, emath_rt::ExactInt)> {
    match value {
        Value::I64(n) => Some((emath_rt::ExactInt::from(*n), emath_rt::ExactInt::one())),
        Value::ExactInt(n) => Some((n.clone(), emath_rt::ExactInt::one())),
        Value::Rat { num, den } => Some((
            emath_rt::ExactInt::from(*num),
            emath_rt::ExactInt::from(*den),
        )),
        Value::ExactRat { num, den } => Some((num.clone(), den.clone())),
        _ => None,
    }
}

pub(super) fn value_from_exact(value: emath_rt::ExactInt) -> Value {
    match value.to_i64() {
        Some(n) => Value::I64(n),
        None => Value::ExactInt(value),
    }
}

pub(super) fn value_from_rat(num: emath_rt::ExactInt, den: emath_rt::ExactInt) -> Value {
    match (num.to_i128(), den.to_i128()) {
        (Some(num), Some(den)) => Value::Rat { num, den },
        _ => Value::ExactRat { num, den },
    }
}

