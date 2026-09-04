//! Versioned CODATA constants (04 section 2.6).
//!
//! Every constant is a measured-or-exact quantity with provenance.
//! Adjustments are versioned by year and NEVER unversioned: `codata2018`
//! and `codata2022` are distinct adjustment identities even when a
//! constant's value did not change between adjustments — the version
//! enters the identity hash through the canonical encoding, which is the
//! reproducibility safeguard (a re-executed artifact must remember which
//! adjustment it was computed against).
//!
//! Exactness classes: `c`, `h`, `k_B`, `e`, `N_A` are exact by the 2019 SI
//! definition (definitional constants, zero uncertainty). `G` is measured:
//! `6.67430(15)e-11` with u(G)/G ≈ 2.2e-5, CODATA provenance, and the C2
//! parenthesized-denominator unit spelling `m^3/(kg*s^2)`. `hbar` is an
//! exact definitional alias of `h/(2*pi)`: its value is COMPUTED from `h`
//! (never stored as an independent decimal), so the alias identity holds
//! exactly at f64 precision by construction, not by coincidence of two
//! rounded strings.
//!
//! Values are stored as the NIST decimal spelling (string), never
//! round-tripped through f64 on construction; [`CodataConstant::value_f64`]
//! is the derived strict-f64 view.
//!
//! Doctor support: [`mixed_codata_adjustments`] scans `use` import path
//! segments and flags a project mixing adjustment versions or using an
//! unversioned import (`use sci::constants::*` is a reproducibility
//! refusal, never a default).
//!
//! No-claim boundary: this is the std content layer. Language-level
//! admission of `use sci::constants::codata2018::*` imports lands with the
//! import-resolution slice (imports currently refuse at parse), and only
//! constants verifiable against offline NIST records are pinned here (G is
//! unchanged between the 2018 and 2022 adjustments; adjustment-differing
//! values for further constants require a NIST table import — follow-up).

#![forbid(unsafe_code)]

use crate::hash::fnv1a64_bytes;

/// CODATA adjustment year. Adjustments are identities, not default
/// versions: importing without naming one is a refusal, never a default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CodataAdjustment {
    Y2018,
    Y2022,
}

impl CodataAdjustment {
    /// Year spelling used in import paths and provenance citations.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Y2018 => "2018",
            Self::Y2022 => "2022",
        }
    }

    /// Parse an adjustment version segment (`codata2018`).
    #[must_use]
    pub fn parse(segment: &str) -> Option<Self> {
        match segment {
            "codata2018" => Some(Self::Y2018),
            "codata2022" => Some(Self::Y2022),
            _ => None,
        }
    }
}

/// Exactness class of a CODATA constant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodataKind {
    /// Exact by SI definition (`c`, `h`, `k_B`, `e`, `N_A`) or by a
    /// definitional alias (`hbar = h/(2*pi)`). The `source` names the
    /// definition.
    Exact { source: &'static str },
    /// Measured with declared uncertainty in CODATA parenthetical
    /// spelling: `6.67430(15)e-11` means `6.67430 ± 0.00015` scaled by
    /// `10^-11`.
    Measured {
        uncertainty_digits: &'static str,
        exponent: i32,
    },
}

impl CodataKind {
    /// True when the constant carries no uncertainty.
    #[must_use]
    pub const fn is_exact(&self) -> bool {
        matches!(self, Self::Exact { .. })
    }
}

/// One CODATA constant: value, unit, exactness class, and adjustment
/// provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodataConstant {
    /// Full name (`speed of light in vacuum`).
    pub name: &'static str,
    /// Symbol (`c`, `h`, `hbar`, `G`, ...).
    pub symbol: &'static str,
    /// NIST decimal spelling of the value (no uncertainty parentheses).
    pub value: String,
    pub unit: &'static str,
    pub kind: CodataKind,
    /// Adjustment this constant is pinned to.
    pub adjustment: CodataAdjustment,
}

impl CodataConstant {
    /// Derived strict-f64 view of the value.
    #[must_use]
    pub fn value_f64(&self) -> f64 {
        self.value.parse().unwrap_or(f64::NAN)
    }

    /// True when a parenthetical uncertainty is declared.
    #[must_use]
    pub const fn has_uncertainty(&self) -> bool {
        !self.kind.is_exact()
    }

    /// CODATA citation provenance for this constant (adjustment is the
    /// citation's version key; feeds `Provenance::Citation` at the
    /// lowering seam).
    #[must_use]
    pub fn citation_reference(&self) -> String {
        format!("CODATA {} adjustment, NIST", self.adjustment.as_str())
    }

    /// Canonical deterministic encoding over (schema, adjustment, name,
    /// symbol, value, unit, kind) — `codata2018::G` and `codata2022::G`
    /// hash differently even with identical values, because the
    /// adjustment is part of the encoding.
    #[must_use]
    pub fn canonical(&self) -> String {
        fn field(out: &mut String, name: &str, value: &str) {
            out.push_str(name);
            out.push(':');
            out.push_str(&value.len().to_string());
            out.push(':');
            out.push_str(value);
            out.push('\n');
        }
        let mut out = String::new();
        field(&mut out, "schema", "emath.codata.v1");
        field(&mut out, "adjustment", self.adjustment.as_str());
        field(&mut out, "name", self.name);
        field(&mut out, "symbol", self.symbol);
        field(&mut out, "value", &self.value);
        field(&mut out, "unit", self.unit);
        match &self.kind {
            CodataKind::Exact { source } => {
                field(&mut out, "kind", "exact");
                field(&mut out, "source", source);
            }
            CodataKind::Measured {
                uncertainty_digits,
                exponent,
            } => {
                field(&mut out, "kind", "measured");
                field(&mut out, "uncertainty_digits", uncertainty_digits);
                field(&mut out, "exponent", &exponent.to_string());
            }
        }
        out
    }

    /// FNV-1a64 identity over the canonical encoding.
    #[must_use]
    pub fn identity(&self) -> u64 {
        fnv1a64_bytes(self.canonical().as_bytes())
    }
}

/// The CODATA constant set pinned per adjustment. Only constants
/// verifiable against offline NIST records are carried; G is unchanged
/// between the 2018 and 2022 adjustments (NIST-recorded), so the two
/// catalogs carry the same values under distinct adjustment identities.
#[must_use]
pub fn codata_catalog(adjustment: CodataAdjustment) -> Vec<CodataConstant> {
    let exact_si_2019 =
        |name: &'static str, symbol: &'static str, value: &str, unit: &'static str| {
            CodataConstant {
                name,
                symbol,
                value: value.to_string(),
                unit,
                kind: CodataKind::Exact {
                    source: "SI 2019 definition",
                },
                adjustment,
            }
        };
    vec![
        CodataConstant {
            name: "speed of light in vacuum",
            symbol: "c",
            value: "299792458".to_string(),
            unit: "m/s",
            kind: CodataKind::Exact {
                source: "SI definition of the metre",
            },
            adjustment,
        },
        exact_si_2019("Planck constant", "h", "6.62607015e-34", "J*s"),
        exact_si_2019("Boltzmann constant", "k_B", "1.380649e-23", "J/K"),
        exact_si_2019("elementary charge", "e", "1.602176634e-19", "C"),
        exact_si_2019("Avogadro constant", "N_A", "6.02214076e23", "1/mol"),
        CodataConstant {
            name: "Newtonian constant of gravitation",
            symbol: "G",
            value: "6.67430e-11".to_string(),
            unit: "m^3/(kg*s^2)",
            kind: CodataKind::Measured {
                uncertainty_digits: "15",
                exponent: -11,
            },
            adjustment,
        },
    ]
}

/// `hbar` as the exact definitional alias of `h/(2*pi)`: the value is
/// COMPUTED from the `h` entry of the same adjustment, never stored as an
/// independent decimal, so the alias holds exactly at f64 precision by
/// construction.
#[must_use]
pub fn hbar(adjustment: CodataAdjustment) -> Option<CodataConstant> {
    let h = codata_catalog(adjustment)
        .into_iter()
        .find(|constant| constant.symbol == "h")?;
    let value_f64 = h.value_f64() / (2.0 * std::f64::consts::PI);
    Some(CodataConstant {
        name: "reduced Planck constant",
        symbol: "hbar",
        value: format!("{value_f64:e}"),
        unit: "J*s",
        kind: CodataKind::Exact {
            source: "exact alias of h/(2*pi)",
        },
        adjustment,
    })
}

/// Adjustment named by a `use sci::constants::<seg>::*` import segment.
/// `None` means unversioned (`sci::constants::*`) — a reproducibility
/// refusal at admission, never a default.
#[must_use]
pub fn codata_use_adjustment(segment: &str) -> Option<CodataAdjustment> {
    CodataAdjustment::parse(segment)
}

/// Scan `use` import path segments and report the first conflicting pair
/// of CODATA adjustments (or an unversioned constants import). The doctor
/// surface flags projects mixing `codata2018` with `codata2022`, and
/// refuses unversioned `sci::constants::*` imports — mixed adjustments
/// silently change uncertainties and unversioned imports silently change
/// values across library updates.
#[must_use]
pub fn mixed_codata_adjustments(use_segments: &[&str]) -> Option<String> {
    // A path must name exactly ONE adjustment version (`codata2018` or
    // `codata2022`). Zero version segments means the path carries no
    // adjustment at all; two distinct versions means the path mixes
    // adjustments and is ambiguous.
    let mut versions: Vec<&str> = Vec::new();
    for segment in use_segments {
        let version = segment.split("::").next().unwrap_or(segment);
        if version == "codata2018" || version == "codata2022" {
            if !versions.iter().any(|existing| *existing == version) {
                versions.push(version);
            }
        }
    }
    if versions.len() > 1 {
        return Some(format!(
            "mixed CODATA adjustments: {} — pick one adjustment per package",
            versions
                .iter()
                .map(|version| format!("`{version}`"))
                .collect::<Vec<_>>()
                .join(" and ")
        ));
    }
    if versions.is_empty() {
        return Some(
            "unversioned CODATA import (`sci::constants::*`): name an adjustment \
             (`codata2018` or `codata2022`); unversioned imports never default"
                .to_string(),
        );
    }
    None
}

/// A bare `constants` segment (no adjustment version) is an unversioned
/// import: the doctor refuses it rather than guessing the adjustment.
#[must_use]
pub fn unversioned_constants_import(use_segments: &[&str]) -> bool {
    let has_constants = use_segments.iter().any(|segment| *segment == "constants");
    let has_version = use_segments
        .iter()
        .any(|segment| segment.starts_with("codata"));
    has_constants && !has_version
}
