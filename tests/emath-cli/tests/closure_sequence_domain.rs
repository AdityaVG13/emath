//! Closures over sequence carriers at the arrow surface (fbpb6).
//!
//! A closure literal whose DOMAIN is a sequence carrier
//! (`function v in sequence(Rat): ...`) must compute end to end, and the
//! element semantics must be checked through BOTH lanes with the same
//! discipline as the scalar Rat closures:
//!
//! - the admission lane (`admit_input_type` on the declared `A -> B`
//!   input types) must compare the closure's recorded domain against
//!   the declared domain — a scalar-domain closure must not silently
//!   satisfy a `sequence(Rat) -> Rat` slot, and an element mismatch in
//!   the wrong direction (`sequence(Rat)` closure into `sequence(Int)`
//!   slot) must refuse;
//! - the runtime lane (`apply_closure_chain`) must check the argument
//!   against the closure's recorded domain before binding it — a scalar
//!   fed to a sequence-domain closure must refuse, not silently bind.
//!
//! Failure-first: before the seam landed, all three misadmit examples
//! below RAN (the element/carrier checks did not exist), so the rows
//! that assert the named refusals failed against the pre-seam engine.
//! The two probe rows pin the already-green compute path.

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

emath function admission_misadmit:
    inputs:
        f: sequence(Rat) -> Rat
        xs: sequence(Rat)
    outputs:
        result: Rat
    definitions:
        result = f(xs)
    tests:
        example <scalar_domain_closure_must_refuse>:
            given f = function v in Rat: v
            given xs = [1 / 2, 3 / 2]
            expect diagnostic.code == type
        example <admission_refusal_names_both_domains>:
            given f = function v in Rat: v
            given xs = [1 / 2, 3 / 2]

emath function element_misadmit:
    inputs:
        g: sequence(Rat) -> Rat
        xs: sequence(Rat)
    outputs:
        result: Rat
    definitions:
        result = g(xs)
    tests:
        example <rat_elements_into_int_element_closure_must_refuse>:
            given g = function v in sequence(Int): v[0]
            given xs = [1 / 2, 3 / 2]
            expect diagnostic.code == type
        example <element_refusal_names_the_element_domain>:
            given g = function v in sequence(Int): v[0]
            given xs = [1 / 2, 3 / 2]

emath function runtime_misadmit:
    inputs:
        g: sequence(Rat) -> Rat
    outputs:
        result: Rat
    definitions:
        result = g(1 / 2)
    tests:
        example <scalar_into_sequence_closure_must_refuse>:
            given g = function v in sequence(Rat): v
            expect diagnostic.code == type
        example <runtime_refusal_names_the_argument>:
            given g = function v in sequence(Rat): v

# The ADMISSION-lane element check pinned INDEPENDENTLY of the runtime
# lane: the mismatched-element closure is never called, so only the
# given-binding check (`type_admits`'s Fn arm) can refuse it. Without
# this row, an element-blind admission mutant survives — the runtime
# lane catches every called-closure case (fbpb6 mutation evidence).
emath function uncalled_element_misadmit:
    inputs:
        g: sequence(Int) -> Int
    outputs:
        result: Int
    definitions:
        result = 7
    tests:
        example <uncalled_element_mismatch_must_refuse>:
            given g = function v in sequence(Rat): v[0]
            expect diagnostic.code == type
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

    // Green lanes: sequence-domain and sequence-codomain closures compute.
    p.contains("sequence-domain closure computes", &text, "ok head_of_sequence");
    p.contains("sequence-codomain closure computes", &text, "ok closure_returns_sequence");

    // Admission lane: a scalar-domain closure must not satisfy a
    // sequence(Rat) -> Rat slot; the refusal names both domains.
    p.contains(
        "admission lane names the domain mismatch",
        &text,
        "closure domain `Rat` does not match the declared input domain `sequence(Rat)`",
    );

    // Element semantics, wrong direction: the Rat elements cross the
    // recorded sequence(Int) element domain at the runtime lane (the
    // admission lane's Int->Rat widening lets the binding through).
    p.contains(
        "runtime element check names the element domain",
        &text,
        "closure domain `sequence(Int)` does not admit",
    );

    // The uncalled mismatched-element closure must refuse at the
    // ADMISSION lane alone — pinning the row GREEN kills an
    // element-blind admission mutant that the runtime lane cannot
    // catch (the closure is never applied).
    p.contains(
        "admission element check refuses the uncalled mismatch",
        &text,
        "ok uncalled_element_mismatch_must_refuse",
    );

    // Runtime lane: a scalar argument into a sequence-domain closure
    // must refuse instead of silently binding.
    p.contains(
        "runtime lane names the argument mismatch",
        &text,
        "closure domain `sequence(Rat)` does not admit the argument `1/2`",
    );

    // The misadmit rows (demand + message rows) must fail; the two
    // probe rows must pass.
    p.demand(
        "misadmit rows refuse",
        code != EXIT_OK as i32,
        format!("the misadmit examples must refuse, got exit {code}: {text}"),
    );

    let _ = std::fs::remove_dir_all(&dir);
    p.finish();
}
