//! Negative/positive witnesses for the domain, numeric, shape and
//! binder-logic fixes (wrong answers with no panic).

use emath_core::QualifiedName;
use emath_ir::{
    BinderKind, BinderVariable, BranchConvention, Domain, ExprId, ExprNode, Interval, NumericType,
    Shape, branch_point, promote,
};
use emath_test_harness::Probe;

#[test]
fn intent() {
    let mut p = Probe::new("Negative/positive witnesses for the domain, numeric, shape and");
    p.case("empty_box_contains_nothing_and_is_not_field", |p| {

    let boxed = Domain::Box(vec![]);
    p.demand("empty_box_contains_nothing_and_is_not_field#1", !boxed.contains(0.0), "empty_box_contains_nothing_and_is_not_field#1: !boxed.contains(0.0)");
    p.demand("empty_box_contains_nothing_and_is_not_field#2", !boxed.contains(-3.5), "empty_box_contains_nothing_and_is_not_field#2: !boxed.contains(-3.5)");
    p.ne("empty_box_contains_nothing_and_is_not_field#3", boxed.canonical(), Domain::Field.canonical());
    // No deterministic branch point for an empty box.
    p.eq("empty_box_contains_nothing_and_is_not_field#4", branch_point(&boxed, BranchConvention::Lower), None);
    p.eq("empty_box_contains_nothing_and_is_not_field#5", branch_point(&boxed, BranchConvention::Center), None);

    });
    p.case("box_bounds_span_all_axes", |p| {

    // A scalar must satisfy every axis; disjoint axes mean no scalar is
    // inside, and the reported bounds must agree with that instead of
    // reporting only the first axis.
    let boxed = Domain::Box(vec![
        Interval::closed(0.0, 1.0),
        Interval::closed(10.0, 20.0),
    ]);
    p.demand("box_bounds_span_all_axes#1", !boxed.contains(0.5), "box_bounds_span_all_axes#1: !boxed.contains(0.5)");
    p.eq("box_bounds_span_all_axes#2", boxed.lower_bound(), 10.0);
    p.eq("box_bounds_span_all_axes#3", boxed.upper_bound(), 1.0);
    // Overlapping axes stay honest.
    let overlapping = Domain::Box(vec![Interval::closed(0.0, 5.0), Interval::closed(1.0, 3.0)]);
    p.demand("box_bounds_span_all_axes#4", overlapping.contains(2.0), "box_bounds_span_all_axes#4: overlapping.contains(2.0)");
    p.eq("box_bounds_span_all_axes#5", overlapping.lower_bound(), 1.0);
    p.eq("box_bounds_span_all_axes#6", overlapping.upper_bound(), 3.0);

    });
    p.case("union_bounds_cover_every_member", |p| {

    let union = Domain::Union(vec![
        Domain::Interval(Interval::closed(0.0, 1.0)),
        Domain::Interval(Interval::closed(10.0, 11.0)),
    ]);
    p.eq("union_bounds_cover_every_member#1", union.lower_bound(), 0.0);
    p.eq("union_bounds_cover_every_member#2", union.upper_bound(), 11.0);
    for member in [0.0, 0.5, 1.0, 10.0, 11.0] {
        p.demand("union_bounds_cover_every_member#3", union.contains(member), "union_bounds_cover_every_member#3: union.contains(member)");
        p.demand("union_bounds_cover_every_member#4", member >= union.lower_bound() && member <= union.upper_bound(), "union_bounds_cover_every_member#4: member >= union.lower_bound() && member <= union.upper_bound()");
    }
    p.demand("union_bounds_cover_every_member#5", !union.contains(5.5), "union_bounds_cover_every_member#5: !union.contains(5.5)");

    });
    p.case("empty_set_boundary_is_defined", |p| {

    let empty = Domain::finite_set(vec![]);
    p.demand("empty_set_boundary_is_defined#1", !empty.contains(0.0), "empty_set_boundary_is_defined#1: !empty.contains(0.0)");
    p.demand("empty_set_boundary_is_defined#2", empty.lower_bound().is_nan(), "empty_set_boundary_is_defined#2: empty.lower_bound().is_nan()");
    p.demand("empty_set_boundary_is_defined#3", empty.upper_bound().is_nan(), "empty_set_boundary_is_defined#3: empty.upper_bound().is_nan()");
    p.eq("empty_set_boundary_is_defined#4", branch_point(&empty, BranchConvention::Lower), None);
    p.eq("empty_set_boundary_is_defined#5", branch_point(&empty, BranchConvention::Center), None);

    });
    p.case("field_does_not_contain_nan", |p| {

    p.demand("field_does_not_contain_nan#1", !Domain::Field.contains(f64::NAN), "field_does_not_contain_nan#1: !Domain::Field.contains(f64::NAN)");
    p.demand("field_does_not_contain_nan#2", Domain::Field.contains(0.0), "field_does_not_contain_nan#2: Domain::Field.contains(0.0)");
    p.demand("field_does_not_contain_nan#3", Domain::Field.contains(f64::INFINITY), "field_does_not_contain_nan#3: Domain::Field.contains(f64::INFINITY)");

    });
    p.case("finite_set_drops_nan_and_dedups_infinities", |p| {

    let set = Domain::finite_set(vec![f64::NAN, f64::INFINITY, 2.0, f64::INFINITY, 2.0]);
    p.demand("finite_set_drops_nan_and_dedups_infinities#1", set.contains(f64::INFINITY), "finite_set_drops_nan_and_dedups_infinities#1: set.contains(f64::INFINITY)");
    p.demand("finite_set_drops_nan_and_dedups_infinities#2", set.contains(2.0), "finite_set_drops_nan_and_dedups_infinities#2: set.contains(2.0)");
    p.demand("finite_set_drops_nan_and_dedups_infinities#3", !set.contains(f64::NAN), "finite_set_drops_nan_and_dedups_infinities#3: !set.contains(f64::NAN)");
    let canonical = set.canonical();
    p.eq(format!("{canonical}"), canonical.matches("inf").count(), 1);
    p.demand(format!("{canonical}"), !canonical.contains("NaN"), format!("{canonical}"));

    });
    p.case("mixed_sign_promote_refuses_at_any_equal_width", |p| {

    let u32 = NumericType::integer(false, 32);
    let i32 = NumericType::integer(true, 32);
    let error = promote(u32, i32).expect_err("u32+i32 must refuse");
    p.demand("mixed_sign_promote_refuses_at_any_equal_width#1", error.code == "E-TYPE-311", format!("expected {:?}, got {:?}", "E-TYPE-311", error.code));

    let u8 = NumericType::integer(false, 8);
    let i8 = NumericType::integer(true, 8);
    p.demand("mixed_sign_promote_refuses_at_any_equal_width#2", promote(u8, i8).expect_err("u8+i8 must refuse").code == "E-TYPE-311", format!("expected {:?}, got {:?}", "E-TYPE-311", promote(u8, i8).expect_err("u8+i8 must refuse").code));

    });
    p.case("mixed_sign_promote_widens_to_lossless_side", |p| {

    // u32+i64 -> i64 (covers every u32 value).
    let widened = promote(
        NumericType::integer(false, 32),
        NumericType::integer(true, 64),
    )
    .expect("lossless widening must promote");
    p.demand("mixed_sign_promote_widens_to_lossless_side#1", widened.signed, "mixed_sign_promote_widens_to_lossless_side#1: widened.signed");
    p.eq("mixed_sign_promote_widens_to_lossless_side#2", widened.bits, 64);
    // u64+i32 -> u64 (covers every i32 value).
    let widened = promote(
        NumericType::integer(false, 64),
        NumericType::integer(true, 32),
    )
    .expect("lossless widening must promote");
    p.demand("mixed_sign_promote_widens_to_lossless_side#3", !widened.signed, "mixed_sign_promote_widens_to_lossless_side#3: !widened.signed");
    p.eq("mixed_sign_promote_widens_to_lossless_side#4", widened.bits, 64);

    });
    p.case("rank_zero_never_broadcasts", |p| {

    let scalar = Shape::scalar();
    let vector = Shape::vector("n");
    p.demand("rank-0 must not broadcast to rank-1", !scalar.broadcastable_with(&vector), "rank-0 must not broadcast to rank-1");
    p.demand("rank_zero_never_broadcasts#2", !vector.broadcastable_with(&scalar), "rank_zero_never_broadcasts#2: !vector.broadcastable_with(&scalar)");
    // Scalar-scalar is identity, not a broadcast.
    p.demand("rank_zero_never_broadcasts#3", scalar.broadcastable_with(&scalar), "rank_zero_never_broadcasts#3: scalar.broadcastable_with(&scalar)");

    });
    p.case("binder_bound_names_are_not_free", |p| {

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
    p.eq("binder_bound_names_are_not_free#1", free, vec![QualifiedName("n".into())]);

    });
    p.finish();
}



















