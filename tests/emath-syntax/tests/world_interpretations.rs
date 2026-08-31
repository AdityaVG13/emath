//! `emath-v9-06-2rdq.15`: `emath world` declarative interpretations for
//! custom terms (Wave 14).
//!
//! A world interprets custom/open terms through operator maps; it never
//! silently applies to strict source. Admission is recognition-level
//! (`use std.kinds.world` + `emath world Name:`), evidence-neutral
//! (E1/not-run, no checker), and the strict lane refuses the mapped glyph
//! as unknown rather than inheriting the interpretation.

use emath_core::limits::Limits;
use emath_core::tree::{ExprKind, StmtKind};
use emath_ir::{ClaimVerdict, EvidenceLevel};
use emath_sema::CompilerSession;
use emath_syntax::{install_source_parser, parse_str};

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

#[test]
fn world_happy_path_admits() {
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
    assert!(
        !parse_diagnostics.has_errors(),
        "{:?}",
        parse_diagnostics.errors().collect::<Vec<_>>()
    );
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
    let entries = operators.expect("operators section").suite.statements.clone();
    assert_eq!(entries.len(), 2);
    assert!(
        matches!(
            &entries[0].kind,
            StmtKind::Command { head, argument: Some(_) }
                if head.len() == 2 && head[0] == "operator" && head[1] == "⊕"
        ),
        "operator map entry must be `operator <glyph>` with a target, got {:?}",
        entries[0].kind
    );

    let checked = check("world-happy", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    assert_eq!(checked.package.declarations.len(), 1);
    let world = &checked.package.declarations[0];
    assert_eq!(world.kind_label, "world");
    assert_eq!(world.evidence.len(), 1);
    assert_eq!(world.evidence[0].verdict, ClaimVerdict::NotRun);
    assert_eq!(world.evidence[0].level, EvidenceLevel::E1);
    assert_eq!(world.evidence[0].checker, None);

    let repeated = check("world-happy-repeat", source);
    assert_eq!(
        checked.package.meaning_id(&[]).unwrap(),
        repeated.package.meaning_id(&[]).unwrap(),
        "world admission must be deterministic (same source, same identity)"
    );
}

#[test]
fn operator_map_target_is_a_path() {
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
            assert!(
                matches!(&expr.kind, ExprKind::Path { segments, .. } if !segments.is_empty()),
                "map target must be a path, got {:?}",
                expr.kind
            );
        }
        other => panic!("expected operator command with path argument, got {other:?}"),
    }
}

#[test]
fn malformed_operator_entry_refuses() {
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
    assert!(
        checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-027"),
        "bare word in operators must refuse E-KIND-027, got {:?}",
        checked.diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    assert!(checked.package.declarations.is_empty());
}

#[test]
fn world_never_applies_to_strict() {
    // Firewall (bead negative control): a strict function in the same file
    // using the world-mapped glyph must be refused by the strict lane —
    // the interpretation never silently applies to strict source.
    let invalid = check(
        "invalid-world-strict",
        include_str!("../../../tests/invalid/world_interpretations.emath"),
    );
    assert!(
        invalid.diagnostics.has_errors(),
        "strict use of a world-mapped glyph must refuse"
    );
    assert!(
        invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-TYPE-003"),
        "expected the strict unknown-name refusal, got {:?}",
        invalid.diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}
