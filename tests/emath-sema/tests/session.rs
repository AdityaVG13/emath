//! `emath-sema` compiler-session tests (migrated from
//! `crates/emath-sema/src/session.rs`).

use std::collections::BTreeSet;

use emath_core::Severity;
use emath_core::limits::Limits;
use emath_ir::{Mig, MigNodeKind};
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

#[test]
fn omitted_inputs_section_admits_constant_definitions() {
    // Kind schema: `inputs:` is AtMostOne. A constant-only declaration
    // with only `definitions:` must admit and lift those definitions
    // onto the output surface. An undeclared name still refuses
    // (E-TYPE-002); omitting inputs must not swallow name errors.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let text = "emath function TwentyOne:\n    definitions:\n        y = 3 * 7\n";
    let file = session.load_text("twenty-one", text);
    let plan = session.plan(file);
    let codes: Vec<&str> = plan
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.is_empty(),
        "omitted `inputs:` must admit, got {codes:?}"
    );
    assert_eq!(plan.package.declarations.len(), 1);
    let declaration = &plan.package.declarations[0];
    assert!(
        declaration.inputs.is_empty(),
        "constant-only declaration must have no inputs"
    );
    assert!(
        declaration.outputs.iter().any(|field| field.name == "y"),
        "omitted `outputs:` must expose definition `y`"
    );
    assert_eq!(plan.requests.len(), 1);
    assert_eq!(plan.requests[0].kind, "evaluate");
    assert_eq!(plan.requests[0].target, "y");

    let unknown = session.check_owned(
        "unknown-without-inputs",
        "emath function Bad:\n    definitions:\n        y = missing\n",
    );
    assert!(
        unknown
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-TYPE-002"),
        "unknown variable must still refuse when `inputs:` is omitted, got {:?}",
        unknown
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn untyped_input_name_defaults_to_float64() {
    // Bare `inputs: x` defaults to Float64 and records that default as
    // note N-TYPE-001. An explicit `x: Float64` admits the same shape
    // without the note.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let bare =
        "emath function Square:\n    inputs:\n        x\n    definitions:\n        y = x * x\n";
    let typed = "emath function Square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n";
    let bare_result = session.check_owned("bare-input", bare);
    let typed_result = session.check_owned("typed-input", typed);
    assert!(
        !bare_result.diagnostics.has_errors(),
        "bare input name must admit, got {:?}",
        bare_result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
    assert!(
        !typed_result.diagnostics.has_errors(),
        "annotated input must admit, got {:?}",
        typed_result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
    let bare_decl = &bare_result.package.declarations[0];
    let typed_decl = &typed_result.package.declarations[0];
    assert_eq!(bare_decl.inputs.len(), 1);
    assert_eq!(typed_decl.inputs.len(), 1);
    assert_eq!(bare_decl.inputs[0].name, "x");
    assert_eq!(typed_decl.inputs[0].name, "x");
    let bare_ty = bare_result
        .package
        .types
        .get(bare_decl.inputs[0].ty.index())
        .expect("bare input type");
    let typed_ty = typed_result
        .package
        .types
        .get(typed_decl.inputs[0].ty.index())
        .expect("typed input type");
    assert_eq!(bare_ty, typed_ty);
    assert!(
        matches!(bare_ty, emath_ir::TypeNode::Float64),
        "bare input must resolve to Float64, got {bare_ty:?}"
    );
    assert!(
        bare_result.diagnostics.items().iter().any(|diagnostic| {
            diagnostic.code == "N-TYPE-001"
                && diagnostic.severity == Severity::Note
                && diagnostic.message.contains("Float64")
        }),
        "defaulted input must emit note N-TYPE-001, got {:?}",
        bare_result.diagnostics.items()
    );
    assert!(
        typed_result
            .diagnostics
            .items()
            .iter()
            .all(|diagnostic| diagnostic.code != "N-TYPE-001"),
        "explicit Float64 must not emit the default note"
    );
}

#[test]
fn head_args_square_admits_and_matches_inputs_form() {
    // Head-args are identity-equivalent to the same names in `inputs:`.
    // `-> Float64` declares an output named after the declaration, so
    // `square = x * x` binds that output (not a silent unused `-> T`).
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let head = "\
emath function square(x: Float64) -> Float64:
    definitions:
        square = x * x
";
    let section = "\
emath function square:
    inputs:
        x: Float64
    outputs:
        square: Float64
    definitions:
        square = x * x
";
    let head_result = session.check_owned("head-args-square", head);
    let section_result = session.check_owned("section-square", section);
    assert!(
        !head_result.diagnostics.has_errors(),
        "head-args square must admit, got {:?}",
        head_result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
    assert!(
        !section_result.diagnostics.has_errors(),
        "inputs: form must admit, got {:?}",
        section_result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
    let head_decl = &head_result.package.declarations[0];
    let section_decl = &section_result.package.declarations[0];
    assert_eq!(head_decl.inputs.len(), 1);
    assert_eq!(head_decl.inputs[0].name, "x");
    assert_eq!(section_decl.inputs[0].name, "x");
    assert!(
        head_decl.outputs.iter().any(|field| field.name == "square"),
        "-> T must declare output named after the declaration"
    );
    assert!(
        head_decl.definitions.contains_key("square"),
        "definition square must admit"
    );
    let head_ty = head_result
        .package
        .types
        .get(head_decl.inputs[0].ty.index())
        .expect("head input type");
    let section_ty = section_result
        .package
        .types
        .get(section_decl.inputs[0].ty.index())
        .expect("section input type");
    assert_eq!(head_ty, section_ty);
    assert!(matches!(head_ty, emath_ir::TypeNode::Float64));
}

#[test]
fn untyped_head_args_default_to_float64() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(
        "untyped-head-args",
        "emath function square(x) -> Float64:\n    definitions:\n        square = x * x\n",
    );
    assert!(
        !result.diagnostics.has_errors(),
        "untyped head-args must admit, got {:?}",
        result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
    assert!(
        result.diagnostics.items().iter().any(|diagnostic| {
            diagnostic.code == "N-TYPE-001"
                && diagnostic.severity == Severity::Note
                && diagnostic.message.contains("Float64")
        }),
        "untyped head-arg must emit N-TYPE-001, got {:?}",
        result.diagnostics.items()
    );
}

#[test]
fn head_args_mixed_with_inputs_refused() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(
        "mixed-head-args",
        "emath function square(x: Float64) -> Float64:\n    inputs:\n        x: Float64\n    definitions:\n        square = x * x\n",
    );
    assert!(
        result
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-SYN-122"),
        "mixed head-args + inputs: must refuse E-SYN-122, got {:?}",
        result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
}

/// The exit gate: one source package parses into the
/// mathematical intent graph with every semantic plane represented.
const SIX_PLANE_SOURCE: &str = "emath policy SixPlanes:
    inputs:
        x: Float64

    outputs:
        score: Float64

    state:
        scale: Float64

    constructors:
        public fn new(scale: Float64) -> Result<Self, ConfigError>:
            require is_finite(scale)

            Self:
                scale = scale

    definitions:
        score = state.scale * x

    goals:
        evaluate <score>:
            produce rust.library

    tests:
        example <unit_scale>:
            given scale = 1
            given x = 3
            expect score == 3

    exports:
        public constructor new
        public function score

    compile:
        target rust
        profile library
        numeric strict-f64
        safety forbid-unsafe
";

#[test]
fn one_source_package_parses_into_the_intent_graph_with_every_plane() {
    // Definition plane: inputs/outputs/state/definitions. Construction
    // plane: constructor + require obligation + Self assignment. Goal
    // plane: the evaluate goal. Evidence plane: the example test.
    // Execution plane: the compile spec. Evolution plane: the exports.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let file = session.load_text("six-planes", SIX_PLANE_SOURCE);
    let result = session.plan(file);
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.is_empty(),
        "six-plane source must admit, got {codes:?}"
    );

    let mig = Mig::from_package(&result.package);
    let kinds: BTreeSet<&'static str> = mig.nodes.iter().map(|node| node.kind.name()).collect();
    for kind in [
        MigNodeKind::Declaration,
        MigNodeKind::Input,
        MigNodeKind::Output,
        MigNodeKind::State,
        MigNodeKind::Definition,
        MigNodeKind::Constructor,
        MigNodeKind::Obligation,
        MigNodeKind::Assignment,
        MigNodeKind::Goal,
        MigNodeKind::Test,
        MigNodeKind::CompileSpec,
        MigNodeKind::Export,
    ] {
        assert!(
            kinds.contains(kind.name()),
            "intent graph must represent `{}`, got {kinds:?}",
            kind.name()
        );
    }

    // Snapshot stability: an independent session over the same source
    // yields a byte-identical intent graph and the same identity.
    let mut second_session = CompilerSession::new(Limits::default());
    let second_file = second_session.load_text("six-planes", SIX_PLANE_SOURCE);
    let second = second_session.plan(second_file);
    let second_mig = Mig::from_package(&second.package);
    assert_eq!(mig.canonical(), second_mig.canonical());
    assert_eq!(mig.identity(), second_mig.identity());
}

#[test]
fn expect_less_example_admits_as_worked_example() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let source = "\
emath function Square:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y = x * x

    goals:
        evaluate <y>:
            produce rust.library

    tests:
        example <four_squared>:
            given x = 4

    compile:
        target rust
        profile library
        numeric strict-f64
";
    let result = session.check_owned("worked", source);
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        !codes.iter().any(|code| *code == "E-NAME-026"),
        "expect-less example must not raise E-NAME-026, got {codes:?}"
    );
    assert!(
        codes.is_empty(),
        "expect-less example must admit, got {codes:?}"
    );
    assert_eq!(result.package.tests.len(), 1);
    assert!(
        result.package.tests[0].expect.is_none(),
        "worked example stores expect: None"
    );
}

#[test]
fn empty_example_body_admits_as_worked_example() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let source = "\
emath function TwentyOne:
    outputs:
        y: Float64

    definitions:
        y = 3 * 7

    tests:
        example <worked>:
";
    let result = session.check_owned("empty-example", source);
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.is_empty(),
        "empty example body must admit, got {codes:?}"
    );
    assert_eq!(result.package.tests.len(), 1);
    assert!(result.package.tests[0].given.is_empty());
    assert!(result.package.tests[0].expect.is_none());
}

fn assert_empty_source_refuses_epkg081(name: &str, source: &str) {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, source);
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.iter().any(|code| *code == "E-PKG-081"),
        "{name} must refuse empty source with E-PKG-081, got {codes:?}"
    );
    assert!(
        result.package.declarations.is_empty(),
        "{name} must not admit a declaration"
    );
}

#[test]
fn empty_file_is_refused_with_e_pkg_081() {
    assert_empty_source_refuses_epkg081("empty", "");
}

#[test]
fn whitespace_only_file_is_refused_with_e_pkg_081() {
    assert_empty_source_refuses_epkg081("whitespace", "  \n\n    \n");
}

#[test]
fn comment_only_file_is_refused_with_e_pkg_081() {
    assert_empty_source_refuses_epkg081(
        "comments",
        "# expect: E-PKG-081 empty source has no declarations\n# still nothing\n",
    );
}

#[test]
fn package_only_file_is_refused_with_e_pkg_081() {
    assert_empty_source_refuses_epkg081("package-only", "package foo.bar\n");
}

// ---------------------------------------------------------------------------
// Notation declarations (gap B): admitted functions whose bodies use
// declared glyphs (and their aliases) must lower through the normal
// builtin table, and qualified spellings of the same builtins must stay
// equivalent so that hand-written `core::math::pow` and a glyph that
// maps to it admit identically.
// ---------------------------------------------------------------------------

fn notation_function(definitions: &str) -> String {
    format!(
        "\
emath function F:
    inputs:
        x: Float64
        y: Float64
    outputs:
        r: Float64
    definitions:
        {definitions}
notation infixl 40 \"⊕\" => core::math::pow alias \"pw\"
"
    )
}

#[test]
fn notation_glyph_and_alias_uses_admit() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("notation-admit", &notation_function("r = x ⊕ y"));
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(codes.is_empty(), "glyph use must admit, got {codes:?}");
    let result = session.check_owned("notation-alias-admit", &notation_function("r = x pw y"));
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(codes.is_empty(), "alias use must admit, got {codes:?}");
}

#[test]
fn qualified_builtin_calls_spell_with_path_separators() {
    // Regression: `core::math::pow` in an expression used to be
    // re-joined with `.` at lowering, producing `unknown function
    // core.math.pow` (E-TYPE-003). The qualified spelling must equal
    // the glyph desugar and admit through the normal builtin table.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(
        "qualified-pow",
        "\
emath function F:
    inputs:
        x: Float64
    outputs:
        r: Float64
    definitions:
        r = core::math::pow(x, 2.0)
",
    );
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        !codes.iter().any(|code| *code == "E-TYPE-003"),
        "qualified builtin must not be an unknown function, got {codes:?}"
    );
}

#[test]
fn core_logic_not_admits_on_bool_and_refuses_on_non_bool() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let bool_source = "\
emath function N:
    inputs:
        p: Bool
    outputs:
        q: Bool
    definitions:
        q = core::logic::not(p)
";
    let result = session.check_owned("logic-not-bool", bool_source);
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.is_empty(),
        "core::logic::not on a Bool must admit, got {codes:?}"
    );

    let float_source = "\
emath function N:
    inputs:
        p: Float64
    outputs:
        q: Bool
    definitions:
        q = core::logic::not(p)
";
    let result = session.check_owned("logic-not-float", float_source);
    assert!(
        result
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-TYPE-012"),
        "core::logic::not on a Float64 must refuse with E-TYPE-012"
    );
}

#[test]
fn reserved_notation_glyph_is_refused_through_the_session() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let source = "\
emath function F:
    outputs:
        r: Float64
    definitions:
        r = 1.0
notation prefix 90 \"or\" => core::logic::not
";
    let result = session.check_owned("notation-reserved", source);
    assert!(
        result
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-NOTATION-RESERVED"),
        "reserved glyph must refuse through the session"
    );
}
