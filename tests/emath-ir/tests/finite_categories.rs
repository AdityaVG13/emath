//! emath-r3-abstract-algebra-88wo (thin B39 slice): finite-category
//! diagram commutativity — the orch-decided masa-style kernel.
//!
//! The bead's law, sliced to the numeric-kernel + EMIR seam (B38
//! forms/manifolds and B45 algebraic geometry stay deferred — world and
//! parser lanes; functor/natural-transformation surfaces are NOT
//! claimed):
//! - **Carrier law (orch decision: dense composition table)**: a
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
//! - **Face path-pairs (orch decision)**: a face is a flat record
//!   `[start, end, len_l, len_r, left…, right…]` (both paths ≥ 1
//!   morphism — identities are explicit carrier morphisms, empty paths
//!   refuse `E-CAT-002`). A face is commutative iff the two path
//!   composites are the SAME morphism index. The result is the
//!   per-face mask in face order (the Pareto-mask convention).
//! - Determinism class: fixed-order law passes, first-failure refusal,
//!   index-fold path evaluation; identical inputs are bit-identical.

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::{
    compile_reference, std_cell_registry, ParamShape, TermCompileError,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{Signature, SymbolId, Term, VariableId};

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    // The .14 seam law: LoadInput per input, result = last register.
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

/// Registry-path evaluation of a fixed-shape cell.
fn cell_seval(
    name: &str,
    operator: &str,
    arity: usize,
    params: Vec<(String, ParamShape)>,
    inputs: &[Value],
) -> Result<Value, EvalFault> {
    let term = Term::Apply {
        operator: SymbolId(operator.into()),
        arguments: params
            .iter()
            .map(|(name, _)| Term::Variable(VariableId(name.clone())))
            .collect(),
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId(operator.into()), arity)
        .expect("single-operator signature is conflict-free");
    let _cell = compile_reference(&term, &signature, &params, Vec::new(), name)
        .expect("category cell compiles through the call surface");
    let count = inputs.len();
    let mut ops: Vec<(EmirOp, Span)> = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    ops.push((
        EmirOp::ApplyCapability {
            capability: name.to_string(),
            class: CellClass::Pure,
            args: (0..count as u32).map(EmirValue).collect(),
        },
        Span::default(),
    ));
    let program = EmirProgram {
        ops,
        result: EmirValue(count as u32),
        input_count: count as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

/// Z/3 certifies as a category (the gate returns TRUE — the value is
/// the certification; every failure is a typed refusal, never false).
#[test]
fn category_check_certifies_z3() {
    let (dom, cod, comp) = z3_category();
    let checked = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[dom, cod, comp],
    )
    .expect("Z/3 is a category");
    assert!(bool_of(&checked));
}

/// Composition-law refusals (E-CAT-004): an aligned pair with a
/// missing entry, and a defined entry on a misaligned pair.
#[test]
fn category_entry_law_refusals() {
    let (dom, cod, comp) = z3_category();
    let Value::Matrix { mut data, rows, cols } = comp else {
        panic!("z3 comp is a matrix")
    };
    let _ = (rows, cols);
    data[1 * 3 + 2] = -1.0; // comp[1][2]: aligned (single object) but missing
    let fault = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[dom.clone(), cod.clone(), matrix(3, 3, &data)],
    )
    .expect_err("aligned pair without a composite");
    assert_eq!(refused_code(&fault), "E-CAT-004");

    let (dom, cod, comp) = free_arrow_category();
    let Value::Matrix { mut data, .. } = comp else {
        panic!("free-arrow comp is a matrix")
    };
    data[0 * 3 + 2] = 0.0; // comp[id0][f]: misaligned (cod[f]=1 ≠ dom[id0]=0)
    let fault = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[dom, cod, matrix(3, 3, &data)],
    )
    .expect_err("defined entry on a misaligned pair");
    assert_eq!(refused_code(&fault), "E-CAT-004");
}

/// Identity-law refusal (E-CAT-005): break the identity action
/// (comp[0][1] = 2) and NO morphism of Z/3 acts as the identity —
/// the object has no identity morphism.
#[test]
fn category_identity_law_refusal() {
    let (dom, cod, comp) = z3_category();
    let Value::Matrix { mut data, .. } = comp else {
        panic!("z3 comp is a matrix")
    };
    data[0 * 3 + 1] = 2.0; // id no longer acts on m1; no other candidate works
    let fault = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[dom, cod, matrix(3, 3, &data)],
    )
    .expect_err("object 0 lost its identity morphism");
    assert_eq!(refused_code(&fault), "E-CAT-005");
}

/// Associativity-law refusal (E-CAT-006): comp[1][1] = 0 keeps every
/// entry/identity law intact but breaks (1∘1)∘2 = 2 ≠ 1 = 1∘(1∘2).
#[test]
fn category_associativity_law_refusal() {
    let (dom, cod, comp) = z3_category();
    let Value::Matrix { mut data, .. } = comp else {
        panic!("z3 comp is a matrix")
    };
    data[1 * 3 + 1] = 0.0;
    let fault = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[dom, cod, matrix(3, 3, &data)],
    )
    .expect_err("(1∘1)∘2 ≠ 1∘(1∘2) after the mutation");
    assert_eq!(refused_code(&fault), "E-CAT-006");
}

/// Honesty fence (E-CAT-007): a 65-morphism one-object carrier is a
/// category but TOO LARGE to certify associativity by the k ≤ 64
/// bound — refused, never commute-checked over an unverified table.
#[test]
fn oversized_category_refuses() {
    let k = 65usize;
    let comp: Vec<f64> = (0..k)
        .flat_map(|i| (0..k).map(move |j| ((i + j) % k) as f64))
        .collect();
    let dom = Value::Vector(vec![0.0; k]);
    let cod = Value::Vector(vec![0.0; k]);
    let fault = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[dom, cod, matrix(k, k, &comp)],
    )
    .expect_err("k = 65 exceeds the certifiable associativity bound");
    assert_eq!(refused_code(&fault), "E-CAT-007");
}

/// Index/shape/finiteness refusals: NaN entry (E-CAT-001), non-square
/// table and mismatched lengths and malformed face records (E-CAT-002),
/// out-of-range / non-integral indices (E-CAT-003).
#[test]
fn category_index_shape_and_finiteness_refusals() {
    let (dom, cod, comp) = z3_category();
    let Value::Matrix { mut data, .. } = comp else {
        panic!("z3 comp is a matrix")
    };
    data[2 * 3 + 1] = f64::NAN;
    let fault = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[dom.clone(), cod.clone(), matrix(3, 3, &data)],
    )
    .expect_err("NaN composition entry");
    assert_eq!(refused_code(&fault), "E-CAT-001");
    // Restore the clean table: the finiteness pass precedes the index
    // pass, so the later cases must not carry the NaN.
    data[2 * 3 + 1] = 0.0;

    let fault = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[dom.clone(), cod.clone(), matrix(3, 2, &data[..6])],
    )
    .expect_err("non-square composition table");
    assert_eq!(refused_code(&fault), "E-CAT-002");

    let short_cod = Value::Vector(vec![0.0, 0.0]);
    let fault = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[dom.clone(), short_cod, matrix(3, 3, &data)],
    )
    .expect_err("dom and cod lengths differ");
    assert_eq!(refused_code(&fault), "E-CAT-002");

    // Out-of-range E-CAT-003 manifests in comp TABLE entries (with
    // implicit object indexing, a dom/cod value only widens the object
    // set — it cannot be out of range): an entry naming morphism 7 in
    // a 3-morphism table refuses at parse.
    let fault = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[
            dom.clone(),
            cod.clone(),
            matrix(3, 3, &[7.0, 1.0, 2.0, 1.0, 2.0, 0.0, 2.0, 0.0, 1.0]),
        ],
    )
    .expect_err("composition entry 7 is not a morphism index");
    assert_eq!(refused_code(&fault), "E-CAT-003");

    let fractional = Value::Vector(vec![0.0, 0.5, 0.0]);
    let fault = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[fractional, cod.clone(), matrix(3, 3, &data)],
    )
    .expect_err("a morphism index must be a whole number");
    assert_eq!(refused_code(&fault), "E-CAT-003");

    let (dom, cod, comp) = z3_category();
    let faces = Value::Vector(vec![0.0, 0.0, 5.0, 1.0]);
    let fault = eval(
        vec![EmirOp::CategoryDiagramCommutative(
            EmirValue(0),
            EmirValue(1),
            EmirValue(2),
            EmirValue(3),
        )],
        &[dom, cod, comp, faces],
    )
    .expect_err("face record overruns the stream");
    assert_eq!(refused_code(&fault), "E-CAT-002");
}

/// Commutativity on Z/3: the square face (1∘2 vs 2∘1, both = 0) is
/// commutative; the triangle face (1 vs 2) is NOT — the per-face mask
/// in face order.
#[test]
fn category_commutative_mask_computes() {
    let (dom, cod, comp) = z3_category();
    // Face 1: start 0, end 0, left [1, 2], right [2, 1].
    // Face 2: start 0, end 0, left [1], right [2].
    let faces = Value::Vector(vec![0.0, 0.0, 2.0, 2.0, 1.0, 2.0, 2.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0]);
    let mask = eval(
        vec![EmirOp::CategoryDiagramCommutative(
            EmirValue(0),
            EmirValue(1),
            EmirValue(2),
            EmirValue(3),
        )],
        &[dom, cod, comp, faces],
    )
    .expect("faces evaluate");
    assert_eq!(vector_of(&mask), vec![1.0, 0.0], "1∘2 = 2∘1 = 0 but 1 ≠ 2");
}

/// Path-geometry refusals through the commutative op: a dangling path
/// segment (comp[f][f] undefined in the free category) refuses
/// `E-CAT-004`; a path that does not run the face's declared
/// start→end refuses `E-CAT-002`.
#[test]
fn category_path_geometry_refusals() {
    let (dom, cod, comp) = free_arrow_category();
    // Face: start 0, end 1, left [f, f] (dangling: f ∘ f undefined),
    // right [f].
    let faces = Value::Vector(vec![0.0, 1.0, 2.0, 1.0, 2.0, 2.0, 2.0]);
    let fault = eval(
        vec![EmirOp::CategoryDiagramCommutative(
            EmirValue(0),
            EmirValue(1),
            EmirValue(2),
            EmirValue(3),
        )],
        &[dom.clone(), cod.clone(), comp.clone(), faces],
    )
    .expect_err("f ∘ f is not defined");
    assert_eq!(refused_code(&fault), "E-CAT-004");

    // Face: start 0, end 0 declared, but the only path [f] runs 0→1.
    let faces = Value::Vector(vec![0.0, 0.0, 1.0, 1.0, 2.0, 2.0]);
    let fault = eval(
        vec![EmirOp::CategoryDiagramCommutative(
            EmirValue(0),
            EmirValue(1),
            EmirValue(2),
            EmirValue(3),
        )],
        &[dom, cod, comp, faces],
    )
    .expect_err("path [f] does not end at the declared end object");
    assert_eq!(refused_code(&fault), "E-CAT-002");
}

/// Registry cells (the anti-LOC law): `std.category.check` and
/// `std.category.commutative` agree BIT-FOR-BIT with the bare ops, and
/// the all-finite guard refuses a NaN parameter one layer earlier
/// (`E-CELL-006`).
#[test]
fn category_cell_preserves_parity_and_guards() {
    let (dom, cod, comp) = z3_category();
    let faces = Value::Vector(vec![0.0, 0.0, 2.0, 2.0, 1.0, 2.0, 2.0, 1.0]);
    let cell_mask = cell_seval(
        "std.category.commutative",
        "diagram_commutative",
        4,
        vec![
            ("dom".to_string(), ParamShape::Vector),
            ("cod".to_string(), ParamShape::Vector),
            ("comp".to_string(), ParamShape::Matrix),
            ("faces".to_string(), ParamShape::Vector),
        ],
        &[dom.clone(), cod.clone(), comp.clone(), faces.clone()],
    )
    .expect("commutative cell computes");
    let bare_mask = eval(
        vec![EmirOp::CategoryDiagramCommutative(
            EmirValue(0),
            EmirValue(1),
            EmirValue(2),
            EmirValue(3),
        )],
        &[dom.clone(), cod.clone(), comp.clone(), faces.clone()],
    )
    .expect("bare commutative op computes");
    assert_eq!(vector_of(&cell_mask), vector_of(&bare_mask));

    let nan_dom = Value::Vector(vec![0.0, f64::NAN, 0.0]);
    let fault = cell_seval(
        "std.category.check",
        "category_check",
        3,
        vec![
            ("dom".to_string(), ParamShape::Vector),
            ("cod".to_string(), ParamShape::Vector),
            ("comp".to_string(), ParamShape::Matrix),
        ],
        &[nan_dom, cod, comp],
    )
    .expect_err("the all-finite guard keeps NaN out of the cell seam");
    assert_eq!(refused_code(&fault), "E-CELL-006");
}

/// Cell/bare parity, properly: the check cell returns TRUE on Z/3
/// exactly like the bare op.
#[test]
fn category_cell_check_preserves_parity() {
    let (dom, cod, comp) = z3_category();
    let cell_check = cell_seval(
        "std.category.check",
        "category_check",
        3,
        vec![
            ("dom".to_string(), ParamShape::Vector),
            ("cod".to_string(), ParamShape::Vector),
            ("comp".to_string(), ParamShape::Matrix),
        ],
        &[dom.clone(), cod.clone(), comp.clone()],
    )
    .expect("check cell computes");
    let bare_check = eval(
        vec![EmirOp::CategoryCheck(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[dom, cod, comp],
    )
    .expect("bare check op computes");
    assert_eq!(bool_of(&cell_check), bool_of(&bare_check));
}

/// Shape law at COMPILE: a scalar where a vector is needed (and a
/// vector where a matrix is needed) refuses through the closed
/// vocabulary's shape law.
#[test]
fn category_compile_shape_refusals() {
    let scalar_params = vec![
        ("dom".to_string(), ParamShape::Scalar),
        ("cod".to_string(), ParamShape::Vector),
        ("comp".to_string(), ParamShape::Matrix),
    ];
    let term = Term::Apply {
        operator: SymbolId("category_check".into()),
        arguments: vec![
            Term::Variable(VariableId("dom".into())),
            Term::Variable(VariableId("cod".into())),
            Term::Variable(VariableId("comp".into())),
        ],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("category_check".into()), 3)
        .expect("single-operator signature is conflict-free");
    let error = compile_reference(
        &term,
        &signature,
        &scalar_params,
        Vec::new(),
        "std.category.check",
    )
    .expect_err("a scalar dom is not a morphism carrier");
    assert!(
        matches!(error, TermCompileError::ShapeMismatch { .. }),
        "unexpected refusal: {error:?}"
    );
    let vector_params = vec![
        ("dom".to_string(), ParamShape::Vector),
        ("cod".to_string(), ParamShape::Vector),
        ("comp".to_string(), ParamShape::Vector),
        ("faces".to_string(), ParamShape::Vector),
    ];
    let term = Term::Apply {
        operator: SymbolId("diagram_commutative".into()),
        arguments: vec![
            Term::Variable(VariableId("dom".into())),
            Term::Variable(VariableId("cod".into())),
            Term::Variable(VariableId("comp".into())),
            Term::Variable(VariableId("faces".into())),
        ],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("diagram_commutative".into()), 4)
        .expect("single-operator signature is conflict-free");
    let error = compile_reference(
        &term,
        &signature,
        &vector_params,
        Vec::new(),
        "std.category.commutative",
    )
    .expect_err("a vector comp is not a composition-table carrier");
    assert!(
        matches!(error, TermCompileError::ShapeMismatch { .. }),
        "unexpected refusal: {error:?}"
    );
}

/// The registry exposes exactly the two category cells (the cohort
/// count law lives in the fjxh_14 suite; this pins the 88wo slice).
#[test]
fn category_registry_exposes_cells() {
    let registry = std_cell_registry();
    for name in ["std.category.check", "std.category.commutative"] {
        assert!(
            registry.contains_key(name),
            "missing category cell {name}: {:?}",
            registry.keys().collect::<Vec<_>>()
        );
    }
}
