//! (thin nucleus slice): probability distributions +
//! seeded sampling at the EMIR seam.
//!
//! The law, thinned to the compute layer:
//! - **One RNG story**: seed and declared split path enter the
//!   counter-based `emath_core::stochastic` contract. Same seed and
//!   path ⟹ bit-identical draws across runs and eval invocations.
//! - **Three admitted distributions**: Normal(μ, σ) via Box–Muller,
//!   Uniform(a, b), Bernoulli(p) — each as a seeded `*-sample`
//!   capability and an exact `*-density`/`*-pmf` capability
//!   (`std.capability.probability.*`).
//! - **Typed refusals**: invalid parameters refuse `E-PROB-001`
//!   (σ ≤ 0, a > b, p ∉ [0,1], non-integer or over-budget draws —
//!   the negative seed's shape); non-finite parameters/points refuse
//!   `E-PROB-002`; wrong parameter arity refuses `E-PROB-003`.
//! - **Discriminating laws** (no tautologies): uniform draws stay in
//!   [a, b) (kills scale/offset mutants); Bernoulli draws are exactly
//!   {0.0, 1.0} with the edge params p ∈ {0, 1} exact; densities
//!   match closed forms at 1e-12 (Normal pdf(0) = 1/√(2π)); the
//!   empirical mean of a fixed-seed Normal draw lands in a fixed band
//!   (a biased-generator mutant shifts it); second seed ⟹ different
//!   draws (kills constant-output mutants).

use std::path::{Path, PathBuf};

use emath_core::Span;
use emath_core::limits::Limits;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::{KernelArity, install_language_distribution, native_kernel};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{SymbolId, Term};
use emath_test_harness::{Probe, boot};

fn language_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

/// Capability cells resolve against the installed Language Image
/// (thread-local bindings); every evaluating test installs the
/// checked-in distribution first.
fn install_language() {
    let distribution = load_language_distribution(&language_root()).expect("language distribution");
    install_language_distribution(&distribution).expect("probability kernels install");
}

/// The active universal seam for domain math: an `ApplyCapability`
/// over a capsule-active FeatureID (no domain-named `EmirOp`).
fn cell(capability: &str, args: Vec<EmirValue>) -> EmirOp {
    EmirOp::ApplyCapability {
        capability: capability.to_string(),
        class: CellClass::Pure,
        args,
    }
}

const NORMAL_SAMPLE: &str = "std.capability.probability.normal-sample";
const UNIFORM_SAMPLE: &str = "std.capability.probability.uniform-sample";
const BERNOULLI_SAMPLE: &str = "std.capability.probability.bernoulli-sample";
const NORMAL_DENSITY: &str = "std.capability.probability.normal-density";
const UNIFORM_DENSITY: &str = "std.capability.probability.uniform-density";
const BERNOULLI_PMF: &str = "std.capability.probability.bernoulli-pmf";

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    install_language();
    let mut program_ops: Vec<(EmirOp, Span)> = (0..inputs.len())
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    program_ops.extend(ops.into_iter().map(|op| (op, Span::default())));
    let result = EmirValue(program_ops.len() as u32 - 1);
    let program = EmirProgram {
        ops: program_ops,
        result,
        input_count: inputs.len() as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

fn sample(feature_id: &str, params: &[f64], seed: f64, draws: f64) -> Result<Vec<f64>, EvalFault> {
    let value = eval(
        vec![cell(
            feature_id,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[
            Value::Vector(params.to_vec()),
            Value::F64(seed),
            Value::F64(draws),
        ],
    )?;
    let Value::Vector(draws) = value else {
        panic!("expected a draw vector, got {value:?}")
    };
    Ok(draws)
}

fn normal_sample(params: &[f64], seed: f64, draws: f64) -> Result<Vec<f64>, EvalFault> {
    sample(NORMAL_SAMPLE, params, seed, draws)
}

#[test]
fn intent() {
    boot();
    let mut p = Probe::new("(thin nucleus slice): probability distributions +");
    p.case("seed_reproducibility_bit_exact", |p| {

    // THE law: fixed seed ⟹ deterministic draws across runs and
    // across separate eval invocations (bit-for-bit). A mutant that
    // derives state from anything but the seed fails.
    let a = normal_sample(&[0.0, 1.0], 42.0, 64.0).expect("seeded sample computes");
    let b = normal_sample(&[0.0, 1.0], 42.0, 64.0).expect("second invocation computes");
    p.eq("seed_reproducibility_bit_exact#1", a.len(), 64);
    p.eq("same seed must reproduce the exact draw vector", a.clone(), b);
    // A different seed produces a different stream (kills
    // constant-output and seed-ignored mutants).
    let c = normal_sample(&[0.0, 1.0], 43.0, 64.0).expect("other seed computes");
    p.ne("different seeds must give different streams", a, c);

    });
    p.case("declared_stream_paths_split_and_replay", |p| {

    let sample_path = |path: &str| {
        let value = eval(
            vec![cell(
                NORMAL_SAMPLE,
                vec![EmirValue(0), EmirValue(1), EmirValue(2), EmirValue(3)],
            )],
            &[
                Value::Vector(vec![0.0, 1.0]),
                Value::F64(42.0),
                Value::F64(16.0),
                Value::Text(path.to_string()),
            ],
        )
        .expect("declared stream samples");
        let Value::Vector(values) = value else {
            panic!("expected draw vector"); };
        values
    };
    let first = sample_path("campaign.chain-a");
    p.eq("declared_stream_paths_split_and_replay#2", first.clone(), sample_path("campaign.chain-a"));
    p.ne("declared_stream_paths_split_and_replay#3", first, sample_path("campaign.chain-b"));

    });
    p.case("public_stream_example_executes", |p| {

    emath_syntax::install_source_parser();
    // The CLI bootstrap contract (cli_dispatch): the Language Image is
    // located, loaded, and installed before any semantic command. The
    // capability aliases (`normal_sample`, ...) are capsule
    // `presentation: aliases=` data — admission resolves them
    // generically from the installed image, never a static name table.
    let distribution = load_language_distribution(&language_root()).expect("language distribution");
    emath_sema::language::install_language_distribution(&distribution)
        .expect("probability bindings install");
    let mut session = emath_sema::CompilerSession::new(Limits::default());
    let checked = session.check_owned(
        "seeded-sampling",
        include_str!("../../../language/examples/probability/seeded_sampling.emath"),
    );
    p.demand(format!("{:?}", checked.diagnostics.errors().collect::<Vec<_>>()), !checked.diagnostics.has_errors(), format!("{:?}", checked.diagnostics.errors().collect::<Vec<_>>()));
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let definitions = &report.declarations[0].tests[0].definitions;
    p.eq("public_stream_example_executes#2", definitions.get("replayed"), Some(&Value::Bool(true)));
    p.eq("public_stream_example_executes#3", definitions.get("split"), Some(&Value::Bool(true)));
    let mut planner = emath_sema::CompilerSession::new(Limits::default());
    let planned_file = planner.load_text(
        "seeded-sampling",
        include_str!("../../../language/examples/probability/seeded_sampling.emath"),
    );
    let planned = planner.plan(planned_file);
    p.demand("public_stream_example_executes#4", !planned.diagnostics.has_errors(), "public_stream_example_executes#4: !planned.diagnostics.has_errors()");

    let generated = emath_rust_backend::BackendInput {
        package: &planned.package,
        crate_name: "seeded-sampling".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("stream-aware sampling generates Rust");
    let rust = emath_rust_backend::rust_ir::render::render_module(&generated.module).code;
    p.demand(format!("{rust}"), rust.contains("prob_sample_in_stream"), format!("{rust}"));
    p.demand(format!("{rust}"), rust.contains("prob_sample_in_stream(0"), format!("{rust}"));
    p.demand(format!("{rust}"), rust.contains("campaign.chain-a"), format!("{rust}"));

    });
    p.case("uniform_range_and_moments", |p| {

    // Uniform(2, 5): every draw in [2, 5) EXACTLY (kills scale,
    // offset, and interval-order mutants), and the fixed-seed
    // empirical mean lands in a fixed band around 3.5.
    let draws =
        sample(UNIFORM_SAMPLE, &[2.0, 5.0], 7.0, 10_000.0).expect("uniform sample computes");
    p.eq("uniform_range_and_moments#1", draws.len(), 10_000);
    let mut sum = 0.0;
    for draw in &draws {
        p.demand(format!("uniform draw out of [a, b): {draw}"), (2.0..5.0).contains(draw), format!("uniform draw out of [a, b): {draw}"));
        sum += draw;
    }
    let mean = sum / draws.len() as f64;
    p.demand(format!("uniform mean near 3.5 under fixed seed, got {mean}"), (mean - 3.5).abs() < 0.05, format!("uniform mean near 3.5 under fixed seed, got {mean}"));

    });
    p.case("normal_moments_fixed_band", |p| {

    // Normal(0, 1): the fixed-seed empirical mean of 20k Box–Muller
    // draws is a DETERMINISTIC number (no flake possible); the band
    // catches biased generators (wrong sign, dropped uniform, mean
    // not subtracted). |mean| < 0.025 ≈ 3.5 standard errors.
    let draws = normal_sample(&[0.0, 1.0], 11.0, 20_000.0).expect("normal computes");
    p.eq("normal_moments_fixed_band#1", draws.len(), 20_000);
    let mean: f64 = draws.iter().sum::<f64>() / draws.len() as f64;
    p.demand(format!("standard normal mean near 0 under fixed seed, got {mean}"), mean.abs() < 0.025, format!("standard normal mean near 0 under fixed seed, got {mean}"));
    // Affine transform law: Normal(5, 2) = 5 + 2·Normal(0, 1) with the
    // same seed (kills scale/shift mutants in the sampler).
    let shifted = normal_sample(&[5.0, 2.0], 11.0, 20_000.0).expect("shifted computes");
    for (standard, transformed) in draws.iter().zip(shifted.iter()) {
        p.demand(format!("affine law: {transformed} vs {}", 5.0 + 2.0 * standard), (transformed - (5.0 + 2.0 * standard)).abs() < 1e-9, format!("affine law: {transformed} vs {}", 5.0 + 2.0 * standard));
    }

    });
    p.case("bernoulli_edges_and_law", |p| {

    // Bernoulli(p): draws are EXACTLY {0.0, 1.0}; the edges p ∈ {0, 1}
    // are exact (all-zero / all-one); p = 0.7 gives ~0.7 ones.
    let fraction = |prob: f64| -> (f64, bool) {
        let draws = sample(BERNOULLI_SAMPLE, &[prob], 5.0, 10_000.0).expect("bernoulli computes");
        let valid = draws.iter().all(|draw| draw == &0.0 || draw == &1.0);
        (draws.iter().sum::<f64>() / draws.len() as f64, valid)
    };
    let (f0, ok0) = fraction(0.0);
    p.demand("bernoulli draws are all in {0,1} (p=0)", ok0, "bernoulli draws are all in {0,1} (p=0)");
    p.eq("p = 0 edge: all zeros", f0, 0.0);
    let (f1, ok1) = fraction(1.0);
    p.demand("bernoulli draws are all in {0,1} (p=1)", ok1, "bernoulli draws are all in {0,1} (p=1)");
    p.eq("p = 1 edge: all ones", f1, 1.0);
    let (mean, ok7) = fraction(0.7);
    p.demand("bernoulli draws are all in {0,1} (p=0.7)", ok7, "bernoulli draws are all in {0,1} (p=0.7)");
    p.demand(format!("bernoulli(0.7) fraction near 0.7 under fixed seed, got {mean}"), (mean - 0.7).abs() < 0.02, format!("bernoulli(0.7) fraction near 0.7 under fixed seed, got {mean}"));

    });
    p.case("densities_closed_forms", |p| {

    // Densities/PMFs at exact points (1e-12): Normal pdf(0) =
    // 1/√(2π); Uniform pdf inside = 1/(b−a) and pdf outside = 0;
    // Bernoulli pmf(1) = p, pmf(0) = 1−p.
    let density = |feature_id: &str, params: &[f64], x: f64| -> Result<f64, EvalFault> {
        let value = eval(
            vec![cell(feature_id, vec![EmirValue(0), EmirValue(1)])],
            &[Value::Vector(params.to_vec()), Value::F64(x)],
        )?;
        let Value::F64(d) = value else {
            panic!("expected a density scalar, got {value:?}"); };
        Ok(d)
    };
    let two_pi_sqrt = std::f64::consts::SQRT_2 * std::f64::consts::PI.sqrt();
    let normal_pdf_0 = density(NORMAL_DENSITY, &[0.0, 1.0], 0.0).expect("pdf computes");
    p.demand(format!("normal pdf(0) = 1/√(2π), got {normal_pdf_0}"), (normal_pdf_0 - 1.0 / two_pi_sqrt).abs() < 1e-12, format!("normal pdf(0) = 1/√(2π), got {normal_pdf_0}"));
    let uniform_mid = density(UNIFORM_DENSITY, &[2.0, 5.0], 3.5).expect("pdf computes");
    p.demand(format!("uniform density = 1/(b−a), got {uniform_mid}"), (uniform_mid - 1.0 / 3.0).abs() < 1e-12, format!("uniform density = 1/(b−a), got {uniform_mid}"));
    let uniform_out = density(UNIFORM_DENSITY, &[2.0, 5.0], 9.0).expect("pdf computes");
    p.eq("uniform density outside [a, b] is 0", uniform_out, 0.0);
    let pmf_one = density(BERNOULLI_PMF, &[0.3], 1.0).expect("pmf computes");
    let pmf_zero = density(BERNOULLI_PMF, &[0.3], 0.0).expect("pmf computes");
    p.demand("densities_closed_forms#5", (pmf_one - 0.3).abs() < 1e-12 && (pmf_zero - 0.7).abs() < 1e-12, "densities_closed_forms#5: (pmf_one - 0.3).abs() < 1e-12 && (pmf_zero - 0.7).abs() < 1e-12");

    });
    p.case("invalid_parameters_refuse_typed", |p| {

    // E-PROB-001: σ ≤ 0 (the negative seed's shape), a > b,
    // p ∉ [0,1], non-integer draws. E-PROB-002: non-finite params.
    // E-PROB-003: wrong arity.
    let cases: Vec<(&str, Vec<f64>, f64, &str)> = vec![
        (NORMAL_SAMPLE, vec![0.0, 0.0], 1.0, "sigma zero"),
        (NORMAL_SAMPLE, vec![0.0, -1.0], 1.0, "sigma negative"),
        (UNIFORM_SAMPLE, vec![5.0, 2.0], 1.0, "a > b"),
        (BERNOULLI_SAMPLE, vec![1.5], 1.0, "p over one"),
        (BERNOULLI_SAMPLE, vec![-0.1], 1.0, "p under zero"),
        (NORMAL_SAMPLE, vec![0.0, 1.0], 2.5, "fractional draws"),
    ];
    for (feature_id, params, draws, label) in cases {
        let error = sample(feature_id, &params, 1.0, draws).expect_err(label);
        let fault = format!("{error:?}");
        p.demand(format!("{label} must name E-PROB-001, got {fault}"), fault.contains("E-PROB-001"), format!("{label} must name E-PROB-001, got {fault}"));
    }
    let non_finite =
        sample(NORMAL_SAMPLE, &[f64::NAN, 1.0], 1.0, 4.0).expect_err("non-finite refuses");
    p.demand(format!("non-finite must name E-PROB-002, got {non_finite:?}"), format!("{non_finite:?}").contains("E-PROB-002"), format!("non-finite must name E-PROB-002, got {non_finite:?}"));
    let wrong_arity = sample(BERNOULLI_SAMPLE, &[0.5, 0.5], 1.0, 4.0).expect_err("arity refuses");
    p.demand(format!("arity must name E-PROB-003, got {wrong_arity:?}"), format!("{wrong_arity:?}").contains("E-PROB-003"), format!("arity must name E-PROB-003, got {wrong_arity:?}"));
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/probability_sampling.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    p.demand(format!("seed expects the invalid-param refusal, found: {expect_line}"), expect_line.contains("E-PROB-001"), format!("seed expects the invalid-param refusal, found: {expect_line}"));

    });
    p.case("cell_registry_and_shape_law", |p| {

    // The .emath surface: std.probability.* sampling is distribution
    // DATA bound by FeatureID (cohort 28), with the capsule contract's
    // arity law at the kernel ABI (samples admit (params, seed,
    // draws[, stream])), and a scalar params slot refuses typed at the
    // kernel (E-TYPE-012) — the closed vocabulary's shape law.
    install_language();
    for feature_id in [
        NORMAL_SAMPLE,
        UNIFORM_SAMPLE,
        BERNOULLI_SAMPLE,
        NORMAL_DENSITY,
        UNIFORM_DENSITY,
        BERNOULLI_PMF,
    ] {
        p.demand(format!("capability kernel {feature_id} bound"), native_kernel(feature_id).is_some(), format!("capability kernel {feature_id} bound"));
    }
    p.eq("sampling admits (params, seed, draws[, stream])", native_kernel(NORMAL_SAMPLE)
            .expect("normal-sample bound")
            .arity_contract(), KernelArity::Bounded { min: 3, max: 4 });

    // Shape law at the ABI: a scalar params slot refuses typed.
    let error = eval(
        vec![cell(
            NORMAL_SAMPLE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[Value::F64(0.0), Value::F64(42.0), Value::F64(4.0)],
    )
    .expect_err("scalar params refuse");
    let fault = format!("{error:?}");
    p.demand(format!("scalar params must refuse typed, got {fault}"), fault.contains("E-TYPE-012"), format!("scalar params must refuse typed, got {fault}"));

    });
    p.case("bundle_fixture", |p| {

    // WorldResultBundle fixture (e2e clause; the VM path is touched).
    struct ProbWorld;
    impl emath_genesis::FirstOrderWorld for ProbWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let first = normal_sample(&[0.0, 1.0], 42.0, 8.0).unwrap_or_default();
            let again = normal_sample(&[0.0, 1.0], 42.0, 8.0).unwrap_or_default();
            if first.len() == 8 && first == again {
                Ok("seeded-sampling-reproducible".to_string())
            } else {
                Ok("seeded-sampling-diverged".to_string())
            }
        }

        fn apply(
            &self,
            operator: &SymbolId,
            _arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            Err(emath_genesis::EvalError::UnknownSymbol(operator.clone()))
        }

        fn evidence(&self) -> emath_genesis::WorldEvidence {
            emath_genesis::WorldEvidence::seed(
                "probability-nucleus",
                &["splitmix64", "box-muller", "seeded-reproducible"],
            )
        }
    }

    let term = Term::Constant(SymbolId("prob[fixture]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &ProbWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    p.demand("bundle_fixture#1", matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ), "bundle_fixture#1: matches!(\n        result.disposition,\n        emath_genesis::Disposition::Answer { .. }\n    )");
    p.demand("bundle_fixture#2", result.world == "probability-nucleus", format!("expected {:?}, got {:?}", "probability-nucleus", result.world));
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    p.demand("bundle_fixture#3", bundle.bundle_id.starts_with("fnv1a64:"), "bundle_fixture#3: bundle.bundle_id.starts_with(\"fnv1a64:\")");

    });
    p.finish();
}



















