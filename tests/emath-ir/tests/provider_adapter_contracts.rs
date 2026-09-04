//! Generated provider adapter contracts and
//! conformance fixtures.
//!
//! The law: cell schemas GENERATE the adapter contract
//! capability key, IR-facing trait shape (as data), deterministic
//! conformance fixtures, and reference-oracle comparison. Providers are
//! untrusted workers: their outputs are admitted only by bit-exact (or
//! policy-declared) comparison against the local oracle. Provider-native
//! types never appear in the IR-facing signature — the allowlist gate
//! refuses them typed (Neutral IR Constitution §7; same rule as
//! ). The provider is never the public meaning of
//! an operation: the oracle stays in emath-ir, the adapter only carries
//! the comparison contract.

use std::collections::BTreeMap;

use emath_core::QualifiedName;
use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_genesis::{
    Disposition, EvalError, FirstOrderWorld, ResultBundle, WorldBudget, evaluate_labeled,
};
use emath_ir::capability::{CellClass as SchemaClass, CellSchema, MigrationPolicy};
use emath_provider_api::adapter::{
    AdapterContractError, ConformanceVerdict, ProviderBinding, compare_outputs, ir_type_gate,
};
use emath_term::{SymbolId, Term};

const STD_TENSOR_SOFTMAX: &str = "std.tensor.softmax";

/// The softmax cell schema (descriptor) as admitted on main.
fn softmax_schema() -> CellSchema {
    CellSchema {
        name: QualifiedName::single(STD_TENSOR_SOFTMAX),
        class: SchemaClass::Pure,
        version: "1.0.0".to_string(),
        migration: MigrationPolicy::Frozen,
        arity: 1,
        about: None,
    }
}

/// The REAL provider binding used by the fixture harness: the compiled
/// cell evaluated through the VM seam (the provider-native side
/// is any implementation that speaks the adapter's IR-facing shape; the
/// seam is the local stand-in so the comparison is live, not mocked).
fn run_softmax_seam(vector: &[f64]) -> Result<Vec<f64>, EvalFault> {
    let program = EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()),
            (
                EmirOp::ApplyCapability {
                    capability: STD_TENSOR_SOFTMAX.to_string(),
                    class: CellClass::Pure,
                    args: vec![EmirValue(0)],
                },
                Span::default(),
            ),
        ],
        result: EmirValue(1),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    match evaluate_with_budget(
        &program,
        &[Value::Vector(vector.to_vec())],
        &[],
        EvalBudget::default(),
    ) {
        Ok(Value::Vector(values)) => Ok(values),
        Ok(_) => Err(EvalFault::TypeConfusion {
            register: 0,
            op: "fixture-provider",
        }),
        Err(fault) => Err(fault),
    }
}

#[test]
fn contract_is_generated_from_schema() {
    // The contract is GENERATED from the cell schema: the capability key
    // is deterministic (regenerating is identical), tracks every
    // identity-affecting schema field, and ignores presentation.
    let contract = emath_provider_api::adapter::adapter_contract(&softmax_schema())
        .expect("softmax adapter contract generates");
    assert!(
        contract
            .capability_key
            .starts_with("adapter:std.tensor.softmax@"),
        "{}",
        contract.capability_key
    );
    assert_eq!(contract.spec.arity, 1);
    assert_eq!(contract.spec.numeric_policy, "strict-f64");

    let regenerated =
        emath_provider_api::adapter::adapter_contract(&softmax_schema()).expect("regenerates");
    assert_eq!(contract.capability_key, regenerated.capability_key);

    // Identity-affecting schema mutation moves the key; presentation
    // mutation does not.
    let bumped = CellSchema {
        version: "2.0.0".to_string(),
        ..softmax_schema()
    };
    let bumped_contract =
        emath_provider_api::adapter::adapter_contract(&bumped).expect("bumped contract generates");
    assert_ne!(contract.capability_key, bumped_contract.capability_key);
    let annotated = CellSchema {
        about: Some("presentation only".to_string()),
        ..softmax_schema()
    };
    let annotated_contract = emath_provider_api::adapter::adapter_contract(&annotated)
        .expect("annotated contract generates");
    assert_eq!(contract.capability_key, annotated_contract.capability_key);

    // The IR-facing signature carries only IR-owned tokens.
    for token in &contract.spec.ir_signature {
        ir_type_gate(token).expect("IR-owned token passes the gate");
    }
}

#[test]
fn fixtures_compare_native_vs_provider_bit_exact() {
    // Conformance: the generated fixtures' oracle outputs (the local
    // reference semantics in emath-ir) and the provider binding's
    // outputs (here: the compiled cell through the real VM seam) agree
    // BIT-FOR-BIT on every case.
    let contract = emath_provider_api::adapter::adapter_contract(&softmax_schema())
        .expect("contract generates");
    assert!(!contract.fixtures.is_empty(), "fixtures are generated");
    for fixture in &contract.fixtures {
        let expected = fixture.expected.as_ref().expect("softmax has an oracle");
        for input in &fixture.inputs {
            let native =
                emath_ir::capability::softmax_reference_strict_f64(input).expect("oracle computes");
            assert_eq!(&native, expected, "oracle pinned for {input:?}");
            let provider = run_softmax_seam(input).expect("provider binding evaluates");
            assert_eq!(
                compare_outputs(expected, &provider),
                ConformanceVerdict::Conformant,
                "bit-exact conformance for {input:?}"
            );
        }
    }

    // The comparison DISCRIMINATES: one flipped bit in the provider
    // output is a typed divergence naming the position and both bit
    // patterns.
    let mut mutant = emath_ir::capability::softmax_reference_strict_f64(&[1.0, 2.0, 3.0])
        .expect("oracle computes");
    mutant[1] = f64::from_bits(mutant[1].to_bits() + 1);
    match compare_outputs(
        &emath_ir::capability::softmax_reference_strict_f64(&[1.0, 2.0, 3.0])
            .expect("oracle computes"),
        &mutant,
    ) {
        ConformanceVerdict::Diverged { index, .. } => assert_eq!(index, 1),
        other => panic!("expected Diverged, got {other:?}"),
    }
}

#[test]
fn wrong_axis_binding_fails_typed() {
    // Softmax provider binding: the reduction axis is part of the
    // binding contract; a provider reporting the wrong axis FAILS typed
    // (E-PROVIDER-002), never silently reinterpreted.
    let binding = ProviderBinding {
        capability: STD_TENSOR_SOFTMAX.to_string(),
        reduction_axis: 0,
    };
    binding.check_axis(0).expect("declared axis accepts itself");
    match binding.check_axis(1) {
        Err(AdapterContractError::AxisMismatch {
            capability,
            declared,
            reported,
        }) => {
            assert_eq!(capability, STD_TENSOR_SOFTMAX);
            assert_eq!(declared, 0);
            assert_eq!(reported, 1);
        }
        other => panic!("expected AxisMismatch, got {other:?}"),
    }
}

#[test]
fn provider_native_types_fail_the_gate() {
    // The gate is an ALLOWLIST: only IR-owned tokens pass. Provider-
    // native types (torch/jax/ndarray) in the public IR-facing signature
    // refuse typed (E-PROVIDER-001) — the negative seed's scenario.
    for native in ["torch::Tensor", "jax.Array", "numpy.ndarray"] {
        match ir_type_gate(native) {
            Err(AdapterContractError::NativeTypeInIr { token }) => {
                assert_eq!(token, native);
            }
            other => panic!("expected NativeTypeInIr for {native}, got {other:?}"),
        }
    }
    for owned in [
        "scalar<f64>",
        "vector<f64>",
        "matrix<f64>",
        "tensor<f64>",
        "bool",
    ] {
        ir_type_gate(owned).expect("IR-owned token passes");
    }

    // Generating a contract whose declared IR signature names a
    // provider-native type refuses at generation time too (the gate is
    // not bypassable by skipping the check).
    let leaked = CellSchema {
        name: QualifiedName::single("test.leaky"),
        class: SchemaClass::Provider,
        version: "1.0.0".to_string(),
        migration: MigrationPolicy::Frozen,
        arity: 1,
        about: None,
    };
    // The Provider-class cell has no local oracle: its contract carries
    // structural fixtures only, and the leak gate is exercised through
    // the signature tokens below.
    let contract = emath_provider_api::adapter::adapter_contract(&leaked)
        .expect("structural contract generates for an oracle-less cell");
    assert!(contract.fixtures.iter().all(|f| f.expected.is_none()));
    let mut signature = BTreeMap::new();
    signature.insert("input".to_string(), "torch::Tensor".to_string());
    match emath_provider_api::adapter::gate_signature(&signature) {
        Err(AdapterContractError::NativeTypeInIr { token }) => {
            assert_eq!(token, "torch::Tensor");
        }
        other => panic!("expected NativeTypeInIr, got {other:?}"),
    }

    // Negative seed: the seeded silent-success declares the typed gate
    // refusal.
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/provider_adapter_contracts.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-PROVIDER-001"),
        "seed expects the IR-purity gate refusal, found: {expect_line}"
    );
}

#[test]
fn conformance_lands_in_bundle() {
    // WorldResultBundle fixture: the conformance run
    // is a labeled world record in the envelope — the provider is
    // a checked WORKER, never the public meaning (the oracle stays in
    // emath-ir).
    struct ConformanceWorld;
    impl FirstOrderWorld for ConformanceWorld {
        type Value = String;
        type Error = EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let contract = emath_provider_api::adapter::adapter_contract(&softmax_schema())
                .expect("contract generates");
            for fixture in &contract.fixtures {
                let expected = fixture.expected.as_ref().expect("oracle present");
                for input in &fixture.inputs {
                    let provider = run_softmax_seam(input).expect("provider binding evaluates");
                    assert_eq!(
                        compare_outputs(expected, &provider),
                        ConformanceVerdict::Conformant
                    );
                }
            }
            Ok("conformant".to_string())
        }

        fn apply(
            &self,
            operator: &SymbolId,
            _arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            Err(EvalError::UnknownSymbol(operator.clone()))
        }

        fn evidence(&self) -> emath_genesis::WorldEvidence {
            emath_genesis::WorldEvidence::seed(
                "provider-conformance",
                &["oracle-bit-parity", "axis-contract"],
            )
        }
    }

    let term = Term::Constant(SymbolId("softmax-conformance[std.tensor.softmax]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = evaluate_labeled(
        &term,
        &ConformanceWorld,
        &environment,
        WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(result.disposition, Disposition::Answer { .. }));
    assert_eq!(result.world, "provider-conformance");
    let bundle = ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
