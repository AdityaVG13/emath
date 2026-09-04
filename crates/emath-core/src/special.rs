//! `core::special_functions` (05 section 3.3 #1, Phase 11) — contracts
//! and strict-f64 reference implementations.
//!
//! Every result carries an EXPLICIT numeric error bound; nothing here
//! claims correctly-rounded output where that is not proven. Principal
//! branches are named (`W₀`), carriers are declared (real slices), and
//! poles/branch-cut exits refuse with named reasons — never an implicit
//! continuation. Std only, `forbid(unsafe_code)` honored.
//!
//! The [`SpecialFunctionEvaluator`] trait is the provider seam: the
//! strict-f64 reference impls live here; high-precision and
//! interval-certified backends implement the same contract without
//! becoming core semantics.

/// The special functions with contracts in this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialFn {
    /// Γ(z) — gamma; poles at 0, −1, −2, …
    Gamma,
    /// B(a, b) — beta; positive-real slice a > 0, b > 0.
    Beta,
    /// erf(x) — error function; entire, real line.
    Erf,
    /// ζ(s) — Riemann zeta; simple pole at s = 1; reference carrier is
    /// real s > 1 (the η-series branch).
    Zeta,
    /// W₀(z) — Lambert W, PRINCIPAL branch; real carrier z ≥ −1/e.
    LambertW0,
    /// K(m) — complete elliptic integral of the first kind (parameter
    /// convention); carrier m ∈ [0, 1); K(1) diverges.
    EllipticK,
    /// E(m) — complete elliptic integral of the second kind (parameter
    /// convention); carrier m ∈ [0, 1).
    EllipticE,
    /// Π(n, m) — complete elliptic integral of the third kind.
    /// Contract-only: no reference impl yet (see the no-claim section
    /// of the contract cell).
    EllipticPi,
}

/// Why an evaluation refused. Named, never silent: a pole is not a
/// large number, and a branch exit is not a continuation.
#[derive(Clone, Debug, PartialEq)]
pub enum DomainRefusal {
    /// The argument hits a pole of the function.
    Pole { function: &'static str, at: f64 },
    /// The argument is outside the declared real carrier slice.
    OutsideCarrier {
        function: &'static str,
        carrier: &'static str,
        argument: f64,
    },
    /// Contract exists, reference implementation does not (yet).
    NotImplemented { function: &'static str },
    /// Wrong argument count for the function.
    Arity {
        function: &'static str,
        expected: usize,
        found: usize,
    },
}

/// One evaluation: the value plus the DECLARED error bound. The bound
/// covers the true deviation from the exact special-function value
/// (verified in the contract tests against independently-known
/// references); it is a labeled bound, not a correct-rounding claim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Evaluated {
    pub value: f64,
    pub error_bound: f64,
}

/// The provider seam (05 §3.3 #1): high-precision / interval-certified
/// backends implement this; core semantics never bake a backend in.
pub trait SpecialFunctionEvaluator {
    fn evaluate(&self, function: SpecialFn, args: &[f64]) -> Result<Evaluated, DomainRefusal>;
}

/// The strict-f64 reference implementation: std-only series /
/// continued-fraction / AGM evaluation with certified error bounds.
pub struct StrictF64Reference;

impl SpecialFunctionEvaluator for StrictF64Reference {
    fn evaluate(&self, function: SpecialFn, args: &[f64]) -> Result<Evaluated, DomainRefusal> {
        let name = function_name(function);
        let expected = match function {
            SpecialFn::Beta => 2,
            SpecialFn::EllipticPi => 3,
            _ => 1,
        };
        if args.len() != expected {
            return Err(DomainRefusal::Arity {
                function: name,
                expected,
                found: args.len(),
            });
        }
        match function {
            SpecialFn::Gamma => gamma_eval(args[0]),
            SpecialFn::Beta => beta_eval(args[0], args[1]),
            SpecialFn::Erf => erf_eval(args[0]),
            SpecialFn::Zeta => zeta_eval(args[0]),
            SpecialFn::LambertW0 => lambert_w0_eval(args[0]),
            SpecialFn::EllipticK => elliptic_k_eval(args[0]),
            SpecialFn::EllipticE => elliptic_e_eval(args[0]),
            SpecialFn::EllipticPi => Err(DomainRefusal::NotImplemented { function: name }),
        }
    }
}

/// Evaluate through the strict-f64 reference without requiring callers
/// to import the provider trait. Generated Rust artifacts use this
/// entry point from their embedded evaluator module.
pub fn evaluate_strict(function: SpecialFn, args: &[f64]) -> Result<Evaluated, DomainRefusal> {
    StrictF64Reference.evaluate(function, args)
}

fn function_name(function: SpecialFn) -> &'static str {
    match function {
        SpecialFn::Gamma => "gamma",
        SpecialFn::Beta => "beta",
        SpecialFn::Erf => "erf",
        SpecialFn::Zeta => "zeta",
        SpecialFn::LambertW0 => "lambert_w0",
        SpecialFn::EllipticK => "elliptic_k",
        SpecialFn::EllipticE => "elliptic_e",
        SpecialFn::EllipticPi => "elliptic_pi",
    }
}

// ---- Γ (Stirling + upward recurrence) --------------------------------

/// Γ(z) via the recurrence to `w = z + n ≥ 12` and the asymptotic
/// Stirling expansion there. The Bernoulli series lives in the
/// EXPONENT (`log Γ = (w−1/2)ln w − w + ½ln 2π + Σ B_{2k}/(2k(2k−1)
/// w^{2k−1}}`); truncating that sum after the B14 term contributes
/// ≤1.9e-21 relative error at w ≥ 12 (terms strictly decreasing
/// there), so `exp` of the truncated sum is the certified correction —
/// NOT a multiplicative first-order stand-in. The declared bound also
/// covers the ≤12 recurrence divisions and exp/pow roundoff
/// (~3e-15 relative total); 1e-14 is declared with margin.
fn stirling_gamma(w: f64) -> f64 {
    let log_correction = 1.0 / (12.0 * w) - 1.0 / (360.0 * w.powi(3)) + 1.0 / (1260.0 * w.powi(5))
        - 1.0 / (1680.0 * w.powi(7))
        + 1.0 / (1188.0 * w.powi(9))
        - 691.0 / (360_360.0 * w.powi(11))
        + 1.0 / (156.0 * w.powi(13));
    (2.0 * std::f64::consts::PI).sqrt() * w.powf(w - 0.5) * (-w).exp() * log_correction.exp()
}

fn gamma_eval(z: f64) -> Result<Evaluated, DomainRefusal> {
    if z <= 0.0 && z.fract() == 0.0 {
        return Err(DomainRefusal::Pole {
            function: "gamma",
            at: z,
        });
    }
    if !z.is_finite() {
        return Err(DomainRefusal::OutsideCarrier {
            function: "gamma",
            carrier: "finite real z; poles at 0, −1, −2, …",
            argument: z,
        });
    }
    if z < 0.5 {
        // Reflection: Γ(z) = π / (sin(πz) Γ(1−z)).
        let mirror = gamma_direct(1.0 - z);
        let sin_pi_z = (std::f64::consts::PI * z).sin();
        // sin(πz) ≈ 0 near negative integers: the value explodes; the
        // declared carrier for the reflection branch stops at |sin| ≥
        // 1e-3 (between-pole points stay admitted, bound inflated).
        if sin_pi_z.abs() < 1e-3 {
            return Err(DomainRefusal::OutsideCarrier {
                function: "gamma",
                carrier: "reflection branch needs |sin(πz)| ≥ 1e-3 (near-pole points refuse)",
                argument: z,
            });
        }
        let value = std::f64::consts::PI / (sin_pi_z * mirror);
        // Bound: direct-branch relative bound (1e-14) plus reflection
        // conditioning (1/|sin| amplification).
        let bound = value.abs() * 1e-14 * (1.0 / sin_pi_z.abs());
        return Ok(Evaluated {
            value,
            error_bound: bound,
        });
    }
    let value = gamma_direct(z);
    Ok(Evaluated {
        value,
        error_bound: value.abs() * 1e-14,
    })
}

fn gamma_direct(z: f64) -> f64 {
    // Shift up to w ≥ 12: Γ(z) = Γ(w) / (z·(z+1)···(w−1)).
    let mut w = z;
    let mut product = 1.0_f64;
    while w < 12.0 {
        product *= w;
        w += 1.0;
    }
    stirling_gamma(w) / product
}

// ---- B(a, b) ---------------------------------------------------------

fn beta_eval(a: f64, b: f64) -> Result<Evaluated, DomainRefusal> {
    if a <= 0.0 || b <= 0.0 {
        return Err(DomainRefusal::OutsideCarrier {
            function: "beta",
            carrier: "positive-real slice a > 0, b > 0",
            argument: if a <= 0.0 { a } else { b },
        });
    }
    let ga = gamma_eval(a)?;
    let gb = gamma_eval(b)?;
    let gab = gamma_eval(a + b)?;
    let value = ga.value * gb.value / gab.value;
    // First-order composition of the relative bounds.
    let bound = value.abs()
        * (ga.error_bound / ga.value.abs()
            + gb.error_bound / gb.value.abs()
            + gab.error_bound / gab.value.abs());
    Ok(Evaluated {
        value,
        error_bound: bound,
    })
}

// ---- erf -------------------------------------------------------------

fn erf_eval(x: f64) -> Result<Evaluated, DomainRefusal> {
    if !x.is_finite() {
        return Err(DomainRefusal::OutsideCarrier {
            function: "erf",
            carrier: "finite real x (erf is entire)",
            argument: x,
        });
    }
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let ax = x.abs();
    if ax >= 4.0 {
        // Tail certificate: 1 − erf(4) ≈ 1.54e-8 and decreasing; the
        // constant 1 with that bound is the declared value here.
        return Ok(Evaluated {
            value: sign as f64,
            error_bound: 1.55e-8,
        });
    }
    // Maclaurin series, terms via the ratio t_{n+1} = t_n·(−x²(2n+1))/(n+1)(2n+3):
    // t_n = (−1)^n x^{2n+1}/(n!(2n+1)). Alternating with eventually
    // decreasing magnitude on |x| ≤ 4; the alternating-tail bound is
    // certified once terms decrease (tracked explicitly).
    let inv_sqrt_pi = 2.0 / std::f64::consts::PI.sqrt();
    let mut term = ax; // t_0
    let mut sum = ax;
    let mut n = 0.0_f64;
    let mut decreasing = false;
    let mut next_term;
    loop {
        next_term = -term * (ax * ax) * (2.0 * n + 1.0) / ((n + 1.0) * (2.0 * n + 3.0));
        n += 1.0;
        if next_term.abs() <= term.abs() {
            decreasing = true;
        }
        term = next_term;
        sum += term;
        if decreasing && term.abs() < 1e-18 {
            break;
        }
        if n > 400.0 {
            break;
        }
    }
    let bound = inv_sqrt_pi * term.abs();
    // Roundoff honesty: ~N additions each with ≤½ulp relative error
    // make the accumulated error up to ~Σ|terms|·1e-16 — for large
    // args the alternating remainder alone understates it. Declare the
    // larger of the two.
    let roundoff = inv_sqrt_pi * sum.abs() * (n * 1e-16);
    Ok(Evaluated {
        value: sign as f64 * inv_sqrt_pi * sum,
        error_bound: bound.max(roundoff),
    })
}

// ---- ζ(s) ------------------------------------------------------------

fn zeta_eval(s: f64) -> Result<Evaluated, DomainRefusal> {
    if !s.is_finite() {
        return Err(DomainRefusal::OutsideCarrier {
            function: "zeta",
            carrier: "real s > 1",
            argument: s,
        });
    }
    if s == 1.0 {
        return Err(DomainRefusal::Pole {
            function: "zeta",
            at: s,
        });
    }
    if s <= 1.0 {
        return Err(DomainRefusal::OutsideCarrier {
            function: "zeta",
            carrier: "real s > 1 (η-series reference branch)",
            argument: s,
        });
    }
    // ζ(s) = η(s)/(1 − 2^{1−s}); η alternating with |R_N| ≤ (N+1)^{−s}.
    let divisor = 1.0 - (1.0 - s).exp2();
    let mut n = 1.0_f64;
    let mut eta = 0.0_f64;
    let mut tail_bound;
    loop {
        let term = 1.0 / n.powf(s);
        eta += if ((n as u64) % 2) == 1 { term } else { -term };
        tail_bound = (n + 1.0).powf(-s) / divisor.abs();
        if tail_bound < 1e-14 * eta.abs().max(1e-300) || n > 8.0e6 {
            break;
        }
        n += 1.0;
    }
    let value = eta / divisor;
    // Roundoff honesty: the η partial sum has up to ~n·½ulp relative
    // error, amplified through the (fixed) divisor; the declared bound
    // is the larger of the alternating tail and the roundoff.
    let roundoff = eta.abs() * (n * 1e-16) / divisor.abs();
    Ok(Evaluated {
        value,
        error_bound: tail_bound.max(roundoff),
    })
}

// ---- W₀(z) -----------------------------------------------------------

fn lambert_w0_eval(z: f64) -> Result<Evaluated, DomainRefusal> {
    if !z.is_finite() {
        return Err(DomainRefusal::OutsideCarrier {
            function: "lambert_w0",
            carrier: "real z ≥ −1/e (principal branch)",
            argument: z,
        });
    }
    let branch_point = -std::f64::consts::E.recip();
    if z < branch_point {
        return Err(DomainRefusal::OutsideCarrier {
            function: "lambert_w0",
            carrier: "real z ≥ −1/e (principal branch W₀; branch cut at (−∞, −1/e))",
            argument: z,
        });
    }
    if z == branch_point {
        return Ok(Evaluated {
            value: -1.0,
            error_bound: 0.0,
        });
    }
    if z == 0.0 {
        return Ok(Evaluated {
            value: 0.0,
            error_bound: 0.0,
        });
    }
    // Initial guess: series near 0, log form for larger arguments
    // (`ln(1+z)` is finite and positive for z > −1/e + …, never the
    // ln(1) = 0 → ln ln z = −∞ blowup of the naive asymptotic form).
    let mut w = if z.abs() < 1.0 {
        let z2 = z * z;
        z - z2 + 1.5 * z2 * z - 8.0 / 3.0 * z2 * z2
    } else {
        (1.0 + z).ln()
    };
    // Halley iterations (cubic convergence), to fixed point.
    for _ in 0..100 {
        let e_w = w.exp();
        let p = w * e_w;
        let delta = p - z;
        let numerator = delta / (e_w * (w + 1.0) - (w + 2.0) * delta / (2.0 * w + 2.0));
        let next = w - numerator;
        if (next - w).abs() <= 1e-16 * (1.0 + next.abs()) {
            w = next;
            break;
        }
        w = next;
    }
    // Residual certificate: |w − W(z)| ≈ |w e^w − z| / (e^w·|1+w|).
    let residual = (w * w.exp() - z).abs();
    let derivative = w.exp() * (1.0 + w).abs();
    let bound = if derivative > 0.0 {
        residual / derivative
    } else {
        residual
    };
    Ok(Evaluated {
        value: w,
        error_bound: bound,
    })
}

// ---- K(m), E(m) ------------------------------------------------------

fn elliptic_domain_check(m: f64) -> Result<(), DomainRefusal> {
    if !m.is_finite() || !(0.0..1.0).contains(&m) {
        return Err(DomainRefusal::OutsideCarrier {
            function: "elliptic",
            carrier: "parameter m ∈ [0, 1) (K(1) diverges)",
            argument: m,
        });
    }
    Ok(())
}

/// AGM evaluation of K(m) = π/(2·AGM(1, √(1−m))).
/// Certificate: b_N ≤ a_∞ ≤ a_N, so |Δa| ≤ a_N − b_N, propagated
/// through K = π/(2a) as a relative bound.
fn elliptic_k_eval(m: f64) -> Result<Evaluated, DomainRefusal> {
    elliptic_domain_check(m)?;
    let mut a = 1.0_f64;
    let mut b = (1.0 - m).sqrt();
    for _ in 0..80 {
        let next_a = (a + b) / 2.0;
        b = (a * b).sqrt();
        a = next_a;
        if a - b <= 1e-16 * a {
            break;
        }
    }
    let value = std::f64::consts::PI / (2.0 * a);
    let bound = value * (a - b) / a;
    Ok(Evaluated {
        value,
        error_bound: bound,
    })
}

/// E(m) = (π/2)·₂F₁(−1/2, 1/2; 1; m) by the hypergeometric series.
/// Certificate: all terms after t₀ are negative; the tail is bounded
/// with the exact ratio |t_{n+1}| ≤ r·|t_n| at the stopping index:
/// tail ≤ |t_{N+1}|·(N+1)²/(2N+5/4).
fn elliptic_e_eval(m: f64) -> Result<Evaluated, DomainRefusal> {
    elliptic_domain_check(m)?;
    let mut term = 1.0_f64; // t_0 = 1
    let mut sum = 1.0_f64;
    let mut n = 0.0_f64;
    let mut next;
    loop {
        // t_{n+1}/t_n = ((n−1/2)(n+1/2)/(n+1)²)·m — the hypergeometric
        // ratio in the ARGUMENT m (dropping it made E(0) ≠ π/2).
        next = term * ((n - 0.5) * (n + 0.5)) / ((n + 1.0) * (n + 1.0)) * m;
        n += 1.0;
        term = next;
        sum += next;
        if next.abs() * (n + 1.0) * (n + 1.0) / (2.0 * n + 1.25) < 1e-16 || n > 1.0e7 {
            break;
        }
    }
    let value = std::f64::consts::PI / 2.0 * sum;
    // Tail certificate recomputed at the stopping index (all-same-sign
    // series: partial sums approach from one side), plus roundoff.
    let tail = next.abs() * (n + 1.0) * (n + 1.0) / (2.0 * n + 1.25);
    let roundoff = sum.abs() * (n * 1e-16);
    let bound = std::f64::consts::PI / 2.0 * (tail.max(roundoff));
    Ok(Evaluated {
        value,
        error_bound: bound,
    })
}
