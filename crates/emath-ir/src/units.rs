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

/// Capsule-authored unit catalog: `language/spec/capabilities/units-catalog.emath`,
/// FeatureID `std.capability.units.catalog`. Named units, aliases, and the
/// currency/time-zone refusal list are capsule DATA parsed here; Rust keeps
/// no named-unit table of its own. The catalog capsule is capsule-active
/// authority (see `language.lock`); drift between this parser and the
/// capsule fails `domain_science_capsule_cutover` bit-exactly.
const UNIT_CATALOG_CAPSULE: &str =
    include_str!("../../../language/spec/capabilities/units-catalog.emath");

/// One capsule-declared unit entry (parsed, never hardcoded).
struct CatalogEntry {
    name: String,
    dims: [i64; 7],
    scale: f64,
    offset: f64,
    family: UnitFamily,
}

struct CatalogData {
    entries: Vec<CatalogEntry>,
    aliases: Vec<(String, String)>,
    refused: Vec<String>,
}

/// Extract `key=value` subfields of a capsule `semantics` value.
fn semantics_field<'a>(semantics: &'a str, field: &str) -> Option<&'a str> {
    semantics
        .split(';')
        .find_map(|part| part.trim().strip_prefix(field)?.strip_prefix('='))
}

fn capsule_block<'a>(text: &'a str, header: &str) -> Option<&'a str> {
    let start = text.find(header)?;
    let rest = &text[start..];
    let end = rest[1..]
        .find("\nemath feature ")
        .map(|offset| offset + 1)
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

fn parse_catalog(text: &str) -> CatalogData {
    let empty = CatalogData {
        entries: Vec::new(),
        aliases: Vec::new(),
        refused: Vec::new(),
    };
    let Some(block) = capsule_block(text, "emath feature UnitCatalog:") else {
        return empty;
    };
    let Some(semantics) = block.lines().find_map(|raw| {
        let line = raw.trim();
        let (key, value) = line.split_once(':')?;
        (key.trim() == "semantics").then(|| value.trim().trim_matches('"'))
    }) else {
        return empty;
    };
    let parse_dims = |body: &str| -> [i64; 7] {
        let mut dims = [0_i64; 7];
        for (slot, exponent) in body.split(',').enumerate().take(7) {
            dims[slot] = exponent.trim().parse().unwrap_or(0);
        }
        dims
    };
    let entries = semantics_field(semantics, "catalog")
        .map(|catalog| {
            catalog
                .split('|')
                .filter_map(|entry| {
                    let mut fields = entry.split('~');
                    let name = fields.next()?;
                    Some(CatalogEntry {
                        name: name.to_string(),
                        dims: parse_dims(fields.next()?),
                        scale: fields.next()?.parse().ok()?,
                        offset: fields.next()?.parse().ok()?,
                        family: match fields.next()? {
                            "info" => UnitFamily::Information,
                            _ => UnitFamily::Si,
                        },
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let aliases = semantics_field(semantics, "aliases")
        .map(|aliases| {
            aliases
                .split('|')
                .filter_map(|alias| {
                    let (name, target) = alias.split_once('>')?;
                    Some((name.to_string(), target.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    let refused = semantics_field(semantics, "refusals")
        .map(|refusals| refusals.split('|').map(str::to_string).collect())
        .unwrap_or_default();
    CatalogData {
        entries,
        aliases,
        refused,
    }
}

fn catalog_data() -> &'static CatalogData {
    static CATALOG: std::sync::OnceLock<CatalogData> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| parse_catalog(UNIT_CATALOG_CAPSULE))
}

/// Looks up a language-surface unit name. Resolution order is
/// capsule-declared: alias chain, then exact catalog entry, then the
/// capsule's typed refusal list (currencies and time zones,
/// `E-UNIT-CURRENCY-1`), then SI-prefix composition. Unknown names are
/// `E-UNIT-104`.
pub fn lookup_unit(name: &str) -> Result<Unit, UnitError> {
    let data = catalog_data();
    let canonical = data
        .aliases
        .iter()
        .find(|(alias, _)| alias == name)
        .map(|(_, target)| target.as_str())
        .unwrap_or(name);
    if let Some(entry) = data.entries.iter().find(|entry| entry.name == canonical) {
        return Ok(Unit::with_family(
            entry.name.clone(),
            UnitDim(entry.dims),
            entry.scale,
            entry.offset,
            entry.family,
        ));
    }
    // Currencies and time zones are refused IN CORE by capsule policy.
    // They are socially versioned data and live in packages (never the
    // nucleus); a distinct code makes the policy a typed refusal, not a
    // generic unknown-unit miss.
    if data.refused.iter().any(|refused| refused == canonical) {
        return Err(UnitError {
            code: E_UNIT_CURRENCY_CORE,
            message: format!(
                "`{name}` is a currency or time zone: these live in versioned packages, \
                     never in the core unit table (declare them in a package, not core)"
            ),
        });
    }
    prefixed_unit(name).unwrap_or_else(|| {
        Err(UnitError {
            code: "E-UNIT-104",
            message: format!("unknown unit `{name}`"),
        })
    })
}

/// Typed refusal code for currency/time-zone spellings used in core:
/// the policy violation is distinct from the generic
/// `E-UNIT-104` unknown-unit miss.
pub const E_UNIT_CURRENCY_CORE: &str = "E-UNIT-CURRENCY-1";

/// SI prefixes admitted systematically over every known unit spelling:
/// `nm`, `mmol`, `kPa`, `MJ`, `mK`, `nC`, `kB`, … Strip one
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
