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

    /// Stable English name for common SI dimension vectors (`length`,
    /// `duration`, `area`, …). Unknown combinations have no kind name.
    #[must_use]
    pub fn kind_name(self) -> Option<&'static str> {
        if self == Self::base(1, 0, 0, 0, 0, 0, 0) {
            Some("length")
        } else if self == Self::base(0, 1, 0, 0, 0, 0, 0) {
            Some("mass")
        } else if self == Self::base(0, 0, 1, 0, 0, 0, 0) {
            Some("duration")
        } else if self == Self::base(0, 0, 0, 1, 0, 0, 0) {
            Some("electric current")
        } else if self == Self::base(0, 0, 0, 0, 1, 0, 0) {
            Some("temperature")
        } else if self == Self::base(0, 0, 0, 0, 0, 1, 0) {
            Some("amount")
        } else if self == Self::base(0, 0, 0, 0, 0, 0, 1) {
            Some("luminous intensity")
        } else if self == Self::base(2, 0, 0, 0, 0, 0, 0) {
            Some("area")
        } else if self == Self::base(3, 0, 0, 0, 0, 0, 0) {
            Some("volume")
        } else if self == Self::base(1, 0, -1, 0, 0, 0, 0) {
            Some("speed")
        } else if self == Self::base(1, 0, -2, 0, 0, 0, 0) {
            Some("acceleration")
        } else if self == Self::base(1, 1, -2, 0, 0, 0, 0) {
            Some("force")
        } else if self == Self::one() {
            Some("dimensionless")
        } else {
            None
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
    /// SI scale factor (`value_si = (value + offset) * scale`).
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
            format!("{}*{}", self.name, other.name),
            self.dims.mul(other.dims),
            self.scale * other.scale,
            0.0,
            self.family,
        ))
    }

    /// Raise a linear unit to an integer power. Affine units only admit `^1`.
    pub fn pow(&self, exponent: i32) -> Result<Self, UnitError> {
        if self.is_affine() && exponent != 1 {
            return Err(UnitError {
                code: "E-UNIT-102",
                message: format!(
                    "affine unit misuse: cannot raise `{}` to power {exponent}",
                    self.name
                ),
            });
        }
        Ok(Self::with_family(
            format!("{}^{exponent}", self.name),
            self.dims.pow(exponent),
            self.scale.powi(exponent),
            if exponent == 1 { self.offset } else { 0.0 },
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

    /// Convert a magnitude expressed in this unit to SI
    /// (`value_si = (value + offset) * scale`).
    #[must_use]
    pub fn to_si(&self, value: f64) -> f64 {
        (value + self.offset) * self.scale
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
        "km" | "Kilometre" | "Kilometer" => Ok(Unit::new(
            "km".into(),
            UnitDim::base(1, 0, 0, 0, 0, 0, 0),
            1_000.0,
            0.0,
        )),
        "kg" | "Mass" => Ok(Unit::si("kg".into(), UnitDim::base(0, 1, 0, 0, 0, 0, 0))),
        "K" | "Temperature" => Ok(Unit::si("K".into(), UnitDim::base(0, 0, 0, 0, 1, 0, 0))),
        "degC" | "Celsius" => Ok(Unit::new(
            "degC".into(),
            UnitDim::base(0, 0, 0, 0, 1, 0, 0),
            1.0,
            273.15,
        )),
        "degF" | "Fahrenheit" => Ok(Unit::new(
            "degF".into(),
            UnitDim::base(0, 0, 0, 0, 1, 0, 0),
            5.0 / 9.0,
            // Nearest-f64 pre-scale offset normalized through the freezing
            // point, so both 32°F and 212°F compare exactly with their
            // decimal Kelvin spellings after source lowering.
            273.15 / (5.0 / 9.0) - 32.0,
        )),
        "L" | "liter" | "litre" => Ok(Unit::new(
            "L".into(),
            UnitDim::base(3, 0, 0, 0, 0, 0, 0),
            1e-3,
            0.0,
        )),
        "cal" => Ok(Unit::new(
            "cal".into(),
            UnitDim::base(2, 1, -2, 0, 0, 0, 0),
            4.184,
            0.0,
        )),
        "cal_IT" => Ok(Unit::new(
            "cal_IT".into(),
            UnitDim::base(2, 1, -2, 0, 0, 0, 0),
            4.1868,
            0.0,
        )),
        "N" | "Force" => Ok(Unit::si("N".into(), UnitDim::base(1, 1, -2, 0, 0, 0, 0))),
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
        // --- core::units_ext (bead 8u7h, Phase 13) --------------------------
        // SI companions: the base electric/amount units and the pressure/
        // energy derived units physics law contracts need.
        "A" | "Ampere" => Ok(Unit::si("A".into(), UnitDim::base(0, 0, 0, 1, 0, 0, 0))),
        "mol" | "Mole" => Ok(Unit::si("mol".into(), UnitDim::base(0, 0, 0, 0, 0, 1, 0))),
        "C" | "Coulomb" => Ok(Unit::si("C".into(), UnitDim::base(0, 0, 1, 1, 0, 0, 0))),
        "V" | "Volt" => Ok(Unit::si("V".into(), UnitDim::base(2, 1, -3, -1, 0, 0, 0))),
        "J" | "Joule" => Ok(Unit::si("J".into(), UnitDim::base(2, 1, -2, 0, 0, 0, 0))),
        "Pa" | "Pascal" => Ok(Unit::si("Pa".into(), UnitDim::base(-1, 1, -2, 0, 0, 0, 0))),
        "bar" => Ok(Unit::new("bar".into(), UnitDim::base(-1, 1, -2, 0, 0, 0, 0), 1e5, 0.0)),
        "eV" => Ok(Unit::new(
            "eV".into(),
            UnitDim::base(2, 1, -2, 0, 0, 0, 0),
            1.602_176_634e-19, // exact: SI 2019 definition of the electronvolt
            0.0,
        )),
        "min" | "Minute" => Ok(Unit::new("min".into(), UnitDim::base(0, 0, 1, 0, 0, 0, 0), 60.0, 0.0)),
        "g" | "Gram" => Ok(Unit::new("g".into(), UnitDim::base(0, 1, 0, 0, 0, 0, 0), 1e-3, 0.0)),
        // Affine-temperature extension of the Phase 2 family (C13 order):
        // Rankine is an ABSOLUTE scale (offset 0), same dimension as K.
        "degR" | "Rankine" => Ok(Unit::new(
            "degR".into(),
            UnitDim::base(0, 0, 0, 0, 1, 0, 0),
            5.0 / 9.0,
            0.0,
        )),
        // Angle units: dimensionless BY DECLARATION. The SI radian is
        // m/m; the policy here is explicit, not accidental — an angle
        // carries the dimensionless vector with angle scales, so
        // `dimension of` a degree equals that of a radian.
        "rad" | "Radian" => Ok(Unit::si("rad".into(), UnitDim::one())),
        "deg" | "Degree" => Ok(Unit::new(
            "deg".into(),
            UnitDim::one(),
            std::f64::consts::PI / 180.0, // π/180, nearest f64
            0.0,
        )),
        "arcmin" => Ok(Unit::new("arcmin".into(), UnitDim::one(), std::f64::consts::PI / 10_800.0, 0.0)),
        "arcsec" => Ok(Unit::new("arcsec".into(), UnitDim::one(), std::f64::consts::PI / 648_000.0, 0.0)),
        "grad" | "Gradian" => Ok(Unit::new("grad".into(), UnitDim::one(), std::f64::consts::PI / 200.0, 0.0)),
        "turn" | "Revolution" => Ok(Unit::new(
            "turn".into(),
            UnitDim::one(),
            2.0 * std::f64::consts::PI,
            0.0,
        )),
        // Astronomical scales (exact by definition):
        "AU" | "AstronomicalUnit" => Ok(Unit::new(
            "AU".into(),
            UnitDim::base(1, 0, 0, 0, 0, 0, 0),
            1.495_978_707e11, // IAU 2012, exact
            0.0,
        )),
        "pc" | "Parsec" => Ok(Unit::new(
            "pc".into(),
            UnitDim::base(1, 0, 0, 0, 0, 0, 0),
            3.085_677_581_491_367_3e16, // 648000/π AU
            0.0,
        )),
        "ly" | "LightYear" => Ok(Unit::new(
            "ly".into(),
            UnitDim::base(1, 0, 0, 0, 0, 0, 0),
            9.460_730_472_580_8e15, // Julian light year
            0.0,
        )),
        // Geodetic / imperial lengths (exact by definition):
        "nmi" | "NauticalMile" => Ok(Unit::new("nmi".into(), UnitDim::base(1, 0, 0, 0, 0, 0, 0), 1_852.0, 0.0)),
        "mi" | "Mile" => Ok(Unit::new("mi".into(), UnitDim::base(1, 0, 0, 0, 0, 0, 0), 1_609.344, 0.0)),
        "ft" | "Foot" => Ok(Unit::new("ft".into(), UnitDim::base(1, 0, 0, 0, 0, 0, 0), 0.3048, 0.0)),
        // Currencies and time zones are refused IN CORE. They are
        // socially versioned data and live in packages (never the
        // nucleus); a distinct code makes the policy a typed refusal,
        // not a generic unknown-unit miss.
        "USD" | "EUR" | "GBP" | "JPY" | "CNY" | "CHF" | "CAD" | "AUD" | "INR" | "RUB"
        | "BTC" | "ETH" | "UTC" | "GMT" | "EST" | "PST" | "CST" | "CET" | "JST" => {
            Err(UnitError {
                code: E_UNIT_CURRENCY_CORE,
                message: format!(
                    "`{name}` is a currency or time zone: these live in versioned packages, \
                     never in the core unit table (declare them in a package, not core)"
                ),
            })
        }
        name => prefixed_unit(name)
            .unwrap_or_else(|| {
                Err(UnitError {
                    code: "E-UNIT-104",
                    message: format!("unknown unit `{name}`"),
                })
            }),
    }
}

/// Typed refusal code for currency/time-zone spellings used in core
/// (bead 8u7h): the policy violation is distinct from the generic
/// `E-UNIT-104` unknown-unit miss.
pub const E_UNIT_CURRENCY_CORE: &str = "E-UNIT-CURRENCY-1";

/// SI prefixes admitted systematically over every known unit spelling
/// (bead 8u7h): `nm`, `mmol`, `kPa`, `MJ`, `mK`, `nC`, `kB`, … Strip one
/// prefix, resolve the base recursively, scale. Exact spellings always
/// win (the match above), so `kg`, `ms`, `km`, `MiB` are untouched.
/// Returns `None` when no prefix strips to a known unit.
fn prefixed_unit(name: &str) -> Option<Result<Unit, UnitError>> {
    const PREFIXES: &[(&str, f64)] = &[
        ("T", 1e12),
        ("G", 1e9),
        ("M", 1e6),
        ("k", 1e3),
        ("c", 1e-2),
        ("m", 1e-3),
        ("u", 1e-6),
        ("n", 1e-9),
        ("p", 1e-12),
        ("f", 1e-15),
    ];
    for (prefix, factor) in PREFIXES {
        if name.len() > prefix.len() && name.starts_with(prefix) {
            let base = &name[prefix.len()..];
            match lookup_unit(base) {
                Ok(unit) => {
                    if unit.is_affine() {
                        return Some(Err(UnitError {
                            code: "E-UNIT-102",
                            message: format!(
                                "affine unit misuse: `{}` cannot take SI prefix `{prefix}`",
                                unit.name
                            ),
                        }));
                    }
                    return Some(Ok(Unit::with_family(
                        name.into(),
                        unit.dims,
                        unit.scale * factor,
                        0.0,
                        unit.family,
                    )));
                }
                // A currency behind a prefix keeps its policy refusal
                // (mUSD is still a currency, not an unknown unit).
                Err(error) if error.code == E_UNIT_CURRENCY_CORE => return Some(Err(error)),
                Err(_) => continue,
            }
        }
    }
    None
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
    let one = Unit::with_family("1".into(), UnitDim::one(), 1.0, 0.0, inner_unit.family);
    one.div(&inner_unit).map_err(|error| UnitError {
        code: "E-UNIT-105",
        message: format!(
            "`Per<{inner}>` is not a well-formed unit: {}",
            error.message
        ),
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
