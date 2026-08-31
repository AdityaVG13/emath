//! Measures as DECLARED world data, integration with the measure as an
//! EXPLICIT argument, and declared-kernel transforms over the discrete
//! world (bead emath-r3-measures-transforms-r2mt, thin std-layer slice;
//! B20 measure worlds + the B25 kernel core).
//!
//! Honesty contract:
//! - **The measure is a world parameter, never ambient.** A measure
//!   exists only through its constructor, and every integrate/transform
//!   call takes it as an explicit argument (the `wrt mu` worlds
//!   discipline; the structural mirror of the language's MeaningHole
//!   refusal for an ambient integral).
//! - **Lebesgue and discrete are different worlds.` DiscreteMeasure and
//!   LebesgueOn are distinct types with no conversion impl between them —
//!   worlds do not merge silently. Riemann is deliberately absent: finite
//!   Riemann sums are the numeric-solver lanes' business, not a measure.
//! - **Exactness where declared.** Discrete integrals are exact atom
//!   sums; step-function integrals over a declared length world are
//!   exact value×length sums. General measurable functions are NOT
//!   claimed (no silent quadrature).
//! - **Transforms carry declared kernels.** Fourier kernel e^{-i t x},
//!   Laplace kernel e^{-s x}, over the discrete world, exact-by-
//!   definition atom sums. Kernel evaluations that leave the finite
//!   domain refuse typed. Continuous-kernel transform binders are
//!   language-lane design work (B25 NEEDS-DESIGN-WORK), fenced.

use crate::signal::Complex;

/// Typed refusal codes for the integral layer.
pub const E_INTEGRAL_MASS: &str = "E-INTEGRAL-1";
pub const E_INTEGRAL_DOMAIN: &str = "E-INTEGRAL-2";
pub const E_INTEGRAL_COVERAGE: &str = "E-INTEGRAL-3";
pub const E_INTEGRAL_KERNEL: &str = "E-INTEGRAL-4";

/// A discrete measure: masses on declared atoms. The only source of this
/// world is the constructor; masses are finite and non-negative.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscreteMeasure {
    atoms: Vec<(f64, f64)>,
}

impl DiscreteMeasure {
    pub fn new(atoms: Vec<(f64, f64)>) -> Result<Self, String> {
        for (point, mass) in &atoms {
            if !point.is_finite() || !mass.is_finite() || *mass < 0.0 {
                return Err(format!(
                    "{E_INTEGRAL_MASS}: atom ({point}, {mass}) needs a finite point \
                     and a finite non-negative mass"
                ));
            }
        }
        Ok(DiscreteMeasure { atoms })
    }

    pub fn atoms(&self) -> &[(f64, f64)] {
        &self.atoms
    }

    /// Total mass: the declared sum over atoms.
    pub fn total_mass(&self) -> f64 {
        self.atoms.iter().map(|(_, m)| m).sum()
    }
}

/// Lebesgue (length) measure on a declared non-degenerate interval
/// `[lo, hi]`. A different world from `DiscreteMeasure`: no conversion
/// impl exists between the two types.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LebesgueOn {
    lo: f64,
    hi: f64,
}

impl LebesgueOn {
    pub fn new(lo: f64, hi: f64) -> Result<Self, String> {
        if !lo.is_finite() || !hi.is_finite() || hi <= lo {
            return Err(format!(
                "{E_INTEGRAL_DOMAIN}: Lebesgue domain [{lo}, {hi}] must be a \
                 non-degenerate interval of finite endpoints"
            ));
        }
        Ok(LebesgueOn { lo, hi })
    }

    pub fn length(&self) -> f64 {
        self.hi - self.lo
    }
}

/// Integrates `f` against a DISCRETE measure: the exact atom sum
/// `Σ_j f(x_j)·m_j` (the measure is an explicit argument — `wrt mu`).
/// A function value that is not finite on an atom refuses, naming it.
pub fn integrate_discrete(f: impl Fn(f64) -> f64, mu: &DiscreteMeasure) -> Result<f64, String> {
    let mut total = 0.0;
    for &(point, mass) in mu.atoms() {
        let value = f(point);
        if !value.is_finite() {
            return Err(format!(
                "{E_INTEGRAL_MASS}: integrand is not finite at atom {point}"
            ));
        }
        total += value * mass;
    }
    Ok(total)
}

/// A step function: constant `value` on each declared cell `[lo, hi]`.
/// Cells must be finite, non-degenerate, sorted, and non-overlapping
/// (an overlap would make "the value at x" ambiguous). Gaps are allowed
/// at construction — the object may be partial — and coverage of a
/// declared integration domain is enforced by [`integrate_step`].
#[derive(Clone, Debug, PartialEq)]
pub struct StepFunction {
    cells: Vec<(f64, f64, f64)>,
}

impl StepFunction {
    pub fn new(mut cells: Vec<(f64, f64, f64)>) -> Result<Self, String> {
        for &(lo, hi, value) in &cells {
            if !lo.is_finite() || !hi.is_finite() || !value.is_finite() || hi <= lo {
                return Err(format!(
                    "{E_INTEGRAL_COVERAGE}: cell [{lo}, {hi}] with value {value} needs \
                     finite endpoints with lo < hi and a finite value"
                ));
            }
        }
        cells.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in cells.windows(2) {
            if pair[0].1 > pair[1].0 {
                return Err(format!(
                    "{E_INTEGRAL_COVERAGE}: cells [{}, {}] and [{}, {}] overlap — the \
                     value at the intersection is ambiguous",
                    pair[0].0, pair[0].1, pair[1].0, pair[1].1
                ));
            }
        }
        Ok(StepFunction { cells })
    }

    pub fn cells(&self) -> &[(f64, f64, f64)] {
        &self.cells
    }
}

/// Integrates a step function against a declared Lebesgue world: the
/// exact sum `Σ value·length`. The cells must COVER the declared domain
/// exactly — starting at `lo`, ending at `hi`, no gaps — and short,
/// gappy, or overhanging coverage refuses typed. Never a silent clip or
/// a silently-zero gap.
pub fn integrate_step(f: &StepFunction, mu: &LebesgueOn) -> Result<f64, String> {
    let cells = f.cells();
    match (cells.first(), cells.last()) {
        (Some(&(lo, _, _)), Some(&(_, hi, _))) if lo == mu.lo && hi == mu.hi => {}
        _ => {
            return Err(format!(
                "{E_INTEGRAL_COVERAGE}: step cells do not cover the declared domain \
                 [{}, {}] exactly",
                mu.lo, mu.hi
            ));
        }
    }
    for pair in cells.windows(2) {
        if pair[0].1 != pair[1].0 {
            return Err(format!(
                "{E_INTEGRAL_COVERAGE}: step cells gap at {} over the declared domain \
                 [{}, {}]",
                pair[0].1, mu.lo, mu.hi
            ));
        }
    }
    let mut integral = 0.0;
    for &(lo, hi, value) in cells {
        integral += value * (hi - lo);
    }
    Ok(integral)
}

/// Fourier transform of a DISCRETE measure with the declared kernel
/// `e^{-i t x}`: `(Fμ)(t) = Σ_j m_j·e^{-i t x_j}`, an exact-by-definition
/// atom sum. Kernel-sign witnesses are pinned by tests (e^{-iπ/2} = -i).
pub fn fourier_transform(mu: &DiscreteMeasure, t: f64) -> Result<Complex, String> {
    let mut total = Complex::new(0.0, 0.0);
    for &(point, mass) in mu.atoms() {
        let angle = -t * point;
        // e^{i angle} = cos(angle) + i·sin(angle)
        total = total + Complex::new(mass * angle.cos(), mass * angle.sin());
    }
    if !total.re.is_finite() || !total.im.is_finite() {
        return Err(format!(
            "{E_INTEGRAL_KERNEL}: Fourier kernel evaluation at t={t} left the finite domain"
        ));
    }
    Ok(total)
}

/// Laplace transform of a DISCRETE measure with the declared kernel
/// `e^{-s x}`: `(Lμ)(s) = Σ_j m_j·e^{-s x_j}` for complex `s`. Overflow
/// enforcement is single-point at the sum level: any kernel evaluation
/// that leaves the finite domain (inf, NaN, envelope overflow) poisons
/// the total and refuses typed — never a silent `inf`. (A kernel-level
/// guard is redundant; the mutation check proved the sum guard catches
/// every path.)
pub fn laplace_transform(mu: &DiscreteMeasure, s: Complex) -> Result<Complex, String> {
    let mut total = Complex::new(0.0, 0.0);
    for &(point, mass) in mu.atoms() {
        // e^{-s·x} = e^{-σx}·(cos(ωx) - i·sin(ωx)) for s = σ + iω.
        let decay = (-s.re * point).exp();
        let omega = -s.im * point;
        // e^{-iωx} with ωx = -omega: cos(omega) - i·(-sin(omega)) — the
        // imaginary part is decay·sin(omega).
        let kernel = Complex::new(decay * omega.cos(), decay * omega.sin());
        total = total + Complex::new(mass, 0.0) * kernel;
    }
    if !total.re.is_finite() || !total.im.is_finite() {
        return Err(format!(
            "{E_INTEGRAL_KERNEL}: Laplace sum at s={} left the finite domain",
            s
        ));
    }
    Ok(total)
}
