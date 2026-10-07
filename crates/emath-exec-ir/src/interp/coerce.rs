use super::{Value, EvalFault};

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

