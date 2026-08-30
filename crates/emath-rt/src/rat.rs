//! Exact-rational kernel (`Rat`) — the single home for exact rational
//! arithmetic on an `i128` carrier. Follows the gcd-reduced / positive-denominator
//! conventions of `body.rs`'s rational matrix and `emath-core`'s `geometry::Rational`
//! (i64), generalized to `i128` num/den.
//!
//! Laws:
//! - canonical form: `den > 0`, `gcd(|num|, den) == 1`, zero is `0/1`;
//! - every operation is checked; overflow and zero denominators are typed
//!   errors, never silent wraps and never panics;
//! - NO float conversion methods — the no-hidden-float bead law starts here.

/// An exact rational number with `i128` num/den, always canonical:
/// `den > 0` and `gcd(|num|, den) == 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rat {
    num: i128,
    den: i128,
}

/// Kernel-level error for exact-rational arithmetic. The cell layer maps
/// these to `E-CELL-*` codes later; this module deliberately does not
/// reference error codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RatError {
    /// A denominator was zero (division by a zero-valued rational, or
    /// explicit construction with `den == 0`).
    ZeroDenominator,
    /// A checked intermediate product/sum overflowed `i128`.
    Overflow,
}

impl std::fmt::Display for RatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RatError::ZeroDenominator => {
                write!(f, "rational operation violated precondition: denominator must be nonzero")
            }
            RatError::Overflow => {
                write!(f, "rational operation violated precondition: intermediate value overflows i128")
            }
        }
    }
}

impl std::error::Error for RatError {}

impl Rat {
    /// Exact constructor. `den == 0` is a typed error, never a panic.
    pub fn new(num: i128, den: i128) -> Result<Self, RatError> {
        if den == 0 {
            return Err(RatError::ZeroDenominator);
        }
        let mut rat = Rat { num, den };
        rat.canonicalize();
        Ok(rat)
    }

    /// Numerator (canonical: `den > 0`, `gcd(|num|, den) == 1`).
    pub fn num(self) -> i128 {
        self.num
    }

    /// Denominator (canonical: strictly positive).
    pub fn den(self) -> i128 {
        self.den
    }

    /// Exact negation; `i128::MIN` numerator overflows and is refused.
    pub fn neg(self) -> Result<Self, RatError> {
        Rat::new(self.num.checked_neg().ok_or(RatError::Overflow)?, self.den)
    }

    /// `self + other`, all intermediates checked.
    pub fn add(self, other: Self) -> Result<Self, RatError> {
        let ad = self.num.checked_mul(other.den).ok_or(RatError::Overflow)?;
        let cb = other.num.checked_mul(self.den).ok_or(RatError::Overflow)?;
        let bd = self.den.checked_mul(other.den).ok_or(RatError::Overflow)?;
        let num = ad.checked_add(cb).ok_or(RatError::Overflow)?;
        Rat::new(num, bd)
    }

    /// `self - other`, all intermediates checked.
    pub fn sub(self, other: Self) -> Result<Self, RatError> {
        self.add(other.neg()?)
    }

    /// `self * other`, all intermediates checked.
    pub fn mul(self, other: Self) -> Result<Self, RatError> {
        let num = self.num.checked_mul(other.num).ok_or(RatError::Overflow)?;
        let den = self.den.checked_mul(other.den).ok_or(RatError::Overflow)?;
        Rat::new(num, den)
    }

    /// `self / other`. Dividing by a zero-valued rational is a typed error.
    pub fn div(self, other: Self) -> Result<Self, RatError> {
        if other.num == 0 {
            return Err(RatError::ZeroDenominator);
        }
        let num = self.num.checked_mul(other.den).ok_or(RatError::Overflow)?;
        let den = self.den.checked_mul(other.num).ok_or(RatError::Overflow)?;
        Rat::new(num, den)
    }

    /// In-place reduction to canonical form: sign-normalize the denominator,
    /// then divide out `gcd(|num|, den)`; zero collapses to `0/1`.
    fn canonicalize(&mut self) {
        if self.den < 0 {
            self.num = match self.num.checked_neg() {
                Some(n) => n,
                None => return, // unreachable via `new`; `new` re-checks below
            };
            self.den = match self.den.checked_neg() {
                Some(d) => d,
                None => return,
            };
        }
        let g = gcd(self.num.unsigned_abs(), self.den.unsigned_abs());
        if g > 1 {
            self.num /= g as i128;
            self.den /= g as i128;
        }
        if self.num == 0 {
            self.den = 1;
        }
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}
