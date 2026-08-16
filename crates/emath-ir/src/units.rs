//! Units and dimensions.
//!
//! Dimension vectors, canonical SI-style encoding, scale factors and
//! affine units. Affine units (offsets, e.g. Celsius) may only add/subtract
//! within their own family; multiplying/dividing them is a typed refusal.

use emath_core::fnv1a64_bytes;

/// SI base dimension vector (m, kg, s, A, K, mol, cd).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitDim([i64; 7]);

impl UnitDim {
    /// Construct from base exponents.
    #[must_use]
    pub const fn base(m: i64, kg: i64, s: i64, a: i64, k: i64, mol: i64, cd: i64) -> Self {
        Self([m, kg, s, a, k, mol, cd])
    }

    /// Dimensionless.
    #[must_use]
    pub const fn one() -> Self {
        Self([0; 7])
    }

    /// Product.
    #[must_use]
    pub const fn mul(self, other: Self) -> Self {
        let mut exponents = [0; 7];
        let mut index = 0;
        while index < 7 {
            exponents[index] = self.0[index] + other.0[index];
            index += 1;
        }
        Self(exponents)
    }

    /// Quotient.
    #[must_use]
    pub const fn div(self, other: Self) -> Self {
        let mut exponents = [0; 7];
        let mut index = 0;
        while index < 7 {
            exponents[index] = self.0[index] - other.0[index];
            index += 1;
        }
        Self(exponents)
    }

    /// Canonical SI-style rendering, e.g. `m^1*s^-1`.
    #[must_use]
    pub fn render(self) -> String {
        let mut parts = Vec::new();
        for (index, exponent) in self.0.iter().enumerate() {
            if *exponent != 0 {
                parts.push(format!("{}^{exponent}", SI_NAMES[index]));
            }
        }
        if parts.is_empty() {
            "1".to_string()
        } else {
            parts.join("*")
        }
    }
}

/// SI base unit names.
pub const SI_NAMES: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];

/// A unit: dimension signature, SI scale factor and optional affine offset.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    /// Display name.
    pub name: String,
    /// Dimension signature.
    pub dims: UnitDim,
    /// SI scale factor (`value_si = value * scale + offset`).
    pub scale: f64,
    /// Affine offset (0 for linear units).
    pub offset: f64,
    identity: u64,
}

impl Unit {
    /// Constructs and seals a unit.
    #[must_use]
    pub fn new(name: String, dims: UnitDim, scale: f64, offset: f64) -> Self {
        let mut unit = Self {
            name,
            dims,
            scale,
            offset,
            identity: 0,
        };
        unit.identity = fnv1a64_bytes(unit.canonical().as_bytes());
        unit
    }

    /// Linear SI body unit.
    #[must_use]
    pub fn si(name: String, dims: UnitDim) -> Self {
        Self::new(name, dims, 1.0, 0.0)
    }

    /// Whether the unit is affine (has a non-zero offset).
    #[must_use]
    pub fn is_affine(&self) -> bool {
        self.offset != 0.0
    }

    /// Dimension signature.
    #[must_use]
    pub fn dimensions(&self) -> UnitDim {
        self.dims
    }

    /// Product of two units; affine units are refused in multiplication.
    pub fn mul(&self, other: &Self) -> Result<Self, UnitError> {
        if self.is_affine() || other.is_affine() {
            return Err(UnitError {
                code: "E-UNIT-102",
                message: format!(
                    "affine unit misuse: cannot multiply `{}` by `{}`",
                    self.name, other.name
                ),
            });
        }
        Ok(Self::new(
            format!("{}.{}", self.name, other.name),
            self.dims.mul(other.dims),
            self.scale * other.scale,
            0.0,
        ))
    }

    /// Quotient of two units; affine units are refused in division.
    pub fn div(&self, other: &Self) -> Result<Self, UnitError> {
        if self.is_affine() || other.is_affine() {
            return Err(UnitError {
                code: "E-UNIT-102",
                message: format!(
                    "affine unit misuse: cannot divide `{}` by `{}`",
                    self.name, other.name
                ),
            });
        }
        Ok(Self::new(
            format!("{}/{}", self.name, other.name),
            self.dims.div(other.dims),
            self.scale / other.scale,
            0.0,
        ))
    }

    /// Whether two units are dimensionally compatible.
    #[must_use]
    pub fn compatible_with(&self, other: &Self) -> bool {
        self.dims == other.dims
    }

    /// Canonical encoding (`v1`-versioned).
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "unit:v1:{}:{}:{:e}:{:e}",
            self.name,
            self.dims.render(),
            self.scale,
            self.offset
        )
    }

    /// FNV-1a64 identity.
    #[must_use]
    pub fn identity(&self) -> u64 {
        self.identity
    }
}

/// Unit system failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitError {
    /// Stable code (`E-UNIT-100`/`E-UNIT-101`/`E-UNIT-102`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Checks dimensional compatibility of two operands; mismatch is typed.
pub fn check_compatible(left: &Unit, right: &Unit) -> Result<(), UnitError> {
    if left.compatible_with(right) {
        Ok(())
    } else {
        Err(UnitError {
            code: "E-UNIT-101",
            message: format!(
                "dimension mismatch: {} ({}), {} ({})",
                left.name,
                left.dims.render(),
                right.name,
                right.dims.render()
            ),
        })
    }
}
