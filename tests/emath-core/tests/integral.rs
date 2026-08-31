//! emath-r3-measures-transforms-r2mt (thin std-layer slice): measures as
//! DECLARED world data, integration with the measure as an EXPLICIT
//! argument (the `wrt mu` worlds discipline at the std layer), and the
//! declared-kernel Fourier/Laplace transforms over the discrete world.
//!
//! Lebesgue and discrete measures are DIFFERENT TYPES with no conversion
//! impl between them — worlds do not merge silently. There is no ambient
//! measure anywhere: the constructor is the only source of a measure, and
//! every integrate/transform call takes it as a parameter (the structural
//! mirror of the language's MeaningHole refusal for an ambient integral).

use emath_core::integral::{
    fourier_transform, integrate_discrete, integrate_step, laplace_transform, DiscreteMeasure,
    LebesgueOn, StepFunction, E_INTEGRAL_COVERAGE, E_INTEGRAL_DOMAIN, E_INTEGRAL_KERNEL,
    E_INTEGRAL_MASS,
};
use emath_core::signal::Complex;

/// Measures are DECLARED and VALIDATED: a negative or non-finite mass
/// refuses, a non-finite atom refuses, and the total mass is the declared
/// sum. There is no ambient measure anywhere (structural).
#[test]
fn discrete_measure_is_declared_and_validated() {
    assert!(
        DiscreteMeasure::new(vec![(0.0, -1.0)])
            .unwrap_err()
            .contains(E_INTEGRAL_MASS),
        "negative mass refuses typed"
    );
    assert!(
        DiscreteMeasure::new(vec![(f64::NAN, 1.0)])
            .unwrap_err()
            .contains(E_INTEGRAL_MASS),
        "non-finite atom refuses typed"
    );
    assert!(
        DiscreteMeasure::new(vec![(0.0, f64::INFINITY)])
            .unwrap_err()
            .contains(E_INTEGRAL_MASS),
        "non-finite mass refuses typed"
    );
    let mu = DiscreteMeasure::new(vec![(0.0, 1.0), (2.0, 3.0)]).expect("declared measure");
    assert_eq!(mu.total_mass(), 4.0);
    assert_eq!(mu.atoms(), &[(0.0, 1.0), (2.0, 3.0)]);
}

/// Discrete-world integration is the declared sum, with the measure as an
/// EXPLICIT argument (wrt mu, never ambient). The zero measure is legal
/// and integrates to zero. A function that is not finite on an atom
/// refuses naming the atom.
#[test]
fn discrete_measure_integral_is_the_declared_sum() {
    let mu = DiscreteMeasure::new(vec![(0.0, 1.5), (2.0, 0.5)]).expect("declared");
    // ∫ x² dμ = 1.5·0² + 0.5·2² = 2.0, exact.
    let integral = integrate_discrete(|x| x * x, &mu).expect("finite on atoms");
    assert_eq!(integral, 2.0);
    // The zero measure is a legal world: the integral is zero.
    let zero = DiscreteMeasure::new(vec![]).expect("zero measure");
    assert_eq!(integrate_discrete(|x| x, &zero).expect("zero"), 0.0);
    // Non-finite on an atom refuses with the atom named.
    let err = integrate_discrete(|x| if x == 2.0 { f64::NAN } else { x }, &mu).unwrap_err();
    assert!(err.contains(E_INTEGRAL_MASS), "{err}");
    assert!(err.contains("2"), "refusal names the bad atom: {err}");
}

/// Lebesgue-world integration of a declared step function is EXACT
/// (Σ value·length over cells tiling the domain). Cells that gap, overlap,
/// or fail to cover the declared domain refuse typed — coverage is part
/// of the world contract, never repaired silently.
#[test]
fn step_integration_over_declared_lebesgue_is_exact() {
    // Bad domains refuse: inverted, empty, non-finite.
    assert!(
        LebesgueOn::new(2.0, 1.0).unwrap_err().contains(E_INTEGRAL_DOMAIN),
        "inverted domain refuses"
    );
    assert!(
        LebesgueOn::new(1.0, 1.0).unwrap_err().contains(E_INTEGRAL_DOMAIN),
        "empty domain refuses"
    );
    let mu = LebesgueOn::new(0.0, 2.0).expect("declared length world");
    // Malformed step data refuses at construction.
    assert!(StepFunction::new(vec![(1.0, 0.0, 1.0)]).is_err(), "inverted cell");
    assert!(StepFunction::new(vec![(0.0, 1.0, f64::NAN)]).is_err(), "non-finite value");
    assert!(
        StepFunction::new(vec![(0.0, 1.0, 1.0), (0.5, 2.0, 1.0)]).is_err(),
        "overlapping cells refuse"
    );
    // ∫ over [0,2] of the step (0,1)->3, (1,2)->-1 is 3·1 + (-1)·1 = 2.
    let f = StepFunction::new(vec![(0.0, 1.0, 3.0), (1.0, 2.0, -1.0)]).expect("tiles");
    assert_eq!(integrate_step(&f, &mu).expect("covered"), 2.0);
    // A constant on [1,3]: 2.5·2 = 5.0 exact.
    let mu2 = LebesgueOn::new(1.0, 3.0).expect("declared");
    let c = StepFunction::new(vec![(1.0, 3.0, 2.5)]).expect("tiles");
    assert_eq!(integrate_step(&c, &mu2).expect("covered"), 5.0);
    // Coverage gaps and short coverage refuse — never silently clipped.
    let gappy = StepFunction::new(vec![(0.0, 1.0, 1.0), (1.5, 2.0, 1.0)]).expect("cells valid");
    let err = integrate_step(&gappy, &mu).unwrap_err();
    assert!(err.contains(E_INTEGRAL_COVERAGE), "{err}");
    let short = StepFunction::new(vec![(0.0, 1.0, 1.0)]).expect("cells valid");
    assert!(integrate_step(&short, &mu).unwrap_err().contains(E_INTEGRAL_COVERAGE));
}

/// Fourier transform over the DISCRETE world with the declared kernel
/// e^{-i t x}: point mass at the origin is 1 everywhere; a mass at a=1
/// evaluated at t=π/2 is exactly -i (the kernel SIGN is pinned here);
/// a symmetric pair is real.
#[test]
fn fourier_transform_known_pairs() {
    let origin = DiscreteMeasure::new(vec![(0.0, 1.0)]).expect("declared");
    let f0 = fourier_transform(&origin, 0.0).expect("finite");
    assert!((f0.re - 1.0).abs() < 1e-12 && f0.im.abs() < 1e-12);
    let fhalf = fourier_transform(&origin, std::f64::consts::FRAC_PI_2).expect("finite");
    assert!((fhalf.re - 1.0).abs() < 1e-12 && fhalf.im.abs() < 1e-12);
    // Kernel sign witness: m=1 at a=1, t=π/2 -> e^{-iπ/2} = -i.
    let at_one = DiscreteMeasure::new(vec![(1.0, 1.0)]).expect("declared");
    let f = fourier_transform(&at_one, std::f64::consts::FRAC_PI_2).expect("finite");
    assert!(f.re.abs() < 1e-12, "real part vanishes: {f}");
    assert!((f.im + 1.0).abs() < 1e-12, "imaginary part is -1: {f}");
    // Symmetric pair (mass 1/2 at ±1), t = π: 0.5(e^{-iπ} + e^{iπ}) = -1.
    let pair = DiscreteMeasure::new(vec![(-1.0, 0.5), (1.0, 0.5)]).expect("declared");
    let fp = fourier_transform(&pair, std::f64::consts::PI).expect("finite");
    assert!((fp.re + 1.0).abs() < 1e-12, "{fp}");
    assert!(fp.im.abs() < 1e-12, "{fp}");
}

/// Laplace transform over the DISCRETE world with the declared kernel
/// e^{-s x}: known pairs exactly, and a kernel evaluation that leaves the
/// finite domain refuses typed (never inf).
#[test]
fn laplace_transform_known_pairs() {
    let at_origin = DiscreteMeasure::new(vec![(0.0, 1.0)]).expect("declared");
    let l = laplace_transform(&at_origin, emath_core::signal::Complex::new(1.0, 0.0))
        .expect("finite");
    assert!((l.re - 1.0).abs() < 1e-12 && l.im.abs() < 1e-12);
    // Mass 1 at a=1, real s=0.5: e^{-0.5}.
    let at_one = DiscreteMeasure::new(vec![(1.0, 1.0)]).expect("declared");
    let l2 = laplace_transform(&at_one, emath_core::signal::Complex::new(0.5, 0.0))
        .expect("finite");
    assert!((l2.re - (-0.5f64).exp()).abs() < 1e-12, "{l2}");
    // Complex s = 1 + iπ at a=1: e^{-(1+iπ)} = -e^{-1}.
    let s = emath_core::signal::Complex::new(1.0, std::f64::consts::PI);
    let l3 = laplace_transform(&at_one, s).expect("finite");
    assert!((l3.re + (-1.0f64).exp()).abs() < 1e-12, "{l3}");
    assert!(l3.im.abs() < 1e-12, "{l3}");
    // Kernel-sign witness: s = 1 + iπ/2 at a=1: e^{-1}·e^{-iπ/2} = -i·e^{-1}.
    let s2 = emath_core::signal::Complex::new(1.0, std::f64::consts::FRAC_PI_2);
    let l4 = laplace_transform(&at_one, s2).expect("finite");
    assert!(l4.re.abs() < 1e-12, "{l4}");
    assert!((l4.im + (-1.0f64).exp()).abs() < 1e-12, "{l4}");
    // A kernel evaluation that overflows refuses typed: s = -1000, x = 1.
    let err = laplace_transform(&at_one, emath_core::signal::Complex::new(-1000.0, 0.0))
        .unwrap_err();
    assert!(err.contains(E_INTEGRAL_KERNEL), "{err}");
}
