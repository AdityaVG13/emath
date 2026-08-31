//! emath-epic-machine-fjxh.1: CapabilityId terms in stable IR.
//!
//! Domain operations are applications of stable ids over a package-side
//! cell arena (`SemanticPackage::capabilities`); the core
//! `ExprNode`/`UnaryOp`/`BinaryOp` enums never grow a domain-named variant.
//! The legacy sin/exp vocabulary keeps its core spelling as the compat path
//! until the migration cohort moves it onto cells.

use emath_core::{QualifiedName, Span};
use emath_ir::constructor::{Field, Visibility};
use emath_ir::goal::CompileSpec;
use emath_ir::meaning::MeaningError;
use emath_ir::canonical::canonical_expr;
use emath_ir::{
    Capability, CapabilityId, CellClass, DeclarationId, ExprId, ExprNode, Literal,
    SemanticPackage, TypeNode, UnaryOp, canonical_capability,
};

/// Acceptance negative seed (bead emath-epic-machine-fjxh.1).
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

/// One `capability` declaration carrying `definitions` (the only surface
/// `meaning_id` walks in this fixture).
fn push_capability_declaration(
    package: &mut SemanticPackage,
    name: &str,
    float_ty: emath_ir::TypeId,
    definitions: Vec<(String, ExprId)>,
) {
    let id = DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(emath_ir::Declaration {
        id,
        name: QualifiedName::single(name),
        kind: QualifiedName::single("capability"),
        kind_label: "capability".into(),
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
fn capability_terms_carry_cells_without_core_enum_growth() {
    let mut package = SemanticPackage::new();
    let softmax = package.push_capability(cell("std.math.softmax"));
    let x = var(&mut package, "x");
    let applied = apply_term(&mut package, softmax, vec![x]);

    // Stable term shape: the payload is a cell id, not a domain-named
    // variant. Adding `Softmax` above added zero enum variants; it appended
    // arena data.
    assert!(
        matches!(
            package.expr(applied),
            Some(ExprNode::Apply {
                capability,
                arguments,
            }) if *capability == softmax && arguments == &[x]
        ),
        "Apply term payload must be the stable cell id with its arguments"
    );
    assert_eq!(
        package.capability(softmax).map(|c| c.name.0.as_str()),
        Some("std.math.softmax")
    );
    assert_eq!(canonical_capability(&cell("std.math.softmax")), "cap:std.math.softmax");
}

#[test]
fn capability_identity_is_name_based_not_slot_based() {
    // Package A: the cell is the first interned capability.
    let mut left = SemanticPackage::new();
    let left_cell = left.push_capability(cell("std.math.softmax"));
    let left_x = var(&mut left, "x");
    let left_apply = apply_term(&mut left, left_cell, vec![left_x]);

    // Package right: an unrelated cell is interned first, so the softmax
    // cell lands at a different arena slot. Same cell name, same term
    // structure: term identity must not move. (fjxh.2 made the interned
    // cell set part of package MeaningID, so meaning equality is asserted
    // only between packages admitting the same cell set below.)
    let mut right = SemanticPackage::new();
    let _unrelated = right.push_capability(cell("pack.other.op"));
    let right_cell = right.push_capability(cell("std.math.softmax"));
    let right_x = var(&mut right, "x");
    let right_apply = apply_term(&mut right, right_cell, vec![right_x]);

    assert_ne!(left_cell, right_cell, "arena slots differ by construction");
    assert_eq!(
        canonical_expr(&left, left_apply),
        canonical_expr(&right, right_apply),
        "cell name, not arena slot, carries identity"
    );

    // A different cell name is different admitted math.
    let mut other = SemanticPackage::new();
    let other_cell = other.push_capability(cell("std.math.sigmoid"));
    let other_x = var(&mut other, "x");
    let other_apply = apply_term(&mut other, other_cell, vec![other_x]);
    assert_ne!(
        canonical_expr(&left, left_apply),
        canonical_expr(&other, other_apply)
    );

    // And meaning identity follows the same rule: same names, same meaning;
    // different names, different meaning. fjxh.2 added the interned cell
    // set to the meaning preimage, so both packages must admit the same
    // set to share a MeaningID; slot order still does not matter because
    // cells enter the bytes by name in intern order.
    let mut right_meaning_package = SemanticPackage::new();
    let rmp_cell = right_meaning_package.push_capability(cell("std.math.softmax"));
    let rmp_x = var(&mut right_meaning_package, "x");
    let rmp_apply = apply_term(&mut right_meaning_package, rmp_cell, vec![rmp_x]);
    let left_ty = float_type(&mut left);
    push_capability_declaration(
        &mut left,
        "Softmax",
        left_ty,
        vec![("value".into(), left_apply)],
    );
    let rmp_ty = float_type(&mut right_meaning_package);
    push_capability_declaration(
        &mut right_meaning_package,
        "Softmax",
        rmp_ty,
        vec![("value".into(), rmp_apply)],
    );
    let left_meaning = left.meaning_id(&[]).expect("well-formed capability term");
    let right_meaning = right_meaning_package
        .meaning_id(&[])
        .expect("well-formed capability term");
    assert_eq!(left_meaning, right_meaning);

    let mut renamed = SemanticPackage::new();
    let renamed_cell = renamed.push_capability(cell("std.math.sigmoid"));
    let renamed_x = var(&mut renamed, "x");
    let renamed_apply = apply_term(&mut renamed, renamed_cell, vec![renamed_x]);
    let renamed_ty = float_type(&mut renamed);
    push_capability_declaration(
        &mut renamed,
        "Softmax",
        renamed_ty,
        vec![("value".into(), renamed_apply)],
    );
    let renamed_meaning = renamed.meaning_id(&[]).expect("well-formed capability term");
    assert_ne!(left_meaning, renamed_meaning);
}

#[test]
fn legacy_core_vocabulary_still_runs_unmoved() {
    // Compat path: sin/exp keep their core op spelling; the legacy terms
    // canonicalize and receive meaning exactly as before this bead.
    assert_eq!(UnaryOp::Sin.name(), "sin");
    assert_eq!(UnaryOp::Exp.name(), "exp");
    assert_eq!(emath_ir::BinaryOp::StrictFloatAdd.name(), "f64-add");

    let mut package = SemanticPackage::new();
    let x = var(&mut package, "x");
    let sin = package.push_expr(
        ExprNode::Unary {
            operation: UnaryOp::Sin,
            value: x,
        },
        Span::default(),
    );
    let half = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(0.5_f64.to_bits())),
        Span::default(),
    );
    let sum = package.push_expr(
        ExprNode::Binary {
            operation: emath_ir::BinaryOp::StrictFloatAdd,
            left: sin,
            right: half,
        },
        Span::default(),
    );
    let ty = float_type(&mut package);
    push_capability_declaration(&mut package, "Legacy", ty, vec![("value".into(), sum)]);
    assert!(package.meaning_id(&[]).is_ok(), "legacy sin/exp path still computes");

    // Term kinds stay discriminated: a legacy unary term and a capability
    // application never share canonical bytes.
    let mut with_cell = SemanticPackage::new();
    let cell_id = with_cell.push_capability(cell("std.math.sin"));
    let cell_x = var(&mut with_cell, "x");
    let applied = apply_term(&mut with_cell, cell_id, vec![cell_x]);
    assert_ne!(
        canonical_expr(&package, sin),
        canonical_expr(&with_cell, applied)
    );
}

#[test]
fn dangling_capability_application_is_a_typed_refusal_not_silent_success() {
    // The negative seed names the required diagnostic.
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|line| line.trim_start().starts_with("# expect:"))
        .expect("negative seed must name its required diagnostic");
    assert!(
        expect_line.contains("MeaningError::MissingCapability"),
        "seed expects the typed IR refusal, found: {expect_line}"
    );

    // The seeded hazard at the IR seam: a capability application whose cell
    // id was never admitted (the `Undeclared(x)` lowering). Silent success
    // would hash a MeaningID over the dangling term.
    let mut package = SemanticPackage::new();
    let x = var(&mut package, "x");
    let dangling = apply_term(&mut package, CapabilityId(u32::MAX), vec![x]);
    let ty = float_type(&mut package);
    push_capability_declaration(
        &mut package,
        "Softmax",
        ty,
        vec![("value".into(), dangling)],
    );

    assert_eq!(
        package.meaning_id(&[]),
        Err(MeaningError::MissingCapability(CapabilityId(u32::MAX))),
        "dangling capability application must be a typed refusal"
    );

    // Canonical bytes stay deterministic in the refused state.
    assert_eq!(
        canonical_expr(&package, dangling),
        canonical_expr(&package, dangling)
    );
}
