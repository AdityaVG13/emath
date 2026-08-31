//! `emath-v9-06-2rdq.19`: `emath migration` — typed cell/source migration
//! cards for meaning-affecting changes (Wave 9 V9-13 + Wave 12 cell
//! editions; extend, not duplicate: the card is a data-driven
//! `std.kinds.migration` application, never a parser keyword or a
//! stable-IR branch).
//!
//! Contract pinned here:
//! - `from:` states what moved (`kind:`, `to:`, optional `changes:` list);
//! - every declared change must be classified in `rules:` as
//!   presentation | meaning | evidence | provider — an unclassified
//!   change refuses (`E-MIGR-011`), a silent semantic change is never
//!   admitted by omission;
//! - authority never increases through the card alone (`E-MIGR-012`):
//!   `raise` refuses, and a meaning-classified change without the
//!   `evidence:` section refuses (new evidence is the only support);
//! - a card missing its `from:` section is refused by the schema
//!   (`E-KIND-003`).
//!
//! Failure-first: every admission/refusal pin below is RED until
//! `admit_migration` lands.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

#[test]
fn rules_classify_line_carries_area_and_word() {
    // Parse-shape contract for `admit_migration`: `classify
    // numeric_policy = meaning` must arrive as a command whose head
    // STARTS with `classify` (head-word collection stops before the
    // assignment `=`) and whose argument carries the area and the
    // classification word as an `Assignment`.
    //
    // The classification word itself may be a `Str` (quoted) or a
    // single-segment `Path` (bare word — the parser lane's spelling for
    // bare command-tail words moved between the two after the original
    // close). Admission reads the word text from both; the vocabulary
    // fence is pinned separately by `unknown_classification_refuses`.
    let (tree, diags) = emath_syntax::parse_str(
        "use std.kinds.migration\n\nemath migration m:\n    from:\n        kind: \"a\"\n        to: \"b\"\n    rules:\n        classify numeric_policy = meaning\n",
    );
    assert!(!diags.has_errors(), "{diags:?}");
    let Some(emath_core::tree::Item::Declaration(decl)) = tree.items.last() else {
        panic!("decl");
    };
    let rules = decl
        .sections_vec()
        .into_iter()
        .find(|s| s.name == "rules")
        .expect("rules");
    assert!(
        !rules.suite.statements.is_empty(),
        "rules section must carry statements"
    );
    for stmt in &rules.suite.statements {
        let emath_core::tree::StmtKind::Command { head, argument } = &stmt.kind else {
            panic!("not a command: {stmt:?}");
        };
        assert_eq!(
            head.first().map(String::as_str),
            Some("classify"),
            "head was {head:?}"
        );
        match argument {
            Some(emath_core::tree::CommandArgument::Assignment { name, value }) => {
                assert_eq!(name, "numeric_policy", "assignment name");
                let word = match &value.kind {
                    emath_core::tree::ExprKind::Str(word) => Some(word.clone()),
                    emath_core::tree::ExprKind::Path { segments, .. } if segments.len() == 1 => {
                        Some(segments[0].clone())
                    }
                    other => panic!("value kind was {other:?}"),
                };
                assert_eq!(word.as_deref(), Some("meaning"), "classification word");
            }
            other => panic!("argument shape was {other:?}"),
        }
    }
}

fn check_source(source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned("v9-06-2rdq-19", source)
}

fn card(rules: &str, evidence: &str) -> String {
    format!(
        "use std.kinds.migration\n\nemath migration softmax_policy_1_to_2:\n    from:\n        kind: \"std.tensor.softmax\"\n        to: \"std.tensor.softmax/v2\"\n        changes: \"numeric_policy\"\n{rules}{evidence}\n"
    )
}

#[test]
fn migration_card_with_classified_meaning_and_evidence_admits() {
    // Positive: the numeric-policy change is declared, classified as
    // `meaning`, and supported by the evidence section. Admits with no
    // errors, the package records the declaration under kind
    // `migration`, and the trace names the card (an arm that parses but
    // does not record would leave the card invisible in receipts).
    let checked = check_source(&card(
        "    rules:\n        classify numeric_policy = meaning\n",
        "    evidence:\n        claim <policy_made_explicit>:\n            statement: \"edition 2 writes the numeric policy the soft-max cell always had\"\n            level E1\n",
    ));
    assert!(
        !checked.diagnostics.has_errors(),
        "classified + evidenced migration must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let migration_decl = checked
        .package
        .declarations
        .iter()
        .find(|decl| decl.name.0 == "softmax_policy_1_to_2");
    assert!(
        migration_decl.is_some(),
        "migration card must be recorded as a package declaration"
    );
    assert_eq!(
        migration_decl.map(|decl| decl.kind_label.as_str()),
        Some("migration"),
        "recorded kind must be `migration`"
    );
    let trace_text = format!("{:?}", checked.trace);
    assert!(
        trace_text.contains("softmax_policy_1_to_2"),
        "trace must name the migration card, got: {trace_text}"
    );
}

#[test]
fn migration_card_missing_from_section_refuses() {
    // Schema gate (E-KIND-003): exactly one `from:` section is required;
    // a head with no `from:` is not a migration card.
    let source =
        "use std.kinds.migration\n\nemath migration vague:\n    rules:\n        classify layout = presentation\n";
    let checked = check_source(source);
    assert!(
        checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-003"),
        "missing `from:` must refuse E-KIND-003, got {:?}",
        checked.diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}

#[test]
fn silent_numeric_policy_change_refuses() {
    // The bead's headline negative: a `changes:` area with no
    // classification is refused — the change must be typed, never
    // silently admitted (E-MIGR-011).
    let checked = check_source(&card(
        "    rules:\n        classify layout = presentation\n",
        "",
    ));
    assert!(
        checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-MIGR-011"),
        "unclassified numeric_policy change must refuse E-MIGR-011, got {:?}",
        checked.diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}

#[test]
fn unknown_classification_refuses() {
    // `classified` to a word outside the vocabulary is the same lie as no
    // classification: refuse E-MIGR-011.
    let checked = check_source(&card(
        "    rules:\n        classify numeric_policy = maybe\n",
        "",
    ));
    assert!(
        checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-MIGR-011"),
        "unknown classification must refuse E-MIGR-011, got {:?}",
        checked.diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}

#[test]
fn meaning_change_without_new_evidence_refuses() {
    // Authority never increases through the card alone: a meaning-
    // classified change with no `evidence:` section refuses E-MIGR-012.
    let checked = check_source(&card(
        "    rules:\n        classify numeric_policy = meaning\n",
        "",
    ));
    assert!(
        checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-MIGR-012"),
        "meaning change without evidence must refuse E-MIGR-012, got {:?}",
        checked.diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}

#[test]
fn authority_raise_never_admits() {
    // `raise` in `rules:` is refused outright: the card classifies, it
    // does not self-grant (mirror of the method-card E-KIND-027 fence).
    let source =
        "use std.kinds.migration\n\nemath migration power_grab:\n    from:\n        kind: \"std.tensor.softmax\"\n        to: \"std.tensor.softmax/v2\"\n    rules:\n        raise authority = true\n";
    let checked = check_source(source);
    assert!(
        checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-MIGR-012"),
        "`raise` must refuse E-MIGR-012, got {:?}",
        checked.diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}

#[test]
fn presentation_only_change_admits_without_evidence() {
    // Boundary: a purely presentational reclassification (renamed section,
    // formatting) needs no evidence section — it changes no meaning.
    let checked = check_source(&card(
        "    rules:\n        classify numeric_policy = presentation\n",
        "",
    ));
    assert!(
        !checked.diagnostics.has_errors(),
        "presentation-only migration must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
}
