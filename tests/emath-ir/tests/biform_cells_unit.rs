//! Biform cells: spec vs algorithm with
//! independent evidence. Positive fixture + negative authority-launder
//! control, proven with the softmax cell only as fixture data (schema
//! descriptor + evidence id tokens — Softmax is never a Rust branch).
//!
//! Targeted verify: `cargo test -p emath-ir-tests --test biform_cells_unit`.

use emath_core::id::QualifiedName;
use emath_ir::capability::{
    AdmissionRefusal, CellClass, CellSchema, ClosureRefusal, MigrationPolicy, ProjectionKind,
    ProjectionStatus, admit_cell, admit_cell_mutation, canonical_cell, cell_id, missing_required,
    plan_cell_closure, required_projections,
};
use emath_ir::capability::{
    BiformAuthority, BiformRefusal, BiformSide, BiformSideDisposition, SideEvidence,
    assess_biform_closure, biform_side_disposition,
};
use emath_test_harness::Probe;

const STD_MATH_SOFTMAX: &str = "std.math.softmax";
const SPEC_EVIDENCE: &str = "evidence:std.math.softmax:spec:v1";
const ALGO_EVIDENCE: &str = "evidence:std.math.softmax:algorithm:v2";

/// The fixture files are the language truth: the positive example and the
/// negative launder seed, read as data (never re-derived from this test's
/// constants).
const POSITIVE_FIXTURE: &str =
    include_str!("../../../tests/fixtures/language/intro/01_softmax_cell.emath");
const LAUNDER_FIXTURE: &str = include_str!("../../../tests/invalid/biform_authority_launder.emath");

/// Extract the `evidence:` tokens a fixture binds, in order. The tokens
/// are quoted string literals on the surface (`evidence: "…"`); the
/// quotes are surface, not part of the evidence-object id.
fn evidence_tokens(fixture: &str) -> Vec<String> {
    fixture
        .lines()
        .filter_map(|line| line.trim().strip_prefix("evidence:"))
        .map(|token| token.trim().trim_matches('"').trim().to_string())
        .collect()
}

fn softmax_biform_schema() -> CellSchema {
    CellSchema {
        name: QualifiedName::single(STD_MATH_SOFTMAX),
        class: CellClass::Biform,
        version: "1.0.0".into(),
        migration: MigrationPolicy::Frozen,
        arity: 1,
        about: Some(
            "softmax as biform cell: spec law (stable-max) vs algorithm (reference eval)".into(),
        ),
    }
}

/// Both sides supplied, each with its own independent evidence object:
/// the spec attested authored, the algorithm attested verified.
fn full_sides() -> Vec<SideEvidence> {
    vec![
        SideEvidence {
            side: BiformSide::Spec,
            evidence_id: SPEC_EVIDENCE.into(),
            authority: BiformAuthority::Authored,
        },
        SideEvidence {
            side: BiformSide::Algorithm,
            evidence_id: ALGO_EVIDENCE.into(),
            authority: BiformAuthority::Verified,
        },
    ]
}

/// The seven universal supplied projections plus specification and
/// algorithm rows, docs bound to the cell identity.
fn full_supply(schema: &CellSchema) -> Vec<(ProjectionKind, ProjectionStatus, Option<String>)> {
    let id = cell_id(schema).0;
    vec![
        (ProjectionKind::Semantics, ProjectionStatus::Provided, None),
        (ProjectionKind::Docs, ProjectionStatus::Provided, Some(id)),
        (ProjectionKind::Assurance, ProjectionStatus::Provided, None),
        (ProjectionKind::Evidence, ProjectionStatus::Provided, None),
        (ProjectionKind::Evolution, ProjectionStatus::Provided, None),
        (
            ProjectionKind::Specification,
            ProjectionStatus::Provided,
            None,
        ),
        (ProjectionKind::Algorithm, ProjectionStatus::Provided, None),
    ]
}

#[test]
fn intent() {
    let mut p = Probe::new("Biform cells: spec vs algorithm with");
    p.case("biform_cells_unit", |p| {

    let schema = softmax_biform_schema();

    // The biform class is part of the closed taxonomy, as data.
    p.demand("biform_cells_unit#1", CellClass::Biform.as_str() == "biform", format!("expected {:?}, got {:?}", "biform", CellClass::Biform.as_str()));
    p.eq("biform_cells_unit#2", CellClass::parse("biform").expect("biform token parses"), CellClass::Biform);
    p.eq("biform_cells_unit#3", admit_cell(&schema)
            .expect("biform softmax cell admits")
            .class, CellClass::Biform);

    // Separate spec and algorithm evidence: satisfying tests of the
    // algorithm do not prove the spec.
    let sides = full_sides();
    p.ne("spec and algorithm evidence IDs must differ", &sides[0].evidence_id, &sides[1].evidence_id);

    // Both sides validate: spec by authored law evidence, algorithm by
    // verified reference-eval evidence.
    p.eq("biform_cells_unit#5", biform_side_disposition(&schema, BiformSide::Spec, &sides), BiformSideDisposition::Provided {
            evidence_id: SPEC_EVIDENCE.into(),
            authority: BiformAuthority::Authored,
        });
    p.eq("biform_cells_unit#6", biform_side_disposition(&schema, BiformSide::Algorithm, &sides), BiformSideDisposition::Provided {
            evidence_id: ALGO_EVIDENCE.into(),
            authority: BiformAuthority::Verified,
        });
    p.demand("fully supplied biform cell validates", assess_biform_closure(&schema, &sides).is_empty(), "fully supplied biform cell validates");

    // Projection closure still requires the class's required projections:
    // the universal seven plus the spec/algorithm pair (biform replaces
    // the pure reference/compilation pair).
    let required = required_projections(CellClass::Biform);
    p.eq("biform: 7 universal + spec + algorithm", required.len(), 9);
    p.demand("biform_cells_unit#9", required.contains(&ProjectionKind::Specification), "biform_cells_unit#9: required.contains(&ProjectionKind::Specification)");
    p.demand("biform_cells_unit#10", required.contains(&ProjectionKind::Algorithm), "biform_cells_unit#10: required.contains(&ProjectionKind::Algorithm)");

    // Full closure is stable: no Refused row, no refusals.
    let supplied = full_supply(&schema);
    p.demand("full biform closure is stable", missing_required(&schema, &supplied).is_empty(), "full biform closure is stable");
    let plan = plan_cell_closure(&schema, &supplied);
    p.eq("closed projection set stays fully visible", plan.len(), 11);
    p.demand("closed plan carries no refusals", !plan.iter().any(|(_, s)| *s == ProjectionStatus::Refused), "closed plan carries no refusals");

    // Dropping the biform rows is a visible refusal pair, never a silent
    // hole: one typed MissingRequired per side row.
    let holes: Vec<_> = supplied
        .iter()
        .cloned()
        .filter(|(k, _, _)| !matches!(k, ProjectionKind::Specification | ProjectionKind::Algorithm))
        .collect();
    let refusals = missing_required(&schema, &holes);
    p.eq(format!("one E-CELL-007 per missing biform row: {refusals:?}"), refusals.len(), 2);
    p.demand("biform_cells_unit#15", refusals.iter().any(|r| matches!(
        r,
        ClosureRefusal::MissingRequired {
            projection: ProjectionKind::Specification,
            ..
        }
    )), "biform_cells_unit#15: refusals.iter().any(|r| matches!(\n        r,\n        ClosureRefusal::MissingRequired {\n            p");
    p.demand("biform_cells_unit#16", refusals.iter().any(|r| matches!(
        r,
        ClosureRefusal::MissingRequired {
            projection: ProjectionKind::Algorithm,
            ..
        }
    )), "biform_cells_unit#16: refusals.iter().any(|r| matches!(\n        r,\n        ClosureRefusal::MissingRequired {\n            p");

    });
    p.case("biform_authority_launder_negative", |p| {

    let schema = softmax_biform_schema();

    // 1) Algorithm tests green claimed as spec proof: one evidence object
    //    on both sides is a side-evidence collision (E-CELL-011).
    let launder = vec![
        SideEvidence {
            side: BiformSide::Spec,
            evidence_id: ALGO_EVIDENCE.into(),
            authority: BiformAuthority::Verified,
        },
        SideEvidence {
            side: BiformSide::Algorithm,
            evidence_id: ALGO_EVIDENCE.into(),
            authority: BiformAuthority::Verified,
        },
    ];
    let refusals = assess_biform_closure(&schema, &launder);
    p.demand(format!("same evidence object on both sides is refused: {refusals:?}"), refusals.iter().any(|r| matches!(
            r,
            BiformRefusal::SideEvidenceCollision {
                spec_evidence_id,
                algorithm_evidence_id,
                ..
            } if spec_evidence_id == algorithm_evidence_id
        )), format!("same evidence object on both sides is refused: {refusals:?}"));
    p.demand("biform_authority_launder_negative#2", refusals[0].code() == "E-CELL-011", format!("expected {:?}, got {:?}", "E-CELL-011", refusals[0].code()));

    // 2) A missing spec treated as proved (algorithm-only evidence) is a
    //    typed missing-side disposition (E-CELL-009), never a silent hole.
    let algo_only = vec![SideEvidence {
        side: BiformSide::Algorithm,
        evidence_id: ALGO_EVIDENCE.into(),
        authority: BiformAuthority::Verified,
    }];
    let refusals = assess_biform_closure(&schema, &algo_only);
    p.demand(format!("missing spec side is refused: {refusals:?}"), refusals.iter().any(|r| matches!(
            r,
            BiformRefusal::MissingSide {
                side: BiformSide::Spec,
                ..
            }
        )), format!("missing spec side is refused: {refusals:?}"));
    p.demand("biform_authority_launder_negative#4", refusals[0].code() == "E-CELL-009", format!("expected {:?}, got {:?}", "E-CELL-009", refusals[0].code()));
    p.eq("biform_authority_launder_negative#5", biform_side_disposition(&schema, BiformSide::Spec, &algo_only), BiformSideDisposition::Refused {
            refusal: BiformRefusal::MissingSide {
                name: STD_MATH_SOFTMAX.into(),
                side: BiformSide::Spec,
            },
        });

    // 3) Authority non-escalation: a provider receipt cannot raise spec
    //    authority (E-CELL-010), though it may attest the algorithm.
    let provider_spec = vec![
        SideEvidence {
            side: BiformSide::Spec,
            evidence_id: SPEC_EVIDENCE.into(),
            authority: BiformAuthority::Provider,
        },
        SideEvidence {
            side: BiformSide::Algorithm,
            evidence_id: ALGO_EVIDENCE.into(),
            authority: BiformAuthority::Provider,
        },
    ];
    let refusals = assess_biform_closure(&schema, &provider_spec);
    p.demand(format!("provider receipt on the spec side escalates: {refusals:?}"), refusals.iter().any(|r| matches!(
            r,
            BiformRefusal::AuthorityEscalation {
                side: BiformSide::Spec,
                ..
            }
        )), format!("provider receipt on the spec side escalates: {refusals:?}"));
    p.demand("biform_authority_launder_negative#7", refusals[0].code() == "E-CELL-010", format!("expected {:?}, got {:?}", "E-CELL-010", refusals[0].code()));
    p.eq("biform_authority_launder_negative#8", biform_side_disposition(&schema, BiformSide::Algorithm, &provider_spec), BiformSideDisposition::Provided {
            evidence_id: ALGO_EVIDENCE.into(),
            authority: BiformAuthority::Provider,
        });

    // Guard: the assessments above are non-trivial — a mutation of the
    // authority sum (dropping the launder) must un-refuse, or the
    // refusals are tautological.
    p.demand("clean supply closes; refusals above are caused by the mutations", assess_biform_closure(&schema, &full_sides()).is_empty(), "clean supply closes; refusals above are caused by the mutations");

    });
    p.case("biform_fixture_evidence_independent", |p| {

    // The positive language example (01_softmax_cell.emath) is read as
    // data: its spec and algorithm sections bind distinct evidence
    // objects, and a closure supplied exactly those fixtures' evidence
    // validates — green algorithm tests never stamp the spec proved.
    let tokens = evidence_tokens(POSITIVE_FIXTURE);
    p.eq("example fixture must bind two evidence objects (spec + algorithm)", tokens.len(), 2);
    p.ne("spec evidence != algorithm evidence", &tokens[0], &tokens[1]);

    let schema = softmax_biform_schema();
    let sides = vec![
        SideEvidence {
            side: BiformSide::Spec,
            evidence_id: tokens[0].clone(),
            authority: BiformAuthority::Authored,
        },
        SideEvidence {
            side: BiformSide::Algorithm,
            evidence_id: tokens[1].clone(),
            authority: BiformAuthority::Verified,
        },
    ];
    p.demand("both fixture-bound sides validate", assess_biform_closure(&schema, &sides).is_empty(), "both fixture-bound sides validate");
    // The spec side stays spec authority: the algorithm's verified
    // evidence does not leak onto it.
    p.eq("biform_fixture_evidence_independent#4", biform_side_disposition(&schema, BiformSide::Spec, &sides), BiformSideDisposition::Provided {
            evidence_id: tokens[0].clone(),
            authority: BiformAuthority::Authored,
        });

    });
    p.case("biform_launder_fixture_refuses", |p| {

    // The negative seed (biform_authority_launder.emath) binds one
    // evidence object to both sides. A closure supplied exactly that
    // fixture's bindings refuses with E-CELL-011 (side-evidence
    // collision): the launder is impossible, not silently accepted.
    let tokens = evidence_tokens(LAUNDER_FIXTURE);
    p.eq("launder fixture binds one token per side", tokens.len(), 2);
    p.eq("launder fixture reuses the algorithm evidence on the spec side", &tokens[0], &tokens[1]);

    let schema = softmax_biform_schema();
    let launder = vec![
        SideEvidence {
            side: BiformSide::Spec,
            evidence_id: tokens[0].clone(),
            authority: BiformAuthority::Verified,
        },
        SideEvidence {
            side: BiformSide::Algorithm,
            evidence_id: tokens[1].clone(),
            authority: BiformAuthority::Verified,
        },
    ];
    let refusals = assess_biform_closure(&schema, &launder);
    p.eq(format!("{refusals:?}"), refusals.len(), 1);
    p.demand("biform_launder_fixture_refuses#4", refusals[0].code() == "E-CELL-011", format!("expected {:?}, got {:?}", "E-CELL-011", refusals[0].code()));

    });
    p.case("biform_missing_side_and_duplicate_edges", |p| {

    let schema = softmax_biform_schema();
    let mut supplied = full_supply(&schema);

    // Spec-only evidence: the algorithm side is required but missing —
    // a typed MissingSide refusal (never "proved by the spec").
    let spec_only: Vec<_> = supplied
        .iter()
        .cloned()
        .filter(|(k, _, _)| k != &ProjectionKind::Algorithm)
        .collect();
    p.demand("missing algorithm row stays a visible E-CELL-007", missing_required(&schema, &spec_only)
            .iter()
            .any(|r| matches!(
                r,
                ClosureRefusal::MissingRequired {
                    projection: ProjectionKind::Algorithm,
                    ..
                }
            )), "missing algorithm row stays a visible E-CELL-007");

    // Duplicate entries for one side: first-wins determinism, still a
    // single Provided disposition — the closure never wobbles.
    let sides = vec![
        SideEvidence {
            side: BiformSide::Spec,
            evidence_id: SPEC_EVIDENCE.into(),
            authority: BiformAuthority::Verified,
        },
        SideEvidence {
            side: BiformSide::Spec,
            evidence_id: "evidence:std.math.softmax:spec:v0-stale".into(),
            authority: BiformAuthority::Authored,
        },
        SideEvidence {
            side: BiformSide::Algorithm,
            evidence_id: ALGO_EVIDENCE.into(),
            authority: BiformAuthority::Provider,
        },
    ];
    p.eq("first spec-side entry wins deterministically", biform_side_disposition(&schema, BiformSide::Spec, &sides), BiformSideDisposition::Provided {
            evidence_id: SPEC_EVIDENCE.into(),
            authority: BiformAuthority::Verified,
        });
    p.demand("duplicate side entries do not refuse", assess_biform_closure(&schema, &sides).is_empty(), "duplicate side entries do not refuse");

    // Non-biform classes: sides are NotApplicable, never refused — the
    // biform split stays class-scoped.
    let pure = CellSchema {
        name: emath_core::id::QualifiedName::single("std.math.softmax"),
        class: CellClass::Pure,
        version: "1.0.0".into(),
        migration: MigrationPolicy::Frozen,
        arity: 1,
        about: None,
    };
    p.eq("biform_missing_side_and_duplicate_edges#4", biform_side_disposition(&pure, BiformSide::Spec, &sides), BiformSideDisposition::NotApplicable);
    p.demand("non-biform class never refuses on biform sides", assess_biform_closure(&pure, &sides).is_empty(), "non-biform class never refuses on biform sides");

    });
    p.case("biform_identity_stable_across_supplies", |p| {

    // Side evidence is evidence, not identity: binding different
    // evidence objects (or none) must never move the CellId — proofs and
    // tests attach without changing the cell's meaning identity
    // While the biform class token itself is
    // identity-affecting and stable.
    let schema = softmax_biform_schema();
    let id_a = cell_id(&schema);

    p.demand("biform class token participates in canonical identity", canonical_cell(&schema).contains("class:6:biform"), "biform class token participates in canonical identity");
    let canonical = canonical_cell(&schema);
    p.demand("side evidence objects never enter canonical identity", !canonical.contains("evidence:"), "side evidence objects never enter canonical identity");
    // The schema has no side fields, so identity trivially ignores any
    // side supply; the fixture above proves the canonical form agrees.
    p.eq("identity ignores side supply", cell_id(&schema), id_a.clone());

    // Presentation (`about`) stays excluded; class change moves identity.
    let mut about_changed = schema.clone();
    about_changed.about = Some("different presentation".into());
    p.eq("about is presentation-only", cell_id(&about_changed), id_a.clone());

    let mut reclassed = schema.clone();
    reclassed.class = CellClass::Pure;
    p.ne("class is identity-affecting: pure != biform", cell_id(&reclassed), id_a.clone());

    });
    p.case("biform_mutation_kill", |p| {

    // The migration policy decides identity-affecting mutation: a frozen
    // biform cell refuses any change (E-CELL-003), and a class rewrite
    // (biform -> pure) is never silently admitted. An explicit
    // bump-and-note with a real note and version change admits.
    let schema = softmax_biform_schema();

    let mut bump = schema.clone();
    bump.version = "1.1.0".into();
    bump.migration = MigrationPolicy::Frozen;
    match admit_cell_mutation(&schema, &bump) {
        Err(AdmissionRefusal::IdentityMutationRefused { .. }) => {}
        other => { p.fail("biform_mutation_kill#1", format!("frozen biform bump must refuse, got {other:?}")); return; },
    }

    let mut reclassed = schema.clone();
    reclassed.class = CellClass::Pure;
    reclassed.migration = MigrationPolicy::BumpAndNote {
        note: "reclassify to pure".into(),
    };
    match admit_cell_mutation(&schema, &reclassed) {
        Err(AdmissionRefusal::IdentityMutationRefused { .. }) => {}
        other => { p.fail("biform_mutation_kill#2", format!("biform -> pure class rewrite must refuse, got {other:?}")); return; },
    }

    let mut admitted = schema.clone();
    admitted.version = "2.0.0".into();
    admitted.migration = MigrationPolicy::BumpAndNote {
        note: "stable-max law rephrased; spec evidence v2".into(),
    };
    let result = admit_cell_mutation(&schema, &admitted).expect("bump-and-note admits");
    p.eq("biform_mutation_kill#3", result.class, CellClass::Biform);

    });
    p.finish();
}













