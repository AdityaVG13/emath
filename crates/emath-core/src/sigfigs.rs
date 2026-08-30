//! Significant figures and unit-preserving formatting (bead
//! emath-r3-sigfigs-formatting-yf28, 04 sections 1.6 + 1.7).
//!
//! §1.6 Sig-figs are a DISPLAY CONTRACT, not uncertainty propagation:
//! `@significant_figures(display)` records the literal's sf count and
//! `emath fmt` rounds to the minimum input sf; enforce mode turns
//! under-reporting into a warning receipt. Sig-figs and uncertainty are
//! different evidence kinds and are never merged.
//!
//! §1.7 Unit-preserving formatting: `format: "0.1 %"` and
//! `format: preferred_unit min` change presentation only — the quantity's
//! value and identity are untouched, and the format is excluded from the
//! identity hash. A format unit incompatible with the quantity's dimension
//! is refused (`E-UNIT-FMT`).
//!
//! Documented sf convention: leading zeros never significant; trailing
//! zeros after a decimal point significant; trailing zeros of an integer
//! without a decimal point not significant (`1230` → 3, `1.230` → 4,
//! `0.0012` → 2, `1000.` → 4).
//!
//! Determinism: f64, fixed rules, Rust round-half-away-from-zero on the
//! `f64::round` step. No-claim boundary: std-side semantics layer;
//! file-level SIR recording during parse lands with the syntax/sema
//! integration slice.

#![forbid(unsafe_code)]

use crate::hash::fnv1a64_bytes;
use crate::units::{Quantity, UnitTable};

/// Refusal: format unit incompatible with the quantity's dimension, or a
/// malformed format spec.
pub const E_UNIT_FMT: &str = "E-UNIT-FMT";

/// Warning receipt (enforce mode): the literal carries fewer significant
/// figures than declared. A receipt, never a refusal.
pub const E_SF_UNDER_REPORT: &str = "E-SF-UNDER-REPORT";

/// Warning receipt: Measured (uncertainty) values mixed with bare
/// sf-values in one precision context. A receipt, never a refusal.
pub const E_SF_MIXED_KINDS: &str = "E-SF-MIXED-KINDS";

/// Sig-fig attribute mode: `display` (record + round) or `enforce`
/// (under-report = warning receipt).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigFigMode {
    Display,
    Enforce,
}

/// A recorded `@significant_figures` spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SigFigSpec {
    pub mode: SigFigMode,
    pub count: u32,
}

impl SigFigSpec {
    /// Enforce-mode check: a literal with fewer significant figures than
    /// declared produces a warning receipt. More-or-equal sf is admitted.
    #[must_use]
    pub fn enforce_check(&self, literal_sf: u32) -> Option<PrecisionWarning> {
        if self.mode == SigFigMode::Enforce && literal_sf < self.count {
            Some(PrecisionWarning::UnderReported {
                declared: self.count,
                literal: literal_sf,
            })
        } else {
            None
        }
    }
}

/// Precision warnings are receipts, never refusals: sig-figs are a display
/// contract and mixing kinds is a communication hazard, not a math error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrecisionWarning {
    /// Enforce mode: the literal carries fewer sf than declared.
    UnderReported { declared: u32, literal: u32 },
    /// Measured (uncertainty) values mixed with bare sf-values in one
    /// context — different evidence kinds, must be labeled separately.
    MixedMeasuredBareSf { measured: usize, bare_sf: usize },
}

/// Count significant figures in a decimal literal per the documented
/// convention. Returns `None` for non-numeric text or literals with no
/// nonzero digit (no precision information).
#[must_use]
pub fn count_sig_figs(literal: &str) -> Option<u32> {
    let body = literal.trim().trim_start_matches(['+', '-']);
    let mantissa = body.split(['e', 'E']).next()?;
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((int_part, frac_part)) => (int_part, Some(frac_part)),
        None => (mantissa, None),
    };
    let digits: String = format!("{integer}{}", fraction.unwrap_or(""));
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let Some(first_nonzero) = digits.bytes().position(|b| b != b'0') else {
        return None; // "0", "0.0": no precision information
    };
    let significant = &digits[first_nonzero..];
    let sf = match fraction {
        // With a decimal point every digit from the first nonzero onward is
        // significant, including trailing zeros ("1.230" → 4, "1000." → 4).
        Some(_) => significant.len(),
        // Bare integer: trailing zeros are scale, not precision ("1230" → 3).
        None => significant.trim_end_matches('0').len().max(1),
    };
    Some(sf as u32)
}

/// Round to `sf` significant figures (half away from zero on the retained
/// digit). `sf == 0` returns the value unchanged; ±0.0 returns 0.0.
/// Implementation formats to `sf-1` decimals in scientific notation and
/// re-parses, which yields exactly the double the rounded decimal literal
/// denotes (no rescale-multiply artifacts).
#[must_use]
pub fn round_to_sig_figs(value: f64, sf: u32) -> f64 {
    if sf == 0 || value == 0.0 || !value.is_finite() {
        return value;
    }
    format!("{:.*e}", (sf - 1) as usize, value)
        .parse()
        .unwrap_or(value)
}

/// A presentation-only format: `preferred_unit <unit>` or a decimal
/// pattern `0.<k>` with an optional literal suffix (`0.1 %`).
#[derive(Clone, Debug, PartialEq)]
pub enum FormatSpec {
    PreferredUnit { unit: String },
    Pattern { decimals: u32, suffix: String },
}

/// Malformed format spec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatError {
    pub code: &'static str,
    pub message: String,
}

impl FormatSpec {
    /// Parse a format spec. `preferred_unit` requires exactly one unit
    /// token; a decimal pattern is `0` or `0.0…` followed by an optional
    /// suffix (suffix tokens are joined with single spaces).
    pub fn parse(spec: &str) -> Result<Self, FormatError> {
        let malformed = |message: &str| FormatError {
            code: E_UNIT_FMT,
            message: format!("malformed format spec `{spec}`: {message}"),
        };
        let mut tokens = spec.split_whitespace();
        let Some(head) = tokens.next() else {
            return Err(malformed("empty"));
        };
        if head == "preferred_unit" {
            let unit = tokens.next().ok_or_else(|| {
                malformed("preferred_unit requires a unit (`preferred_unit min`)")
            })?;
            if tokens.next().is_some() {
                return Err(malformed("preferred_unit takes exactly one unit"));
            }
            if !unit.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'%' || b == b'/') {
                return Err(malformed("unit tokens must be alphanumeric/_/%//"));
            }
            return Ok(FormatSpec::PreferredUnit {
                unit: unit.to_string(),
            });
        }
        // Decimal pattern: `0` or `0.<digits>` — the digit COUNT after the
        // dot is the displayed decimal count (`0.1` = one decimal,
        // `0.01` = two). Digits themselves are position markers.
        let valid = head == "0"
            || (head.starts_with("0.")
                && head.len() > 2
                && head.bytes().skip(2).all(|b| b.is_ascii_digit()));
        if !valid {
            return Err(malformed("pattern must look like `0.1` (zeros after the dot)"));
        }
        let decimals = head
            .split_once('.')
            .map(|(_, frac)| frac.len() as u32)
            .unwrap_or(0);
        let suffix = tokens.collect::<Vec<_>>().join(" ");
        Ok(FormatSpec::Pattern { decimals, suffix })
    }
}

/// A quantity paired with a presentation format. Identity is the
/// quantity's identity; the format never enters any hash.
#[derive(Clone, Debug, PartialEq)]
pub struct FormattedQuantity {
    pub quantity: Quantity,
    pub format: FormatSpec,
}

/// Identity of a quantity independent of any presentation format.
#[must_use]
pub fn quantity_identity(quantity: &Quantity) -> u64 {
    let canonical = format!("{quantity:?}");
    fnv1a64_bytes(canonical.as_bytes())
}

impl FormattedQuantity {
    /// Identity excludes the format by construction: same value + unit +
    /// kind under two formats is the same quantity.
    #[must_use]
    pub fn identity(&self) -> u64 {
        quantity_identity(&self.quantity)
    }

    /// Render the presentation string.
    ///
    /// - `preferred_unit <unit>`: resolve, check dimension compatibility,
    ///   convert through SI, render (rounded to `sf_digits` when given).
    /// - pattern: fixed decimals on the value in its own unit, plus the
    ///   literal suffix. Presentation only — never mutates the quantity.
    pub fn display(
        &self,
        table: &UnitTable,
        sf_digits: Option<u32>,
    ) -> Result<String, FormatError> {
        let incompatible = |message: String| FormatError {
            code: E_UNIT_FMT,
            message,
        };
        match &self.format {
            FormatSpec::PreferredUnit { unit } => {
                let target = table.resolve(unit).map_err(|err| {
                    incompatible(format!("format unit `{unit}` is not available: {}", err.message))
                })?;
                if target.dims != self.quantity.unit.dims {
                    return Err(incompatible(format!(
                        "format unit `{}` is not dimension-compatible with `{}`",
                        target.name, self.quantity.unit.name
                    )));
                }
                let si = self.quantity.unit.to_si(self.quantity.value);
                let mut out = target.from_si(si);
                if let Some(sf) = sf_digits.filter(|sf| *sf > 0) {
                    out = round_to_sig_figs(out, sf);
                }
                Ok(format!("{out} {unit}"))
            }
            FormatSpec::Pattern { decimals, suffix } => {
                let body = format!("{:.*}", *decimals as usize, self.quantity.value);
                if suffix.is_empty() {
                    Ok(body)
                } else {
                    Ok(format!("{body} {suffix}"))
                }
            }
        }
    }
}

/// Tracks how many Measured vs bare-sf values entered one context.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrecisionLedger {
    measured: usize,
    bare_sf: usize,
}

impl PrecisionLedger {
    pub fn record_measured(&mut self) {
        self.measured += 1;
    }

    pub fn record_bare_sf(&mut self) {
        self.bare_sf += 1;
    }

    /// Mixing kinds warns as long as both are present.
    #[must_use]
    pub fn mix_warning(&self) -> Option<PrecisionWarning> {
        if self.measured > 0 && self.bare_sf > 0 {
            Some(PrecisionWarning::MixedMeasuredBareSf {
                measured: self.measured,
                bare_sf: self.bare_sf,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sf_convention_holds() {
        assert_eq!(count_sig_figs("1230"), Some(3));
        assert_eq!(count_sig_figs("1.230"), Some(4));
        assert_eq!(count_sig_figs("0.0012"), Some(2));
        assert_eq!(count_sig_figs("1000."), Some(4));
        assert_eq!(count_sig_figs("abc"), None);
        assert_eq!(count_sig_figs("0.0"), None);
    }

    #[test]
    fn enforce_ladder() {
        let spec = SigFigSpec {
            mode: SigFigMode::Enforce,
            count: 3,
        };
        assert!(spec.enforce_check(2).is_some());
        assert!(spec.enforce_check(3).is_none());
        let display = SigFigSpec {
            mode: SigFigMode::Display,
            count: 3,
        };
        assert!(display.enforce_check(1).is_none(), "display mode never warns");
    }
}
