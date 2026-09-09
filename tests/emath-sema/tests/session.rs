//! `emath-sema` compiler-session tests (migrated from
//! `crates/emath-sema/src/session.rs`).

use std::collections::BTreeSet;

use emath_core::Severity;
use emath_core::limits::Limits;
use emath_ir::{Mig, MigNodeKind};
use emath_sema::CompilerSession;
use emath_test_harness::{Probe, Source, boot, error_codes};

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
notation infixl 40 \"\u{2295}\" => core::math::pow alias \"pw\"
"
    )
}

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

const PROFILE_FIXTURE_BODY: &str = "\
emath function P:
    inputs:
        t: Float64
    outputs:
        y: Float64 in m
    definitions:
        y = 5 m
";

#[test]
fn compiler_session_admission_refusals_and_intent_graph() {
    boot();
    let mut p = Probe::new("the compiler session admits honest programs and refuses every other with one typed diagnostic");
    p.case("token-budget", |p| {
        // Source always uses default limits; a custom budget needs a session.
        let mut session = CompilerSession::new(Limits {
            max_tokens: 8,
            max_source_bytes: 1 << 20,
            max_nesting: 8,
        });
        let result = session.check_owned("token-heavy", "def f(x) = x + y + z");
        let codes = error_codes(&result.diagnostics);
        p.demand("e-syn-108", codes.contains(&"E-SYN-108"), format!("tiny max_tokens must refuse E-SYN-108, got {codes:?}"));
    });
    p.case("flat-goal", |p| {
        // A one-line `evaluate <y>:` heading with no payload never becomes a
        // goal: the parser refuses the missing block (E-SYN-112) or the sema
        // layer fires the flat-goal guidance (E-GOAL-042).
        let result = Source::from_str(
            "flat-goal",
            "\
emath function Flat:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
    goals:
        evaluate <y>:
",
        )
        .check();
        let rendered: Vec<String> = result
            .diagnostics
            .errors()
            .map(|d| format!("{} {}", d.code, d.message))
            .collect();
        p.demand(
            "refused-with-guidance",
            rendered.iter().any(|m| {
                (m.contains("E-SYN-112") && m.contains("indented block"))
                    || (m.contains("E-GOAL-042") && m.contains("flat"))
            }),
            format!("flat-goal heading must refuse with guidance, got {rendered:?}"),
        );
    });
    p.case("empty-definitions", |p| {
        let text = "\
emath function Square:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:

    goals:
        evaluate <y>:
            produce rust.library

    tests:
        example <three_squared>:
            given x = 3
            expect y == 9

    compile:
        target rust
        profile library
        numeric strict-f64
";
        let result = Source::from_str("empty-definitions", text).check();
        let codes = error_codes(&result.diagnostics);
        // Exactly one root parse refusal plus the honest no-definition
        // refusal; no E-TYPE-002 duplicate, no E-SYN-101 junk, no cascade.
        p.eq("codes", codes.clone(), vec!["E-NAME-023", "E-SYN-112"]);
        let again = Source::from_str("empty-definitions-again", text).check();
        p.eq("deterministic", error_codes(&again.diagnostics), codes);
    });
    p.case("empty-definitions-goalz", |p| {
        let result = Source::from_str(
            "empty-definitions-goalz",
            "\
emath function Square:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:

    goalz:
        evaluate <y>:
            produce rust.library
",
        )
        .check();
        let codes = error_codes(&result.diagnostics);
        p.demand("root-fires", codes.contains(&"E-SYN-112"), format!("root empty-block refusal must fire, got {codes:?}"));
        p.demand("honest-stays", codes.contains(&"E-NAME-023"), format!("no-definition refusal must stay, got {codes:?}"));
        p.demand("later-error-visible", codes.contains(&"E-SEC-101"), format!("later section typo must stay visible, got {codes:?}"));
        p.eq("fires-once", codes.iter().filter(|code| *code == &"E-SYN-112").count(), 1);
        p.demand("no-duplicate", !codes.contains(&"E-TYPE-002"), format!("consequent duplicate must be suppressed, got {codes:?}"));
    });
    p.case("adjacent-empty", |p| {
        let result = Source::from_str(
            "adjacent-empty",
            "\
emath function Square:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:

    goals:
",
        )
        .check();
        let codes = error_codes(&result.diagnostics);
        p.eq("each-refuses-once", codes.iter().filter(|code| *code == &"E-SYN-112").count(), 2);
        p.eq("undefined-once", codes.iter().filter(|code| *code == &"E-NAME-023").count(), 1);
        p.demand("no-junk", codes.iter().all(|c| !c.starts_with("E-SYN-101")), format!("no argument-list junk, got {codes:?}"));
    });
    p.case("bind-vs-constrain", |p| {
        // `=` in definitions binds and admits; `==` constrains and must name
        // both readings.
        Source::from_str(
            "bind-eq",
            "\
emath function Bind:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * 2
    goals:
        evaluate <y>:
            produce rust.library
",
        )
        .must_admit(p);
        let result = Source::from_str(
            "eqeq-in-defs",
            "\
emath function Constrain:
    inputs:
        x: Float64
    definitions:
        x == 5
",
        )
        .check();
        let rendered: Vec<String> = result
            .diagnostics
            .errors()
            .map(|d| format!("{} {}", d.code, d.message))
            .collect();
        let joined = rendered.join("\n");
        p.contains("code", &joined, "E-SYN-101");
        p.contains("binds", &joined, "binds with `=`");
        p.contains("constrains", &joined, "`==`");
    });
    p.case("ctor-defaults", |p| {
        let result = Source::from_str(
            "ctor-defaults",
            "\
emath policy Defaults:
    state:
        scale: Float64
    constructors:
        public fn new(scale: Float64 = 1.0) -> Self:
            Self:
                scale = scale
    definitions:
        y = state.scale * 2
",
        )
        .check();
        let codes = error_codes(&result.diagnostics);
        p.demand("parses", !codes.iter().any(|c| c.starts_with("E-SYN-")), format!("parameter defaults must parse, got {codes:?}"));
    });
    p.case("names", |p| {
        let mut dup = function_decl("Left", &["y = x"]);
        dup.push_str(&function_decl("Left", &["y = x * 2"]));
        Source::from_str("dup", &dup).must_refuse(p, &["E-NAME-022"]);
        Source::from_str("underscore", &function_decl("_", &["y = x"])).must_refuse(p, &["E-NAME-023"]);
        // Latin `a` vs Cyrillic `а` (U+0430) collide either order; a shared
        // prefix alone (`magnitude` vs `magnitude2`) is not a collision.
        let mut fwd = function_decl("magnitude", &["y = x"]);
        fwd.push_str(&function_decl("m\u{0430}gnitude", &["y = x"]));
        Source::from_str("confusable-forward", &fwd).must_refuse(p, &["E-NAME-024"]);
        let mut bwd = function_decl("m\u{0430}gnitude", &["y = x"]);
        bwd.push_str(&function_decl("magnitude", &["y = x"]));
        Source::from_str("confusable-backward", &bwd).must_refuse(p, &["E-NAME-024"]);
        let mut distinct = function_decl("magnitude", &["y = x"]);
        distinct.push_str(&function_decl("magnitude2", &["y = x"]));
        let admitted = Source::from_str("distinct", &distinct).must_admit(p);
        p.eq("both-admit", admitted.package.declarations.len(), 2);
    });
    p.case("goals-attach-by-id", |p| {
        // Goals elaborate per declaration and attach by id, never by span.
        let mut session = CompilerSession::new(Limits::default());
        let mut text = function_decl("Left", &["y = x", "y2 = x", "y3 = x"]);
        text.push_str(
            "emath function Right:\n    inputs:\n        a: Float64\n    outputs:\n        b: Float64\n    definitions:\n        b = a\n    goals:\n        evaluate <b>:\n            produce rust.library\n",
        );
        let file = session.load_text("two-decls", text);
        let plan = session.plan(file);
        p.eq("declarations", plan.package.declarations.len(), 2);
        let (left, right) = (&plan.package.declarations[0], &plan.package.declarations[1]);
        p.eq("left-goals", left.goals.len(), 3);
        p.eq("right-goals", right.goals.len(), 1);
        let right_goal = plan.package.goals.get(right.goals[0].index()).expect("Right's goal id must resolve");
        p.eq("right-target", right_goal.target.as_str(), "b");
        for (declaration, goal_ids) in
            [(left, left.goals.as_slice()), (right, right.goals.as_slice())]
        {
            for goal_id in goal_ids {
                let goal = plan.package.goals.get(goal_id.index()).expect("attached goal id must resolve");
                p.demand(
                    format!("goal-{}-inside-{}", goal.target, declaration.name.leaf()),
                    declaration.source.contains(goal.source.start),
                    format!("goal {} must lie inside its own declaration span", goal.target),
                );
            }
        }
    });
    p.case("omitted-sections", |p| {
        // `outputs:` omitted lifts the definition onto the output surface
        // with an evaluate goal; `inputs:` omitted admits constants while an
        // unknown name still refuses E-TYPE-002.
        let mut session = CompilerSession::new(Limits::default());
        let file = session.load_text(
            "greeter-omitted-outputs",
            "emath function Greeter:\n    inputs:\n        x: Float64\n    definitions:\n        y = x\n",
        );
        let plan = session.plan(file);
        p.eq("greeter-errors", error_codes(&plan.diagnostics), Vec::<&str>::new());
        p.eq("greeter-decls", plan.package.declarations.len(), 1);
        p.demand(
            "greeter-output",
            plan.package.declarations[0].outputs.iter().any(|f| f.name == "y"),
            "omitted outputs must expose definition y",
        );
        p.eq("greeter-requests", plan.requests.len(), 1);
        p.eq("greeter-kind", plan.requests[0].kind.as_str(), "evaluate");
        p.eq("greeter-target", plan.requests[0].target.as_str(), "y");
        p.eq("greeter-produce", plan.requests[0].produce.as_str(), "rust.library");
        p.demand("greeter-planned", !plan.plans.is_empty(), "evaluate goal must plan");
        let mut session = CompilerSession::new(Limits::default());
        let file = session.load_text(
            "twenty-one",
            "emath function TwentyOne:\n    definitions:\n        y = 3 * 7\n",
        );
        let plan = session.plan(file);
        p.eq("const-errors", error_codes(&plan.diagnostics), Vec::<&str>::new());
        p.demand(
            "const-no-inputs",
            plan.package.declarations[0].inputs.is_empty(),
            "constant-only declaration must have no inputs",
        );
        p.demand(
            "const-output",
            plan.package.declarations[0].outputs.iter().any(|f| f.name == "y"),
            "omitted outputs must expose definition y",
        );
        Source::from_str(
            "unknown-without-inputs",
            "emath function Bad:\n    definitions:\n        y = missing\n",
        )
        .must_refuse(p, &["E-TYPE-002"]);
    });
    p.case("defaulted-input-type", |p| {
        // Bare `inputs: x` defaults to Float64 with note N-TYPE-001; an
        // explicit `x: Float64` admits identically without the note.
        let bare = Source::from_str(
            "bare-input",
            "emath function Square:\n    inputs:\n        x\n    definitions:\n        y = x * x\n",
        )
        .must_admit(p);
        let typed = Source::from_str(
            "typed-input",
            "emath function Square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n",
        )
        .must_admit(p);
        let (bare_decl, typed_decl) = (&bare.package.declarations[0], &typed.package.declarations[0]);
        p.eq("bare-name", bare_decl.inputs[0].name.as_str(), "x");
        p.eq("typed-name", typed_decl.inputs[0].name.as_str(), "x");
        let bare_ty = bare.package.types.get(bare_decl.inputs[0].ty.index()).expect("bare input type");
        let typed_ty = typed.package.types.get(typed_decl.inputs[0].ty.index()).expect("typed input type");
        p.eq("same-type", bare_ty, typed_ty);
        p.demand("is-float64", matches!(bare_ty, emath_ir::TypeNode::Float64), format!("bare input must resolve to Float64, got {bare_ty:?}"));
        p.demand(
            "default-note",
            bare.diagnostics.items().iter().any(|d| {
                d.code == "N-TYPE-001" && d.severity == Severity::Note && d.message.contains("Float64")
            }),
            "defaulted input must emit note N-TYPE-001",
        );
        p.demand(
            "no-note-when-typed",
            typed.diagnostics.items().iter().all(|d| d.code != "N-TYPE-001"),
            "explicit Float64 must not emit the default note",
        );
    });
    p.case("head-args", |p| {
        // Head-args are identity-equivalent to `inputs:`; `-> T` declares an
        // output named after the declaration. Mixing both refuses E-SYN-122,
        // and an untyped head-arg defaults to Float64 with N-TYPE-001.
        let head = Source::from_str(
            "head-args-square",
            "emath function square(x: Float64) -> Float64:\n    definitions:\n        square = x * x\n",
        )
        .must_admit(p);
        let section = Source::from_str(
            "section-square",
            "emath function square:\n    inputs:\n        x: Float64\n    outputs:\n        square: Float64\n    definitions:\n        square = x * x\n",
        )
        .must_admit(p);
        let (head_decl, section_decl) = (&head.package.declarations[0], &section.package.declarations[0]);
        p.eq("head-input", head_decl.inputs[0].name.as_str(), "x");
        p.eq("section-input", section_decl.inputs[0].name.as_str(), "x");
        p.demand(
            "arrow-output",
            head_decl.outputs.iter().any(|f| f.name == "square"),
            "-> T must declare output named after the declaration",
        );
        p.demand("definition", head_decl.definitions.contains_key("square"), "definition square must admit");
        let head_ty = head.package.types.get(head_decl.inputs[0].ty.index()).expect("head input type");
        let section_ty = section.package.types.get(section_decl.inputs[0].ty.index()).expect("section input type");
        p.eq("same-type", head_ty, section_ty);
        p.demand("is-float64", matches!(head_ty, emath_ir::TypeNode::Float64), format!("head input must be Float64, got {head_ty:?}"));
        let untyped = Source::from_str(
            "untyped-head-args",
            "emath function square(x) -> Float64:\n    definitions:\n        square = x * x\n",
        )
        .must_admit(p);
        p.demand(
            "untyped-note",
            untyped.diagnostics.items().iter().any(|d| {
                d.code == "N-TYPE-001" && d.severity == Severity::Note && d.message.contains("Float64")
            }),
            "untyped head-arg must emit N-TYPE-001",
        );
        Source::from_str(
            "mixed-head-args",
            "emath function square(x: Float64) -> Float64:\n    inputs:\n        x: Float64\n    definitions:\n        square = x * x\n",
        )
        .must_refuse(p, &["E-SYN-122"]);
    });
    p.case("six-planes", |p| {
        // One source package parses into the intent graph with every plane;
        // the in-language example (1 * 3 == 3) must evaluate to Passed.
        Source::from_str("six-planes-eval", SIX_PLANE_SOURCE).eval_tests(p);
        let mut session = CompilerSession::new(Limits::default());
        let file = session.load_text("six-planes", SIX_PLANE_SOURCE);
        let result = session.plan(file);
        p.eq("six-plane-errors", error_codes(&result.diagnostics), Vec::<&str>::new());
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
            p.demand(kind.name(), kinds.contains(kind.name()), format!("intent graph must represent `{}`", kind.name()));
        }
        let mut second = CompilerSession::new(Limits::default());
        let second_file = second.load_text("six-planes", SIX_PLANE_SOURCE);
        let second_result = second.plan(second_file);
        let second_mig = Mig::from_package(&second_result.package);
        p.eq("canonical", mig.canonical(), second_mig.canonical());
        p.eq("identity", mig.identity(), second_mig.identity());
    });
    p.case("worked-examples", |p| {
        // An example without `expect` (or with an empty body) is a worked
        // example: it admits with expect None, never E-NAME-026.
        let worked = Source::from_str(
            "worked",
            "\
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
",
        )
        .must_admit(p);
        p.eq("worked-tests", worked.package.tests.len(), 1);
        p.demand("worked-no-expect", worked.package.tests[0].expect.is_none(), "worked example stores expect: None");
        let empty = Source::from_str(
            "empty-example",
            "\
emath function TwentyOne:
    inputs:
        n: Float64
    outputs:
        y: Float64

    definitions:
        y = 3 * 7

    tests:
        example <worked>:
",
        )
        .must_admit(p);
        p.eq("empty-tests", empty.package.tests.len(), 1);
        p.demand("empty-no-given", empty.package.tests[0].given.is_empty(), "empty body has no givens");
        p.demand("empty-no-expect", empty.package.tests[0].expect.is_none(), "empty body stores expect: None");
    });
    p.case("empty-source", |p| {
        for (name, text) in [
            ("empty", ""),
            ("whitespace", "  \n\n    \n"),
            ("comments", "# expect: E-PKG-081 empty source has no declarations\n# still nothing\n"),
            ("package-only", "package foo.bar\n"),
        ] {
            p.case(name, |p| {
                let result = Source::from_str(name, text).check();
                let codes = error_codes(&result.diagnostics);
                p.demand("e-pkg-081", codes.contains(&"E-PKG-081"), format!("{name} must refuse E-PKG-081, got {codes:?}"));
                p.eq("no-declaration", result.package.declarations.len(), 0);
            });
        }
    });
    p.case("notation", |p| {
        Source::from_str("notation-glyph", &notation_function("r = x ⊕ y")).must_admit(p);
        Source::from_str("notation-alias", &notation_function("r = x pw y")).must_admit(p);
        let qualified = Source::from_str(
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
        )
        .check();
        let codes = error_codes(&qualified.diagnostics);
        p.demand("qualified-pow", !codes.contains(&"E-TYPE-003"), format!("qualified builtin must not be unknown, got {codes:?}"));
        Source::from_str(
            "logic-not-bool",
            "\
emath function N:
    inputs:
        p: Bool
    outputs:
        q: Bool
    definitions:
        q = core::logic::not(p)
",
        )
        .must_admit(p);
        Source::from_str(
            "logic-not-float",
            "\
emath function N:
    inputs:
        p: Float64
    outputs:
        q: Bool
    definitions:
        q = core::logic::not(p)
",
        )
        .must_refuse(p, &["E-TYPE-012"]);
        Source::from_str(
            "notation-reserved",
            "\
emath function F:
    outputs:
        r: Float64
    definitions:
        r = 1.0
notation prefix 90 \"or\" => core::logic::not
",
        )
        .must_refuse(p, &["E-NOTATION-RESERVED"]);
    });
    p.case("units-profiles", |p| {
        // Publication/engineering refuse a bare quantity (E-UNIT-106) and
        // publication additionally requires provenance (E-UNIT-107);
        // permissive and the default keep admitting.
        let publication = format!("@units_profile(publication)\n{PROFILE_FIXTURE_BODY}");
        let result = Source::from_str("profile-publication-quantity", &publication).check();
        let rendered: Vec<String> = result
            .diagnostics
            .errors()
            .map(|d| format!("{} {}", d.code, d.message))
            .collect();
        let joined = rendered.join("\n");
        p.contains("e-unit-106", &joined, "E-UNIT-106");
        p.contains("names-profile", &joined, "publication");
        p.contains("e-unit-107", &joined, "E-UNIT-107");
        let with_provenance = format!(
            "@units_profile(publication)\n{PROFILE_FIXTURE_BODY}    provenance:\n        t:\n            kind: \"Assumed\"\n            reason: \"probe\"\n"
        );
        let result = Source::from_str("profile-publication-ok", &with_provenance).check();
        let codes107 = error_codes(&result.diagnostics);
        p.demand("provenance-satisfies-107", !codes107.contains(&"E-UNIT-107"), "publication with provenance must not refuse E-UNIT-107");
        Source::from_str(
            "profile-engineering",
            &format!("@units_profile(engineering)\n{PROFILE_FIXTURE_BODY}"),
        )
        .must_refuse(p, &["E-UNIT-106"]);
        Source::from_str(
            "profile-permissive",
            &format!("@units_profile(permissive)\n{PROFILE_FIXTURE_BODY}"),
        )
        .must_admit(p);
        Source::from_str("profile-default", PROFILE_FIXTURE_BODY).must_admit(p);
        let bad = Source::from_str(
            "profile-bad-level",
            &format!("@units_profile(science)\n{PROFILE_FIXTURE_BODY}"),
        )
        .check();
        let rendered: Vec<String> = bad
            .diagnostics
            .errors()
            .map(|d| format!("{} {}", d.code, d.message))
            .collect();
        let joined = rendered.join("\n");
        p.contains("bad-level-code", &joined, "E-SYN-117");
        p.contains("bad-level-names", &joined, "units_profile");
        let duplicate = Source::from_str(
            "profile-duplicate",
            &format!("@units_profile(lab)\n@units_profile(lab)\n{PROFILE_FIXTURE_BODY}"),
        )
        .check();
        let rendered: Vec<String> = duplicate
            .diagnostics
            .errors()
            .map(|d| format!("{} {}", d.code, d.message))
            .collect();
        let joined = rendered.join("\n");
        p.contains("dup-code", &joined, "E-SYN-117");
        p.contains("dup-one", &joined, "one units_profile");
        let lab_provenance = format!("@units_profile(lab)\n{PROFILE_FIXTURE_BODY}    provenance:\n        t:\n            kind: \"Assumed\"\n            reason: \"probe\"\n");
        let table = Source::from_str("profile-table", &lab_provenance).must_admit(p);
        p.eq("table", table.units_profiles, vec![("P".to_string(), "lab".to_string())]);
        let plain = Source::from_str("profile-table-empty", PROFILE_FIXTURE_BODY).must_admit(p);
        p.eq("no-rows", plain.units_profiles.len(), 0);
    });
    p.finish();
}
