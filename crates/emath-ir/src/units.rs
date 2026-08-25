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

    /// Scale all exponents by `n` (power).
    #[must_use]
    pub const fn pow(self, n: i32) -> Self {
        let mut exponents = [0; 7];
        let mut index = 0;
        while index < 7 {
            exponents[index] = self.0[index] * n as i64;
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

/// Unit family. Information units share a dimensionless SI vector but
/// are never compatible with SI quantities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitFamily {
    /// SI / derived SI.
    Si,
    /// Digital information (byte, MiB).
    Information,
}

impl UnitFamily {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Si => "si",
            Self::Information => "info",
        }
    }
}

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
    /// Family: SI vs information. Same-dimension SI and byte counts do not mix.
    pub family: UnitFamily,
    identity: u64,
}

impl Unit {
    /// Constructs and seals a unit.
    #[must_use]
    pub fn new(name: String, dims: UnitDim, scale: f64, offset: f64) -> Self {
        Self::with_family(name, dims, scale, offset, UnitFamily::Si)
    }

    /// Constructs a unit in an explicit family.
    #[must_use]
    pub fn with_family(
        name: String,
        dims: UnitDim,
        scale: f64,
        offset: f64,
        family: UnitFamily,
    ) -> Self {
        let mut unit = Self {
            name,
            dims,
            scale,
            offset,
            family,
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
        if self.family != other.family {
            return Err(UnitError {
                code: "E-UNIT-101",
                message: format!(
                    "dimension mismatch: cannot multiply `{}` ({}) by `{}` ({})",
                    self.name,
                    self.family.as_str(),
                    other.name,
                    other.family.as_str()
                ),
            });
        }
        Ok(Self::with_family(
            format!("{}.{}", self.name, other.name),
            self.dims.mul(other.dims),
            self.scale * other.scale,
            0.0,
            self.family,
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
        if self.family != other.family {
            return Err(UnitError {
                code: "E-UNIT-101",
                message: format!(
                    "dimension mismatch: cannot divide `{}` ({}) by `{}` ({})",
                    self.name,
                    self.family.as_str(),
                    other.name,
                    other.family.as_str()
                ),
            });
        }
        Ok(Self::with_family(
            format!("{}/{}", self.name, other.name),
            self.dims.div(other.dims),
            self.scale / other.scale,
            0.0,
            self.family,
        ))
    }

    /// Whether two units are dimensionally compatible.
    #[must_use]
    pub fn compatible_with(&self, other: &Self) -> bool {
        self.family == other.family && self.dims == other.dims
    }

    /// Whether this unit is dimensionless SI (scale may be non-1).
    #[must_use]
    pub fn is_dimensionless(&self) -> bool {
        self.family == UnitFamily::Si && self.dims == UnitDim::one()
    }

    /// Canonical encoding, dimension-signature-first: a changed display
    /// label alone never changes identity if dims, scale and offset
    /// are unchanged.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self.family {
            UnitFamily::Si => format!(
                "unit:{}:{:e}:{:e}:{}",
                self.dims.render(),
                self.scale,
                self.offset,
                self.name
            ),
            UnitFamily::Information => format!(
                "unit:info:{}:{:e}:{:e}:{}",
                self.dims.render(),
                self.scale,
                self.offset,
                self.name
            ),
        }
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
    /// Stable code (`E-UNIT-100`/`E-UNIT-101`/`E-UNIT-102`/`E-UNIT-104`/`E-UNIT-105`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Looks up a language-surface unit name. Unknown names are `E-UNIT-104`.
pub fn lookup_unit(name: &str) -> Result<Unit, UnitError> {
    match name {
        "s" | "Duration" | "Time" => Ok(Unit::si("s".into(), UnitDim::base(0, 0, 1, 0, 0, 0, 0))),
        "ms" | "Millisecond" => Ok(Unit::new(
            "ms".into(),
            UnitDim::base(0, 0, 1, 0, 0, 0, 0),
            1e-3,
            0.0,
        )),
        "m" | "Length" => Ok(Unit::si("m".into(), UnitDim::base(1, 0, 0, 0, 0, 0, 0))),
        "kg" | "Mass" => Ok(Unit::si("kg".into(), UnitDim::base(0, 1, 0, 0, 0, 0, 0))),
        "K" | "Temperature" => Ok(Unit::si("K".into(), UnitDim::base(0, 0, 0, 0, 1, 0, 0))),
        "N" | "Force" => Ok(Unit::si(
            "N".into(),
            UnitDim::base(1, 1, -2, 0, 0, 0, 0),
        )),
        "Hz" => Ok(Unit::si("Hz".into(), UnitDim::base(0, 0, -1, 0, 0, 0, 0))),
        "W" => Ok(Unit::si("W".into(), UnitDim::base(2, 1, -3, 0, 0, 0, 0))),
        "Byte" | "Bytes" | "B" => Ok(Unit::with_family(
            "B".into(),
            UnitDim::one(),
            1.0,
            0.0,
            UnitFamily::Information,
        )),
        "MiB" => Ok(Unit::with_family(
            "MiB".into(),
            UnitDim::one(),
            1_048_576.0,
            0.0,
            UnitFamily::Information,
        )),
        other => Err(UnitError {
            code: "E-UNIT-104",
            message: format!("unknown unit `{other}`"),
        }),
    }
}

/// Constructs `Per<Inner>` (inverse of a looked-up inner unit).
pub fn per_unit(inner: &str) -> Result<Unit, UnitError> {
    let inner_unit = lookup_unit(inner)?;
    if inner_unit.is_affine() {
        return Err(UnitError {
            code: "E-UNIT-105",
            message: format!("`Per<{inner}>` is invalid: affine units have no inverse"),
        });
    }
    let one = Unit::with_family(
        "1".into(),
        UnitDim::one(),
        1.0,
        0.0,
        inner_unit.family,
    );
    one.div(&inner_unit).map_err(|error| UnitError {
        code: "E-UNIT-105",
        message: format!("`Per<{inner}>` is not a well-formed unit: {}", error.message),
    })
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
