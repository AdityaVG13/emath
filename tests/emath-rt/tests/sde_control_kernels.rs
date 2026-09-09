//! Scalar SDE kernels: Ito vs Stratonovich rules on ascending carriers,
//! deterministic seeds, typed refusals, and refinement metamorphics.

use emath_core::stochastic::{Seed, StreamPath, local_stream_seed};
use emath_rt::splitmix64_next;
use emath_rt::stochastic::{SdeError, SdeRule, sde_euler_maruyama};
use emath_test_harness::Probe;

fn spec_uniform01(state: &mut u64) -> f64 {
    let bits = splitmix64_next(state) >> 11;
    (bits as f64) * (1.0 / (1u64 << 53) as f64)
}

fn spec_zs(seed: u64, count: usize) -> Vec<f64> {
    let mut state = local_stream_seed(&Seed::new(seed), &StreamPath::root()).expect("root seed");
    (0..count)
        .map(|_| {
            let u1 = 1.0 - spec_uniform01(&mut state);
            let u2 = spec_uniform01(&mut state);
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
        })
        .collect()
}

fn poly(coeffs: &[f64], x: f64) -> f64 {
    coeffs.iter().rev().fold(0.0, |acc, &c| acc * x + c)
}

fn poly_deriv(coeffs: &[f64]) -> Vec<f64> {
    coeffs.iter().enumerate().skip(1).map(|(i, &c)| c * i as f64).collect()
}

fn spec_ito(drift: &[f64], diffusion: &[f64], x0: f64, h: f64, zs: &[f64]) -> Vec<f64> {
    let mut xs = vec![x0];
    let mut x = x0;
    let sqrt_h = h.sqrt();
    for &z in zs {
        x = x + poly(drift, x) * h + poly(diffusion, x) * sqrt_h * z;
        xs.push(x);
    }
    xs
}

fn spec_strat(drift: &[f64], diffusion: &[f64], x0: f64, h: f64, zs: &[f64]) -> Vec<f64> {
    let mut xs = vec![x0];
    let mut x = x0;
    let sqrt_h = h.sqrt();
    let d_sigma = poly_deriv(diffusion);
    for &z in zs {
        let sigma = poly(diffusion, x);
        x = x + poly(drift, x) * h + sigma * sqrt_h * z + 0.5 * sigma * poly(&d_sigma, x) * h * z * z;
        xs.push(x);
    }
    xs
}

const SEED_A: f64 = 7.0;
const H: f64 = 0.01;
const STEPS: usize = 64;

#[test]
fn sde_kernels() {
    let mut p = Probe::new("SDE Ito/Stratonovich match spec recurrences with typed refusals");
    let drift = [0.0_f64, 0.25];
    let diffusion = [0.0_f64, 0.35];
    let zs = spec_zs(SEED_A.to_bits(), STEPS);
    p.case("spec", |p| {
        p.eq("ito", sde_euler_maruyama(SdeRule::Ito, &drift, &diffusion, 1.0, H, STEPS, Some(SEED_A)).unwrap(), spec_ito(&drift, &diffusion, 1.0, H, &zs));
        p.eq("strat", sde_euler_maruyama(SdeRule::Stratonovich, &drift, &diffusion, 1.0, H, STEPS, Some(SEED_A)).unwrap(), spec_strat(&drift, &diffusion, 1.0, H, &zs));
        p.eq("ito-len", sde_euler_maruyama(SdeRule::Ito, &drift, &diffusion, 1.0, H, STEPS, Some(SEED_A)).unwrap().len(), STEPS + 1);
    });
    p.case("distinct", |p| {
        let ito = sde_euler_maruyama(SdeRule::Ito, &drift, &diffusion, 1.0, H, STEPS, Some(SEED_A)).unwrap();
        let strat = sde_euler_maruyama(SdeRule::Stratonovich, &drift, &diffusion, 1.0, H, STEPS, Some(SEED_A)).unwrap();
        p.ne("rules-differ", ito.clone(), strat.clone());
        p.demand("correction-pos", strat[STEPS] > ito[STEPS], "Stratonovich correction is positive");
        let additive = [0.5_f64];
        p.eq(
            "additive-agree",
            sde_euler_maruyama(SdeRule::Ito, &drift, &additive, 1.0, H, STEPS, Some(SEED_A)).unwrap(),
            sde_euler_maruyama(SdeRule::Stratonovich, &drift, &additive, 1.0, H, STEPS, Some(SEED_A)).unwrap(),
        );
        let empty: [f64; 0] = [];
        let ito0 = sde_euler_maruyama(SdeRule::Ito, &drift, &empty, 1.0, H, STEPS, Some(SEED_A)).unwrap();
        p.eq("zero-collapse", ito0.clone(), sde_euler_maruyama(SdeRule::Stratonovich, &drift, &empty, 1.0, H, STEPS, Some(SEED_A)).unwrap());
        let mut want = vec![1.0];
        let mut last = 1.0;
        for _ in 0..STEPS {
            last = last + poly(&drift, last) * H;
            want.push(last);
        }
        p.eq("ode-euler", ito0.clone(), want);
        p.close("closed-form", ito0[STEPS], 1.0 * (1.0 + 0.25 * H).powi(STEPS as i32), 1e-9);
    });
    p.case("determinism", |p| {
        let a = sde_euler_maruyama(SdeRule::Ito, &drift, &diffusion, 1.0, H, STEPS, Some(SEED_A)).unwrap();
        p.eq("replay", a.clone(), sde_euler_maruyama(SdeRule::Ito, &drift, &diffusion, 1.0, H, STEPS, Some(SEED_A)).unwrap());
        p.ne("reseed", a, sde_euler_maruyama(SdeRule::Ito, &drift, &diffusion, 1.0, H, STEPS, Some(SEED_A + 1.0)).unwrap());
        p.eq("half-len", sde_euler_maruyama(SdeRule::Ito, &drift, &diffusion, 1.0, H / 2.0, STEPS * 2, Some(SEED_A)).unwrap().len(), STEPS * 2 + 1);
    });
    p.case("refuse", |p| {
        for seed in [None, Some(f64::NAN), Some(f64::INFINITY), Some(-1.0), Some(2.0_f64.powi(64))] {
            p.eq(format!("seed/{seed:?}"), sde_euler_maruyama(SdeRule::Ito, &drift, &diffusion, 1.0, H, STEPS, seed).unwrap_err().code(), "E-SIM-SEED");
        }
        let ok = [0.0_f64, 0.25];
        let bad = [0.0_f64, f64::NAN];
        for (label, dr, df, x0, h) in [("drift", &bad[..], &ok[..], 1.0, H), ("diffusion", &ok[..], &bad[..], 1.0, H), ("state", &ok[..], &ok[..], f64::INFINITY, H), ("step", &ok[..], &ok[..], 1.0, f64::NAN)] {
            p.eq(format!("nonfinite/{label}"), sde_euler_maruyama(SdeRule::Ito, dr, df, x0, h, STEPS, Some(SEED_A)).unwrap_err().code(), "E-SIM-001");
        }
        for (label, h, steps) in [("zero-step", 0.0, STEPS), ("neg-step", -H, STEPS), ("zero-n", H, 0)] {
            p.eq(format!("domain/{label}"), sde_euler_maruyama(SdeRule::Ito, &drift, &diffusion, 1.0, h, steps, Some(SEED_A)).unwrap_err().code(), "E-SIM-002");
        }
        p.eq("budget", sde_euler_maruyama(SdeRule::Ito, &drift, &diffusion, 1.0, H, emath_rt::stochastic::SDE_MAX_STEPS + 1, Some(SEED_A)).unwrap_err().code(), "E-SIM-003");
        p.eq("code-seed", SdeError::Seed.code(), "E-SIM-SEED");
        p.eq("code-nf", SdeError::NonFinite.code(), "E-SIM-001");
        p.eq("code-dom", SdeError::Domain.code(), "E-SIM-002");
        p.eq("code-bud", SdeError::Budget.code(), "E-SIM-003");
    });
    p.case("refinement", |p| {
        let drift_c = [1.0_f64];
        let diff_c = [0.5_f64];
        let (h_c, n_c, ratio) = (0.04, 4usize, 16u32);
        let horizon = h_c * n_c as f64;
        let coarse = sde_euler_maruyama(SdeRule::Ito, &drift_c, &diff_c, 0.0, h_c, n_c, Some(SEED_A)).unwrap();
        let zs_c = spec_zs(SEED_A.to_bits(), n_c);
        let mut want = vec![0.0];
        let mut x = 0.0;
        for &z in &zs_c {
            x = x + 1.0 * h_c + 0.5 * h_c.sqrt() * z;
            want.push(x);
        }
        p.eq("coarse-spec", coarse, want);
        let terminal = |h: f64, steps: usize| {
            (0..24u32).map(|s| sde_euler_maruyama(SdeRule::Ito, &drift_c, &diff_c, 0.0, h, steps, Some(SEED_A + s as f64)).unwrap()[steps]).collect::<Vec<f64>>()
        };
        let var = |xs: &[f64]| {
            let mean = xs.iter().sum::<f64>() / xs.len() as f64;
            xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / xs.len() as f64
        };
        let truth = 0.25 * horizon;
        let (ct, rt) = (terminal(h_c, n_c), terminal(h_c / ratio as f64, n_c * ratio as usize));
        p.demand("refined-var", (var(&rt) - truth).abs() < 0.15 * truth, "refined variance tracks σ²T");
        p.demand("coarse-var", (var(&ct) - truth).abs() < 0.3 * truth, "coarse variance tracks σ²T");
    });
    p.finish();
}
