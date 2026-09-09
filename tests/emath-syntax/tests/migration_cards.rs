//!: `emath migration` — typed cell/source migration
//! cards for meaning-affecting changes (V9-13 + cell
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

fn check_source(source: &str) -> emath_sema::admit::CheckResult {
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned("migration-cards", source)
}

fn card(rules: &str, evidence: &str) -> String {
    format!(
        "use std.kinds.migration\n\nemath migration softmax_policy_1_to_2:\n    from:\n        kind: \"std.tensor.softmax\"\n        to: \"std.tensor.softmax/v2\"\n        changes: \"numeric_policy\"\n{rules}{evidence}\n"
    )
}

use emath_test_harness::{Probe, boot};

#[test]
fn migration_cards() {
    boot();
    let mut probe = Probe::new("`emath migration` — typed cell/source migration cards for meaning-affecting changes (V9-13 + cell editions; extend, not duplicate: the card is a");
    probe.case("rules_classify_line_carries_area_and_word", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!diags.has_errors(), format!( "{diags:?}"));
    if p.failures().len() != f0 { return; }
    let Some(emath_core::tree::Item::Declaration(decl)) = tree.items.last() else {
        panic!("decl");
    };
    let rules = decl
        .sections_vec()
        .into_iter()
        .find(|s| s.name == "rules")
        .expect("rules");
    p.demand("2",!rules.suite.statements.is_empty(), format!(
        "rules section must carry statements"
    ));
    if p.failures().len() != f0 { return; }
    for stmt in &rules.suite.statements {
        let emath_core::tree::StmtKind::Command { head, argument } = &stmt.kind else {
            panic!("not a command: {stmt:?}");
        };
        p.eq("3", head.first().map(String::as_str), Some("classify"));
        match argument {
            Some(emath_core::tree::CommandArgument::Assignment { name, value }) => {
                p.demand("4", (name) == ("numeric_policy"), format!("expected {:?}, got {:?}", ("numeric_policy"), (name)));
                if p.failures().len() != f0 { return; }
                let word = match &value.kind {
                    emath_core::tree::ExprKind::Str(word) => Some(word.clone()),
                    emath_core::tree::ExprKind::Path { segments, .. } if segments.len() == 1 => {
                        Some(segments[0].clone())
                    }
                    other => panic!("value kind was {other:?}"),
                };
                p.eq("5", word.as_deref(), Some("meaning"));
            }
            other => panic!("argument shape was {other:?}"),
        }
    }

    });
    probe.case("migration_card_with_classified_meaning_and_evidence_admits", |p| {
    let f0 = p.failures().len();

    // Positive: the numeric-policy change is declared, classified as
    // `meaning`, and supported by the evidence section. Admits with no
    // errors, the package records the declaration under kind
    // `migration`, and the trace names the card (an arm that parses but
    // does not record would leave the card invisible in receipts).
    let checked = check_source(&card(
        "    rules:\n        classify numeric_policy = meaning\n",
        "    evidence:\n        claim <policy_made_explicit>:\n            statement: \"edition 2 writes the numeric policy the soft-max cell always had\"\n            level E1\n",
    ));
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "classified + evidenced migration must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let migration_decl = checked
        .package
        .declarations
        .iter()
        .find(|decl| decl.name.0 == "softmax_policy_1_to_2");
    p.demand("2",migration_decl.is_some(), format!(
        "migration card must be recorded as a package declaration"
    ));
    if p.failures().len() != f0 { return; }
    p.eq("3", migration_decl.map(|decl| decl.kind_label.as_str()), Some("migration"));
    let trace_text = format!("{:?}", checked.trace);
    p.demand("4",trace_text.contains("softmax_policy_1_to_2"), format!(
        "trace must name the migration card, got: {trace_text}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("migration_card_missing_from_section_refuses", |p| {
    let f0 = p.failures().len();

    // Schema gate (E-KIND-003): exactly one `from:` section is required;
    // a head with no `from:` is not a migration card.
    let source = "use std.kinds.migration\n\nemath migration vague:\n    rules:\n        classify layout = presentation\n";
    let checked = check_source(source);
    p.demand("1",checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-003"), format!(
        "missing `from:` must refuse E-KIND-003, got {:?}",
        checked
            .diagnostics
            .errors()
            .map(|e| e.code)
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("silent_numeric_policy_change_refuses", |p| {
    let f0 = p.failures().len();

    // The headline negative: a `changes:` area with no
    // classification is refused — the change must be typed, never
    // silently admitted (E-MIGR-011).
    let checked = check_source(&card(
        "    rules:\n        classify layout = presentation\n",
        "",
    ));
    p.demand("1",checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-MIGR-011"), format!(
        "unclassified numeric_policy change must refuse E-MIGR-011, got {:?}",
        checked
            .diagnostics
            .errors()
            .map(|e| e.code)
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("unknown_classification_refuses", |p| {
    let f0 = p.failures().len();

    // `classified` to a word outside the vocabulary is the same lie as no
    // classification: refuse E-MIGR-011.
    let checked = check_source(&card(
        "    rules:\n        classify numeric_policy = maybe\n",
        "",
    ));
    p.demand("1",checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-MIGR-011"), format!(
        "unknown classification must refuse E-MIGR-011, got {:?}",
        checked
            .diagnostics
            .errors()
            .map(|e| e.code)
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("meaning_change_without_new_evidence_refuses", |p| {
    let f0 = p.failures().len();

    // Authority never increases through the card alone: a meaning-
    // classified change with no `evidence:` section refuses E-MIGR-012.
    let checked = check_source(&card(
        "    rules:\n        classify numeric_policy = meaning\n",
        "",
    ));
    p.demand("1",checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-MIGR-012"), format!(
        "meaning change without evidence must refuse E-MIGR-012, got {:?}",
        checked
            .diagnostics
            .errors()
            .map(|e| e.code)
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("authority_raise_never_admits", |p| {
    let f0 = p.failures().len();

    // `raise` in `rules:` is refused outright: the card classifies, it
    // does not self-grant (mirror of the method-card E-KIND-027 fence).
    let source = "use std.kinds.migration\n\nemath migration power_grab:\n    from:\n        kind: \"std.tensor.softmax\"\n        to: \"std.tensor.softmax/v2\"\n    rules:\n        raise authority = true\n";
    let checked = check_source(source);
    p.demand("1",checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-MIGR-012"), format!(
        "`raise` must refuse E-MIGR-012, got {:?}",
        checked
            .diagnostics
            .errors()
            .map(|e| e.code)
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("presentation_only_change_admits_without_evidence", |p| {
    let f0 = p.failures().len();

    // Boundary: a purely presentational reclassification (renamed section,
    // formatting) needs no evidence section — it changes no meaning.
    let checked = check_source(&card(
        "    rules:\n        classify numeric_policy = presentation\n",
        "",
    ));
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "presentation-only migration must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
