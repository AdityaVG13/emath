//! capability-cell schema, identity, and bounded
//! admission (schema `emath.capability-cell.v1`).
//!
//! Cells are data with a closed class taxonomy, a required schema version
//! and migration policy, and a typed bounded-admission refusal set.
//! Identity is exactly the identity-affecting fields: mutating one moves
//! `CellId`; mutating `about` never does. No domain-named core enum variants
//! enter the IR in this.

use emath_core::QualifiedName;
use emath_ir::SemanticPackage;
use emath_ir::canonical::canonical_expr;
use emath_ir::meaning::MeaningError;
use emath_ir::{
    AdmissionRefusal, Capability, CapabilityId, CellClass, CellSchema, ExprId, ExprNode,
    MAX_CELL_ARITY, MigrationPolicy, admit_cell, admit_cell_mutation, canonical_cell, cell_id,
};

/// Acceptance negative seed.
const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/capability_cells.emath");

fn softmax(version: &str, migration: MigrationPolicy) -> CellSchema {
    CellSchema {
        name: QualifiedName::single("std.math.softmax"),
        class: CellClass::Pure,
        version: version.to_string(),
        migration,
        arity: 1,
        about: None,
    }
}

fn apply_softmax(package: &mut SemanticPackage) -> (CapabilityId, ExprId) {
    let id = package.push_capability(Capability {
        name: QualifiedName::single("std.math.softmax"),
        class: emath_ir::CellClass::Pure,
    });
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("x")),
        emath_core::Span::default(),
    );
    let applied = package.push_expr(
        ExprNode::Apply {
            capability: id,
            arguments: vec![x],
        },
        emath_core::Span::default(),
    );
    (id, applied)
}

#[test]
fn softmax_cell_validates_with_identity() {
    // Positive: the Softmax pure-cell descriptor validates under bounded
    // admission and mints a stable identity.
    let schema = softmax("1.0.0", MigrationPolicy::Frozen);
    let admitted = admit_cell(&schema).expect("softmax pure cell admits");
    assert_eq!(admitted.name.0, "std.math.softmax");

    // Identity is deterministic and name-bearing: same descriptor, same id.
    let again = cell_id(&softmax("1.0.0", MigrationPolicy::Frozen));
    assert_eq!(cell_id(&schema), again);

    // Canonical preimage carries exactly the identity fields.
    let canonical = canonical_cell(&schema);
    for token in [
        "emath.capability-cell.v1",
        "name:16:std.math.softmax",
        "class:4:pure",
        "version:5:1.0.0",
        "migration:6:frozen",
        "arity:1:1",
    ] {
        assert!(
            canonical.contains(token),
            "canonical missing `{token}`: {canonical}"
        );
    }
    assert!(!canonical.contains("about"), "about is presentation-only");
}

#[test]
fn identity_field_mutation_moves_cell_id() {
    // Identity: version bump moves the id.
    let v1 = cell_id(&softmax("1.0.0", MigrationPolicy::Frozen));
    let v2 = cell_id(&softmax("1.1.0", MigrationPolicy::Frozen));
    assert_ne!(v1, v2, "version is identity-affecting");

    // Identity: class change moves the id.
    let mut provider = softmax("1.0.0", MigrationPolicy::Frozen);
    provider.class = CellClass::Provider;
    assert_ne!(v1, cell_id(&provider), "class is identity-affecting");

    // Identity: migration policy token moves the id.
    let migratable = softmax(
        "1.0.0",
        MigrationPolicy::BumpAndNote {
            note: "initial".into(),
        },
    );
    assert_ne!(
        v1,
        cell_id(&migratable),
        "migration policy is identity-affecting"
    );

    // Identity: arity moves the id.
    let mut wider = softmax("1.0.0", MigrationPolicy::Frozen);
    wider.arity = 2;
    assert_ne!(v1, cell_id(&wider), "arity is identity-affecting");

    // Presentation: `about` never moves identity.
    let mut documented = softmax("1.0.0", MigrationPolicy::Frozen);
    documented.about = Some("stable maximum of exp(x - max)".into());
    assert_eq!(v1, cell_id(&documented), "about is presentation-only");
}

#[test]
fn mutation_is_policy_gated_never_silent() {
    let from = softmax("1.0.0", MigrationPolicy::Frozen);

    // Frozen cell: identity-affecting change refuses by name.
    let mut to = softmax("2.0.0", MigrationPolicy::Frozen);
    to.arity = 3;
    assert_eq!(
        admit_cell_mutation(&from, &to),
        Err(AdmissionRefusal::IdentityMutationRefused {
            name: "std.math.softmax".into(),
            from_version: "1.0.0".into(),
            to_version: "2.0.0".into(),
        })
    );

    // bump-and-note with a note: version bump admits.
    let migratable_from = softmax(
        "1.0.0",
        MigrationPolicy::BumpAndNote {
            note: "initial".into(),
        },
    );
    let migratable_to = softmax(
        "2.0.0",
        MigrationPolicy::BumpAndNote {
            note: "arity widened to 3".into(),
        },
    );
    assert!(admit_cell_mutation(&migratable_from, &migratable_to).is_ok());

    // bump-and-note with an empty note: refuses (a policy without a note
    // is not a policy).
    let noteless_to = CellSchema {
        migration: MigrationPolicy::BumpAndNote {
            note: String::new(),
        },
        ..softmax("2.0.0", MigrationPolicy::Frozen)
    };
    assert_eq!(
        admit_cell_mutation(&migratable_from, &noteless_to),
        Err(AdmissionRefusal::IdentityMutationRefused {
            name: "std.math.softmax".into(),
            from_version: "1.0.0".into(),
            to_version: "2.0.0".into(),
        })
    );

    // Class change refuses even under bump-and-note: the taxonomy is not
    // migrated by version bumps.
    let mut reclassified = softmax(
        "2.0.0",
        MigrationPolicy::BumpAndNote {
            note: "reclassify".into(),
        },
    );
    reclassified.class = CellClass::Intrinsic;
    assert!(matches!(
        admit_cell_mutation(&migratable_from, &reclassified),
        Err(AdmissionRefusal::IdentityMutationRefused { .. })
    ));

    // Same-descriptor call is a no-op admission, not a mutation.
    assert!(admit_cell_mutation(&from, &from).is_ok());
}

#[test]
fn bounded_admission_refuses_by_code() {
    // Closed taxonomy: unknown class token refuses with E-CELL-001.
    assert_eq!(
        CellClass::parse("transcendental-quick"),
        Err(AdmissionRefusal::UnknownCellClass {
            class: "transcendental-quick".into(),
        })
    );
    assert_eq!(
        CellClass::parse("transcendental-quick").unwrap_err().code(),
        "E-CELL-001"
    );

    // The negative seed names exactly this diagnostic.
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|line| line.trim_start().starts_with("# expect:"))
        .expect("negative seed must name its required diagnostic");
    assert!(
        expect_line.contains("E-CELL-001"),
        "seed expects the closed-taxonomy refusal, found: {expect_line}"
    );

    // Missing version refuses with E-CELL-002.
    let versionless = CellSchema {
        version: String::new(),
        ..softmax("1.0.0", MigrationPolicy::Frozen)
    };
    assert_eq!(
        admit_cell(&versionless),
        Err(AdmissionRefusal::MissingVersion {
            name: "std.math.softmax".into(),
        })
    );
    assert_eq!(admit_cell(&versionless).unwrap_err().code(), "E-CELL-002");

    // Arity above the bound refuses with E-CELL-004.
    let wide = CellSchema {
        arity: MAX_CELL_ARITY + 1,
        ..softmax("1.0.0", MigrationPolicy::Frozen)
    };
    assert_eq!(
        admit_cell(&wide),
        Err(AdmissionRefusal::ArityExceeded {
            name: "std.math.softmax".into(),
            arity: MAX_CELL_ARITY + 1,
        })
    );

    // Malformed name refuses with E-CELL-005 (bare leaf, no namespace path).
    let nameless = CellSchema {
        name: QualifiedName::single("softmax"),
        ..softmax("1.0.0", MigrationPolicy::Frozen)
    };
    assert_eq!(
        admit_cell(&nameless),
        Err(AdmissionRefusal::MalformedName {
            name: "softmax".into(),
        })
    );

    // Boundary: arity exactly at the bound admits.
    let boundary = CellSchema {
        arity: MAX_CELL_ARITY,
        ..softmax("1.0.0", MigrationPolicy::Frozen)
    };
    assert!(
        admit_cell(&boundary).is_ok(),
        "arity == MAX_CELL_ARITY admits"
    );
}

#[test]
fn admitted_cell_terms_stay_slot_stable() {
    // Wiring: a cell descriptor that admits produces the same arena term
    // and the same expression identity regardless of intern order; a
    // dangling capability application still refuses with the
    // typed refusal (admission never papers over the seam).
    let schema = softmax("1.0.0", MigrationPolicy::Frozen);
    let admitted = admit_cell(&schema).expect("softmax admits");

    let mut first = SemanticPackage::new();
    let first_cell = first.push_capability(admitted.clone());
    let (_, first_apply) = apply_softmax(&mut first);

    let mut second = SemanticPackage::new();
    let _unrelated = second.push_capability(Capability {
        name: QualifiedName::single("std.math.sigmoid"),
        class: emath_ir::CellClass::Pure,
    });
    let second_cell = second.push_capability(admitted);
    let (_, second_apply) = apply_softmax(&mut second);

    assert_ne!(first_cell, second_cell);
    assert_eq!(
        canonical_expr(&first, first_apply),
        canonical_expr(&second, second_apply),
        "cell identity is name-based, not slot-based"
    );

    // The dangling seam stays typed: a refused cell can never be interned,
    // so the only reachable failure is MissingCapability at meaning time.
    let refused = CellSchema {
        arity: MAX_CELL_ARITY + 1,
        ..softmax("1.0.0", MigrationPolicy::Frozen)
    };
    assert!(admit_cell(&refused).is_err());
    let mut dangling = SemanticPackage::new();
    let dangling_id = dangling.push_capability(Capability {
        name: QualifiedName::single("std.math.softmax"),
        class: emath_ir::CellClass::Pure,
    });
    let x = dangling.push_expr(
        ExprNode::Variable(QualifiedName::single("x")),
        emath_core::Span::default(),
    );
    let _ = dangling.push_expr(
        ExprNode::Apply {
            capability: dangling_id,
            arguments: vec![x],
        },
        emath_core::Span::default(),
    );
    // Dangle the cell: intern the Apply first, then clear the arena, so the
    // term references a slot that admission never filled.
    dangling.capabilities.clear();
    assert_eq!(
        dangling.meaning_id(&[]),
        Err(MeaningError::MissingCapability(dangling_id))
    );
}
