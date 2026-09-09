//! (thin B39 slice): finite-category
//! diagram commutativity kernel).
//!
//! The law, sliced to the numeric-kernel + EMIR seam (B38
//! forms/manifolds and B45 algebraic geometry stay deferred — world and
//! parser lanes; functor/natural-transformation surfaces are NOT
//! claimed):
//! - **Carrier law (design: dense composition table)**: a
//!   category is `(dom, cod, comp)` — `dom`/`cod` are per-morphism
//!   object indices (k entries each), `comp` is a k×k table where
//!   `comp[i][j] = m_i ∘ m_j` (j FIRST, then i) and requires
//!   `cod[j] == dom[i]`; the composite's dom/cod are `dom[j]`/`cod[i]`.
//!   An entry `-1.0` is UNDEFINED; every aligned pair MUST be defined,
//!   every misaligned pair MUST be `-1`. Objects are implicit
//!   `0..n`, `n = max(dom ∪ cod) + 1`. Equal morphism INDEX means
//!   equal morphism (the dense-table representation law).
//! - **Category laws are CERTIFIED, never assumed**: the gate refuses
//!   `E-CAT-001` (non-finite entry), `E-CAT-002` (shape: dims, face
//!   encoding, path/endpoint geometry), `E-CAT-003` (out-of-range or
//!   non-integral index), `E-CAT-004` (composition law: aligned-missing,
//!   misaligned-defined, dangling path segment), `E-CAT-005` (identity
//!   law: an appearing object with no identity morphism), `E-CAT-006`
//!   (associativity law or definedness disagreement), `E-CAT-007`
//!   (carrier too large to certify associativity — k > 64; never
//!   commute-check over an unverified table).
//! - **Face path-pairs **: a face is a flat record
//!   `[start, end, len_l, len_r, left…, right…]` (both paths ≥ 1
//!   morphism — identities are explicit carrier morphisms, empty paths
//!   refuse `E-CAT-002`). A face is commutative iff the two path
//!   composites are the SAME morphism index. The result is the
//!   per-face mask in face order (the Pareto-mask convention).
//! - Determinism class: fixed-order law passes, first-failure refusal,
//!   index-fold path evaluation; identical inputs are bit-identical.

use std::path::{Path, PathBuf};

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::{KernelArity, install_language_distribution, native_kernel};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_test_harness::Probe;

fn language_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

/// Capability cells resolve against the installed Language Image
/// (thread-local bindings); every evaluating test installs the
/// checked-in distribution first.
fn install_language() {
    let distribution = load_language_distribution(&language_root()).expect("language distribution");
    install_language_distribution(&distribution).expect("category kernels install");
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

const CATEGORY_CHECK: &str = "std.capability.category.check";
const CATEGORY_COMMUTATIVE: &str = "std.capability.category.commutative";

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    install_language();
    // The seam law: LoadInput per input, result = last register.
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

fn vector_of(value: &Value) -> Vec<f64> {
    let Value::Vector(v) = value else {
        panic!("expected a vector, got {value:?}")
    };
    v.clone()
}

fn bool_of(value: &Value) -> bool {
    let Value::Bool(b) = value else {
        panic!("expected a bool, got {value:?}")
    };
    *b
}

fn refused_code(fault: &EvalFault) -> String {
    let EvalFault::CapabilityRefused { code, .. } = fault else {
        panic!("expected a typed capability refusal, got {fault:?}")
    };
    code.clone()
}

fn matrix(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}

/// The cyclic group Z/3 as a one-object category: three morphisms
/// (0, 1, 2), composition `m_i ∘ m_j = (i + j) mod 3`. All table laws
/// hold (0 is the identity; addition is associative).
fn z3_category() -> (Value, Value, Value) {
    let comp: Vec<f64> = (0..3)
        .flat_map(|i| (0..3).map(move |j| ((i + j) % 3) as f64))
        .collect();
    (
        Value::Vector(vec![0.0, 0.0, 0.0]),
        Value::Vector(vec![0.0, 0.0, 0.0]),
        matrix(3, 3, &comp),
    )
}

/// The free category on one arrow: objects {0, 1}, morphisms
/// {id0, id1, f:0→1}. Aligned pairs are exactly (id0,id0), (id1,id1),
/// (id1,f), (f,id0); everything else is −1.
fn free_arrow_category() -> (Value, Value, Value) {
    // index: 0=id0, 1=id1, 2=f
    let comp = vec![
        0.0, -1.0, -1.0, // comp[id0][x]: only id0 aligned
        -1.0, 1.0, 2.0, // comp[id1][x]: id1, f aligned
        2.0, -1.0, -1.0, // comp[f][x]: only id0 aligned
    ];
    (
        Value::Vector(vec![0.0, 1.0, 0.0]),
        Value::Vector(vec![0.0, 1.0, 1.0]),
        matrix(3, 3, &comp),
    )
}

#[test]
fn intent() {
    let mut p = Probe::new("(thin B39 slice): finite-category");
    p.case("category_check_certifies_z3", |p| {
// Z/3 certifies as a category (the gate returns TRUE — the value is
// the certification; every failure is a typed refusal, never false).

    let (dom, cod, comp) = z3_category();
    let checked = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[dom, cod, comp],
    )
    .expect("Z/3 is a category");
    p.demand("category_check_certifies_z3#1", bool_of(&checked), "category_check_certifies_z3#1: bool_of(&checked)");

    });
    p.case("category_entry_law_refusals", |p| {
// Composition-law refusals (E-CAT-004): an aligned pair with a
// missing entry, and a defined entry on a misaligned pair.

    let (dom, cod, comp) = z3_category();
    let Value::Matrix {
        mut data,
        rows,
        cols,
    } = comp
    else {
        { p.fail("category_entry_law_refusals#1", format!("z3 comp is a matrix")); return; }};
    let _ = (rows, cols);
    data[1 * 3 + 2] = -1.0; // comp[1][2]: aligned (single object) but missing
    let fault = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[dom.clone(), cod.clone(), matrix(3, 3, &data)],
    )
    .expect_err("aligned pair without a composite");
    p.demand("category_entry_law_refusals#2", refused_code(&fault) == "E-CAT-004", format!("expected {:?}, got {:?}", "E-CAT-004", refused_code(&fault)));

    let (dom, cod, comp) = free_arrow_category();
    let Value::Matrix { mut data, .. } = comp else {
        { p.fail("category_entry_law_refusals#3", format!("free-arrow comp is a matrix")); return; }};
    data[0 * 3 + 2] = 0.0; // comp[id0][f]: misaligned (cod[f]=1 ≠ dom[id0]=0)
    let fault = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[dom, cod, matrix(3, 3, &data)],
    )
    .expect_err("defined entry on a misaligned pair");
    p.demand("category_entry_law_refusals#4", refused_code(&fault) == "E-CAT-004", format!("expected {:?}, got {:?}", "E-CAT-004", refused_code(&fault)));

    });
    p.case("category_identity_law_refusal", |p| {
// Identity-law refusal (E-CAT-005): break the identity action
// (comp[0][1] = 2) and NO morphism of Z/3 acts as the identity —
// the object has no identity morphism.

    let (dom, cod, comp) = z3_category();
    let Value::Matrix { mut data, .. } = comp else {
        { p.fail("category_identity_law_refusal#1", format!("z3 comp is a matrix")); return; }};
    data[0 * 3 + 1] = 2.0; // id no longer acts on m1; no other candidate works
    let fault = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[dom, cod, matrix(3, 3, &data)],
    )
    .expect_err("object 0 lost its identity morphism");
    p.demand("category_identity_law_refusal#2", refused_code(&fault) == "E-CAT-005", format!("expected {:?}, got {:?}", "E-CAT-005", refused_code(&fault)));

    });
    p.case("category_associativity_law_refusal", |p| {
// Associativity-law refusal (E-CAT-006): comp[1][1] = 0 keeps every
// entry/identity law intact but breaks (1∘1)∘2 = 2 ≠ 1 = 1∘(1∘2).

    let (dom, cod, comp) = z3_category();
    let Value::Matrix { mut data, .. } = comp else {
        { p.fail("category_associativity_law_refusal#1", format!("z3 comp is a matrix")); return; }};
    data[1 * 3 + 1] = 0.0;
    let fault = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[dom, cod, matrix(3, 3, &data)],
    )
    .expect_err("(1∘1)∘2 ≠ 1∘(1∘2) after the mutation");
    p.demand("category_associativity_law_refusal#2", refused_code(&fault) == "E-CAT-006", format!("expected {:?}, got {:?}", "E-CAT-006", refused_code(&fault)));

    });
    p.case("oversized_category_refuses", |p| {
// Honesty fence (E-CAT-007): a 65-morphism one-object carrier is a
// category but TOO LARGE to certify associativity by the k ≤ 64
// bound — refused, never commute-checked over an unverified table.

    let k = 65usize;
    let comp: Vec<f64> = (0..k)
        .flat_map(|i| (0..k).map(move |j| ((i + j) % k) as f64))
        .collect();
    let dom = Value::Vector(vec![0.0; k]);
    let cod = Value::Vector(vec![0.0; k]);
    let fault = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[dom, cod, matrix(k, k, &comp)],
    )
    .expect_err("k = 65 exceeds the certifiable associativity bound");
    p.demand("oversized_category_refuses#1", refused_code(&fault) == "E-CAT-007", format!("expected {:?}, got {:?}", "E-CAT-007", refused_code(&fault)));

    });
    p.case("category_index_shape_and_finiteness_refusals", |p| {
// Index/shape/finiteness refusals: NaN entry (E-CAT-001), non-square
// table and mismatched lengths and malformed face records (E-CAT-002),
// out-of-range / non-integral indices (E-CAT-003).

    let (dom, cod, comp) = z3_category();
    let Value::Matrix { mut data, .. } = comp else {
        { p.fail("category_index_shape_and_finiteness_refusals#1", format!("z3 comp is a matrix")); return; }};
    data[2 * 3 + 1] = f64::NAN;
    let fault = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[dom.clone(), cod.clone(), matrix(3, 3, &data)],
    )
    .expect_err("NaN composition entry");
    p.demand("category_index_shape_and_finiteness_refusals#2", refused_code(&fault) == "E-CAT-001", format!("expected {:?}, got {:?}", "E-CAT-001", refused_code(&fault)));
    // Restore the clean table: the finiteness pass precedes the index
    // pass, so the later cases must not carry the NaN.
    data[2 * 3 + 1] = 0.0;

    let fault = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[dom.clone(), cod.clone(), matrix(3, 2, &data[..6])],
    )
    .expect_err("non-square composition table");
    p.demand("category_index_shape_and_finiteness_refusals#3", refused_code(&fault) == "E-CAT-002", format!("expected {:?}, got {:?}", "E-CAT-002", refused_code(&fault)));

    let short_cod = Value::Vector(vec![0.0, 0.0]);
    let fault = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[dom.clone(), short_cod, matrix(3, 3, &data)],
    )
    .expect_err("dom and cod lengths differ");
    p.demand("category_index_shape_and_finiteness_refusals#4", refused_code(&fault) == "E-CAT-002", format!("expected {:?}, got {:?}", "E-CAT-002", refused_code(&fault)));

    // Out-of-range E-CAT-003 manifests in comp TABLE entries (with
    // implicit object indexing, a dom/cod value only widens the object
    // set — it cannot be out of range): an entry naming morphism 7 in
    // a 3-morphism table refuses at parse.
    let fault = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[
            dom.clone(),
            cod.clone(),
            matrix(3, 3, &[7.0, 1.0, 2.0, 1.0, 2.0, 0.0, 2.0, 0.0, 1.0]),
        ],
    )
    .expect_err("composition entry 7 is not a morphism index");
    p.demand("category_index_shape_and_finiteness_refusals#5", refused_code(&fault) == "E-CAT-003", format!("expected {:?}, got {:?}", "E-CAT-003", refused_code(&fault)));

    let fractional = Value::Vector(vec![0.0, 0.5, 0.0]);
    let fault = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[fractional, cod.clone(), matrix(3, 3, &data)],
    )
    .expect_err("a morphism index must be a whole number");
    p.demand("category_index_shape_and_finiteness_refusals#6", refused_code(&fault) == "E-CAT-003", format!("expected {:?}, got {:?}", "E-CAT-003", refused_code(&fault)));

    let (dom, cod, comp) = z3_category();
    let faces = Value::Vector(vec![0.0, 0.0, 5.0, 1.0]);
    let fault = eval(
        vec![cell(
            CATEGORY_COMMUTATIVE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2), EmirValue(3)],
        )],
        &[dom, cod, comp, faces],
    )
    .expect_err("face record overruns the stream");
    p.demand("category_index_shape_and_finiteness_refusals#7", refused_code(&fault) == "E-CAT-002", format!("expected {:?}, got {:?}", "E-CAT-002", refused_code(&fault)));

    });
    p.case("category_commutative_mask_computes", |p| {
// Commutativity on Z/3: the square face (1∘2 vs 2∘1, both = 0) is
// commutative; the triangle face (1 vs 2) is NOT — the per-face mask
// in face order.

    let (dom, cod, comp) = z3_category();
    // Face 1: start 0, end 0, left [1, 2], right [2, 1].
    // Face 2: start 0, end 0, left [1], right [2].
    let faces = Value::Vector(vec![
        0.0, 0.0, 2.0, 2.0, 1.0, 2.0, 2.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0,
    ]);
    let mask = eval(
        vec![cell(
            CATEGORY_COMMUTATIVE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2), EmirValue(3)],
        )],
        &[dom, cod, comp, faces],
    )
    .expect("faces evaluate");
    p.eq("1∘2 = 2∘1 = 0 but 1 ≠ 2", vector_of(&mask), vec![1.0, 0.0]);

    });
    p.case("category_path_geometry_refusals", |p| {
// Path-geometry refusals through the commutative op: a dangling path
// segment (comp[f][f] undefined in the free category) refuses
// `E-CAT-004`; a path that does not run the face's declared
// start→end refuses `E-CAT-002`.

    let (dom, cod, comp) = free_arrow_category();
    // Face: start 0, end 1, left [f, f] (dangling: f ∘ f undefined),
    // right [f].
    let faces = Value::Vector(vec![0.0, 1.0, 2.0, 1.0, 2.0, 2.0, 2.0]);
    let fault = eval(
        vec![cell(
            CATEGORY_COMMUTATIVE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2), EmirValue(3)],
        )],
        &[dom.clone(), cod.clone(), comp.clone(), faces],
    )
    .expect_err("f ∘ f is not defined");
    p.demand("category_path_geometry_refusals#1", refused_code(&fault) == "E-CAT-004", format!("expected {:?}, got {:?}", "E-CAT-004", refused_code(&fault)));

    // Face: start 0, end 0 declared, but the only path [f] runs 0→1.
    let faces = Value::Vector(vec![0.0, 0.0, 1.0, 1.0, 2.0, 2.0]);
    let fault = eval(
        vec![cell(
            CATEGORY_COMMUTATIVE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2), EmirValue(3)],
        )],
        &[dom, cod, comp, faces],
    )
    .expect_err("path [f] does not end at the declared end object");
    p.demand("category_path_geometry_refusals#2", refused_code(&fault) == "E-CAT-002", format!("expected {:?}, got {:?}", "E-CAT-002", refused_code(&fault)));

    });
    p.case("category_capsules_bind_public_kernels_and_guards", |p| {
// Both capsule FeatureIDs resolve to the expected public kernel ABI.
// Kernel-backed cells take the native path, so a NaN parameter
// refuses through the kernel's own finiteness pass (`E-CAT-001`).

    install_language();
    let check = native_kernel(CATEGORY_CHECK).expect("category check kernel bound");
    let commutative =
        native_kernel(CATEGORY_COMMUTATIVE).expect("category commutativity kernel bound");
    p.demand("category_capsules_bind_public_kernels_and_guards#1", check.kernel_id == "finite-category-certification", format!("expected {:?}, got {:?}", "finite-category-certification", check.kernel_id));
    p.demand("category_capsules_bind_public_kernels_and_guards#2", commutative.kernel_id == "diagram-commutativity-mask", format!("expected {:?}, got {:?}", "diagram-commutativity-mask", commutative.kernel_id));
    p.eq("category_capsules_bind_public_kernels_and_guards#3", check.arity_contract(), KernelArity::Exact(3));
    p.eq("category_capsules_bind_public_kernels_and_guards#4", commutative.arity_contract(), KernelArity::Exact(4));

    let (dom, cod, comp) = z3_category();
    let faces = Value::Vector(vec![0.0, 0.0, 2.0, 2.0, 1.0, 2.0, 2.0, 1.0]);
    let cell_mask = eval(
        vec![cell(
            CATEGORY_COMMUTATIVE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2), EmirValue(3)],
        )],
        &[dom.clone(), cod.clone(), comp.clone(), faces.clone()],
    )
    .expect("commutative cell computes");
    p.eq("category_capsules_bind_public_kernels_and_guards#5", vector_of(&cell_mask), vec![1.0]);

    let nan_dom = Value::Vector(vec![0.0, f64::NAN, 0.0]);
    let fault = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[nan_dom, cod, comp],
    )
    .expect_err("the kernel's finiteness pass keeps NaN out of the certification");
    p.demand("category_capsules_bind_public_kernels_and_guards#6", refused_code(&fault) == "E-CAT-001", format!("expected {:?}, got {:?}", "E-CAT-001", refused_code(&fault)));

    });
    p.case("category_capsule_check_returns_certification", |p| {
// The category-check capsule returns TRUE on a certified carrier.

    let (dom, cod, comp) = z3_category();
    let cell_check = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[dom.clone(), cod.clone(), comp.clone()],
    )
    .expect("check cell computes");
    p.demand("category_capsule_check_returns_certification#1", bool_of(&cell_check), "category_capsule_check_returns_certification#1: bool_of(&cell_check)");

    });
    p.case("category_carrier_shape_refusals", |p| {
// Carrier shape mismatches refuse at the installed kernel ABI.

    let (_, cod, comp) = z3_category();
    let error = eval(
        vec![cell(
            CATEGORY_CHECK,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[Value::F64(0.0), cod, comp],
    )
    .expect_err("a scalar dom is not a vector carrier");
    p.demand(format!("unexpected refusal: {error:?}"), matches!(error, EvalFault::CapabilityRefused { ref code, .. } if code.contains("E-TYPE-012")), format!("unexpected refusal: {error:?}"));

    let (dom, cod, _) = z3_category();
    let error = eval(
        vec![cell(
            CATEGORY_COMMUTATIVE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2), EmirValue(3)],
        )],
        &[
            dom,
            cod,
            Value::Vector(vec![0.0]),
            Value::Vector(vec![0.0, 0.0, 1.0, 1.0, 0.0, 0.0]),
        ],
    )
    .expect_err("a vector comp is not a matrix carrier");
    p.demand(format!("unexpected refusal: {error:?}"), matches!(error, EvalFault::CapabilityRefused { ref code, .. } if code.contains("E-TYPE-012")), format!("unexpected refusal: {error:?}"));

    });
    p.case("category_language_image_exposes_capsules", |p| {
// The installed Language Image exposes both category capsules.

    install_language();
    for feature_id in [CATEGORY_CHECK, CATEGORY_COMMUTATIVE] {
        p.demand(format!("missing category capsule {feature_id}"), native_kernel(feature_id).is_some(), format!("missing category capsule {feature_id}"));
    }

    });
    p.finish();
}























