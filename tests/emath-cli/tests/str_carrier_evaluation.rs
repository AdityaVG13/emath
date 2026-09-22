//! Str payload evaluation at the constructor seam (e6gvs).
//!
//! A Str literal parses and admits but did not EVALUATE: the constructor
//! seam refused `implementation_unavailable: constructor seam does not
//! evaluate Str("alpha")`, forcing temporal/memory consumers into Int-id
//! carriers with host-side id tables. The seam makes Str a first-class
//! value carrier through admit / eval / emit:
//!
//! - eval: a Str literal evaluates to the Str value; record fields carry
//!   it; structural equality (`==`/`!=`) works on it;
//! - admit: a declared `key: Str` input is a CHECKED carrier — an Int
//!   given where Str is declared refuses, and the ordering operators
//!   refuse at admission (`Str carries structural equality only`) so the
//!   admit lane agrees with the runtime's `type` fault;
//! - emit: a Str carrier converts to the host Text value (cvalue_to_emir).
//!
//! Carrier semantics ONLY: no ordering, no concatenation, no
//! length/indexing — the carrier first, the string LIBRARY never (the
//! recorded stance: that boundary is deliberate).
//!
//! Failure-first: before the seam, the bump/key rows below faulted with
//! the implementation_unavailable refusal, and the two refusal files
//! below ADMITTED cleanly (the checks this test pins as refusals were
//! silent misadmits).

mod common;
use emath_cli::EXIT_OK;
use emath_test_harness::Probe;

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("emath-e6gvs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Temporal-style keyed state with a Str key — no id table.
fn keyed_module(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("str_carrier.emath");
    std::fs::write(
        &path,
        r#"# e6gvs: Str payloads evaluate end to end.

emath object Versioned:
    representation:
        key: Str
        revision: Int

emath function bump:
    inputs:
        v: Versioned
    outputs:
        result: Versioned
    definitions:
        result = Versioned:{key: v.key, revision: v.revision + 1}
    tests:
        example <same_key_after_bump>:
            given v = Versioned:{key: "alpha", revision: 3}
            expect result.key == "alpha"
        example <revision_advances>:
            given v = Versioned:{key: "alpha", revision: 3}
            expect result.revision == 4

emath function equal_keys:
    inputs:
        a: Str
        b: Str
    outputs:
        result: Bool
    definitions:
        result = a == b
    tests:
        example <equal_strs>:
            given a = "alpha"
            given b = "alpha"
            expect result == true
        example <different_strs>:
            given a = "alpha"
            given b = "beta"
            expect result == false

emath function keyed_state:
    inputs:
        k: Str
        rev: Int
    outputs:
        result: Versioned
    definitions:
        result = Versioned:{key: k, revision: rev}
    tests:
        example <str_key_carries>:
            given k = "2026-09"
            given rev = 2
            expect result.key == "2026-09"
            expect result.revision == 2
"#,
    )
    .expect("write keyed module");
    path
}

/// An Int given where `Str` is declared: the admit lane's carrier check
/// must refuse (mutation probe B kills the `ctype_from_type` Str arm).
fn wrong_carrier_module(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("str_carrier_checked.emath");
    std::fs::write(
        &path,
        r#"emath function str_input_carrier_checked:
    inputs:
        k: Str
    outputs:
        result: Int
    definitions:
        result = 5
    tests:
        example <int_given_where_str_declared_must_refuse>:
            given k = 3
            expect result == 5
"#,
    )
    .expect("write carrier-checked module");
    path
}

/// Ordering on Str: structural equality only — the ordering operators
/// refuse at admission (mutation probe B also kills through here).
fn ordering_module(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("str_no_ordering.emath");
    std::fs::write(
        &path,
        r#"emath function no_ordering:
    inputs:
        a: Str
        b: Str
    outputs:
        result: Bool
    definitions:
        result = a < b
"#,
    )
    .expect("write ordering module");
    path
}

#[test]
fn probe() {
    let mut p = Probe::new("Str payloads evaluate end to end and refuse out-of-scope ops");
    let dir = scratch_dir("seam");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let keyed = keyed_module(&dir);
    let keyed_string = keyed.to_string_lossy().into_owned();

    // Green lanes: the Str payload computes through record fields,
    // structural equality, and keyed state construction.
    let (text, code) = common::cli(&["test", &keyed_string]);
    p.eq("test exit", code, EXIT_OK as i32);
    p.contains("bump keeps the Str key", &text, "ok same_key_after_bump");
    p.contains("revision advances", &text, "ok revision_advances");
    p.contains("structural equality holds", &text, "ok equal_strs");
    p.contains("structural equality discriminates", &text, "ok different_strs");
    p.contains("str key carries into state", &text, "ok str_key_carries");

    // Admit lane, carrier check: an Int given where Str is declared
    // refuses with the named message.
    let wrong = wrong_carrier_module(&dir);
    let wrong_string = wrong.to_string_lossy().into_owned();
    let (check_text, check_code) = common::cli(&["check", &wrong_string]);
    p.demand(
        "wrong-carrier given refuses",
        check_code != EXIT_OK as i32,
        format!("an Int given for a `Str` input must refuse, got exit {check_code}: {check_text}"),
    );
    p.contains(
        "wrong-carrier refusal names the given",
        &check_text,
        "does not have the declared input type",
    );

    // Admit lane, boundary: ordering on Str refuses at admission — the
    // carrier exists without the string library.
    let ordering = ordering_module(&dir);
    let ordering_string = ordering.to_string_lossy().into_owned();
    let (order_text, order_code) = common::cli(&["check", &ordering_string]);
    p.demand(
        "ordering refuses",
        order_code != EXIT_OK as i32,
        format!("Str ordering must refuse, got exit {order_code}: {order_text}"),
    );
    p.contains("ordering refusal names the boundary", &order_text, "E-TYPE-012");
    p.contains(
        "ordering refusal names the carrier stance",
        &order_text,
        "structural equality only",
    );

    let _ = std::fs::remove_dir_all(&dir);
    p.finish();
}
