//! `emath-sema` compiler-session tests (migrated from
//! `crates/emath-sema/src/session.rs`).

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

#[test]
fn tiny_session_token_budget_refuses_parse() {
    // Session limits must reach the lexer through the parser backend:
    // a tiny token budget refuses a larger source (E-SYN-108) instead
    // of parsing with `Limits::default()`.
    install_source_parser();
    let mut session = CompilerSession::new(Limits {
        max_tokens: 8,
        max_source_bytes: 1 << 20,
        max_nesting: 8,
    });
    let result = session.check_owned("token-heavy", "def f(x) = x + y + z");
    assert!(
        result
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-SYN-108"),
        "tiny max_tokens must refuse with E-SYN-108, got {:?}",
        result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
}

fn function_decl(name: &str, definitions: &[&str]) -> String {
    let mut text = format!(
        "emath function {name}:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n"
    );
    text.push_str("    definitions:\n");
    for definition in definitions {
        text.push_str("        ");
        text.push_str(definition);
        text.push('\n');
    }
    text
}

#[test]
fn duplicate_declaration_name_is_refused_with_e_name_022() {
    // Two declarations with the same name would collide in generated
    // Rust; the second is a typed refusal (E-NAME-022), never a
    // silent overwrite.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let mut text = function_decl("Left", &["y = x"]);
    text.push_str(&function_decl("Left", &["y = x * 2"]));
    let result = session.check_owned("dup", &text);
    assert!(
        result
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-NAME-022"),
        "duplicate declaration names must refuse with E-NAME-022"
    );
}

#[test]
fn underscore_declaration_name_is_refused_with_e_name_023() {
    // `_` cannot be escaped into a Rust type name; the declaration is
    // refused up front (E-NAME-023).
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("underscore", &function_decl("_", &["y = x"]));
    assert!(
        result
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-NAME-023"),
        "a declaration named `_` must refuse with E-NAME-023"
    );
}

#[test]
fn confusable_lookalike_declaration_is_refused_with_e_name_024() {
    // Two public declarations distinguishable only by lookalike
    // glyphs — Latin `a` vs Cyrillic `а` (U+0430) — are refused with
    // E-NAME-024: the generated API would expose two visually
    // identical names. Order-independent: whichever spelling arrives
    // second collides with the first.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let mut latin_then_cyrillic = function_decl("magnitude", &["y = x"]);
    latin_then_cyrillic.push_str(&function_decl("m\u{0430}gnitude", &["y = x"]));
    let forward = session.check_owned("confusable-forward", &latin_then_cyrillic);
    assert!(
        forward
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-NAME-024"),
        "Cyrillic lookalike after a Latin name must refuse with E-NAME-024"
    );

    let mut cyrillic_then_latin = function_decl("m\u{0430}gnitude", &["y = x"]);
    cyrillic_then_latin.push_str(&function_decl("magnitude", &["y = x"]));
    let backward = session.check_owned("confusable-backward", &cyrillic_then_latin);
    assert!(
        backward
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-NAME-024"),
        "Latin lookalike after a Cyrillic name must refuse with E-NAME-024"
    );
}

#[test]
fn names_that_are_not_lookalikes_are_not_refused() {
    // The confusable lint must not reject names that merely share a
    // prefix: `magnitude` and `magnitude2` fold apart and both admit,
    // with no E-NAME-024 (and no E-NAME-022) on either pass.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let mut text = function_decl("magnitude", &["y = x"]);
    text.push_str(&function_decl("magnitude2", &["y = x"]));
    let result = session.check_owned("distinct", &text);
    assert!(
        !result
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-NAME-024"),
        "distinct spellings must not be refused as confusable"
    );
    assert_eq!(
        result.package.declarations.len(),
        2,
        "both distinct declarations must admit"
    );
}

#[test]
fn goals_attach_to_their_own_declaration_by_id_not_span() {
    // Attach-by-id repair: goals elaborate per declaration and attach
    // by the ids built for that declaration, never by span geometry.
    // Here the first declaration owns three default goals and the
    // second owns one explicit goal; a span-based attach would pile
    // both declarations' goals onto whichever span covered them.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    // Definitions reference inputs only (chained definitions are
    // outside the Phase 1 admission subset), so all three lower.
    let mut text = function_decl("Left", &["y = x", "y2 = x", "y3 = x"]);
    text.push_str(
        "emath function Right:\n    inputs:\n        a: Float64\n    outputs:\n        b: Float64\n    definitions:\n        b = a\n    goals:\n        evaluate <b>:\n            produce rust.library\n",
    );
    let file = session.load_text("two-decls", text);
    let plan = session.plan(file);
    assert_eq!(plan.package.declarations.len(), 2);
    let left = &plan.package.declarations[0];
    let right = &plan.package.declarations[1];
    assert_eq!(
        left.goals.len(),
        3,
        "Left's three definitions must elaborate into three goals"
    );
    assert_eq!(
        right.goals.len(),
        1,
        "Right's explicit goals: section must elaborate into one goal"
    );
    let right_goal = plan
        .package
        .goals
        .get(right.goals[0].index())
        .expect("Right's goal id must resolve");
    assert_eq!(right_goal.target, "b");
    // Every goal attached to a declaration sits inside that
    // declaration's own source span too — the geometric property the
    // old span filter approximated.
    for (declaration, goal_ids) in [
        (&plan.package.declarations[0], left.goals.as_slice()),
        (&plan.package.declarations[1], right.goals.as_slice()),
    ] {
        for goal_id in goal_ids {
            let goal = plan
                .package
                .goals
                .get(goal_id.index())
                .expect("attached goal id must resolve");
            assert!(
                declaration.source.contains(goal.source.start),
                "goal {} (start {}) must lie inside declaration `{}` span {:?}",
                goal.target,
                goal.source.start,
                declaration.name.leaf(),
                declaration.source,
            );
        }
    }
}

#[test]
fn omitted_outputs_section_admits_and_evaluates() {
    // Kind schema: `outputs:` is AtMostOne with default `definitions`.
    // A Greeter with only `inputs:` + `definitions:` must admit, lift
    // the definition onto the output surface, and elaborate an
    // evaluate goal. Confusable E-NAME-024 still refuses (see
    // `confusable_lookalike_declaration_is_refused_with_e_name_024`);
    // the lifted refusal here is only the old definition-must-be-output
    // surface check.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let text = "emath function Greeter:\n    inputs:\n        x: Float64\n    definitions:\n        y = x\n";
    let file = session.load_text("greeter-omitted-outputs", text);
    let plan = session.plan(file);
    let codes: Vec<&str> = plan
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.is_empty(),
        "omitted `outputs:` must admit, got {codes:?}"
    );
    assert_eq!(plan.package.declarations.len(), 1);
    let declaration = &plan.package.declarations[0];
    assert!(
        declaration.outputs.iter().any(|field| field.name == "y"),
        "omitted `outputs:` must expose definition `y`"
    );
    assert_eq!(plan.requests.len(), 1);
    assert_eq!(plan.requests[0].kind, "evaluate");
    assert_eq!(plan.requests[0].target, "y");
    assert_eq!(plan.requests[0].produce, "rust.library");
    assert!(!plan.plans.is_empty(), "evaluate goal must plan");
}
