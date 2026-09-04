//! Thermo-equilibrium slice: Wegscheider
//! cycle consistency and ideal-mixture Gibbs minimization, both in
//! `.emath`-first form over generic nucleus seams — no chemistry Rust
//! crate, no domain op enum, no parser branch.
//!
//! Wegscheider: a closed reaction cycle is thermodynamically consistent
//! exactly when the product of the forward equilibrium constants equals
//! the product of the reverse constants around the cycle — over the
//! rationals. With each K_i = p_i/q_i in lowest terms, consistency is
//! the exact integer equality `∏ p_i == ∏ q_i`. The `std.chem.cycle_consistent(P, Q)`
//! cell certifies that; a nonzero delta `∏P − ∏Q` refuses typed
//! `CycleInconsistency(delta d)` with the witness residual.
//!
//! Gibbs: the ideal-mixture free energy along a reaction extent
//! `G(ξ) = Σ (n0_i + ν_i ξ)(μ0_i + RT ln(n_i/N))` is minimized through
//! the EXISTING goal path (`minimize(expr) wrt ξ`, Newton), with the
//! stoichiometry vector ν certified by the existing `mass_balance` cell
//! (conservation is automatic along ξ when ν is a null vector of S).
//!
//! Failure-first: the cycle fixtures were written BEFORE the cell and
//! failed on the missing capability; the Gibbs goal admission was
//! already green (existing path) and is pinned as a baseline, not
//! claimed as failure-first.

use emath_core::limits::Limits;
use emath_core::Span;
use emath_exec_ir::install::install_pack;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::std_cell_registry;
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

/// The capability path of the Wegscheider cycle-consistency cell.
const CYCLE_CONSISTENT: &str = "std.chem.cycle_consistent";

/// The chemistry field-pack source: exports the thermo cells with the
/// existing ones; nothing is forked.
const CHEM_PACK: &str = "\
package std

emath field_pack chemistry:
    exports:
        cell balance
        cell mass_balance
        cell graph_rewrite_preserve
        cell cycle_consistent
    metadata:
        description mass balance balancing rewrite valence plus Wegscheider cycle consistency
";

/// Admit the pack source at the language layer and return the entry.
fn admitted_pack(source: &str) -> emath_ir::FieldPackEntry {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("chemistry-pack", source);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    let messages: Vec<String> = result
        .diagnostics
        .items()
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        codes.is_empty(),
        "the chemistry pack admits at the language layer, got {messages:?}"
    );
    let mut packs = result.package.field_packs;
    assert_eq!(packs.len(), 1, "one field_pack admitted");
    packs.remove(0)
}

/// Hand-built EMIR applying the cycle-consistency cell to P and Q
/// (numerator and denominator vectors of the cycle's K_i).
fn cycle_program() -> EmirProgram {
    let span = Span::default();
    EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), span),
            (EmirOp::LoadInput(1), span),
            (
                EmirOp::ApplyCapability {
                    capability: CYCLE_CONSISTENT.to_string(),
                    class: CellClass::Pure,
                    args: vec![EmirValue(0), EmirValue(1)],
                },
                span,
            ),
        ],
        result: EmirValue(2),
        input_count: 2,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

/// Run the cycle checker over numerator/denominator vectors.
fn call_cycle(p: Vec<f64>, q: Vec<f64>) -> Result<Value, EvalFault> {
    evaluate_with_budget(
        &cycle_program(),
        &[Value::Vector(p), Value::Vector(q)],
        &[],
        EvalBudget::default(),
    )
}

/// Consistent cycle: K = [2, 1/2, 1] around a three-step cycle —
/// `∏P = 2 == ∏Q = 2`, exact.
#[test]
fn three_step_cycle_is_consistent() {
    let out = call_cycle(vec![2.0, 1.0, 1.0], vec![1.0, 2.0, 1.0])
        .expect("consistent cycle evaluates in the reference VM");
    assert_eq!(out, Value::F64(0.0), "delta is exactly zero");
}

/// Consistent cycle with six-step composition: K = [3, 2, 1/6] —
/// `∏P = 6 == ∏Q = 6`.
#[test]
fn six_cycle_composition_is_consistent() {
    let out = call_cycle(vec![3.0, 2.0, 1.0], vec![1.0, 1.0, 6.0])
        .expect("composed cycle evaluates");
    assert_eq!(out, Value::F64(0.0));
}

/// Consistent cycle at unit scale: all K = 1 trivially consistent.
#[test]
fn unit_cycle_is_consistent() {
    let out = call_cycle(vec![1.0, 1.0], vec![1.0, 1.0])
        .expect("unit cycle evaluates");
    assert_eq!(out, Value::F64(0.0));
}

/// Inconsistent cycle: K = [2, 1, 3] — `∏P = 6`, `∏Q = 1`, delta = 5.
/// The refusal is typed and carries the exact witness delta.
#[test]
fn inconsistent_cycle_refuses_typed_with_witness() {
    match call_cycle(vec![2.0, 1.0, 3.0], vec![1.0, 1.0, 1.0]) {
        Err(EvalFault::CapabilityRefused { capability, code }) => {
            assert_eq!(capability, CYCLE_CONSISTENT);
            assert_eq!(
                code, "CycleInconsistency(residual 5)",
                "refusal names the exact witness delta"
            );
        }
        ok => panic!("inconsistent cycle must refuse typed, got {ok:?}"),
    }
}

/// MR reverse-cycle invariance (score 8): reversing the order of the
/// K_i around a closed cycle changes neither product.
#[test]
fn mr_reverse_cycle_invariance() {
    let p = vec![3.0, 2.0, 1.0];
    let q = vec![1.0, 1.0, 6.0];
    let forwards = call_cycle(p.clone(), q.clone()).expect("forwards consistent");
    let reversed: Vec<f64> = p.iter().rev().copied().collect();
    let qrev: Vec<f64> = q.iter().rev().copied().collect();
    let backwards = call_cycle(reversed, qrev).expect("reversed consistent");
    assert_eq!(forwards, Value::F64(0.0));
    assert_eq!(backwards, Value::F64(0.0));
}

/// MR step permutation invariance (score 8): a rotation of the cycle
/// steps leaves the products unchanged.
#[test]
fn mr_cycle_rotation_invariance() {
    let p = vec![3.0, 2.0, 1.0];
    let q = vec![1.0, 1.0, 6.0];
    for rot in 0..p.len() {
        let mut pr = Vec::with_capacity(p.len());
        let mut qr = Vec::with_capacity(q.len());
        for i in 0..p.len() {
            pr.push(p[(i + rot) % p.len()]);
            qr.push(q[(i + rot) % q.len()]);
        }
        let out = call_cycle(pr, qr).expect("rotated cycle consistent");
        assert_eq!(out, Value::F64(0.0), "rotation {rot} preserves consistency");
    }
}

/// MR composition MR (score 8): concatenating two consistent cycles
/// yields a consistent cycle (products multiply).
#[test]
fn mr_consistent_cycles_compose() {
    let (p1, q1) = (vec![2.0, 1.0, 1.0], vec![1.0, 2.0, 1.0]);
    let (p2, q2) = (vec![3.0, 2.0, 1.0], vec![1.0, 1.0, 6.0]);
    let mut pc = p1.clone();
    pc.extend(&p2);
    let mut qc = q1.clone();
    qc.extend(&q2);
    let out = call_cycle(pc, qc).expect("composed cycles consistent");
    assert_eq!(out, Value::F64(0.0));
}

/// MR cancellation trap (score 10): pairwise-cancelling deltas must
/// NOT sum to zero — the law is on the PRODUCT, so a cycle with one
/// inconsistent segment composing with another is still inconsistent.
#[test]
fn mr_inconsistent_plus_consistent_still_inconsistent() {
    let (pb, qb) = (vec![2.0, 1.0, 3.0], vec![1.0, 1.0, 1.0]); // delta 5
    let (p2, q2) = (vec![3.0, 2.0, 1.0], vec![1.0, 1.0, 6.0]); // consistent
    let mut pc = pb.clone();
    pc.extend(&p2);
    let mut qc = qb.clone();
    qc.extend(&q2);
    let product_p = 6.0 * 6.0; // 6 * 6 = 36
    let product_q = 1.0 * 6.0; // 1 * 6 = 6
    let delta = product_p - product_q;
    match call_cycle(pc, qc) {
        Err(EvalFault::CapabilityRefused { code, .. }) => {
            assert!(
                code.contains(&format!("residual {delta}")),
                "witness is the exact product delta {delta}, got {code}"
            );
        }
        ok => panic!("a broken cycle stays broken under composition, got {ok:?}"),
    }
}

/// Boundary: non-integer K entries have no exact rational product —
/// refuse typed (E-EXACT-001).
#[test]
fn non_integer_entries_refuse_typed() {
    match call_cycle(vec![1.5, 1.0], vec![1.0, 1.0]) {
        Err(EvalFault::Arithmetic { op, detail }) => {
            assert_eq!(op, "exact-product-delta");
            assert!(
                detail.contains("E-EXACT-001"),
                "non-integral entry refuses with the typed code, got {detail}"
            );
        }
        ok => panic!("non-integer K entries must refuse typed, got {ok:?}"),
    }
}

// ===========================================================================
// Gibbs minimization through the existing goal path.
// ===========================================================================

/// A complete ideal-mixture Gibbs model along the reaction extent:
/// `2 H2 + O2 -> 2 H2O` with ν = [2, 1, -2] (certified by the existing
/// mass_balance cell), free energy `G(ξ)` minimized by the EXISTING
/// single-variable Newton goal path. Conservation is automatic along ξ
/// because ν is in the nullspace of S.
const GIBBS_MODEL: &str = "\
# Ideal-mixture Gibbs free energy along the reaction extent xi for
# 2 H2 + O2 -> 2 H2O. n_i(xi) = n0_i + nu_i * xi stays element-
# conserving because nu = [2, 1, -2] is a null vector of the
# composition matrix (certified separately by std.chem.mass_balance).
# G(xi) = sum n_i * (mu0_i + RT * ln(n_i / N)) is convex on the
# interior (n > 0); Newton from x=0.5 minimizes it through the
# existing goal path.
emath function GibbsModel:
    inputs:
        xi: Float64

    definitions:
        n_h2 = 1.0 + 2.0 * xi
        n_o2 = 0.5 + 1.0 * xi
        n_h2o = 0.0 - 2.0 * xi
        N = n_h2 + n_o2 + n_h2o
        mu0 = 0.0 + 0.0 * xi
        G = n_h2 * (mu0 + ln(n_h2 / N)) \
          + n_o2 * (mu0 + ln(n_o2 / N)) \
          + n_h2o * (mu0 + ln(n_h2o / N))
        xi_star = minimize(G) wrt xi

    goals:
        evaluate <xi_star>:
            produce rust.library

    tests:
        example <equilibrium_extent>:
            given xi = 0.5
            expect xi_star > 0.0
";

/// The Gibbs model ADMITS through the existing goal path (baseline:
/// this was green BEFORE this slice — pinned, not failure-first).
#[test]
fn gibbs_goal_model_admits() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("gibbs", GIBBS_MODEL);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    assert!(
        codes.is_empty(),
        "the Gibbs goal model admits, got {codes:?}"
    );
}

/// Conservation contract: the extent direction nu = [2, 1, -2] is a
/// null vector of S (H2, O2, H2O composition) — the existing
/// mass_balance cell certifies it.
#[test]
fn gibbs_extent_is_mass_conserving() {
    let matrix = Value::Matrix {
        rows: 2,
        cols: 3,
        data: vec![2.0, 0.0, 2.0, 0.0, 2.0, 1.0],
    };
    let out = evaluate_with_budget(
        &{
            let span = Span::default();
            EmirProgram {
                ops: vec![
                    (EmirOp::LoadInput(0), span),
                    (EmirOp::LoadInput(1), span),
                    (
                        EmirOp::ApplyCapability {
                            capability: "std.chem.mass_balance".to_string(),
                            class: CellClass::Pure,
                            args: vec![EmirValue(0), EmirValue(1)],
                        },
                        span,
                    ),
                ],
                result: EmirValue(2),
                input_count: 2,
                state_count: 0,
                domain_obligations: Vec::new(),
            }
        },
        &[matrix, Value::Vector(vec![2.0, 1.0, -2.0])],
        &[],
        EvalBudget::default(),
    )
    .expect("conservation certificate evaluates");
    assert_eq!(out, Value::Vector(vec![0.0, 0.0]));
}

/// Negative control: an unbalanced extent direction refuses through
/// the mass_balance cell (never a silent Gibbs drift).
#[test]
fn unconserved_extent_refuses() {
    let matrix = Value::Matrix {
        rows: 2,
        cols: 3,
        data: vec![2.0, 0.0, 2.0, 0.0, 2.0, 1.0],
    };
    let span = Span::default();
    let program = EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), span),
            (EmirOp::LoadInput(1), span),
            (
                EmirOp::ApplyCapability {
                    capability: "std.chem.mass_balance".to_string(),
                    class: CellClass::Pure,
                    args: vec![EmirValue(0), EmirValue(1)],
                },
                span,
            ),
        ],
        result: EmirValue(2),
        input_count: 2,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    match evaluate_with_budget(
        &program,
        &[matrix, Value::Vector(vec![1.0, 1.0, -1.0])],
        &[],
        EvalBudget::default(),
    ) {
        Err(EvalFault::CapabilityRefused { capability, code }) => {
            assert_eq!(capability, "std.chem.mass_balance");
            assert!(
                code.starts_with("MassImbalance"),
                "unconserved extent refuses, got {code}"
            );
        }
        ok => panic!("unconserved extent must refuse typed, got {ok:?}"),
    }
}

/// The pack carries the cycle-consistency cell.
#[test]
fn chemistry_pack_carries_cycle_consistent() {
    let entry = admitted_pack(CHEM_PACK);
    let installed = install_pack(&entry, &["std".to_string()], std_cell_registry())
        .expect("the chemistry pack installs from the existing registry");
    assert!(
        installed
            .exports
            .contains(&CYCLE_CONSISTENT.to_string()),
        "exports carry cycle_consistent: {:?}",
        installed.exports
    );
    installed
        .image
        .validate_partitions()
        .expect("the installed image is self-validating");
}

/// MR unit-scaling law (score 6): scaling every K_i in the cycle by a
/// common unit cancels in the ratio — `∏ (c·p_i) == ∏ (c·q_i)` for
/// consistent cycles, so unit rescaling preserves consistency; and a
/// consistent cycle scaled by c stays consistent exactly.
#[test]
fn mr_common_unit_scaling_preserves_consistency() {
    let p = vec![2.0, 1.0, 1.0];
    let q = vec![1.0, 2.0, 1.0];
    for c in [1.0, 3.0, 7.0] {
        let ps: Vec<f64> = p.iter().map(|x| x * c).collect();
        let qs: Vec<f64> = q.iter().map(|x| x * c).collect();
        let out = call_cycle(ps, qs)
            .expect("unit-scaled consistent cycle evaluates");
        assert_eq!(out, Value::F64(0.0), "scale factor {c}");
    }
}

/// MR species-permutation on the CYCLE level (score 8): reordering the
/// K_i factors (a rotation) preserves the products — already covered by
/// rotation; here the permutation is an arbitrary reordering.
#[test]
fn mr_arbitrary_factor_permutation_invariance() {
    let p = vec![2.0, 1.0, 3.0, 1.0];
    let q = vec![1.0, 2.0, 1.0, 3.0];
    let perm: Vec<usize> = vec![3, 0, 2, 1];
    let mut pp: Vec<f64> = Vec::with_capacity(perm.len());
    let mut qq: Vec<f64> = Vec::with_capacity(perm.len());
    for &i in &perm {
        pp.push(p[i]);
        qq.push(q[i]);
    }
    // Same products: 6 == 6 → consistent.
    let out = call_cycle(pp, qq).expect("permuted factors evaluate");
    assert_eq!(out, Value::F64(0.0));
}

/// Boundary: an empty cycle (no steps) has products 1 == 1 —
/// trivially consistent, and the empty product is exactly one.
#[test]
fn empty_cycle_is_trivially_consistent() {
    let out = call_cycle(vec![], vec![]).expect("empty cycle evaluates");
    assert_eq!(out, Value::F64(0.0));
}

/// Boundary: length-mismatched P and Q refuse typed — the products
/// cannot be compared.
#[test]
fn length_mismatch_refuses_typed() {
    match call_cycle(vec![2.0, 1.0], vec![1.0]) {
        Err(EvalFault::Arithmetic { op, detail }) => {
            assert_eq!(op, "exact-product-delta");
            assert!(
                detail.contains("E-EXACT-001"),
                "length mismatch refuses with the typed code, got {detail}"
            );
        }
        ok => panic!("length mismatch must refuse typed, got {ok:?}"),
    }
}

/// Boundary: a zero K_i factor makes the product zero — a cycle with
/// any K = 0 is degenerate and, unless BOTH sides have a zero,
/// inconsistent (0 vs nonzero delta).
#[test]
fn zero_factor_inconsistency_refuses() {
    // P has a zero factor, Q does not: delta = -qproduct.
    match call_cycle(vec![0.0, 2.0], vec![1.0, 3.0]) {
        Err(EvalFault::CapabilityRefused { code, .. }) => {
            assert!(
                code.starts_with("CycleInconsistency"),
                "zero-factor mismatch refuses, got {code}"
            );
        }
        ok => panic!("zero-factor mismatch must refuse, got {ok:?}"),
    }
}

/// Regression (mail 93): exact products above 2^53 must not be
/// compared through an f64 cast. P = [1e9, 1e9] has `∏P = 10^18`
/// exactly; Q = [999999999, 1000000001] has `∏Q = 10^18 − 1` — two
/// DISTINCT exact products that both cast to the same f64 near 1e18.
/// A `pp as f64 − qq as f64` comparison yields 0.0 and would falsely
/// certify consistency; the u128 compare must refuse typed with the
/// exact witness delta (∏P − ∏Q = +1), never a false zero.
#[test]
fn near_equal_large_products_refuse_exact() {
    match call_cycle(
        vec![1_000_000_000.0, 1_000_000_000.0],
        vec![999_999_999.0, 1_000_000_001.0],
    ) {
        Err(EvalFault::CapabilityRefused { code, .. }) => {
            assert_eq!(
                code, "CycleInconsistency(residual 1)",
                "distinct exact products above 2^53 refuse with the exact witness delta"
            );
        }
        ok => panic!(
            "near-equal large products must refuse exact inconsistency, got {ok:?} \
             (false zero above 2^53)"
        ),
    }
}
