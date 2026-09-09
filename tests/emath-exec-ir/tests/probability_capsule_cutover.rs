//! Focused authority, ABI, replay, and refusal checks for the probability cutover.

use std::path::{Path, PathBuf};

use emath_exec_ir::interp::Value;
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::{
    install_language_distribution, native_kernel, KernelArity,
};
use emath_test_harness::Probe;

fn language_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}



fn install() -> emath_exec_ir::language_image::LanguageDistribution {
    let distribution = load_language_distribution(&language_root()).expect("language distribution");
    install_language_distribution(&distribution).expect("probability kernels install");
    distribution
}

fn kernel(feature_id: &str) -> &'static emath_exec_ir::native_kernel::NativeKernel {
    native_kernel(feature_id).unwrap_or_else(|| panic!("missing kernel for {feature_id}"))
}

fn capability_value(feature: &str, arguments: &[Value]) -> Result<Value, emath_exec_ir::interp::EvalFault> {
    use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue};
    let mut ops = (0..arguments.len()).map(|index| (EmirOp::LoadInput(index as u16), Default::default())).collect::<Vec<_>>();
    ops.push((EmirOp::ApplyCapability { capability: feature.into(), class: CellClass::Pure, args: (0..arguments.len() as u32).map(EmirValue).collect() }, Default::default()));
    emath_exec_ir::interp::evaluate(&EmirProgram { ops, result: EmirValue(arguments.len() as u32), input_count: arguments.len() as u16, state_count: 0, domain_obligations: vec![] }, arguments, &[])
}

#[test]
fn probe() {
    let mut probe = Probe::new("probability_capsule_cutover.rs: every check in one probe");
    
    probe.case("central_abi_represents_exact_and_bounded_arity_without_feature_switches", |probe| {

        install();
        let sampling = kernel("std.capability.probability.normal-sample");
        probe.eq("sampling.arity_contract()", &(sampling.arity_contract()), &(KernelArity::Bounded { min: 3, max: 4 }));
        probe.demand("sampling.admits_arity(3)", sampling.admits_arity(3), "sampling.admits_arity(3)");
        probe.demand("sampling.admits_arity(4)", sampling.admits_arity(4), "sampling.admits_arity(4)");
        probe.demand("!sampling.admits_arity(2)", !sampling.admits_arity(2), "!sampling.admits_arity(2)");
        probe.demand("!sampling.admits_arity(5)", !sampling.admits_arity(5), "!sampling.admits_arity(5)");
    });
    probe.case("special_value_and_bound_preserve_certified_boundary", |probe| {

        install();
        let Value::F64(value) = capability_value("std.capability.special.gamma", &[Value::F64(5.0)]).expect("gamma(5)") else { panic!("scalar value"); };
        let Value::F64(bound) = capability_value("std.capability.special.gamma-error-bound", &[Value::F64(5.0)]).expect("gamma bound") else { panic!("scalar bound"); };
        probe.demand("(value - 24.0).abs() <= bound", (value - 24.0).abs() <= bound, "(value - 24.0).abs() <= bound");
        probe.demand("bound > 0.0", bound > 0.0, "bound > 0.0");
        let pole = capability_value("std.capability.special.gamma", &[Value::F64(0.0)]).expect_err("gamma pole");
        let emath_exec_ir::interp::EvalFault::CarrierRefused { detail, .. } = pole else { panic!("expected authored pole refusal"); };
        probe.eq("pole diagnostic", &detail.as_str(), &"E-SPECIAL-POLE");
        let carrier = capability_value("std.capability.special.zeta", &[Value::F64(f64::NAN)]).expect_err("non-finite carrier");
        let emath_exec_ir::interp::EvalFault::CarrierRefused { detail, .. } = carrier else { panic!("expected authored carrier refusal"); };
        probe.eq("carrier diagnostic", &detail.as_str(), &"E-SPECIAL-DOMAIN");
    });
    probe.case("seeded_streams_replay_and_split_with_preserved_refusals", |probe| {

        install();
        let descriptor = kernel("std.capability.probability.normal-sample");
        let rooted = [
            Value::Vector(vec![0.0, 1.0]),
            Value::F64(42.0),
            Value::F64(8.0),
        ];
        let split = [
            Value::Vector(vec![0.0, 1.0]),
            Value::F64(42.0),
            Value::F64(8.0),
            Value::Text("campaign.chain-a".to_string()),
        ];
        let first = (descriptor.handler)(&rooted).expect("root stream");
        probe.eq("first", &(first), &((descriptor.handler)(&rooted).expect("root replay")));
        probe.ne("first", &(first), &((descriptor.handler)(&split).expect("split stream")));
        probe.eq("(descriptor.handler)(&[ Value::Vector(vec![0.0, 0.0]), Value::F64(42.0), Value:...", &((descriptor.handler)(&[
                Value::Vector(vec![0.0, 0.0]),
                Value::F64(42.0),
                Value::F64(1.0),
            ])
            .expect_err("invalid scale")), &("E-PROB-001"));
        probe.eq("(descriptor.handler)(&[ Value::Vector(vec![0.0, 1.0]), Value::F64(f64::NAN), Va...", &((descriptor.handler)(&[
                Value::Vector(vec![0.0, 1.0]),
                Value::F64(f64::NAN),
                Value::F64(1.0),
            ])
            .expect_err("non-finite seed")), &("E-PROB-002"));
    });
    
    probe.case("elliptic_pi_n0_complete_is_pi_over_two_inside_bound", |probe| {
            install();
            let args = [Value::F64(0.0), Value::F64(std::f64::consts::FRAC_PI_2), Value::F64(0.0)];
            let Value::F64(value) = capability_value("std.capability.special.elliptic-pi", &args).expect("elliptic value") else { panic!("scalar value"); };
            let Value::F64(bound) = capability_value("std.capability.special.elliptic-pi-error-bound", &args).expect("elliptic bound") else { panic!("scalar bound"); };
            probe.demand("complete identity inside bound", (value - std::f64::consts::FRAC_PI_2).abs() <= bound, "Pi(0; pi/2, 0) = pi/2");
            probe.demand("nonnegative bound", bound >= 0.0, "absolute error bound");
            let error = capability_value("std.capability.special.elliptic-pi", &[Value::F64(2.0), Value::F64(std::f64::consts::FRAC_PI_2), Value::F64(0.0)]).expect_err("pole");
            let emath_exec_ir::interp::EvalFault::CarrierRefused { detail, .. } = error else { panic!("expected authored pole refusal"); };
            probe.eq("pole diagnostic", &detail.as_str(), &"E-SPECIAL-POLE");
        });
    
    probe.case("labeled_mean_estimate_is_executable_and_refuses_empty", |probe| {

        install();
        let Value::Record { type_name, fields } =
            capability_value("std.capability.statistics.mean", &[Value::Vector(vec![1.0, 2.0, 3.0])]).expect("labeled mean")
        else {
            panic!("mean returns Estimate record");
        };
        probe.eq("type_name", &(type_name), &("Estimate"));
        probe.eq("fields.get(\"value\")", &(fields.get("value")), &(Some(&Value::F64(2.0))));
        probe.eq("fields.get(\"method\")", &(fields.get("method")), &(Some(&Value::Text("mean".into()))));
        probe.eq("fields.get(\"n\")", &(fields.get("n")), &(Some(&Value::I64(3))));
        let error = capability_value("std.capability.statistics.mean", &[Value::Vector(vec![])]).expect_err("empty sample");
        let emath_exec_ir::interp::EvalFault::CarrierRefused { detail, .. } = error else { panic!("expected authored empty refusal"); };
        probe.demand("empty mean refusal", detail.starts_with("E-STATS-1"), format!("empty mean refusal: {detail}"));
    });
    probe.case("labeled_remaining_statistics_are_executable", |probe| {

        install();
        let Value::Record { type_name, fields } =
            capability_value("std.capability.statistics.median", &[Value::Vector(vec![1.0, 3.0, 2.0])]).expect("labeled median")
        else {
            panic!("median returns Estimate record");
        };
        probe.eq("type_name", &(type_name), &("Estimate"));
        probe.eq("fields.get(\"value\")", &(fields.get("value")), &(Some(&Value::F64(2.0))));
        probe.eq("fields.get(\"method\")", &(fields.get("method")), &(Some(&Value::Text("median".into()))));
        probe.eq("fields.get(\"n\")", &(fields.get("n")), &(Some(&Value::I64(3))));

        let Value::Record { fields, .. } =
            capability_value("std.capability.statistics.variance-sample", &[Value::Vector(vec![1.0, 2.0, 3.0])]).expect("sample variance")
        else {
            panic!("sample variance returns Estimate record");
        };
        probe.eq("fields.get(\"value\")", &(fields.get("value")), &(Some(&Value::F64(1.0))));
        probe.eq("fields.get(\"method\")", &(fields.get("method")), &(Some(&Value::Text("variance_sample".into()))));

        let Value::Record { fields, .. } =
            capability_value("std.capability.statistics.variance-population", &[Value::Vector(vec![1.0, 2.0, 3.0])]).expect("population variance")
        else {
            panic!("population variance returns Estimate record");
        };
        probe.eq("fields.get(\"value\")", &(fields.get("value")), &(Some(&Value::F64(2.0 / 3.0))));
        probe.eq("fields.get(\"method\")", &(fields.get("method")), &(Some(&Value::Text("variance_population".into()))));

        let Value::Record { fields, .. } =
            capability_value("std.capability.statistics.quantile", &[Value::Vector(vec![1.0, 2.0, 3.0, 4.0]), Value::F64(0.5)])
                .expect("labeled quantile")
        else {
            panic!("quantile returns Estimate record");
        };
        probe.eq("fields.get(\"value\")", &(fields.get("value")), &(Some(&Value::F64(2.5))));
        probe.eq("fields.get(\"method\")", &(fields.get("method")), &(Some(&Value::Text("quantile_type7".into()))));
    });
    probe.finish();
}

