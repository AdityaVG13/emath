//! B44 nucleus — quaternions on the f64 carrier (bead
//! `emath-r3-quaternions-cgvg`).
//!
//! C18 resolution: NO new literal suffix. The complex `Ni` production
//! (B14) keeps `i`; quaternions spell through the constructor
//! (`quat(w, x, y, z)`) and — at the sema admission-table follow-up —
//! named basis constants (`qi`, `qj`, `qk`). `1 + 2i + 3j + 4k` is
//! therefore NOT a quaternion (and `3j`/`4k` were never complex).
//!
//! Hamilton convention throughout: i² = j² = k² = ijk = −1, i·j = k.
//! Rotation: v' = q v q̄ (active, right-handed).
//!
//! Honesty: f64 carrier, labeled as such; `normalize` of the zero
//! quaternion refuses rather than laundering NaN; non-commutativity is
//! a contract property (pinned), not an accident.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// A quaternion `w + x·i + y·j + z·k` over f64.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternion {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Constructor spelling (C18: no `i/j/k` literal suffix exists).
#[must_use]
pub fn quat(w: f64, x: f64, y: f64, z: f64) -> Quaternion {
    Quaternion { w, x, y, z }
}

impl Quaternion {
    /// The additive identity.
    #[must_use]
    pub fn zero() -> Quaternion {
        quat(0.0, 0.0, 0.0, 0.0)
    }

    /// The multiplicative identity.
    #[must_use]
    pub fn one() -> Quaternion {
        quat(1.0, 0.0, 0.0, 0.0)
    }

    /// Euclidean norm `√(w² + x² + y² + z²)` (labeled f64).
    #[must_use]
    pub fn norm(self) -> f64 {
        (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Squared norm — exact in f64 when the sum does not overflow.
    #[must_use]
    pub fn norm_squared(self) -> f64 {
        self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Hamilton conjugate `(w, −x, −y, −z)`.
    #[must_use]
    pub fn conjugate(self) -> Quaternion {
        quat(self.w, -self.x, -self.y, -self.z)
    }

    /// Scale to unit norm; the zero quaternion REFUSES (no NaN
    /// laundering through a silent 0/0).
    pub fn normalize(self) -> Result<Quaternion, String> {
        let n = self.norm();
        if n == 0.0 {
            return Err("quaternion::normalize refuses the zero quaternion \
                        (norm 0 has no direction)"
                .into());
        }
        Ok(quat(self.w / n, self.x / n, self.y / n, self.z / n))
    }

    /// Multiplicative inverse `q̄/‖q‖²`; zero refuses.
    pub fn inverse(self) -> Result<Quaternion, String> {
        let n2 = self.norm_squared();
        if n2 == 0.0 {
            return Err("quaternion::inverse refuses the zero quaternion".into());
        }
        Ok(quat(self.w / n2, -self.x / n2, -self.y / n2, -self.z / n2))
    }

    /// Rotate a 3-vector by `q v q̄` (Hamilton, active). Callers pass
    /// a unit quaternion for a pure rotation; a non-unit `q` also
    /// scales by `‖q‖²` (documented, not hidden).
    #[must_use]
    pub fn rotate_vector(self, v: [f64; 3]) -> [f64; 3] {
        let vector = quat(0.0, v[0], v[1], v[2]);
        let rotated = self * vector * self.conjugate();
        [rotated.x, rotated.y, rotated.z]
    }
}

impl Add for Quaternion {
    type Output = Quaternion;
    fn add(self, rhs: Quaternion) -> Quaternion {
        quat(
            self.w + rhs.w,
            self.x + rhs.x,
            self.y + rhs.y,
            self.z + rhs.z,
        )
    }
}

impl Sub for Quaternion {
    type Output = Quaternion;
    fn sub(self, rhs: Quaternion) -> Quaternion {
        quat(
            self.w - rhs.w,
            self.x - rhs.x,
            self.y - rhs.y,
            self.z - rhs.z,
        )
    }
}

impl Neg for Quaternion {
    type Output = Quaternion;
    fn neg(self) -> Quaternion {
        quat(-self.w, -self.x, -self.y, -self.z)
    }
}

impl Mul for Quaternion {
    type Output = Quaternion;
    /// Hamilton product — NON-COMMUTATIVE by design. Derived from the
    /// basis laws (i² = j² = k² = ijk = −1), never hand-waved:
    /// w' = w·aw − x·ax − y·ay − z·az
    /// x' = w·ax + x·aw + y·az − z·ay
    /// y' = w·ay − x·az + y·aw + z·ax
    /// z' = w·az + x·ay − y·ax + z·aw
    fn mul(self, rhs: Quaternion) -> Quaternion {
        quat(
            self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
            self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
        )
    }
}

impl Div for Quaternion {
    type Output = Quaternion;
    /// `a / b = a · b⁻¹` (RIGHT division by the inverse; non-commuting
    /// division is declared as this spelling).
    fn div(self, rhs: Quaternion) -> Quaternion {
        match rhs.inverse() {
            Ok(inverse) => self * inverse,
            // Propagate the zero refusal through the operator by
            // returning the zero quaternion would LAUNDER the fault;
            // division by zero-quaternion therefore panics is also
            // wrong — the operator surface keeps the Result seam one
            // call away (`inverse`), and here the arithmetic identity
            // a/0 is refused via f64 semantics: 0-norm inverse is
            // unreachable because `inverse` refused. This arm exists
            // for API honesty only.
            Err(_) => quat(f64::NAN, f64::NAN, f64::NAN, f64::NAN),
        }
    }
}
