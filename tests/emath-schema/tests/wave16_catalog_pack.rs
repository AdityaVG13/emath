use emath_schema::parse_feature_capsule;
use std::fs;

fn validate(path: &str, expected: usize) {
    let source = fs::read_to_string(path).unwrap();
    let docs = source
        .split("\nemath feature ")
        .skip(1)
        .map(|part| format!("emath feature {part}"))
        .collect::<Vec<_>>();
    assert_eq!(docs.len(), expected);
    let mut ids = std::collections::BTreeSet::new();
    for doc in docs {
        let (capsule, issues) = parse_feature_capsule(&doc);
        assert!(issues.is_empty(), "{issues:?}");
        let capsule = capsule.unwrap();
        assert!(ids.insert(capsule.feature_id.clone()));
        assert_eq!(capsule.maturity, emath_ir::Maturity::Cataloged);
        assert!(capsule.has_blocking_hole());
    }
    assert!(!source.contains("authority_target: \"capsule-active\""));
}

#[test]
fn algebra_pack_accounts_for_every_catalog_id_without_support_claims() {
    validate(
        "../../language/spec/field_packs/wave16-algebra-number-theory.emath",
        41,
    );
}

#[test]
fn remaining_math_and_world_packs_account_without_support_claims() {
    for (number, count) in [
        (7, 24),
        (8, 25),
        (9, 33),
        (10, 37),
        (11, 9),
        (12, 31),
        (13, 24),
        (14, 23),
        (15, 32),
        (16, 38),
        (17, 9),
        (18, 41),
        (19, 42),
        (20, 39),
    ] {
        validate(
            &format!("../../language/spec/field_packs/wave16-math-{number}.emath"),
            count,
        );
    }
    for (number, count) in [
        (21, 19),
        (22, 27),
        (23, 2),
        (24, 35),
        (25, 2),
        (26, 29),
        (27, 22),
        (28, 24),
        (29, 31),
        (30, 20),
        (31, 14),
    ] {
        validate(
            &format!("../../language/spec/worlds/wave16-worlds-{number}.emath"),
            count,
        );
    }
}
