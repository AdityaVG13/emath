//! core::geometry — geometry types over declared fields (Phase 12,
//! bead 36a9) — honest thin slice.
//!
//! A geometry lives over a DECLARED scalar field ([`Field`]): the
//! exact-rational field ([`Rational`]) or the float field (`f64`),
//! and the field is part of the type. The coordinate-free vs
//! coordinate-bound distinction is structural:
//!
//! - [`Point`] is coordinate-BOUND: transforms apply the full affine
//!   map (linear part + translation).
//! - [`FreeVector`] is coordinate-FREE: transforms apply ONLY the
//!   linear part — a pure translation must not move a free vector.
//!   `dot`/`cross` exist only on free vectors; point + point addition
//!   has no impl anywhere in the type system (compile-time negative
//!   control — see the cell contract).
//!
//! Exact-rational reference ops produce EXACT results: no epsilon, no
//! drift (the 3-4-5 unit-circle witness is checked bit-exactly).
//! Overflow, zero division, and non-finite float results refuse typed
//! (`E-GEOMETRY-*`) instead of fabricating a value.
//!
//! G4 honored: no `×`/`·` parser defaults here — operator overloads
//! route through notation packs (BLOCKED follow-up on the notation
//! lanes); this slice exposes named methods only. No `.emath` surface
//! change: the language still refuses `Rat` as a compute type, so the
//! exact field lives inside the Rust std layer until a language
//! decision lands.

/// Typed refusal: degenerate geometry (coincident points, zero conic).
pub const E_GEOMETRY_DEGENERATE: &str = "E-GEOMETRY-1";
/// Typed refusal: exact-rational arithmetic left the i64 envelope.
pub const E_GEOMETRY_OVERFLOW: &str = "E-GEOMETRY-2";
/// Typed refusal: a float computation left the finite domain.
pub const E_GEOMETRY_NONFINITE: &str = "E-GEOMETRY-3";
/// Typed refusal: division by a zero element.
pub const E_GEOMETRY_ZERO_DIV: &str = "E-GEOMETRY-4";

/// A declared scalar field: the arithmetic vocabulary geometry is
/// written against. Every operation is fallible — a field never
/// silently wraps, saturates, or hands back a fabricated non-finite
/// value.
pub trait Field: Clone + PartialEq + std::fmt::Debug {
    /// The additive identity.
    fn zero() -> Self;
    /// The multiplicative identity.
    fn one() -> Self;
    /// The additive inverse of one (exact for every admitted field).
    fn minus_one() -> Self;
    /// Embed an exact integer.
    fn from_i64(value: i64) -> Self;
    /// Exact (or checked) addition.
    fn add(&self, other: &Self) -> Result<Self, String>;
    /// Exact (or checked) subtraction.
    fn sub(&self, other: &Self) -> Result<Self, String>;
    /// Exact (or checked) multiplication.
    fn mul(&self, other: &Self) -> Result<Self, String>;
    /// Division; zero divisors refuse typed.
    fn div(&self, other: &Self) -> Result<Self, String>;
    /// Negation.
    fn neg(&self) -> Result<Self, String>;
    /// Whether this element is the additive identity.
    fn is_zero(&self) -> bool;
}

/// The exact-rational field: signed 64-bit numerator over a positive
/// 64-bit denominator, gcd-reduced at construction. Every operation is
/// exact until the i64 envelope is exhausted — then it refuses typed,
/// never wraps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational {
    num: i64,
    den: i64,
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a
}

impl Rational {
    /// Construct in canonical form (gcd-reduced, positive denominator).
    /// A zero denominator refuses typed.
    pub fn new(num: i64, den: i64) -> Result<Self, String> {
        if den == 0 {
            return Err(format!(
                "{E_GEOMETRY_ZERO_DIV}: rational with zero denominator is not a field element"
            ));
        }
        let (num, den) = if den < 0 { (-num, -den) } else { (num, den) };
        let divisor = gcd(num, den).max(1);
        Ok(Self {
            num: num / divisor,
            den: den / divisor,
        })
    }

    /// Numerator (denominator positive, gcd-reduced).
    #[must_use]
    pub fn num(&self) -> i64 {
        self.num
    }

    /// Denominator (always positive).
    #[must_use]
    pub fn den(&self) -> i64 {
        self.den
    }
}

impl Field for Rational {
    fn zero() -> Self {
        Self { num: 0, den: 1 }
    }

    fn one() -> Self {
        Self { num: 1, den: 1 }
    }

    fn minus_one() -> Self {
        Self { num: -1, den: 1 }
    }

    fn from_i64(value: i64) -> Self {
        Self { num: value, den: 1 }
    }

    fn add(&self, other: &Self) -> Result<Self, String> {
        // a/b + c/d = (ad + cb) / bd, computed in i128 then checked.
        let numerator = (i128::from(self.num) * i128::from(other.den))
            + (i128::from(other.num) * i128::from(self.den));
        let denominator = i128::from(self.den) * i128::from(other.den);
        let num = i64::try_from(numerator).map_err(|_| {
            format!("{E_GEOMETRY_OVERFLOW}: rational addition left the i64 envelope")
        })?;
        let den = i64::try_from(denominator).map_err(|_| {
            format!("{E_GEOMETRY_OVERFLOW}: rational addition left the i64 envelope")
        })?;
        Self::new(num, den)
    }

    fn sub(&self, other: &Self) -> Result<Self, String> {
        self.add(&other.neg()?)
    }

    fn mul(&self, other: &Self) -> Result<Self, String> {
        let num = self.num.checked_mul(other.num).ok_or_else(|| {
            format!("{E_GEOMETRY_OVERFLOW}: rational multiplication left the i64 envelope")
        })?;
        let den = self.den.checked_mul(other.den).ok_or_else(|| {
            format!("{E_GEOMETRY_OVERFLOW}: rational multiplication left the i64 envelope")
        })?;
        Self::new(num, den)
    }

    fn div(&self, other: &Self) -> Result<Self, String> {
        if other.num == 0 {
            return Err(format!(
                "{E_GEOMETRY_ZERO_DIV}: rational division by zero is refused, never infinite"
            ));
        }
        let num = self.num.checked_mul(other.den).ok_or_else(|| {
            format!("{E_GEOMETRY_OVERFLOW}: rational division left the i64 envelope")
        })?;
        let den = self.den.checked_mul(other.num).ok_or_else(|| {
            format!("{E_GEOMETRY_OVERFLOW}: rational division left the i64 envelope")
        })?;
        Self::new(num, den)
    }

    fn neg(&self) -> Result<Self, String> {
        Self::new(-self.num, self.den)
    }

    fn is_zero(&self) -> bool {
        self.num == 0
    }
}

impl Field for f64 {
    fn zero() -> Self {
        0.0
    }

    fn one() -> Self {
        1.0
    }

    fn minus_one() -> Self {
        -1.0
    }

    fn from_i64(value: i64) -> Self {
        value as f64
    }

    fn add(&self, other: &Self) -> Result<Self, String> {
        let sum = self + other;
        if sum.is_finite() {
            Ok(sum)
        } else {
            Err(format!(
                "{E_GEOMETRY_NONFINITE}: float addition left the finite domain ({sum:e})"
            ))
        }
    }

    fn sub(&self, other: &Self) -> Result<Self, String> {
        let difference = self - other;
        if difference.is_finite() {
            Ok(difference)
        } else {
            Err(format!(
                "{E_GEOMETRY_NONFINITE}: float subtraction left the finite domain ({difference:e})"
            ))
        }
    }

    fn mul(&self, other: &Self) -> Result<Self, String> {
        let product = self * other;
        if product.is_finite() {
            Ok(product)
        } else {
            Err(format!(
                "{E_GEOMETRY_NONFINITE}: float multiplication left the finite domain ({product:e})"
            ))
        }
    }

    fn div(&self, other: &Self) -> Result<Self, String> {
        if *other == 0.0 {
            return Err(format!(
                "{E_GEOMETRY_ZERO_DIV}: float division by zero is refused, never infinite"
            ));
        }
        let quotient = self / other;
        if quotient.is_finite() {
            Ok(quotient)
        } else {
            Err(format!(
                "{E_GEOMETRY_NONFINITE}: float division left the finite domain ({quotient:e})"
            ))
        }
    }

    fn neg(&self) -> Result<Self, String> {
        Ok(-*self)
    }

    fn is_zero(&self) -> bool {
        *self == 0.0
    }
}

/// A coordinate-BOUND point: has coordinates, transforms affinely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point<F: Field> {
    pub x: F,
    pub y: F,
}

impl<F: Field> Point<F> {
    /// Construct from coordinates.
    #[must_use]
    pub fn new(x: F, y: F) -> Self {
        Self { x, y }
    }

    /// The free vector pointing from `self` to `other` (the admissible
    /// boundary between bound points — point + point has no impl).
    pub fn displacement(&self, other: &Self) -> Result<FreeVector<F>, String> {
        Ok(FreeVector {
            dx: other.x.sub(&self.x)?,
            dy: other.y.sub(&self.y)?,
        })
    }

    /// Translate by a free vector (the ONLY way to move a point by a
    /// vector quantity).
    pub fn translate(&self, by: &FreeVector<F>) -> Result<Self, String> {
        Ok(Self {
            x: self.x.add(&by.dx)?,
            y: self.y.add(&by.dy)?,
        })
    }
}

/// A coordinate-FREE vector: a displacement with no position. Dot and
/// cross are defined here, never on points; transforms apply the
/// linear part only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreeVector<F: Field> {
    pub dx: F,
    pub dy: F,
}

impl<F: Field> FreeVector<F> {
    /// Construct from components.
    #[must_use]
    pub fn new(dx: F, dy: F) -> Self {
        Self { dx, dy }
    }

    /// Componentwise sum.
    pub fn add(&self, other: &Self) -> Result<Self, String> {
        Ok(Self {
            dx: self.dx.add(&other.dx)?,
            dy: self.dy.add(&other.dy)?,
        })
    }

    /// Dot product (free-vector operation).
    pub fn dot(&self, other: &Self) -> Result<F, String> {
        let xx = self.dx.mul(&other.dx)?;
        let yy = self.dy.mul(&other.dy)?;
        xx.add(&yy)
    }

    /// 2-D scalar cross product (free-vector operation).
    pub fn cross(&self, other: &Self) -> Result<F, String> {
        let a = self.dx.mul(&other.dy)?;
        let b = self.dy.mul(&other.dx)?;
        a.sub(&b)
    }
}

/// A line through a point with a direction: the point-direction form.
/// Construction from two distinct points; coincident points are a
/// typed degenerate refusal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line<F: Field> {
    point: Point<F>,
    direction: FreeVector<F>,
}

impl<F: Field> Line<F> {
    /// The line through two DISTINCT points.
    pub fn through_points(a: &Point<F>, b: &Point<F>) -> Result<Self, String> {
        let direction = a.displacement(b)?;
        if direction.dx.is_zero() && direction.dy.is_zero() {
            return Err(format!(
                "{E_GEOMETRY_DEGENERATE}: degenerate line: through_points needs two distinct points"
            ));
        }
        Ok(Self {
            point: a.clone(),
            direction,
        })
    }

    /// Whether `p` lies on the line: the cross of the direction with
    /// (p − point) is exactly zero.
    pub fn contains(&self, p: &Point<F>) -> Result<bool, String> {
        let offset = self.point.displacement(p)?;
        let cross = self.direction.cross(&offset)?;
        Ok(cross.is_zero())
    }
}

/// A general conic `a·x² + b·xy + c·y² + d·x + e·y + f = 0` over the
/// declared field. Evaluation is exact in the rational field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Conic<F: Field> {
    a: F,
    b: F,
    c: F,
    d: F,
    e: F,
    f: F,
}

impl<F: Field> Conic<F> {
    /// Construct from general coefficients; the all-zero form is a
    /// typed degenerate refusal (it is the whole plane, not a conic).
    pub fn from_coefficients(a: F, b: F, c: F, d: F, e: F, f: F) -> Result<Self, String> {
        if a.is_zero() && b.is_zero() && c.is_zero() && d.is_zero() && e.is_zero() && f.is_zero() {
            return Err(format!(
                "{E_GEOMETRY_DEGENERATE}: degenerate conic: all-zero coefficients are the whole plane"
            ));
        }
        Ok(Self { a, b, c, d, e, f })
    }

    /// The unit circle `x² + y² − 1 = 0`.
    #[must_use]
    pub fn unit_circle() -> Self {
        Self {
            a: Field::one(),
            b: Field::zero(),
            c: Field::one(),
            d: Field::zero(),
            e: Field::zero(),
            f: Field::minus_one(),
        }
    }

    /// Evaluate the implicit form at a point.
    pub fn evaluate(&self, p: &Point<F>) -> Result<F, String> {
        let x2 = p.x.mul(&p.x)?;
        let xy = p.x.mul(&p.y)?;
        let y2 = p.y.mul(&p.y)?;
        let ax2 = self.a.mul(&x2)?;
        let bxy = self.b.mul(&xy)?;
        let cy2 = self.c.mul(&y2)?;
        let dx = self.d.mul(&p.x)?;
        let ey = self.e.mul(&p.y)?;
        ax2.add(&bxy)?.add(&cy2)?.add(&dx)?.add(&ey)?.add(&self.f)
    }

    /// Whether `p` lies exactly on the conic (no epsilon: the rational
    /// field decides equality exactly).
    pub fn contains(&self, p: &Point<F>) -> Result<bool, String> {
        Ok(self.evaluate(p)?.is_zero())
    }
}

/// A 2-D affine transform: linear part (m11 m12 / m21 m22) plus
/// translation. Points get the full map; free vectors get the linear
/// part ONLY — that asymmetry is the coordinate-bound vs
/// coordinate-free distinction in operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform<F: Field> {
    m11: F,
    m12: F,
    m21: F,
    m22: F,
    tx: F,
    ty: F,
}

impl<F: Field> Transform<F> {
    /// The identity map.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            m11: Field::one(),
            m12: Field::zero(),
            m21: Field::zero(),
            m22: Field::one(),
            tx: Field::zero(),
            ty: Field::zero(),
        }
    }

    /// Pure translation by `(tx, ty)`.
    #[must_use]
    pub fn translation(tx: i64, ty: i64) -> Self {
        Self {
            m11: Field::one(),
            m12: Field::zero(),
            m21: Field::zero(),
            m22: Field::one(),
            tx: Field::from_i64(tx),
            ty: Field::from_i64(ty),
        }
    }

    /// Uniform scaling about the origin.
    #[must_use]
    pub fn scaling(factor: F) -> Self {
        Self {
            m11: factor.clone(),
            m12: Field::zero(),
            m21: Field::zero(),
            m22: factor,
            tx: Field::zero(),
            ty: Field::zero(),
        }
    }

    /// Rotation by `quarter_turns` right angles (exact for any i64 n:
    /// the matrix entries are in {−1, 0, 1}).
    #[must_use]
    pub fn rotation_quarter_turns(quarter_turns: i64) -> Self {
        match quarter_turns.rem_euclid(4) {
            0 => Self::identity(),
            1 => Self {
                m11: Field::zero(),
                m12: Field::minus_one(),
                m21: Field::one(),
                m22: Field::zero(),
                tx: Field::zero(),
                ty: Field::zero(),
            },
            2 => Self {
                m11: Field::minus_one(),
                m12: Field::zero(),
                m21: Field::zero(),
                m22: Field::minus_one(),
                tx: Field::zero(),
                ty: Field::zero(),
            },
            _ => Self {
                m11: Field::zero(),
                m12: Field::one(),
                m21: Field::minus_one(),
                m22: Field::zero(),
                tx: Field::zero(),
                ty: Field::zero(),
            },
        }
    }

    /// Apply the FULL affine map to a coordinate-bound point.
    pub fn apply_point(&self, p: &Point<F>) -> Result<Point<F>, String> {
        let xx = self.m11.mul(&p.x)?;
        let xy = self.m12.mul(&p.y)?;
        let yx = self.m21.mul(&p.x)?;
        let yy = self.m22.mul(&p.y)?;
        Ok(Point {
            x: xx.add(&xy)?.add(&self.tx)?,
            y: yx.add(&yy)?.add(&self.ty)?,
        })
    }

    /// Apply ONLY the linear part to a coordinate-free vector — a
    /// translation must not move a free vector.
    pub fn apply_free_vector(&self, v: &FreeVector<F>) -> Result<FreeVector<F>, String> {
        let xx = self.m11.mul(&v.dx)?;
        let xy = self.m12.mul(&v.dy)?;
        let yx = self.m21.mul(&v.dx)?;
        let yy = self.m22.mul(&v.dy)?;
        Ok(FreeVector {
            dx: xx.add(&xy)?,
            dy: yx.add(&yy)?,
        })
    }
}

/// Shorthand point constructor (named, no operator overloads — G4).
#[must_use]
pub fn point<F: Field>(x: F, y: F) -> Point<F> {
    Point::new(x, y)
}

/// Shorthand free-vector constructor (named, no operator overloads — G4).
#[must_use]
pub fn free_vector<F: Field>(dx: F, dy: F) -> FreeVector<F> {
    FreeVector::new(dx, dy)
}
