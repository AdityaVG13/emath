//! CapabilityId terms in stable IR.
//!
//! Domain operations are applications of stable ids over a package-side
//! cell arena (`SemanticPackage::capabilities`); the core
//! `ExprNode`/`UnaryOp`/`BinaryOp` enums never grow a domain-named variant.

use emath_core::{QualifiedName, Span};
use emath_ir::canonical::canonical_expr;
use emath_ir::constructor::{Field, Visibility};
use emath_ir::goal::CompileSpec;
use emath_ir::meaning::MeaningError;
use emath_ir::{
    Capability, CapabilityId, CellClass, DeclarationId, ExprId, ExprNode, SemanticPackage,
    TypeNode, canonical_capability,
};
use emath_test_harness::Probe;

/// Acceptance negative seed.
const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/capability_id_terms.emath");

fn cell(name: &str) -> Capability {
    Capability {
        name: QualifiedName::single(name),
        class: CellClass::Pure,
    }
}

fn var(package: &mut SemanticPackage, name: &str) -> ExprId {
    package.push_expr(
        ExprNode::Variable(QualifiedName::single(name)),
        Span::default(),
    )
}

fn apply_term(package: &mut SemanticPackage, id: CapabilityId, arguments: Vec<ExprId>) -> ExprId {
    package.push_expr(
        ExprNode::Apply {
            capability: id,
            arguments,
        },
        Span::default(),
    )
}

fn float_type(package: &mut SemanticPackage) -> emath_ir::TypeId {
    package.push_type(TypeNode::Float64)
}

/// One `function` declaration carrying `definitions` (the only surface
/// `meaning_id` walks in this fixture).
fn push_function_declaration(
    package: &mut SemanticPackage,
    name: &str,
    float_ty: emath_ir::TypeId,
    definitions: Vec<(String, ExprId)>,
) {
    let id = DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(emath_ir::Declaration {
        id,
        name: QualifiedName::single(name),
        kind: QualifiedName::single("function"),
        kind_label: "function".into(),
        inputs: vec![Field {
            name: "x".into(),
            ty: float_ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: vec![Field {
            name: "value".into(),
            ty: float_ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions: definitions.into_iter().collect(),
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
}

#[test]
fn intent() {
    let mut p = Probe::new("CapabilityId terms in stable IR.");
    p.case("capability_terms_carry_cells_without_core_enum_growth", |p| {

    let mut package = SemanticPackage::new();
    let scalar = package.push_capability(cell("std.capability.scalar"));
    let x = var(&mut package, "x");
    let applied = apply_term(&mut package, scalar, vec![x]);

    // Stable term shape: the payload is a cell id, not a domain-named
    // variant. Adding a cell above added zero enum variants; it appended
    // arena data.
    p.demand("Apply term payload must be the stable cell id with its arguments", matches!(
            package.expr(applied),
            Some(ExprNode::Apply {
                capability,
                arguments,
            }) if *capability == scalar && arguments == &[x]
        ), "Apply term payload must be the stable cell id with its arguments");
    p.eq("capability_terms_carry_cells_without_core_enum_growth#2", package.capability(scalar).map(|c| c.name.0.as_str()), Some("std.capability.scalar"));
    p.demand("capability_terms_carry_cells_without_core_enum_growth#3", canonical_capability(&cell("std.capability.scalar")) == "cap:std.capability.scalar", format!("expected {:?}, got {:?}", "cap:std.capability.scalar", canonical_capability(&cell("std.capability.scalar"))));

    });
    p.case("capability_identity_is_name_based_not_slot_based", |p| {

    // Package A: the cell is the first interned capability.
    let mut left = SemanticPackage::new();
    let left_cell = left.push_capability(cell("std.capability.scalar"));
    let left_x = var(&mut left, "x");
    let left_apply = apply_term(&mut left, left_cell, vec![left_x]);

    // Package right: an unrelated cell is interned first, so the scalar
    // cell lands at a different arena slot. Same cell name, same term
    // structure: term identity must not move.
    let mut right = SemanticPackage::new();
    let _unrelated = right.push_capability(cell("std.kind.object"));
    let right_cell = right.push_capability(cell("std.capability.scalar"));
    let right_x = var(&mut right, "x");
    let right_apply = apply_term(&mut right, right_cell, vec![right_x]);

    p.ne("arena slots differ by construction", left_cell, right_cell);
    p.eq("cell name, not arena slot, carries identity", canonical_expr(&left, left_apply), canonical_expr(&right, right_apply));

    // A different constructor cell name is different admitted math.
    let mut other = SemanticPackage::new();
    let other_cell = other.push_capability(cell("std.kind.function"));
    let other_x = var(&mut other, "x");
    let other_apply = apply_term(&mut other, other_cell, vec![other_x]);
    p.ne("capability_identity_is_name_based_not_slot_based#3", canonical_expr(&left, left_apply), canonical_expr(&other, other_apply));

    let mut right_meaning_package = SemanticPackage::new();
    let rmp_cell = right_meaning_package.push_capability(cell("std.capability.scalar"));
    let rmp_x = var(&mut right_meaning_package, "x");
    let rmp_apply = apply_term(&mut right_meaning_package, rmp_cell, vec![rmp_x]);
    let left_ty = float_type(&mut left);
    push_function_declaration(
        &mut left,
        "Scalar",
        left_ty,
        vec![("value".into(), left_apply)],
    );
    let rmp_ty = float_type(&mut right_meaning_package);
    push_function_declaration(
        &mut right_meaning_package,
        "Scalar",
        rmp_ty,
        vec![("value".into(), rmp_apply)],
    );
    let left_meaning = left.meaning_id(&[]).expect("well-formed capability term");
    let right_meaning = right_meaning_package
        .meaning_id(&[])
        .expect("well-formed capability term");
    p.eq("capability_identity_is_name_based_not_slot_based#4", left_meaning.clone(), right_meaning);

    let mut renamed = SemanticPackage::new();
    let renamed_cell = renamed.push_capability(cell("std.kind.function"));
    let renamed_x = var(&mut renamed, "x");
    let renamed_apply = apply_term(&mut renamed, renamed_cell, vec![renamed_x]);
    let renamed_ty = float_type(&mut renamed);
    push_function_declaration(
        &mut renamed,
        "Scalar",
        renamed_ty,
        vec![("value".into(), renamed_apply)],
    );
    let renamed_meaning = renamed
        .meaning_id(&[])
        .expect("well-formed capability term");
    p.ne("capability_identity_is_name_based_not_slot_based#5", left_meaning, renamed_meaning);

    });
    p.case("dangling_capability_application_is_a_typed_refusal_not_silent_success", |p| {

    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|line| line.contains("MeaningError::MissingCapability"))
        .expect("negative seed must name MeaningError::MissingCapability");
    p.demand(
        format!("seed expects the typed IR refusal, found: {expect_line}"),
        expect_line.contains("MeaningError::MissingCapability"),
        format!("seed expects the typed IR refusal, found: {expect_line}"),
    );

    let mut package = SemanticPackage::new();
    let x = var(&mut package, "x");
    let dangling = apply_term(&mut package, CapabilityId(u32::MAX), vec![x]);
    let ty = float_type(&mut package);
    push_function_declaration(
        &mut package,
        "Scalar",
        ty,
        vec![("value".into(), dangling)],
    );

    p.eq("dangling capability application must be a typed refusal", package.meaning_id(&[]), Err(MeaningError::MissingCapability(CapabilityId(u32::MAX))));

    });
    p.finish();
}
