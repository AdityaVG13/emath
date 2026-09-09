//!: `emath world` declarative interpretations for
//! custom terms.
//!
//! A world interprets custom/open terms through operator maps; it never
//! silently applies to strict source. Admission is recognition-level
//! (`use std.kinds.world` + `emath world Name:`), evidence-neutral
//! (E1/not-run, no checker), and the strict lane refuses the mapped glyph
//! as unknown rather than inheriting the interpretation.

use emath_core::tree::{ExprKind, StmtKind};
use emath_ir::{ClaimVerdict, EvidenceLevel};
use emath_syntax::{parse_str};

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    Source::from_str(name, source).check()
}

use emath_test_harness::{Probe, Source, boot};

#[test]
fn world_interpretations() {
    boot();
    let mut probe = Probe::new("`emath world` declarative interpretations for custom terms. A world interprets custom/open terms through operator maps; it never silently applies to");
    probe.case("world_happy_path_admits", |p| {
    let f0 = p.failures().len();

    let source = "\
use std.kinds.world

emath world Mod17:
    operators:
        \"⊕\" => core::math::add
        \"⊗\" => core::math::mul
    interpretations:
        total
        deterministic
    output: \"Mod17Interpretation\"
";
    let (tree, parse_diagnostics) = parse_str(source);
    p.demand("1",!parse_diagnostics.has_errors(), format!(
        "{:?}",
        parse_diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    // The operator-map entries parse as `operator <glyph>` commands with a
    // path argument — the surface records the map, it does not desugar it.
    let world_item = tree
        .items
        .iter()
        .find_map(|item| match item {
            emath_core::tree::Item::Declaration(decl) if decl.as_kind == "world" => Some(decl),
            _ => None,
        })
        .expect("world declaration in tree");
    let operators = world_item.body.iter().find_map(|stmt| match &stmt.kind {
        StmtKind::Section(section) if section.name == "operators" => Some(section),
        _ => None,
    });
    let entries = operators
        .expect("operators section")
        .suite
        .statements
        .clone();
    p.eq("2", entries.len(), 2);
    p.demand("3",matches!(
            &entries[0].kind,
            StmtKind::Command { head, argument: Some(_) }
                if head.len() == 2 && head[0] == "operator" && head[1] == "⊕"
        ), format!(
        "operator map entry must be `operator <glyph>` with a target, got {:?}",
        entries[0].kind
    ));
    if p.failures().len() != f0 { return; }

    let checked = check("world-happy", source);
    p.demand("4",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    p.eq("5", checked.package.declarations.len(), 1);
    let world = &checked.package.declarations[0];
    p.demand("6", (world.kind_label) == ("world"), format!("expected {:?}, got {:?}", ("world"), (world.kind_label)));
    if p.failures().len() != f0 { return; }
    p.eq("7", world.evidence.len(), 1);
    p.eq("8", world.evidence[0].verdict, ClaimVerdict::NotRun);
    p.eq("9", world.evidence[0].level, EvidenceLevel::E1);
    p.demand("10", world.evidence[0].checker == None, format!("expected None, got {:?}", (world.evidence[0].checker)));
    if p.failures().len() != f0 { return; }

    let repeated = check("world-happy-repeat", source);
    p.eq("11", checked.package.meaning_id(&[]).unwrap(), repeated.package.meaning_id(&[]).unwrap());

    });
    probe.case("operator_map_target_is_a_path", |p| {
    let f0 = p.failures().len();

    // The map target keeps its path shape (`core::math::add`) — admission
    // can resolve the target; the glyph binding stays world-local.
    let source = "\
use std.kinds.world

emath world Mod17:
    operators:
        \"⊕\" => core::math::add
    output: \"Mod17Interpretation\"
";
    let (tree, _) = parse_str(source);
    let decl = tree.items.iter().find_map(|item| match item {
        emath_core::tree::Item::Declaration(decl) if decl.as_kind == "world" => Some(decl),
        _ => None,
    });
    let Some(decl) = decl else {
        panic!("world declaration missing");
    };
    let entry = decl
        .body
        .iter()
        .find_map(|stmt| match &stmt.kind {
            StmtKind::Section(section) if section.name == "operators" => {
                Some(section.suite.statements[0].clone())
            }
            _ => None,
        })
        .expect("operator entry");
    match entry.kind {
        StmtKind::Command {
            argument: Some(emath_core::tree::CommandArgument::Expr(expr)),
            ..
        } => {
            p.demand("1",matches!(&expr.kind, ExprKind::Path { segments, .. } if !segments.is_empty()), format!(
                "map target must be a path, got {:?}",
                expr.kind
            ));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected operator command with path argument, got {other:?}"),
    }

    });
    probe.case("malformed_operator_entry_refuses", |p| {
    let f0 = p.failures().len();

    // An operator map entry must be `"glyph" => target`; a bare word in
    // `operators:` is not a map and must refuse typed, never silently
    // become an unimplementable binding.
    let source = "\
use std.kinds.world

emath world Mod17:
    operators:
        total
    output: \"Mod17Interpretation\"
";
    let checked = check("world-malformed", source);
    p.demand("1",checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-027"), format!(
        "bare word in operators must refuse E-KIND-027, got {:?}",
        checked
            .diagnostics
            .errors()
            .map(|e| e.code)
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",checked.package.declarations.is_empty(), stringify!(checked.package.declarations.is_empty()));
    if p.failures().len() != f0 { return; }

    });
    probe.case("world_never_applies_to_strict", |p| {
    let f0 = p.failures().len();

    // Firewall: a strict function in the same file
    // using the world-mapped glyph must be refused by the strict lane —
    // the interpretation never silently applies to strict source.
    let invalid = check(
        "invalid-world-strict",
        include_str!("../../../tests/invalid/world_interpretations.emath"),
    );
    p.demand("1",invalid.diagnostics.has_errors(), format!(
        "strict use of a world-mapped glyph must refuse"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-TYPE-003"), format!(
        "expected the strict unknown-name refusal, got {:?}",
        invalid
            .diagnostics
            .errors()
            .map(|e| e.code)
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
