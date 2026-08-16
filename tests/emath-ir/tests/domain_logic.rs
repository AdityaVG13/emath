//! Negative/positive witnesses for domain, numeric, shape and
//! binder-logic fixes (wrong answers with no panic).

use emath_core::QualifiedName;
use emath_ir::{
    branch_point, promote, BinderKind, BinderVariable, BranchConvention, Domain, ExprId, ExprNode,
    Interval, NumericType, Shape,
};

#[test]
fn empty_box_contains_nothing_and_is_not_field() {
    let boxed = Domain::Box(vec![]);
    assert!(!boxed.contains(0.0));
    assert!(!boxed.contains(-3.5));
    assert_ne!(boxed.canonical(), Domain::Field.canonical());
    // No deterministic branch point for an empty box.
    assert_eq!(branch_point(&boxed, BranchConvention::Lower), None);
    assert_eq!(branch_point(&boxed, BranchConvention::Center), None);
}

#[test]
fn box_bounds_span_all_axes() {
    // A scalar must satisfy every axis; disjoint axes mean no scalar is
    // inside, and the reported bounds must agree with that instead of
    // reporting only the first axis.
    let boxed = Domain::Box(vec![
        Interval::closed(0.0, 1.0),
        Interval::closed(10.0, 20.0),
    ]);
    assert!(!boxed.contains(0.5));
    assert_eq!(boxed.lower_bound(), 10.0);
    assert_eq!(boxed.upper_bound(), 1.0);
    // Overlapping axes stay honest.
    let overlapping = Domain::Box(vec![Interval::closed(0.0, 5.0), Interval::closed(1.0, 3.0)]);
    assert!(overlapping.contains(2.0));
    assert_eq!(overlapping.lower_bound(), 1.0);
    assert_eq!(overlapping.upper_bound(), 3.0);
}

#[test]
fn union_bounds_cover_every_member() {
    let union = Domain::Union(vec![
        Domain::Interval(Interval::closed(0.0, 1.0)),
        Domain::Interval(Interval::closed(10.0, 11.0)),
    ]);
    assert_eq!(union.lower_bound(), 0.0);
    assert_eq!(union.upper_bound(), 11.0);
    for member in [0.0, 0.5, 1.0, 10.0, 11.0] {
        assert!(union.contains(member));
        assert!(member >= union.lower_bound() && member <= union.upper_bound());
    }
    assert!(!union.contains(5.5));
}

#[test]
fn empty_set_boundary_is_defined() {
    let empty = Domain::finite_set(vec![]);
    assert!(!empty.contains(0.0));
    assert!(empty.lower_bound().is_nan());
    assert!(empty.upper_bound().is_nan());
    assert_eq!(branch_point(&empty, BranchConvention::Lower), None);
    assert_eq!(branch_point(&empty, BranchConvention::Center), None);
}

#[test]
fn field_does_not_contain_nan() {
    assert!(!Domain::Field.contains(f64::NAN));
    assert!(Domain::Field.contains(0.0));
    assert!(Domain::Field.contains(f64::INFINITY));
}

#[test]
fn finite_set_drops_nan_and_dedups_infinities() {
    let set = Domain::finite_set(vec![f64::NAN, f64::INFINITY, 2.0, f64::INFINITY, 2.0]);
    assert!(set.contains(f64::INFINITY));
    assert!(set.contains(2.0));
    assert!(!set.contains(f64::NAN));
    let canonical = set.canonical();
    assert_eq!(canonical.matches("inf").count(), 1, "{canonical}");
    assert!(!canonical.contains("NaN"), "{canonical}");
}

#[test]
fn mixed_sign_promote_refuses_at_any_equal_width() {
    let u32 = NumericType::integer(false, 32);
    let i32 = NumericType::integer(true, 32);
    let error = promote(u32, i32).expect_err("u32+i32 must refuse");
    assert_eq!(error.code, "E-TYPE-311");

    let u8 = NumericType::integer(false, 8);
    let i8 = NumericType::integer(true, 8);
    assert_eq!(
        promote(u8, i8).expect_err("u8+i8 must refuse").code,
        "E-TYPE-311"
    );
}

#[test]
fn mixed_sign_promote_widens_to_lossless_side() {
    // u32+i64 -> i64 (covers every u32 value).
    let widened = promote(
        NumericType::integer(false, 32),
        NumericType::integer(true, 64),
    )
    .expect("lossless widening must promote");
    assert!(widened.signed);
    assert_eq!(widened.bits, 64);
    // u64+i32 -> u64 (covers every i32 value).
    let widened = promote(
        NumericType::integer(false, 64),
        NumericType::integer(true, 32),
    )
    .expect("lossless widening must promote");
    assert!(!widened.signed);
    assert_eq!(widened.bits, 64);
}

#[test]
fn rank_zero_never_broadcasts() {
    let scalar = Shape::scalar();
    let vector = Shape::vector("n");
    assert!(
        !scalar.broadcastable_with(&vector),
        "rank-0 must not broadcast to rank-1"
    );
    assert!(!vector.broadcastable_with(&scalar));
    // Scalar-scalar is identity, not a broadcast.
    assert!(scalar.broadcastable_with(&scalar));
}

#[test]
fn binder_bound_names_are_not_free() {
    // `sum(i in 1..n, i)`: `i` is bound by the binder, `n` stays free.
    let exprs = vec![
        ExprNode::Variable(QualifiedName("i".into())), // body
        ExprNode::Variable(QualifiedName("n".into())), // domain
    ];
    let binder = ExprNode::Binder {
        kind: BinderKind::Sum,
        variables: vec![BinderVariable {
            name: "i".into(),
            domain: ExprId(1),
        }],
        body: ExprId(0),
    };
    let free = binder.free_variables(&exprs);
    assert_eq!(free, vec![QualifiedName("n".into())]);
}
