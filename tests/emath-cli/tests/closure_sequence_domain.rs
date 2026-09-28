//! Closures over sequence carriers at the arrow surface (fbpb6).
//!
//! A closure literal whose DOMAIN is a sequence carrier
//! (`function v in sequence(Rat): ...`) computes end to end. Admission
//! compares closure signatures structurally: a scalar-domain closure
//! does not fill a `sequence(Rat) -> Rat` slot, the domain is checked
//! contravariantly (element types included), the codomain covariantly,
//! and a scalar argument does not fit a sequence-domain closure. Each
//! mismatch is a named E-TYPE-012 refusal, even when the closure is
//! never called.

mod common;
use emath_cli::EXIT_OK;
use emath_test_harness::Probe;

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("emath-fbpb6-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn seam_module(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("closure_sequence_domain.emath");
    std::fs::write(
        &path,
        r#"# fbpb6: closures over sequence carriers at the arrow surface.

emath function probe_seq_domain:
    inputs:
        f: sequence(Rat) -> Rat
        xs: sequence(Rat)
    outputs:
        result: Rat
    definitions:
        result = f(xs)
    tests:
        example <head_of_sequence>:
            given f = function v in sequence(Rat): v[0]
            given xs = [1 / 2, 3 / 2]
            expect result == 1 / 2

emath function probe_seq_codomain:
    inputs:
        f: Rat -> sequence(Rat)
        x: Rat
    outputs:
        result: sequence(Rat)
    definitions:
        result = f(x)
    tests:
        example <closure_returns_sequence>:
            given f = function v in Rat: [v + v, ..[]]
            given x = 1 / 2
            expect result == [1 / 1]
"#,
    )
    .expect("write seam module");
    path
}

#[test]
fn probe() {
    let mut p = Probe::new("closures over sequence carriers compute and check elements");
    let dir = scratch_dir("seam");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let source = seam_module(&dir);

    let source_string = source.to_string_lossy().into_owned();

    // The surface itself must stay ADMITTED: the seam is a value-level
    // element check, not a refusal of the sequence-closure spelling.
    let (check_text, check_code) = common::cli(&["check", &source_string]);
    p.eq("check exit", check_code, EXIT_OK as i32);
    p.demand(
        "check must not refuse the sequence-closure surface",
        !check_text.contains("E-TYPE-012"),
        format!("admission refused the sequence-closure surface: {check_text}"),
    );

    let (text, code) = common::cli(&["test", &source_string]);
    p.eq("test exit", code, EXIT_OK as i32);
    p.contains("sequence-domain closure computes", &text, "ok head_of_sequence");
    p.contains("sequence-codomain closure computes", &text, "ok closure_returns_sequence");

    let misadmits = [
        (
            "scalar-domain closure into a sequence slot",
            "emath function bad(f: sequence(Rat) -> Rat, xs: sequence(Rat)) -> Rat:\n    f(xs)\n    tests:\n        example <e>:\n            given f = function v in Rat: v\n            given xs = [1 / 2]\n            expect result == 1 / 2\n",
            "declared (sequence(Rat) -> Rat), got (Rat -> Rat)",
        ),
        (
            "Int-element domain cannot take Rat elements",
            "emath function bad(g: sequence(Rat) -> Rat, xs: sequence(Rat)) -> Rat:\n    g(xs)\n    tests:\n        example <e>:\n            given g = function v in sequence(Int): v[0]\n            given xs = [1 / 2]\n            expect result == 1 / 2\n",
            "declared (sequence(Rat) -> Rat), got (sequence(Int) -> Int)",
        ),
        (
            "uncalled closure with a Rat codomain into an Int slot",
            "emath function bad(g: sequence(Int) -> Int) -> Int:\n    7\n    tests:\n        example <e>:\n            given g = function v in sequence(Rat): v[0]\n            expect result == 7\n",
            "declared (sequence(Int) -> Int), got (sequence(Rat) -> Rat)",
        ),
        (
            "scalar argument into a sequence-domain closure",
            "emath function bad(g: sequence(Rat) -> Rat) -> Rat:\n    g(1 / 2)\n",
            "expected sequence(Rat), got Rat",
        ),
    ];
    for (index, (label, program, fragment)) in misadmits.into_iter().enumerate() {
        let path = dir.join(format!("misadmit_{index}.emath"));
        std::fs::write(&path, program).expect("write misadmit module");
        let (text, code) = common::cli(&["check", &path.to_string_lossy()]);
        p.demand(label, code != EXIT_OK as i32 && text.contains("E-TYPE-012"), text.clone());
        p.contains(label, &text, fragment);
    }

    let _ = std::fs::remove_dir_all(&dir);
    p.finish();
}
