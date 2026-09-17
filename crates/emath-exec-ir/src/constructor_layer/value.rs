use super::*;
use super::prelude::*;

impl CValue {
    pub(super) fn canon(self) -> Result<Self, ConstructorError> {
        match self {
            Self::Rat { num, den } => {
                if den.is_zero() {
                    return Err(fault("division_by_zero", "exact division by zero"));
                }
                let num = if den.is_negative() {
                    num.checked_neg().map_err(exact_fault)?
                } else {
                    num
                };
                let den = den.abs();
                let g = ExactInt::gcd(&num.abs(), &den).map_err(exact_fault)?;
                Ok(Self::Rat {
                    num: num.quot(&g).map_err(exact_fault)?,
                    den: den.quot(&g).map_err(exact_fault)?,
                })
            }
            other => Ok(other),
        }
    }
}

pub(super) fn representation_of(value: &CValue) -> String {
    match value {
        CValue::Int(_) | CValue::Rat { .. } | CValue::Bool(_) => "exact_scalar".into(),
        CValue::Float64(_) => "rounded_scalar".into(),
        CValue::Code(_) => "code".into(),
        CValue::Absent => "absent".into(),
        _ => "structured".into(),
    }
}

