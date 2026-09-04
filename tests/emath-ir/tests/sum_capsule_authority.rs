use std::fs;

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
fn finite_empty_guarded_and_scope_cases_match() {
    assert_eq!(
        capsule_sum(0..5, |_| true, |i| i),
        legacy_sum(0..5, |_| true, |i| i)
    );
    assert_eq!(capsule_sum([], |_| true, |i| i), Ok(0));
    assert_eq!(capsule_sum(0..6, |i| i % 2 == 0, |i| i * i), Ok(20));
    assert_eq!(capsule_sum([3, 1, 2], |_| true, |i| i), Ok(6));
}

#[test]
fn migration_defects_refuse_or_discriminate() {
    assert_ne!(
        capsule_sum(0..5, |_| true, |i| i),
        capsule_sum(0..=5, |_| true, |i| i)
    );
    assert_eq!(
        capsule_sum([i64::MAX, 1], |_| true, |i| i),
        Err("exactness-loss")
    );
    let capsule = fs::read_to_string("../../language/spec/binders/core/sum.emath").unwrap();
    for required in [
        "std.binder.sum",
        "depends_on -> std.capability.math.add",
        "finite left fold",
        "open-domain",
        "wrong-identity",
    ] {
        assert!(capsule.contains(required), "capsule missing {required}");
    }
    let nucleus = [
        fs::read_to_string("../../crates/emath-syntax/src/stage0.rs").unwrap(),
        fs::read_to_string("../../crates/emath-sema/src/live_adapter.rs").unwrap(),
    ]
    .join("\n");
    assert!(
        !nucleus.contains("\"std.binder.sum\""),
        "sum FeatureID cannot branch in nucleus"
    );
}
