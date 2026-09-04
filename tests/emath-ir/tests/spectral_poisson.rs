//! (thin nucleus slice): spectral Poisson on the unit
//! interval, Dirichlet class, via the discrete sine diagonalization.
//!
//! The law, thinned to one honest end-to-end path (FEM assembly
//! and BC classes beyond Dirichlet are named deferrals, not claims):
//! - **The method**: sample the load `f` at the `n` interior nodes of
//!   a uniform grid on [0,1] (`h = 1/(n+1)`, `u(0) = u(1) = 0`); the
//!   3-point Laplacian diagonalizes EXACTLY in the DST-I sine basis
//!   with eigenvalues `μ_k = −(4/h²)·sin²(kπ/(2(n+1)))`. The solve is
//!   a forward sine transform, diagonal division, inverse transform —
//!   deterministic O(n²) strict-f64, zero new solver machinery.
//! - **Discriminating laws** (no tautologies):
//!   1. `f ≡ 1` → the discrete solution is the sampled exact solution
//!      `x(1−x)/2` to machine precision at EVERY node (quadratics are
//!      exact under the 3-point stencil; a wrong normalization,
//!      eigenvalue form, or sign fails this).
//!   2. `f = sin(πx)` (the first eigenmode) → discrete solution
//!      `sin(πx)·h²/(4sin²(πh/2))` = continuous `sin(πx)/π² ×
//!      (1 + O(h²))` — the midpoint error must HALVE-order (ratio ≈ 4
//!      when n doubles), killing eigenvalue-form mutants.
//!   3. Symmetry metamorphic law: symmetric load ⟹ symmetric solve.
//! - **Typed refusals**: an empty interior (`E-PDE-001` — no nodes,
//!   no solve; the negative seed's shape) and non-finite loads
//!   (`E-PDE-002`) refuse; never a silently wrong field.
//! - **Surface**: capsule FeatureID
//!   `std.capability.pde.poisson-sine` with a checked vector carrier;
//!   scalar loads refuse at the installed kernel ABI.

use std::path::{Path, PathBuf};

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::{KernelArity, install_language_distribution, native_kernel};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{SymbolId, Term};

const POISSON_SINE: &str = "std.capability.pde.poisson-sine";

fn language_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

/// Capability cells resolve against the installed Language Image
/// (thread-local bindings); every evaluating test installs the
/// checked-in distribution first.
fn install_language() {
    let distribution = load_language_distribution(&language_root()).expect("language distribution");
    install_language_distribution(&distribution).expect("pde kernels install");
}

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

fn solve(load: &[f64]) -> Result<Vec<f64>, EvalFault> {
    let value = eval(
        vec![cell(POISSON_SINE, vec![EmirValue(0)])],
        &[Value::Vector(load.to_vec())],
    )?;
    let Value::Vector(field) = value else {
        panic!("expected a vector field, got {value:?}")
    };
    Ok(field)
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

/// The exact solution of `-u'' = 1, u(0) = u(1) = 0`.
fn quadratic_exact(x: f64) -> f64 {
    x * (1.0 - x) / 2.0
}

#[test]
fn constant_load_sample_exact() {
    // f ≡ 1: the discrete solve IS the sampled exact solution
    // (quadratics are exact under the 3-point Laplacian). A mutant
    // with a wrong DST normalization, a wrong eigenvalue form, or a
    // sign flip fails the 1e-12 law at some node.
    for n in [7usize, 15, 31] {
        let h = 1.0 / (n as f64 + 1.0);
        let field = solve(&vec![1.0; n]).expect("constant load solves");
        assert_eq!(field.len(), n, "interior field length law");
        for (j, u) in field.iter().enumerate() {
            let x = (j as f64 + 1.0) * h;
            let exact = quadratic_exact(x);
            assert!(
                (u - exact).abs() < 1e-12,
                "n={n} node {j}: u={u} vs exact {exact}"
            );
        }
    }
}

#[test]
fn first_eigenmode_second_order_convergence() {
    // f = sin(πx) is the first sine eigenmode: the discrete solution
    // is the continuous solution sin(πx)/π² times (1 + O(h²)). The
    // midpoint error must shrink by ≈ 4× when n doubles — the
    // order-2 law (a wrong discrete eigenvalue, e.g. dividing by the
    // continuous π² or by (πk)²/h² without the sine form, breaks the
    // ratio).
    let midpoint_error = |n: usize| -> f64 {
        let h = 1.0 / (n as f64 + 1.0);
        let load: Vec<f64> = (1..=n)
            .map(|j| (std::f64::consts::PI * j as f64 * h).sin())
            .collect();
        let field = solve(&load).expect("eigenmode solves");
        let mid = n / 2; // interior index of x ≈ 0.5
        let x = (mid as f64 + 1.0) * h;
        // Exact solution of -u'' = sin(πx), u(0)=u(1)=0: sin(πx)/π².
        let continuous =
            (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * std::f64::consts::PI);
        (field[mid] - continuous).abs()
    };
    let coarse = midpoint_error(7);
    let fine = midpoint_error(15);
    assert!(coarse < 0.02, "coarse error bounded, got {coarse}");
    assert!(fine < 0.006, "fine error bounded, got {fine}");
    let ratio = coarse / fine;
    assert!(
        (3.0..5.5).contains(&ratio),
        "second-order law: coarse/fine ≈ 4, got {ratio} ({coarse} / {fine})"
    );
}

#[test]
fn symmetric_load_symmetric_field() {
    // Metamorphic law: f(1−x) = f(x) ⟹ u(1−x) = u(x) to machine
    // precision (the sine diagonalization is order-preserving; a
    // mutant that reverses the transform index fails).
    let n = 15usize;
    let h = 1.0 / (n as f64 + 1.0);
    let load: Vec<f64> = (1..=n)
        .map(|j| {
            let x = j as f64 * h;
            1.0 + (std::f64::consts::PI * x).sin()
        })
        .collect();
    let field = solve(&load).expect("symmetric load solves");
    for j in 0..n / 2 {
        assert!(
            (field[j] - field[n - 1 - j]).abs() < 1e-12,
            "symmetry: u_{j} = u_{{n+1-j}}, got {} vs {}",
            field[j],
            field[n - 1 - j]
        );
    }
}

#[test]
fn empty_domain_refuses_typed() {
    // E-PDE-001: no interior nodes, no solve — the negative seed's
    // silent-success shape.
    let error = solve(&[]).expect_err("empty interior refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-PDE-001"),
        "empty interior must name E-PDE-001, got {fault}"
    );
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/spectral_poisson.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-PDE-001"),
        "seed expects the empty-domain refusal, found: {expect_line}"
    );
}

#[test]
fn non_finite_load_refuses_typed() {
    // E-PDE-002: a NaN load sample refuses — never a silently
    // corrupted field.
    let error = solve(&[1.0, f64::NAN, 1.0]).expect_err("non-finite load refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-PDE-002"),
        "non-finite load must name E-PDE-002, got {fault}"
    );
}

#[test]
fn poisson_capsule_kernel_abi_and_shape_law() {
    install_language();
    let kernel = native_kernel(POISSON_SINE).expect("Poisson capsule kernel bound");
    assert_eq!(kernel.kernel_id, "checked-poisson-dirichlet-sine");
    assert_eq!(kernel.arity_contract(), KernelArity::Exact(1));

    // Evaluate through ApplyCapability: f ≡ 1 on n = 7.
    let n = 7usize;
    let mut ops: Vec<(EmirOp, Span)> = vec![(EmirOp::LoadInput(0), Span::default())];
    ops.push((cell(POISSON_SINE, vec![EmirValue(0)]), Span::default()));
    let program = EmirProgram {
        ops,
        result: EmirValue(1),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let value = evaluate_with_budget(
        &program,
        &[Value::Vector(vec![1.0; n])],
        &[],
        EvalBudget::default(),
    )
    .expect("cell evaluates");
    let Value::Vector(field) = value else {
        panic!("expected a vector field")
    };
    let h = 1.0 / (n as f64 + 1.0);
    for (j, u) in field.iter().enumerate() {
        let exact = quadratic_exact((j as f64 + 1.0) * h);
        assert!(
            (u - exact).abs() < 1e-12,
            "cell field node {j}: {u} vs {exact}"
        );
    }

    let error = eval(
        vec![cell(POISSON_SINE, vec![EmirValue(0)])],
        &[Value::F64(1.0)],
    )
    .expect_err("scalar load refuses at the kernel ABI");
    assert!(
        matches!(error, EvalFault::CapabilityRefused { ref code, .. } if code.contains("E-TYPE-012")),
        "scalar load must refuse typed, got {error:?}"
    );
}

#[test]
fn bundle_fixture() {
    // WorldResultBundle fixture (e2e clause; the VM path is touched).
    struct PdeWorld;
    impl emath_genesis::FirstOrderWorld for PdeWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let field = solve(&vec![1.0; 7])
                .map(|field| field[3])
                .unwrap_or(f64::NAN);
            // Midpoint of the n = 7 constant-load field = 1/32.
            if (field - 1.0 / 32.0).abs() < 1e-12 {
                Ok("spectral-poisson-exact".to_string())
            } else {
                Ok("spectral-poisson-diverged".to_string())
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
                "spectral-poisson-nucleus",
                &["dst-diagonalization", "dirichlet-unit-interval"],
            )
        }
    }

    let term = Term::Constant(SymbolId("pde[fixture]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &PdeWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "spectral-poisson-nucleus");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
