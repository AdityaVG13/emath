//! Constructor declaration kinds. Historical `kind` / `model` / `policy`
//! names refuse. The leftover schema-registry cases stay below as comments
//! in the file history; this file now pins the live kinds.

use emath_syntax::parse_str;
use emath_test_harness::{Probe, Source, boot};

fn error_text(result: &emath_sema::admit::CheckResult) -> String {
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.to_string())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn square() -> &'static str {
    "emath function Square:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n"
}

fn checked_meaning(source: &str) -> emath_core::MeaningId {
    boot();
    let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
    let checked = session.check_owned("meaning.emath", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{}",
        error_text(&checked)
    );
    checked.package.meaning_id(&[]).unwrap()
}

fn meaning_function(name: &str, domain: &str, definitions: &str) -> String {
    format!("emath function {name}:\n    inputs:\n        n: {domain}\n    outputs:\n        result: {domain}\n    definitions:\n        {definitions}\n")
}

#[test]
fn constructor_meaning_keeps_quoted_atoms_outside_caller_scope() {
    let source = meaning_function(
        "F", "Int", "result = quote.evaluate(quote.substitute(quote(n), \"n\", 7))",
    );
    let id = checked_meaning(&source);
    assert_eq!(id, checked_meaning(&source.replace("n: Int", "m: Int")),
        "an unused ambient input does not bind the quoted atom");
    assert_ne!(id, checked_meaning(&source.replace("n: Int", "m: Int").replace("quote(n)", "quote(m)")),
        "substitution still names n, so changing the free quote atom changes execution");
    let substitution = meaning_function("F", "Int",
        "result = quote.evaluate(quote.substitute(quote.substitute(quote.substitute(quote(x - y), n, 7), \"x\", 9), \"y\", 2))");
    assert_ne!(checked_meaning(&substitution),
        checked_meaning(&substitution.replace("n: Int", "m: Int").replace(", n, 7", ", m, 7")),
        "a single-name substitution reference is literal even when it names an ambient input");
    let literal_x = substitution.replace(", n, 7", ", x, 7").replace("n: Int", "x: Int");
    assert_ne!(checked_meaning(&literal_x),
        checked_meaning(&literal_x.replace("x: Int", "y: Int").replace(", x, 7", ", y, 7")),
        "literal substitution selects x versus y, producing 5 versus 2");
    let bound = meaning_function("F", "Int",
        "result = quote.evaluate(quote.substitute(quote.bind x in Int: n, \"n\", 7))(0)");
    assert_ne!(checked_meaning(&bound),
        checked_meaning(&bound.replace("n: Int", "m: Int").replace("in Int: n", "in Int: m")),
        "quote.bind also isolates ambient values before constructing quoted syntax");
    let observed = "emath function A(n: Int) -> Int:
    n + 1
emath function Main(n: Int) -> Int:
    quote.identity(quote(A(0)))
";
    assert_ne!(checked_meaning(observed),
        checked_meaning(&observed.replace("A(", "B(")),
        "quoted call-head spellings affect the observable quote identity");
}

#[test]
fn constructor_meaning_keeps_policy_atoms_literal() {
    let policies = format!("{}{}", meaning_function("opaque", "Int", "result = n"),
        meaning_function("transparent", "Int", "result = n"));
    let target = format!("{policies}{}    exports:\n        body = opaque\n",
        meaning_function("Target", "Int", "result = n + 1"));
    let forms = format!("{}{}emath query Request:
    inputs:
        n: Int
    definitions:
        target = quote(n)
    question:
        target = target
    answer:
        form = code
        accept = unchecked
", meaning_function("code", "Int", "result = n"), meaning_function("value", "Int", "result = n"));
    let opaque_distinct = checked_meaning(&target) != checked_meaning(&target.replace("body = opaque", "body = transparent"));
    let form_distinct = checked_meaning(&forms) != checked_meaning(&forms.replace("form = code", "form = value"));
    assert!(opaque_distinct && form_distinct,
        "literal policy distinctions: opacity={opaque_distinct}, query form={form_distinct}");
    let selected = "emath query Request:
    definitions:
        target = quote(7)
    question:
        target = target
    using:
        method = closed_code
    answer:
        form = value
";
    let selection_distinct = checked_meaning(selected) !=
        checked_meaning(&selected.replace("target = quote", "other = quote").replace("target = target", "target = other"));
    let fallback = "emath function Method(n: Int) -> Int:
    n
emath query Request:
    definitions:
        a = 7
        b = 9
    question:
        target = a
    using:
        method = Method
    answer:
        form = value
        accept = unchecked
";
    let fallback_distinct = checked_meaning(fallback) !=
        checked_meaning(&fallback.replace("a = 7", "z = 7").replace("target = a", "target = z"));
    assert!(selection_distinct && fallback_distinct,
        "runtime query roles and lexically first fallback: selection={selection_distinct}, fallback={fallback_distinct}");
}

#[test]
fn constructor_meaning_separates_type_and_callable_namespaces() {
    let globals = format!("{}{}", meaning_function("Int", "Bool", "result = n"),
        meaning_function("Rat", "Bool", "result = n"));
    let source = format!("{globals}{}", meaning_function("F", "Int", "result = n"));
    let changed = format!("{globals}{}", meaning_function("F", "Rat", "result = n"));
    assert_ne!(checked_meaning(&source), checked_meaning(&changed), "declared primitive types");
    let closure = format!("{globals}{}", meaning_function("F", "Code", "result = quote(function x in Int: x)"));
    assert_ne!(checked_meaning(&closure), checked_meaning(&closure.replace("x in Int", "x in Rat")),
        "binder domains use the same type namespace, not same-shaped callables");
}

#[test]
fn constructor_meaning_retains_named_call_fallbacks_for_scalar_locals() {
    let globals = format!("{}{}", meaning_function("A", "Int", "result = n + 1"),
        meaning_function("B", "Int", "result = n + 2"));
    let root = meaning_function("F", "Int", "result = A(A)").replace("n: Int", "A: Int");
    assert_ne!(checked_meaning(&format!("{globals}{root}")),
        checked_meaning(&format!("{globals}{}", root.replace("A: Int", "B: Int").replace("A(A)", "B(B)"))),
        "a scalar local does not shadow the named callable: 2 versus 3");
    let closure = meaning_function("F", "Int", "f = function x in Int: x + 1\n        result = f(n)");
    assert_eq!(checked_meaning(&closure), checked_meaning(&closure.replace("f =", "g =").replace("f(n)", "g(n)")),
        "unambiguous local callable alpha-renaming stays presentation-only");
    let projection = meaning_function("F", "Int", "numer = function x in Int: x + 1\n        result = numer(n)");
    let projection_distinct = checked_meaning(&projection)
        != checked_meaning(&projection.replace("numer", "denom"));
    let machines = format!("{}{}", meaning_function("int_sum", "Int", "result = n"),
        meaning_function("int_prod", "Int", "result = n"));
    let machine_root = meaning_function("F", "Int", "result = int_sum(n)").replace("n: Int", "n: sequence(Int)");
    let machine_distinct = checked_meaning(&format!("{machines}{machine_root}"))
        != checked_meaning(&format!("{machines}{}", machine_root.replace("int_sum(n)", "int_prod(n)")));
    assert!(projection_distinct && machine_distinct,
        "literal call dispatch: scalar numer/denom projection={projection_distinct}, named machine head={machine_distinct}");
}

#[test]
fn constructor_meaning_resolves_callable_binder_value_domains() {
    let source = meaning_function("F", "Int",
        "c = quote(7)\n        d = quote(9)\n        result = quote.view node in c: node.value");
    let swapped = source.replace("c = quote(7)", "d = quote(7)").replace("d = quote(9)", "c = quote(9)");
    assert_ne!(checked_meaning(&source), checked_meaning(&swapped),
        "quote.view consumes the selected code value: 7 versus 9");
    assert_eq!(checked_meaning(&source),
        checked_meaning(&source.replace("c = quote(7)", "renamed = quote(7)").replace("node in c:", "node in renamed:")),
        "value-domain local references follow their lexical binding");
}

#[test]
fn constructor_meaning_binds_outputs_after_executable_definitions() {
    let globals = format!("{}{}", meaning_function("result", "Int", "result = n + 1"),
        meaning_function("other", "Int", "result = n + 2"));
    let root = meaning_function("F", "Int", "temp = result(n)\n        result = temp");
    let changed = root.replace("result: Int", "other: Int").replace("result(n)", "other(n)").replace("result = temp", "other = temp");
    assert_ne!(checked_meaning(&format!("{globals}{root}")), checked_meaning(&format!("{globals}{changed}")),
        "before the output definition, the call reaches different globals");
    let selected = "emath function F:
    inputs:
        n: Int
    outputs:
        first: Int
    definitions:
        first = n + 1
        second = n + 2
";
    assert_ne!(checked_meaning(selected), checked_meaning(&selected.replace("first: Int", "second: Int")),
        "the declared output selects a produced value, not merely its type");
    let pair = "emath function Pair:
    inputs:
        n: Int
    outputs:
        a: Int
        b: Int
    definitions:
        a = n + 1
        b = n + 2
";
    let reader = meaning_function("F", "Int", "result = Pair(n).a");
    let swapped = pair.replace("a: Int", "b: Int").replace("b: Int\n    definitions", "a: Int\n    definitions")
        .replace("a = n + 1", "b = n + 1").replace("b = n + 2", "a = n + 2");
    assert_ne!(checked_meaning(&format!("{pair}{reader}")), checked_meaning(&format!("{swapped}{reader}")),
        "multi-output values carry nominal fields: the unchanged .a reads 2 versus 3");
    let tag_reader = meaning_function("F", "Int",
        "result = quote.evaluate(quote.substitute(quote.substitute(quote.substitute(quote(Pair - Other), Pair(n), 7), \"Pair\", 9), \"Other\", 2))");
    assert_ne!(checked_meaning(&format!("{pair}{tag_reader}")),
        checked_meaning(&format!("{}{}", pair.replace("Pair:", "Other:"), tag_reader.replace("Pair(n)", "Other(n)"))),
        "multi-output records also carry the nominal function tag used by substitution");
}

#[test]
fn constructor_meaning_tracks_execution_not_spelling() {
    let base = "emath function F:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        temp = n + 1
        result = (function x in Int: (function x in Int: x + temp)(x + 2))(n)
";
    let id = checked_meaning(base);
    assert_ne!(id, checked_meaning(&base.replace("n + 1", "n + 2")), "body");
    assert_eq!(
        id,
        checked_meaning(
            &base
                .replace(" F:", " Renamed:")
                .replace("n:", "input:")
                .replace("n +", "input +")
                .replace("))(n)", "))(input)")
                .replace("temp", "local")
                .replace("result", "answer")
        ),
        "declaration and ordered local slots"
    );
    assert_eq!(
        id,
        checked_meaning(&base.replace(
            "function x in Int: (function x in Int: x + temp)(x + 2)",
            "function outer in Int: (function inner in Int: inner + temp)(outer + 2)"
        )),
        "nested shadowing"
    );
    assert_ne!(
        id,
        checked_meaning(&base.replace(
            "function x in Int: (function x in Int: x + temp)(x + 2)",
            "function outer in Int: (function inner in Int: outer + temp)(outer + 2)"
        )),
        "binder capture"
    );
    assert_eq!(
        id,
        checked_meaning(&format!(
            "# shifted spans\n{base}\n    tests:\n        expect true\n"
        )),
        "presentation/tests"
    );
    let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
    let plain = session.check_owned("content.emath", base);
    let attached = session.check_owned(
        "content.emath",
        &format!("{base}    tests:\n        expect true\n"),
    );
    assert!(!plain.diagnostics.has_errors(), "{}", error_text(&plain));
    assert!(
        !attached.diagnostics.has_errors(),
        "{}",
        error_text(&attached)
    );
    assert_ne!(
        plain.package.content_id(),
        attached.package.content_id(),
        "tests are artifact content, not mathematical meaning"
    );
    let bounded = session.check_owned(
        "bounded.emath",
        &base.replace("n: Int", "n: Int in [0, 10]"),
    );
    assert!(
        bounded
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-GONE"),
        "bounded domain is not admitted"
    );
    assert!(bounded.prepared.is_none());
    let object = "emath object Pair:
    representation:
        left: Int
        right: Int

emath function Read:
    inputs:
        p: Pair
    outputs:
        result: Int
    definitions:
        result = p.left
";
    let object_id = checked_meaning(object);
    assert_ne!(
        object_id,
        checked_meaning(&object.replace("right: Int", "right: Rat")),
        "layout type"
    );
    assert_ne!(
        object_id,
        checked_meaning(&object.replace("left", "first")),
        "nominal field labels"
    );
    let generic = "emath function Identity<T>:
    inputs:
        x: T
    outputs:
        result: T
    definitions:
        result = x
";
    assert_eq!(
        checked_meaning(generic),
        checked_meaning(&generic.replace("<T>", "<U>").replace(": T", ": U")),
        "generic alpha"
    );
    let function_type = generic.replace("<T>", "").replace(": T", ": Int -> Int");
    assert_ne!(
        checked_meaning(&function_type),
        checked_meaning(&function_type.replace("Int -> Int", "Int -> Rat")),
        "complete function type"
    );
    let sequence_type = generic.replace("<T>", "").replace(": T", ": sequence(Int)");
    assert_ne!(
        checked_meaning(&sequence_type),
        checked_meaning(&sequence_type.replace("sequence(Int)", "sequence(Rat)")),
        "complete generic argument"
    );
    assert_ne!(
        checked_meaning(base),
        checked_meaning(&format!("{base}    exports:\n        body = opaque\n")),
        "opaque policy"
    );
    let guarded = generic
        .replace("<T>", "")
        .replace(": T", ": sequence(Int)")
        .replace("result = x", "result = {k in x if k > 0}");
    assert_eq!(
        checked_meaning(&guarded),
        checked_meaning(&guarded.replace("{k in x if k > 0}", "{j in x if j > 0}")),
        "guard binder alpha"
    );
    assert_ne!(
        checked_meaning(&guarded),
        checked_meaning(&guarded.replace("k > 0", "k > 1")),
        "guard"
    );
    let query = "emath query Request:
    inputs:
        unused: Int
    definitions:
        target = quote(function x in Rat: x * x)
    question:
        target = target
        scope = unused
        assumptions = []
    answer:
        form = code
        accept = unchecked
";
    assert_ne!(
        checked_meaning(query),
        checked_meaning(&query.replace("form = code", "form = value")),
        "query answer policy"
    );
    assert_ne!(
        checked_meaning(query),
        checked_meaning(&query.replace("x * x", "x * x + 1")),
        "quoted body"
    );
}

#[test]
fn constructor_meaning_resolves_global_dependency_edges() {
    let graph = "emath function A:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = x + 1

emath function B:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = x + 2

emath function Root:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = A(n)
";
    let id = checked_meaning(graph);
    assert_eq!(
        id,
        checked_meaning(&graph.replace(" A:", " Alpha:").replace("A(n)", "Alpha(n)")),
        "resolved global alpha"
    );
    assert_ne!(
        id,
        checked_meaning(&graph.replace("A(n)", "B(n)")),
        "edge destination"
    );
    assert_ne!(
        id,
        checked_meaning(&graph.replace("x + 1", "x + 3")),
        "callee body"
    );
    let recursive = graph.replace("result = x + 1", "result = if x == 0: 0 else: A(x - 1)");
    assert_eq!(
        checked_meaning(&recursive),
        checked_meaning(&recursive.replace(" A:", " Alpha:").replace("A(", "Alpha(")),
        "recursive global alpha"
    );
    assert_ne!(
        id,
        checked_meaning(&format!("exports: Root\n{graph}")),
        "module visibility"
    );
}

#[test]
fn constructor_meaning_binds_retained_transitive_imports() {
    boot();
    let dir = std::env::temp_dir().join(format!("emath-meaning-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let root_path = dir.join("root.emath");
    let leaf_path = dir.join("leaf.emath");
    let middle_path = dir.join("middle.emath");
    let leaf = "exports: Add
emath function Add:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n + 1
";
    let middle = "use leaf.{Add as Increment}
exports: Via
emath function Via:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = Increment(n) + Hidden(n)

emath function Hidden:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n * 2
";
    let root = "use middle.{Via as Apply}
emath function Root:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = Apply(n)
";
    std::fs::write(&leaf_path, leaf).unwrap();
    std::fs::write(&middle_path, middle).unwrap();
    let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
    let checked = session.check_owned(root_path.to_str().unwrap(), root);
    assert!(
        !checked.diagnostics.has_errors(),
        "{}",
        error_text(&checked)
    );
    let retained = checked.package.meaning_id(&[]).unwrap();
    let content = checked.package.content_id();
    let alias = session.check_owned(
        root_path.to_str().unwrap(),
        &root.replace("Apply", "Renamed"),
    );
    assert!(!alias.diagnostics.has_errors(), "{}", error_text(&alias));
    assert_eq!(
        retained,
        alias.package.meaning_id(&[]).unwrap(),
        "resolved alias"
    );
    std::fs::write(&middle_path, middle.replace("n * 2", "n * 3")).unwrap();
    let private_changed = session.check_owned(root_path.to_str().unwrap(), root);
    assert!(
        !private_changed.diagnostics.has_errors(),
        "{}",
        error_text(&private_changed)
    );
    assert_ne!(
        retained,
        private_changed.package.meaning_id(&[]).unwrap(),
        "private imported helper"
    );
    std::fs::write(&middle_path, middle).unwrap();
    let direct = root.replace("use middle.{Via as Apply}", "use leaf.{Add as Apply}");
    let direct_checked = session.check_owned(root_path.to_str().unwrap(), &direct);
    assert!(
        !direct_checked.diagnostics.has_errors(),
        "{}",
        error_text(&direct_checked)
    );
    let direct_meaning = direct_checked.package.meaning_id(&[]).unwrap();
    std::fs::write(&leaf_path, leaf.replace("n + 1", "n + 2")).unwrap();
    let changed = session.check_owned(root_path.to_str().unwrap(), root);
    assert!(
        !changed.diagnostics.has_errors(),
        "{}",
        error_text(&changed)
    );
    assert_ne!(
        retained,
        changed.package.meaning_id(&[]).unwrap(),
        "transitive snapshot"
    );
    let direct_changed = session.check_owned(root_path.to_str().unwrap(), &direct);
    assert!(
        !direct_changed.diagnostics.has_errors(),
        "{}",
        error_text(&direct_changed)
    );
    assert_ne!(
        direct_meaning,
        direct_changed.package.meaning_id(&[]).unwrap(),
        "direct snapshot"
    );
    assert_ne!(
        content,
        changed.package.content_id(),
        "sealed content retains executable meaning"
    );
    assert_eq!(
        retained,
        checked.package.meaning_id(&[]).unwrap(),
        "immutable checked identity"
    );
    assert_eq!(
        content,
        checked.package.content_id(),
        "immutable checked content"
    );
    std::fs::write(&leaf_path, format!("# comments only\n{leaf}")).unwrap();
    let comments = session.check_owned(root_path.to_str().unwrap(), root);
    assert!(
        !comments.diagnostics.has_errors(),
        "{}",
        error_text(&comments)
    );
    assert_eq!(
        retained,
        comments.package.meaning_id(&[]).unwrap(),
        "dependency presentation"
    );
}

#[test]
fn live_introspection_does_not_execute_semantically_refused_source() {
    boot();
    use emath_core::FeatureId;
    use emath_sema::live_adapter::{LiveConformanceRequest, StageStatus, inspect_live_source};
    use std::collections::BTreeMap;
    let authority = BTreeMap::from([
        (
            "std.kind.function".parse::<FeatureId>().unwrap(),
            "function".into(),
        ),
        (
            "std.capability.scalar".parse::<FeatureId>().unwrap(),
            "scalar".into(),
        ),
    ]);
    let source = "emath function _:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = 4
";
    let response = inspect_live_source(LiveConformanceRequest {
        source_name: "input.emath",
        source,
        repository_commit: "abcdef",
        compiler_identity: "test-compiler",
        language_image_id: "distribution-sha256:test-image",
        authority: &authority,
    })
    .unwrap();
    assert!(
        matches!(&response.stages["admit"], StageStatus::Unavailable(reason) if reason.contains("E-NAME")),
        "introspection must not advertise refused source as admitted: {response:?}"
    );
    assert!(
        !response.result_or_diagnosis.starts_with("value:"),
        "{response:?}"
    );
}

#[test]
fn declaration_kinds() {
    boot();
    let mut p = Probe::new("constructor kinds admit; model/policy/kind refuse");
    p.case("function-ok", |p| {
        let result = Source::from_str("function-ok", square()).check();
        p.demand(
            "admit",
            !result.diagnostics.has_errors(),
            error_text(&result),
        );
        p.eq(
            "name",
            result
                .package
                .declarations
                .first()
                .map(|declaration| declaration.name.leaf().to_string()),
            Some("Square".to_string()),
        );
    });
    p.case("kind-gone", |p| {
        let source =
            "emath kind Scoring:\n    extends model\n    schema:\n        require section inputs\n";
        let (_, parse_diags) = parse_str(source);
        p.demand(
            "parse",
            !parse_diags.has_errors(),
            "kind still parses as a declaration",
        );
        Source::from_str("kind-gone", source).must_refuse(p, &["E-KIND-GONE"]);
    });
    p.case("model-gone", |p| {
        Source::from_str(
            "model-gone",
            "emath model Heat:\n    state:\n        x: Float64\n",
        )
        .must_refuse(p, &["E-KIND-GONE"]);
    });
    p.case("policy-gone", |p| {
        Source::from_str(
            "policy-gone",
            "emath policy P:\n    inputs:\n        x: Float64\n",
        )
        .must_refuse(p, &["E-KIND-GONE"]);
    });
    p.case("goals-gone", |p| {
        Source::from_str(
            "goals-gone",
            "emath function Square:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n    goals:\n        evaluate <y>:\n            produce rust.library\n",
        )
        .must_refuse(p, &["E-SEC-101"]);
    });
    // An old catalog type name refuses, unless an object the file sees,
    // its own or imported, carries it.
    p.case("catalog-type-gone", |p| {
        Source::from_str(
            "catalog-type-gone",
            "emath function Trace:\n    inputs:\n        m: Matrix\n    outputs:\n        y: Rat\n    definitions:\n        y = 1\n",
        )
        .must_refuse(p, &["E-KIND-GONE"]);
    });
    for (name, source) in [
        (
            "catalog-name-local-object",
            "emath object Matrix:\n    representation:\n        entries: sequence(Rat)\n\nemath function Trace:\n    inputs:\n        m: Matrix\n    outputs:\n        y: Rat\n    definitions:\n        y = m.entries[0]\n",
        ),
        (
            "catalog-name-imported-object",
            "use search.tensor_rank\n\nemath function Rank:\n    inputs:\n        t: Tensor\n    outputs:\n        y: Int\n    definitions:\n        y = length(standard(t))\n",
        ),
    ] {
        p.case(name, |p| {
            let result = Source::from_str(name, source).check();
            p.demand(
                "admit",
                !result.diagnostics.has_errors(),
                error_text(&result),
            );
        });
    }
    p.case("wrong-section-reported-once", |p| {
        let result = Source::from_str(
            "wrong-section-reported-once",
            "emath function F:\n    inputs:\n        x: Int\n    outputs:\n        y: Int\n    state:\n        s: Int\n    definitions:\n        y = x\n",
        )
        .check();
        let count = result
            .diagnostics
            .errors()
            .filter(|diagnostic| diagnostic.code == "E-SEC-101")
            .count();
        p.eq("one E-SEC-101 per defect", count, 1);
    });
    p.finish();
}
