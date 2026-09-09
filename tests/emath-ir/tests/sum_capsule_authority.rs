use std::fs;
use emath_test_harness::Probe;

fn capsule_sum(
    domain: impl IntoIterator<Item = i64>,
    guard: impl Fn(i64) -> bool,
    body: impl Fn(i64) -> i64,
) -> Result<i64, &'static str> {
    let mut result = 0i64;
    for item in domain {
        if guard(item) {
            result = result.checked_add(body(item)).ok_or("exactness-loss")?;
        }
    }
    Ok(result)
}

fn legacy_sum(
    domain: impl IntoIterator<Item = i64>,
    guard: impl Fn(i64) -> bool,
    body: impl Fn(i64) -> i64,
) -> Result<i64, &'static str> {
    capsule_sum(domain, guard, body)
}

#[test]
fn intent() {
    let mut p = Probe::new("tests/emath-ir/tests/sum_capsule_authority.rs");
    p.case("finite_empty_guarded_and_scope_cases_match", |p| {

    p.eq("finite_empty_guarded_and_scope_cases_match#1", capsule_sum(0..5, |_| true, |i| i), legacy_sum(0..5, |_| true, |i| i));
    p.eq("finite_empty_guarded_and_scope_cases_match#2", capsule_sum([], |_| true, |i| i), Ok(0));
    p.eq("finite_empty_guarded_and_scope_cases_match#3", capsule_sum(0..6, |i| i % 2 == 0, |i| i * i), Ok(20));
    p.eq("finite_empty_guarded_and_scope_cases_match#4", capsule_sum([3, 1, 2], |_| true, |i| i), Ok(6));

    });
    p.case("migration_defects_refuse_or_discriminate", |p| {

    p.ne("migration_defects_refuse_or_discriminate#1", capsule_sum(0..5, |_| true, |i| i), capsule_sum(0..=5, |_| true, |i| i));
    p.eq("migration_defects_refuse_or_discriminate#2", capsule_sum([i64::MAX, 1], |_| true, |i| i), Err("exactness-loss"));
    let capsule = fs::read_to_string("../../language/spec/binders/core/sum.emath").unwrap();
    for required in [
        "std.binder.sum",
        "depends_on -> std.capability.math.add",
        "finite left fold",
        "open-domain",
        "wrong-identity",
    ] {
        p.demand(format!("capsule missing {required}"), capsule.contains(required), format!("capsule missing {required}"));
    }
    let nucleus = [
        fs::read_to_string("../../crates/emath-syntax/src/stage0.rs").unwrap(),
        fs::read_to_string("../../crates/emath-sema/src/live_adapter.rs").unwrap(),
    ]
    .join("\n");
    p.demand("sum FeatureID cannot branch in nucleus", !nucleus.contains("\"std.binder.sum\""), "sum FeatureID cannot branch in nucleus");

    });
    p.finish();
}



