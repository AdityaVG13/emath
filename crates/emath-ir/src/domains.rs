//!: domains.
//!
//! Value domains: intervals, finite sets, boxes, unions and unrestricted
//! fields. Deterministic membership and canonicalization; violations use
//! stable `E-DOM-*` codes.

use emath_core::fnv1a64_bytes;

/// Absolute tolerance used by domain comparisons.
const EPSILON: f64 = 1e-12;

/// A single interval (open/closed bounds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    /// Lower bound.
    pub low: f64,
    /// Upper bound.
    pub high: f64,
    /// Lower bound excluded.
    pub low_open: bool,
    /// Upper bound excluded.
    pub high_open: bool,
}

impl Interval {
    /// Full real axis.
    #[must_use]
    pub const fn unrestricted() -> Self {
        Self {
            low: f64::NEG_INFINITY,
            high: f64::INFINITY,
            low_open: false,
            high_open: false,
        }
    }

    /// Closed interval.
    #[must_use]
    pub const fn closed(low: f64, high: f64) -> Self {
        Self {
            low,
            high,
            low_open: false,
            high_open: false,
        }
    }

    /// Whether `value` lies inside, within tolerance.
    #[must_use]
    pub fn contains(&self, value: f64) -> bool {
        let inside_low = if self.low_open {
            value > self.low + EPSILON
        } else {
            value >= self.low - EPSILON
        };
        let inside_high = if self.high_open {
            value < self.high - EPSILON
        } else {
            value <= self.high + EPSILON
        };
        inside_low && inside_high
    }
}

/// Value domain of a variable or expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Domain {
    /// Closed/open interval.
    Interval(Interval),
    /// Finite sorted set (deduplicated at construction).
    FiniteSet(Vec<f64>),
    /// Cartesian box of per-axis intervals.
    Box(Vec<Interval>),
    /// Union (order preserved).
    Union(Vec<Domain>),
    /// Unrestricted field (e.g. real numbers).
    Field,
}

impl Domain {
    /// Constructs a finite set, sorting and deduplicating entries.
    #[must_use]
    pub fn finite_set(mut values: Vec<f64>) -> Self {
        values.sort_by(|left, right| f64_cmp(*left, *right));
        values.dedup_by(|a, b| {
            let difference = *a - *b;
            difference.abs() <= EPSILON
        });
        Self::FiniteSet(values)
    }

    /// Membership check.
    #[must_use]
    pub fn contains(&self, value: f64) -> bool {
        match self {
            Self::Interval(interval) => interval.contains(value),
            Self::FiniteSet(values) => values.iter().any(|candidate| {
                let difference = candidate - value;
                difference.abs() <= EPSILON
            }),
            Self::Box(axes) => axes.iter().all(|axis| axis.contains(value)),
            Self::Union(parts) => parts.iter().any(|part| part.contains(value)),
            Self::Field => true,
        }
    }

    /// Checks membership, returning a typed `E-DOM-001` violation.
    pub fn require_contains(&self, value: f64, name: &str) -> Result<(), DomainError> {
        if self.contains(value) {
            Ok(())
        } else {
            Err(DomainError {
                code: "E-DOM-001",
                message: format!("{name} value {value} outside domain {self}"),
            })
        }
    }

    /// Deterministic lower bound for branch conventions (`-inf` when none).
    #[must_use]
    pub fn lower_bound(&self) -> f64 {
        match self {
            Self::Interval(interval) => interval.low,
            Self::FiniteSet(values) => values.first().copied().unwrap_or(f64::NEG_INFINITY),
            Self::Box(axes) => axes.first().map_or(f64::NEG_INFINITY, |axis| axis.low),
            Self::Union(parts) => parts
                .iter()
                .map(Self::lower_bound)
                .fold(f64::NEG_INFINITY, f64::max),
            Self::Field => f64::NEG_INFINITY,
        }
    }

    /// Canonical encoding.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Interval(interval) => format!(
                "dom:v1:interval:{:e}:{:e}:{}:{}",
                interval.low, interval.high, interval.low_open, interval.high_open
            ),
            Self::FiniteSet(values) => format!(
                "dom:v1:set:{}",
                values
                    .iter()
                    .map(|value| format!("{value:e}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Box(axes) => format!(
                "dom:v1:box:{}",
                axes.iter()
                    .map(|axis| { format!("{:e}..{:e}", axis.low, axis.high) })
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Union(parts) => format!(
                "dom:v1:union:{}",
                parts
                    .iter()
                    .map(Self::canonical)
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            Self::Field => "dom:v1:field".to_string(),
        }
    }

    /// FNV-1a64 identity.
    #[must_use]
    pub fn identity(&self) -> u64 {
        fnv1a64_bytes(self.canonical().as_bytes())
    }
}

impl std::fmt::Display for Domain {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.canonical())
    }
}

/// Deterministic float sort key.
fn f64_cmp(left: f64, right: f64) -> std::cmp::Ordering {
    left.partial_cmp(&right)
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// Domain violation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainError {
    /// Stable code (`E-DOM-001`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Branch convention used to pick a deterministic representative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchConvention {
    /// Lowest bound.
    Lower,
    /// Highest bound.
    Upper,
    /// Midpoint of the domain.
    Center,
    /// Deterministic canonical member (center or lower).
    Canonical,
}

/// Deterministic branch point for a domain, or `None` when unbounded.
#[must_use]
pub fn branch_point(domain: &Domain, convention: BranchConvention) -> Option<f64> {
    match convention {
        BranchConvention::Lower => {
            let bound = domain.lower_bound();
            f64::is_finite(bound).then_some(bound)
        }
        BranchConvention::Upper => {
            let bound = domain.upper_bound();
            f64::is_finite(bound).then_some(bound)
        }
        BranchConvention::Center => {
            let low = domain.lower_bound();
            let high = match domain {
                Domain::Field => return Some(0.0),
                _ => domain.upper_bound(),
            };
            if f64::is_finite(low) && f64::is_finite(high) {
                Some((low + high) / 2.0)
            } else {
                None
            }
        }
        BranchConvention::Canonical => {
            if let Domain::Interval(interval) = domain {
                if f64::is_finite(interval.low) && f64::is_finite(interval.high) {
                    return Some((interval.low + interval.high) / 2.0);
                }
            }
            branch_point(domain, BranchConvention::Lower)
        }
    }
}

impl Domain {
    /// Deterministic upper bound (`+inf` when none).
    #[must_use]
    pub fn upper_bound(&self) -> f64 {
        match self {
            Self::Interval(interval) => interval.high,
            Self::FiniteSet(values) => values.last().copied().unwrap_or(f64::INFINITY),
            Self::Box(axes) => axes.first().map_or(f64::INFINITY, |axis| axis.high),
            Self::Union(parts) => parts
                .iter()
                .map(Self::upper_bound)
                .fold(f64::INFINITY, f64::min),
            Self::Field => f64::INFINITY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_membership_obeys_openness() {
        let interval = Interval::closed(0.0, 1.0);
        assert!(interval.contains(0.0));
        assert!(interval.contains(0.5));
        assert!(!interval.contains(1.5));
        let open = Interval {
            low: 0.0,
            high: 1.0,
            low_open: true,
            high_open: true,
        };
        assert!(!open.contains(0.0));
        assert!(open.contains(0.5));
    }

    #[test]
    fn finite_sets_are_sorted_and_deduplicated() {
        let domain = Domain::finite_set(vec![3.0, 1.0, 2.0, 2.0]);
        match domain {
            Domain::FiniteSet(ref values) => {
                assert_eq!(values, &[1.0, 2.0, 3.0]);
            }
            _ => panic!("expected finite set"),
        }
        assert!(domain.contains(2.0));
        assert!(!domain.contains(2.5));
        let error = domain.require_contains(-1.0, "x").unwrap_err();
        assert_eq!(error.code, "E-DOM-001");
    }

    #[test]
    fn branch_conventions_are_deterministic() {
        let domain = Domain::Interval(Interval::closed(0.0, 10.0));
        assert_eq!(branch_point(&domain, BranchConvention::Lower), Some(0.0));
        assert_eq!(branch_point(&domain, BranchConvention::Upper), Some(10.0));
        assert_eq!(branch_point(&domain, BranchConvention::Center), Some(5.0));
        assert_eq!(
            branch_point(&Domain::Field, BranchConvention::Canonical),
            None
        );
    }

    #[test]
    fn canonical_identity_is_stable_and_discriminating() {
        let domain = Domain::Interval(Interval::closed(0.0, 1.0));
        let shifted = Domain::Interval(Interval::closed(0.0, 2.0));
        assert_eq!(domain.identity(), domain.identity());
        assert_ne!(domain.identity(), shifted.identity());
        let set = Domain::finite_set(vec![1.0, 2.0, 3.0]);
        assert_eq!(set.canonical(), "dom:v1:set:1e0,2e0,3e0");
    }
}
