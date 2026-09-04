//! B44 nucleus — dual numbers over f64 .
//!
//! The carrier rule is ε² = 0 EXACTLY (truncated to first order by the
//! algebra, not by a tolerance): arithmetic on `(value, epsilon)` pairs
//! propagates exact first-order derivatives — no finite-difference
//! error, no step-size choice. This is forward-mode AD's algebraic
//! core in two floats; the production `grad()` builtin (Wengert
//! tape) serves many-input losses, while `Dual` serves the
//! one-input exact-tangent story and the ε²=0 teaching example.
//!
//! Division: `a+bε / c+dε = (a/c) + (b·c − a·d)/c² ε` — division by a
//! dual with zero real part refuses (1/0 is not a dual number).

use std::ops::{Add, Div, Mul, Neg, Sub};

/// A dual number `value + epsilon·ε` with ε² = 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dual {
    pub value: f64,
    pub epsilon: f64,
}

impl Dual {
    /// Construct `value + epsilon·ε`.
    #[must_use]
    pub fn new(value: f64, epsilon: f64) -> Dual {
        Dual { value, epsilon }
    }

    /// A constant (zero ε-part): constants differentiate to 0.
    #[must_use]
    pub fn constant(value: f64) -> Dual {
        Dual {
            value,
            epsilon: 0.0,
        }
    }

    /// The variable `x` itself (ε-part 1): `new(x, 1)` is THE spelling
    /// for "differentiate by this input".
    #[must_use]
    pub fn variable(value: f64) -> Dual {
        Dual {
            value,
            epsilon: 1.0,
        }
    }
}

impl Add for Dual {
    type Output = Dual;
    fn add(self, rhs: Dual) -> Dual {
        Dual::new(self.value + rhs.value, self.epsilon + rhs.epsilon)
    }
}

impl Sub for Dual {
    type Output = Dual;
    fn sub(self, rhs: Dual) -> Dual {
        Dual::new(self.value - rhs.value, self.epsilon - rhs.epsilon)
    }
}

impl Neg for Dual {
    type Output = Dual;
    fn neg(self) -> Dual {
        Dual::new(-self.value, -self.epsilon)
    }
}

impl Mul for Dual {
    type Output = Dual;
    /// (a+bε)(c+dε) = ac + (ad + bc)ε + bdε² → ac + (ad+bc)ε.
    fn mul(self, rhs: Dual) -> Dual {
        Dual::new(
            self.value * rhs.value,
            self.value * rhs.epsilon + self.epsilon * rhs.value,
        )
    }
}

impl Div for Dual {
    type Output = Dual;
    /// (a+bε)/(c+dε) = (a/c) + ((bc − ad)/c²)ε; zero real part refuses.
    fn div(self, rhs: Dual) -> Dual {
        if rhs.value == 0.0 {
            // Refuse by f64 NaN policy would LAUNDER; the arithmetic
            // identity has no dual answer, so the result carries the
            // fault visibly: division by zero-real-part yields NaN in
            // the value channel (callers needing the typed seam use
            // `Dual::checked_div`).
            let value = self.value / rhs.value;
            let epsilon =
                (self.epsilon * rhs.value - self.value * rhs.epsilon) / (rhs.value * rhs.value);
            return Dual::new(value, epsilon);
        }
        Dual::new(
            self.value / rhs.value,
            (self.epsilon * rhs.value - self.value * rhs.epsilon) / (rhs.value * rhs.value),
        )
    }
}

impl Dual {
    /// Typed division: `Err` when the divisor's real part is zero
    /// (the operator spelling exists for expression ergonomics; this
    /// is the refusal seam).
    pub fn checked_div(self, rhs: Dual) -> Result<Dual, String> {
        if rhs.value == 0.0 {
            return Err("dual division refuses a zero real part \
                        (1/0 is not a dual number)"
                .into());
        }
        Ok(self / rhs)
    }
}
