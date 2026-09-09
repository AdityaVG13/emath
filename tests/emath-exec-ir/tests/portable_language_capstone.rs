use std::path::{Path, PathBuf};

use emath_core::{CanonicalField, SemanticHash};
use emath_exec_ir::interp::{evaluate, EvalFault, Value};
use emath_exec_ir::language_image::{
    load_language_distribution, LanguageDistribution, LanguageImageError,
};
use emath_exec_ir::native_kernel::{
    binding_semantic_hash, install_language_distribution, native_kernel, KernelBindingError,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue};
use emath_ir::CapsuleSlot;
use emath_test_harness::Probe;

fn language_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

/// Production installation verifies the distribution and complete source map
/// before replacing the currently installed bindings.
fn install_checked(distribution: &LanguageDistribution) -> Result<(), String> {
    distribution
        .verify()
        .map_err(|error| format!("{error:?}"))?;
    for capsule in &distribution.capsules {
        if distribution
            .image
            .authored_source(&capsule.feature_id)
            .is_none()
        {
            return Err(format!("missing source map for {}", capsule.feature_id));
        }
    }
    install_language_distribution(distribution).map_err(|error| format!("{error:?}"))
}

fn seam_eval(capability: &str, inputs: &[Value]) -> Result<Value, EvalFault> {
    let count = inputs.len();
    let mut ops = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Default::default()))
        .collect::<Vec<_>>();
    ops.push((
        EmirOp::ApplyCapability {
            capability: capability.to_string(),
            class: CellClass::Pure,
            args: (0..count as u32).map(EmirValue).collect(),
        },
        Default::default(),
    ));
    evaluate(
        &EmirProgram {
            ops,
            result: EmirValue(count as u32),
            input_count: count as u16,
            state_count: 0,
            domain_obligations: Vec::new(),
        },
        inputs,
        &[],
    )
}

fn matrix(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}

fn assert_capsule_binding(probe: &mut Probe, distribution: &LanguageDistribution, feature: &str) {
    let capsule = distribution
        .capsules
        .iter()
        .find(|capsule| capsule.feature_id.as_str() == feature)
        .unwrap_or_else(|| panic!("missing authored capsule {feature}"));
    probe.eq(
        format!("{feature}/authority"),
        distribution.authority.entries[&capsule.feature_id].state.as_str(),
        "capsule-active",
    );
    probe.eq(
        format!("{feature}/hash"),
        binding_semantic_hash(feature).as_deref(),
        Some(capsule.semantic_hash.as_str()),
    );
    probe.demand(
        format!("{feature}/kernel"),
        native_kernel(feature).is_some(),
        format!("missing active kernel {feature}"),
    );
}

#[test]
fn probe() {
    let mut probe = Probe::new("portable_language_capstone.rs: every check in one probe");
    probe.case("checked_in_distribution_is_deterministic_and_executes_every_migrated_family", |probe| {

        let first =
            load_language_distribution(&language_root()).expect("checked-in LanguageDistribution");
        let second = load_language_distribution(&language_root()).expect("deterministic reload");
        probe.eq("first.image.semantic_hash", &(first.image.semantic_hash), &(second.image.semantic_hash));
        probe.eq("first.image.distribution_hash", &(first.image.distribution_hash), &(second.image.distribution_hash));
        probe.eq("first", &(first), &(second));
        install_checked(&first).expect("verified distribution installs");

        for feature in [
            "std.capability.math.add",
            "std.capability.linear.vector-norm",
            "std.capability.tensor.einsum",
            "std.capability.special.gamma",
            "std.capability.probability.normal-density",
            "std.capability.graph.reachability",
            "std.capability.optimize.lp-minimize",
            "std.capability.game.pure-nash-claim",
            "std.capability.control.transfer-eval",
            "std.capability.pde.laplacian",
            "std.capability.geometry.inner-product",
            "std.capability.units.dimension-compose",
            "std.capability.chemistry.conservation-residual",
            "std.capability.tensor.index",
            "std.capability.special.elliptic-pi",
            "std.capability.special.elliptic-pi-error-bound",
            "std.capability.dynamics.simulation-world",
            "std.capability.calculus.goals",
            "std.capability.crypto.caesar_round_trip",
        ] {
            assert_capsule_binding(probe, &first, feature);
        }

        probe.eq("seam_eval(\"std.capability.math.add\", &[Value::I64(2), Value::I64(1)])", &(seam_eval("std.capability.math.add", &[Value::I64(2), Value::I64(1)])), &(Ok(Value::I64(3))));
        probe.eq("seam_eval( \"std.capability.linear.vector-norm\", &[Value::Vector(vec![3.0, 4.0])...", &(seam_eval(
                "std.capability.linear.vector-norm",
                &[Value::Vector(vec![3.0, 4.0])]
            )), &(Ok(Value::F64(5.0))));
        probe.eq("seam_eval( \"std.capability.tensor.einsum\", &[ Value::Text(\"i,i->\".to_string()),...", &(seam_eval(
                "std.capability.tensor.einsum",
                &[
                    Value::Text("i,i->".to_string()),
                    Value::List(vec![
                        Value::Vector(vec![1.0, 2.0, 3.0]),
                        Value::Vector(vec![4.0, 5.0, 6.0]),
                    ]),
                ],
            )), &(Ok(Value::F64(32.0))));
        let Value::F64(gamma) = seam_eval("std.capability.special.gamma", &[Value::F64(5.0)]).unwrap()
        else {
            panic!("gamma scalar")
        };
        probe.demand("(gamma - 24.0).abs() < 1.0e-8", (gamma - 24.0).abs() < 1.0e-8, "(gamma - 24.0).abs() < 1.0e-8");
        let Value::F64(density) = seam_eval(
            "std.capability.probability.normal-density",
            &[Value::Vector(vec![0.0, 1.0]), Value::F64(0.0)],
        )
        .unwrap() else {
            panic!("density scalar")
        };
        probe.demand("(density - 0.398_942_280_401_432_7).abs() < 1.0e-12", (density - 0.398_942_280_401_432_7).abs() < 1.0e-12, "(density - 0.398_942_280_401_432_7).abs() < 1.0e-12");

        let graph = matrix(3, 3, &[0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        probe.eq("seam_eval( \"std.capability.graph.reachability\", &[graph, Value::F64(0.0)] )", &(seam_eval(
                "std.capability.graph.reachability",
                &[graph, Value::F64(0.0)]
            )), &(Ok(Value::Vector(vec![1.0, 1.0, 1.0]))));
        probe.eq("seam_eval( \"std.capability.optimize.lp-minimize\", &[ matrix(2, 2, &[1.0, 0.0, 0...", &(seam_eval(
                "std.capability.optimize.lp-minimize",
                &[
                    matrix(2, 2, &[1.0, 0.0, 0.0, 1.0]),
                    Value::Vector(vec![1.0, 1.0]),
                    Value::Vector(vec![-1.0, -1.0])
                ]
            )), &(Ok(Value::Vector(vec![1.0, 1.0]))));
        probe.eq("seam_eval( \"std.capability.game.pure-nash-claim\", &[ matrix(2, 2, &[2.0, 0.0, 0...", &(seam_eval(
                "std.capability.game.pure-nash-claim",
                &[
                    matrix(2, 2, &[2.0, 0.0, 0.0, 1.0]),
                    matrix(2, 2, &[2.0, 0.0, 0.0, 1.0]),
                    Value::I64(0),
                    Value::I64(0)
                ]
            )), &(Ok(Value::Bool(true))));

        probe.eq("seam_eval( \"std.capability.control.transfer-eval\", &[ Value::Vector(vec![6.0, 1...", &(seam_eval(
                "std.capability.control.transfer-eval",
                &[
                    Value::Vector(vec![6.0, 1.0]),
                    Value::Vector(vec![2.0, 3.0, 1.0]),
                    Value::F64(0.0)
                ]
            )), &(Ok(Value::F64(3.0))));
        probe.eq("seam_eval( \"std.capability.pde.laplacian\", &[Value::Vector(vec![1.0, 2.0, 4.0])...", &(seam_eval(
                "std.capability.pde.laplacian",
                &[Value::Vector(vec![1.0, 2.0, 4.0]), Value::F64(1.0)]
            )), &(Ok(Value::Vector(vec![1.0, 1.0, -2.0]))));
        probe.eq("seam_eval( \"std.capability.geometry.inner-product\", &[ Value::Vector(vec![1.0, ...", &(seam_eval(
                "std.capability.geometry.inner-product",
                &[
                    Value::Vector(vec![1.0, 2.0, 3.0]),
                    Value::Vector(vec![4.0, 5.0, 6.0])
                ]
            )), &(Ok(Value::F64(32.0))));
        probe.eq("seam_eval( \"std.capability.units.dimension-compose\", &[ Value::Vector(vec![1.0,...", &(seam_eval(
                "std.capability.units.dimension-compose",
                &[
                    Value::Vector(vec![1.0, 0.0, -1.0]),
                    Value::Vector(vec![-1.0, 0.0, 1.0])
                ]
            )), &(Ok(Value::Vector(vec![0.0, 0.0, 0.0]))));
        probe.eq("seam_eval( \"std.capability.chemistry.conservation-residual\", &[ matrix(2, 3, &[...", &(seam_eval(
                "std.capability.chemistry.conservation-residual",
                &[
                    matrix(2, 3, &[2.0, 0.0, 2.0, 0.0, 2.0, 1.0]),
                    Value::Vector(vec![2.0, 1.0, -2.0])
                ]
            )), &(Ok(Value::Vector(vec![0.0, 0.0]))));

        probe.eq("seam_eval( \"std.capability.tensor.index\", &[ Value::Vector(vec![3.0, 4.0]), Val...", &(seam_eval(
                "std.capability.tensor.index",
                &[
                    Value::Vector(vec![3.0, 4.0]),
                    Value::Set(vec![Value::I64(1)]),
                ],
            )), &(Ok(Value::F64(4.0))));
        let Value::F64(pi_n0) = seam_eval(
            "std.capability.special.elliptic-pi",
            &[
                Value::F64(0.0),
                Value::F64(std::f64::consts::FRAC_PI_2),
                Value::F64(0.0),
            ],
        )
        .expect("elliptic-pi n=0") else {
            panic!("elliptic-pi scalar")
        };
        probe.demand("(pi_n0 - std::f64::consts::FRAC_PI_2).abs() < 1.0e-10", (pi_n0 - std::f64::consts::FRAC_PI_2).abs() < 1.0e-10, "(pi_n0 - std::f64::consts::FRAC_PI_2).abs() < 1.0e-10");
        let identity = EmirProgram {
            ops: vec![(EmirOp::LoadInput(0), Default::default())],
            result: EmirValue(0),
            input_count: 1,
            state_count: 0,
            domain_obligations: Vec::new(),
        };
        probe.eq("seam_eval( \"std.capability.dynamics.simulation-world\", &[ Value::Program(identi...", &(seam_eval(
                "std.capability.dynamics.simulation-world",
                &[
                    Value::program(identity.clone()),
                    Value::Vector(vec![1.0]),
                    Value::F64(1.0),
                    Value::Text("euler".into()),
                ],
            )), &(Ok(Value::Vector(vec![2.0]))));
        probe.eq("seam_eval( \"std.capability.calculus.goals\", &[ Value::Program(identity), Value:...", &(seam_eval(
                "std.capability.calculus.goals",
                &[
                    Value::program(identity),
                    Value::Vector(vec![3.0]),
                    Value::Text("differentiate".into()),
                    Value::Set(vec![Value::I64(0)]),
                ],
            )), &(Ok(Value::F64(1.0))));
        probe.eq("seam_eval( \"std.capability.crypto.caesar_round_trip\", &[Value::I64(3), Value::I...", &(seam_eval(
                "std.capability.crypto.caesar_round_trip",
                &[Value::I64(3), Value::I64(4), Value::I64(26)],
            )), &(Ok(Value::I64(7))));
    });
    probe.case("stale_tampered_missing_map_and_kernel_signature_all_refuse_before_replacement", |probe| {

        let distribution =
            load_language_distribution(&language_root()).expect("checked-in distribution");
        install_checked(&distribution).expect("baseline install");
        let baseline = binding_semantic_hash("std.capability.math.add");

        let mut stale = distribution.clone();
        stale.image.lock.semantic_hash =
            SemanticHash::new(&[CanonicalField::new("language", b"stale").unwrap()]).unwrap();
        probe.demand("matches!(stale.verify(), Err(LanguageImageError::StaleLock))", matches!(stale.verify(), Err(LanguageImageError::StaleLock)), "matches!(stale.verify(), Err(LanguageImageError::StaleLock))");

        let mut tampered = distribution.clone();
        let sources = tampered
            .image
            .image
            .partitions
            .iter_mut()
            .find(|partition| partition.name == "language.sources")
            .expect("source-map partition");
        sources
            .body
            .push_str("std.capability.forged=language/spec/forged.emath\n");
        probe.demand("matches!( tampered.verify(), Err(LanguageImageError::CorruptImage(_)) )", matches!(
            tampered.verify(),
            Err(LanguageImageError::CorruptImage(_))
        ), "matches!( tampered.verify(), Err(LanguageImageError::CorruptImage(_)) )");

        let mut missing = distribution.clone();
        missing
            .image
            .image
            .partitions
            .retain(|partition| partition.name != "language.sources");
        probe.demand("install_checked(&missing) .unwrap_err() .contains(\"InvalidSourceMap\")", install_checked(&missing)
            .unwrap_err()
            .contains("InvalidSourceMap"), "install_checked(&missing) .unwrap_err() .contains(\"InvalidSourceMap\")");

        let mut bad_signature = distribution.clone();
        let add = bad_signature
            .capsules
            .iter_mut()
            .find(|capsule| capsule.feature_id.as_str() == "std.capability.math.add")
            .unwrap();
        let CapsuleSlot::Value(semantics) = add.slots.get_mut("semantics").unwrap() else {
            panic!("add semantics")
        };
        *semantics = semantics.replace("output=Int", "output=Float64");
        probe.eq("install_language_distribution(&bad_signature)", &(install_language_distribution(&bad_signature)), &(Err(KernelBindingError::SignatureMismatch(
                "std.capability.math.add".to_string()
            ))));
        probe.eq("binding_semantic_hash(\"std.capability.math.add\")", &(binding_semantic_hash("std.capability.math.add")), &(baseline));
    });
    probe.finish();
}

