//! `emath export lean` end-to-end on a small project folder: a chain
//! model, its closed form, a file importing both, and a file with one
//! declaration Lean cannot take (quoted code) plus one that depends on
//! it.
//!
//! Without `--check` the export only writes the Lean project for
//! review: every translatable declaration is written, the two refused
//! ones are named with their reason (exit 3), Lean never runs, and the
//! model's Lean text matches the reviewed golden
//! `tests/fixtures/export_lean/golden/model.lean`. A folder holding a
//! file emath refuses exports nothing (exit 2). With `--check`
//! Lean's kernel checks every authored example: the chain folder is
//! accepted, and the same folder with one wrong expected value is
//! rejected naming that example. `mixed.emath` joins Int and Rat where
//! Lean needs one type (an `if`, a list, a cons, a fold's start), and
//! `remainder.emath` calls `int_rem` beside its own `int_rem` and
//! `exact.modular`'s guarded one, which emath never runs for that call;
//! the whole folder must still check. Its example that faults in emath
//! (a remainder by 0) is not exported, though Lean's total `%` would
//! accept it. A function with several outputs becomes a Lean structure
//! whose fields its examples and callers read. The `--check` half needs `lake`
//! on PATH.
//!
//! Laws: with `law/certify.emath` beside the chain, its five
//! `certify_equal` calls and one `certify_all` call become theorems.
//! n / 0 against 0 is proved by Lean's total division but reported
//! unproven, since emath (where it faults) does not certify it.
//! The chain against its closed form at one point, the running sum
//! against n(n - 1)/2, the powers of 1/2 summed from t = 1, and the
//! chain against its closed form at every
//! parameter point the closed form computes at are proven; a quotient
//! by a sequence of n is reported unproven (exit 3), not rejected. A
//! closed form at the wrong point, or with its parameters swapped, is
//! rejected with where the sides differ (exit 1). Exported again
//! without the laws into the same folder, the chain checks without
//! Mathlib. This half also needs prebuilt Mathlib.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::emath_bin;
use emath_cli::{EXIT_ADMISSION, EXIT_OK, EXIT_PARTIAL, EXIT_REFUSED};
use emath_test_harness::Probe;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn lean_translation_consumes_the_checked_syntax() {
    use emath_core::parse::{SourceParser, register_source_parser};
    use emath_core::{Diagnostics, Edition, FileId, limits::Limits, tree::SyntaxTree};
    use std::sync::OnceLock;
    static HELPER: OnceLock<PathBuf> = OnceLock::new();
    const SOURCE: &str = "emath function Answer:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = 7
";
    struct CheckedSyntax;
    impl SourceParser for CheckedSyntax {
        fn parse(&self, text: &str, file: FileId, limits: &Limits, edition: Edition) -> (SyntaxTree, Diagnostics) {
            if text.starts_with("# edit shared import\n") {
                let helper = HELPER.get().unwrap();
                let old = std::fs::read_to_string(helper).unwrap();
                std::fs::write(helper, old.replace("result = 1", "result = 2")).unwrap();
            }
            let text = if text == SOURCE { text.replace("result = 7", "result = 42") } else { text.into() };
            emath_syntax::parse_with_edition(&text, file, limits, edition)
        }
    }
    if std::env::var_os("EMATH_LEAN_SYNTAX_CHILD").is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "lean_translation_consumes_the_checked_syntax", "--nocapture"])
            .env("EMATH_LEAN_SYNTAX_CHILD", "1")
            .output().unwrap();
        assert!(output.status.success(), "{output:?}");
        return;
    }
    let dir = std::env::temp_dir().join(format!("emath-lean-syntax-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("input.emath");
    let out = dir.join("lean");
    std::fs::write(&source, SOURCE).unwrap();
    register_source_parser(&CheckedSyntax);
    assert_eq!(emath_cli::run(&[
        "export".into(), "lean".into(), source.display().to_string(), "--out".into(), out.display().to_string(),
    ]), EXIT_OK);
    let lean = std::fs::read_to_string(out.join("Export/input.lean")).unwrap();
    assert!(lean.contains("42"), "Lean must translate the syntax consumed by checking, not an independent parse: {lean}");

    let project = dir.join("multiple_roots");
    std::fs::create_dir_all(&project).unwrap();
    let helper = project.join("helper.emath");
    HELPER.set(helper.clone()).unwrap();
    std::fs::write(&helper, format!("exports: Shared\n{}", SOURCE.replace("Answer", "Shared").replace("result = 7", "result = 1"))).unwrap();
    std::fs::write(project.join("bridge.emath"), "exports: Forward
use helper
emath function Forward:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = Shared(unused)
").unwrap();
    let caller = "use bridge
emath function Caller:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = Forward(unused)
";
    std::fs::write(project.join("a.emath"), caller.replace("Caller", "First")).unwrap();
    std::fs::write(project.join("b.emath"), caller.replace("Caller", "Second")).unwrap();
    let stable = dir.join("stable");
    assert_eq!(emath_cli::run(&[
        "export".into(), "lean".into(), project.display().to_string(), "--out".into(), stable.display().to_string(),
    ]), EXIT_OK, "multiple roots may share one unchanged dependency");
    std::fs::write(project.join("b.emath"), format!("# edit shared import\n{}", caller.replace("Caller", "Second"))).unwrap();
    let mixed = dir.join("mixed");
    assert_eq!(emath_cli::run(&[
        "export".into(), "lean".into(), project.display().to_string(), "--out".into(), mixed.display().to_string(),
    ]), EXIT_ADMISSION, "a later root must not mix newly checked transitive imports with an earlier Lean translation");
    assert!(!mixed.exists(), "a mixed-snapshot export must write nothing");
}

fn emath(dir: &Path, args: &[&str]) -> (String, i32) {
    let out = Command::new(emath_bin())
        .args(args)
        .current_dir(dir)
        .env("EMATH_ROOT", workspace())
        .output()
        .expect("run emath");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (text, out.status.code().unwrap_or(-1))
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read fixture dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy fixture");
        }
    }
}

fn lake_available() -> bool {
    Command::new("lake").arg("--version").output().is_ok_and(|out| out.status.success())
}

/// The shared Mathlib packages `emath export lean` uses, when already
/// prebuilt: the test must not start a multi-gigabyte download.
fn prebuilt_mathlib() -> Option<PathBuf> {
    let dir = std::env::var_os("EMATH_LEAN_MATHLIB")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache/emath/lean-mathlib")))?;
    dir.join("mathlib/.lake/build/lib/lean/Mathlib/LinearAlgebra/Charpoly/Basic.olean")
        .is_file()
        .then_some(dir)
}

#[cfg(unix)]
#[test]
fn mathlib_setup_is_shared_and_unchanged_exports_are_reused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("emath-lean-shared-{}", std::process::id()));
    let bin = dir.join("bin");
    let packages = dir.join("packages");
    std::fs::create_dir_all(&bin).unwrap();
    let git = bin.join("git");
    std::fs::write(&git, "#!/bin/sh\nprintf '%s\\n' d13f23b723b8a846827a245b89c10fc7d3f11612\n").unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Lake is substituted only in these child processes. This test pins
    // cache placement/reuse, not kernel acceptance (the real gate below does).
    let lake = bin.join("lake");
    std::fs::write(&lake, r#"#!/bin/sh
case "$*" in
  "build --no-build "*)
    test -f "$EMATH_TEST_PACKAGES/ready"
    ;;
  "exe cache get"*)
    printf '%s\n' "$PWD" >> "$EMATH_TEST_LOG"
    test "$PWD/.lake/packages" -ef "$EMATH_TEST_PACKAGES" || exit 9
    mkdir -p "$EMATH_TEST_PACKAGES/mathlib"
    printf 'leanprover/lean4:v4.34.1\n' > "$EMATH_TEST_PACKAGES/mathlib/lean-toolchain"
    printf ready > "$EMATH_TEST_PACKAGES/ready"
    ;;
  "build")
    test -f "$EMATH_TEST_PACKAGES/ready"
    ;;
  *) exit 8 ;;
esac
"#).unwrap();
    std::fs::set_permissions(&lake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let source = workspace().join("docs/guide/examples/sequences");
    let log = dir.join("fetches");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let invoke = |out: &Path| {
        Command::new(emath_bin())
            .args(["export", "lean", &source.to_string_lossy(), "--out", &out.to_string_lossy(), "--check"])
            .env("EMATH_LEAN_MATHLIB", &packages)
            .env("EMATH_TEST_PACKAGES", &packages)
            .env("EMATH_TEST_LOG", &log)
            .env("PATH", &path)
            .output().unwrap()
    };
    let check = |out: &Path| {
        let output = invoke(out);
        assert!(output.status.success(), "shared Mathlib setup failed: {output:?}");
    };
    let first = dir.join("first");
    let second = dir.join("second");
    check(&first);
    let tracked = ["lakefile.lean", "lake-manifest.json", "lean-toolchain", "Emath/Law.lean", "Export/laws.lean"];
    let before: Vec<_> = tracked.iter().map(|file| {
        std::fs::metadata(first.join(file)).unwrap().modified().unwrap()
    }).collect();
    check(&first);
    check(&second);
    let after: Vec<_> = tracked.iter().map(|file| {
        std::fs::metadata(first.join(file)).unwrap().modified().unwrap()
    }).collect();
    assert_eq!(before, after, "unchanged export files must keep incremental build inputs");
    let fetches = std::fs::read_to_string(&log).unwrap();
    assert_eq!(fetches.lines().count(), 1, "one shared setup, not a fetch per export: {fetches}");
    let setup = PathBuf::from(fetches.trim());
    assert!(setup.canonicalize().unwrap().starts_with(packages.canonicalize().unwrap()), "setup must not belong to an individual export");
    std::fs::write(packages.join("mathlib/lean-toolchain"), "leanprover/lean4:v4.32.0\n").unwrap();
    let refused = invoke(&second);
    let message = String::from_utf8_lossy(&refused.stdout);
    assert!(!refused.status.success() && message.contains("v4.32.0") && message.contains("v4.34.1"), "an incompatible installation must be refused without changing it: {refused:?}");
    assert_eq!(std::fs::read_to_string(packages.join("mathlib/lean-toolchain")).unwrap(), "leanprover/lean4:v4.32.0\n");
    assert_eq!(std::fs::read_to_string(&log).unwrap(), fetches, "an incompatible installation must not trigger fetching");
}

#[test]
fn export_lean() {
    let mut probe = Probe::new("emath export lean: convert for review, then check with Lean");
    let dir = std::env::temp_dir().join(format!("emath_export_lean_{}", std::process::id()));
    let fixtures = workspace().join("tests/fixtures/export_lean");
    copy_tree(&fixtures.join("proj"), &dir.join("proj"));

    let (text, code) = emath(&dir, &["export", "lean", "proj"]);
    probe.demand("folder-partial", code == EXIT_PARTIAL as i32, format!("exit {code}\n{text}"));
    probe.demand(
        "refusals-named",
        text.contains("not translated: refused.emath `code`: `quote`")
            && text.contains("not translated: refused.emath `uses_code`: calls `code`"),
        text.clone(),
    );
    let lean = dir.join("proj/lean");
    for file in ["lakefile.lean", "lean-toolchain", "Emath/Prelude.lean", "Export/refused.lean", "Export/chain/both.lean"] {
        probe.demand(&format!("wrote-{file}"), lean.join(file).is_file(), text.clone());
    }
    let refused = std::fs::read_to_string(lean.join("Export/refused.lean")).unwrap_or_default();
    probe.demand(
        "translated-beside-refused",
        refused.contains("def square") && !refused.contains("def code") && !refused.contains("def uses_code"),
        refused,
    );
    probe.demand(
        "several-outputs-translate",
        !text.contains("mixed.emath `divmod`") && !text.contains("mixed.emath `recombine`"),
        text.clone(),
    );
    probe.demand("recursion-translates", !text.contains("recursive.emath"), text.clone());
    probe.demand(
        "objects-and-library-functions-translate",
        text.lines().filter(|line| line.contains("objects.emath")).collect::<Vec<_>>()
            == [
                "not translated: objects.emath `twice_weight`: a `Rat` value stands where `Int` is declared, which Lean cannot hold",
                "not translated: objects.emath `prime_sum`: calls `SumPrimesBelow` from `discrete.sieve`, which does not translate: `buffer` has no Lean translation",
            ]
            && lean.join("Export/lib/fold/reduce.lean").is_file(),
        text.clone(),
    );
    probe.demand("lean-not-run", !lean.join(".lake").exists(), text.clone());
    let model = std::fs::read_to_string(lean.join("Export/chain/model.lean")).unwrap_or_default();
    let golden = std::fs::read_to_string(fixtures.join("golden/model.lean")).unwrap_or_default();
    probe.demand("model-matches-golden", model == golden, model);

    // A file emath refuses exports nothing, though each declaration in
    // it would translate: here an import collides with a local name.
    let unadmitted = dir.join("unadmitted");
    std::fs::create_dir_all(&unadmitted).expect("create unadmitted");
    std::fs::write(
        unadmitted.join("clash.emath"),
        "use exact.modular\n\nemath function int_rem:\n    inputs:\n        value: Int\n        modulus: Int\n    outputs:\n        result: Int\n    definitions:\n        result = 5\n",
    )
    .expect("write clash");
    let (text, code) = emath(&dir, &["export", "lean", "unadmitted"]);
    probe.demand(
        "unadmitted-file-refused",
        code == EXIT_ADMISSION as i32 && text.contains("does not admit: E-NAME-022") && !unadmitted.join("lean").exists(),
        format!("exit {code}\n{text}"),
    );

    // An example past emath's default work budget is not a fault: the
    // refusal says so, and `--work` lets it through, as in `emath test`.
    let heavy = dir.join("heavy");
    std::fs::create_dir_all(&heavy).expect("create heavy");
    std::fs::write(
        heavy.join("loop.emath"),
        "use fold.reduce\n\nemath function loop_sum:\n    inputs:\n        n: Int\n    outputs:\n        result: Int\n    definitions:\n        result = fold(function acc in Int: function k in Int: acc + k, 0, range(0, n))\n    tests:\n        example <big>:\n            given n = 200000\n            expect result == 19999900000\n",
    )
    .expect("write heavy");
    let (text, code) = emath(&dir, &["export", "lean", "heavy"]);
    probe.demand(
        "budget-named-with-work",
        code == EXIT_PARTIAL as i32 && text.contains("example <big> of `loop_sum`: it runs out of emath's work budget") && text.contains("--work"),
        format!("exit {code}\n{text}"),
    );
    let (text, code) = emath(&dir, &["export", "lean", "heavy", "--work", "100000000"]);
    probe.demand(
        "work-admits-heavy-example",
        code == EXIT_OK as i32
            && std::fs::read_to_string(heavy.join("lean/Export/loop.lean")).is_ok_and(|lean| lean.contains("19999900000")),
        format!("exit {code}\n{text}"),
    );

    if lake_available() {
        // `mixed.emath` joins Int and Rat in an `if`, a list, a cons and
        // a fold's start, as emath does; Lean must accept the same file.
        let (text, code) = emath(&dir, &["export", "lean", "proj", "--check"]);
        probe.demand(
            "folder-accepted-beside-refusals",
            code == EXIT_PARTIAL as i32
                && text.contains("lean: accepted")
                && text.contains("not translated: remainder.emath example <no_length> of `cycle`: it faults in emath"),
            format!("exit {code}\n{text}"),
        );

        let out = dir.join("checked");
        let out_arg = out.to_string_lossy().to_string();
        let (text, code) = emath(&dir, &["export", "lean", "proj/chain", "--out", &out_arg, "--check"]);
        probe.demand("chain-accepted", code == EXIT_OK as i32 && text.contains("lean: accepted"), format!("exit {code}\n{text}"));

        let certify = dir.join("proj/chain/certify.emath");
        std::fs::copy(fixtures.join("law/certify.emath"), &certify).expect("copy certify");
        // Exported for review, then built by hand where Mathlib is not
        // prebuilt: Lake must refuse before it clones or compiles Mathlib.
        let bare = dir.join("bare");
        let missing = dir.join("no-mathlib");
        let exported = Command::new(emath_bin())
            .args(["export", "lean", "proj/chain", "--out", &bare.to_string_lossy()])
            .current_dir(&dir)
            .env("EMATH_ROOT", workspace())
            .env("EMATH_LEAN_MATHLIB", &missing)
            .status()
            .expect("run emath");
        // Git may not reach the network here, so a missing guard fails
        // this check rather than download and compile Mathlib.
        let built = Command::new("lake")
            .arg("build")
            .current_dir(&bare)
            .env("GIT_ALLOW_PROTOCOL", "file")
            .output()
            .expect("run lake");
        let text = format!("{exported}\n{}{}", String::from_utf8_lossy(&built.stdout), String::from_utf8_lossy(&built.stderr));
        probe.demand(
            "bare-lake-refuses-unbuilt-mathlib",
            exported.success() && !built.status.success() && text.contains("Mathlib is not prebuilt") && !missing.exists(),
            text,
        );

        if let Some(mathlib) = prebuilt_mathlib() {
            let laws = dir.join("laws");
            let laws_arg = laws.to_string_lossy().to_string();
            let check = || {
                let out = Command::new(emath_bin())
                    .args(["export", "lean", "proj/chain", "--out", &laws_arg, "--check"])
                    .current_dir(&dir)
                    .env("EMATH_ROOT", workspace())
                    .env("EMATH_LEAN_MATHLIB", &mathlib)
                    .output()
                    .expect("run emath");
                let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                (text, out.status.code().unwrap_or(-1))
            };
            let (text, code) = check();
            probe.demand(
                "law-proven-beside-unproven",
                code == EXIT_PARTIAL as i32
                    && text.contains("lean: not proven")
                    && text.contains("unproven: certify.emath law 3 of `certified`: Lean found no order bound")
                    && text.contains("unproven: certify.emath law 5 of `certified`: Lean proves it, but emath does not certify it")
                    && !text.contains("law 1 of")
                    && !text.contains("law 2 of")
                    && !text.contains("law 4 of")
                    && !text.contains("of `certify`")
                    && !text.contains("not translated")
                    && !text.contains("rejected:"),
                format!("exit {code}\n{text}"),
            );
            let laws_lean = std::fs::read_to_string(laws.join("Export/certify.lean")).unwrap_or_default();
            probe.demand(
                "all-law-hypothesis-is-the-divisor",
                laws_lean.contains("(h₁ : 1 - (a - b) ≠ 0)\n    : ∀ n₀ : ℕ,"),
                laws_lean,
            );
            let source = std::fs::read_to_string(&certify).expect("read certify");
            let wrong = source
                .replace("mean_closed(1 / 2, 1 / 4, n)", "mean_closed(1 / 2, 1 / 5, n)")
                .replace("mean_closed(a, b, n)", "mean_closed(b, a, n)");
            std::fs::write(&certify, wrong).expect("write wrong laws");
            let (text, code) = check();
            probe.demand(
                "wrong-laws-rejected",
                code == EXIT_REFUSED as i32
                    && text.contains("rejected: certify.emath law 1 of `certified`: the two sides differ at n = ")
                    && text.contains(
                        "rejected: certify.emath law 1 of `certified_all`: the two sides differ at a parameter point the hypotheses allow",
                    ),
                format!("exit {code}\n{text}"),
            );
            // Without its laws the folder needs no Mathlib, even where
            // the last export left `Law.lean` and the laws' Lean file.
            std::fs::remove_file(&certify).expect("remove certify");
            let (text, code) = emath(&dir, &["export", "lean", "proj/chain", "--out", &laws_arg, "--check"]);
            probe.demand(
                "stale-laws-not-built",
                code == EXIT_OK as i32 && text.contains("lean: accepted"),
                format!("exit {code}\n{text}"),
            );
        } else {
            eprintln!("export_lean: no prebuilt Mathlib (EMATH_LEAN_MATHLIB or ~/.cache/emath/lean-mathlib); the law half did not run");
        }

        let found = dir.join("proj/chain/found.emath");
        let source = std::fs::read_to_string(&found).expect("read found");
        std::fs::write(&found, source.replace("expect result == 7 / 8", "expect result == 7 / 9")).expect("write wrong");
        let (text, code) = emath(&dir, &["export", "lean", "proj/chain", "--out", &out_arg, "--check"]);
        probe.demand(
            "wrong-value-rejected",
            code == EXIT_REFUSED as i32 && text.contains("lean: rejected") && text.contains("two_steps"),
            format!("exit {code}\n{text}"),
        );
    } else {
        eprintln!("export_lean: `lake` is not on PATH; the --check half did not run");
    }
    probe.finish();
}

/// Settled witness claims export to Lean (bead emath-nwvmi): the claim's
/// certificate and its authored checker, translated from the `.emath`
/// definitions the library uses, stated as checker-accepts and
/// discharged by the kernel (`decide +kernel`, never `native_decide`).
/// Only settled claims export: a vm-only claim and a claim of another
/// settled kind are refused by name while the rest still exports, and
/// a claim whose checker rejects its certificate (the forged Barker
/// receipt, `claim_6`) refuses the export outright, naming the claim,
/// with nothing written.
#[test]
fn export_lean_claims() {
    let mut probe = Probe::new("emath export lean: settled witness claims export and the kernel proves them");
    let out = std::env::temp_dir().join(format!("emath_lean_claims_{}", std::process::id()));
    let out_arg = out.to_string_lossy().to_string();
    let claims = workspace().join("tests/fixtures/verify/claims.emath").to_string_lossy().to_string();

    // The settled witness claim exports as a theorem the kernel
    // decides; the settled-but-not-witness bound and exhaustion
    // claims are refused by name (only construction-witness claims
    // export to Lean today); the rest of the file still translates.
    let (text, code) = emath(&workspace(), &["export", "lean", &claims, "--out", &out_arg]);
    probe.demand(
        "witness-claim-exports",
        code == EXIT_PARTIAL as i32
            && std::fs::read_to_string(out.join("Export/claims.lean")).is_ok_and(|lean| {
                lean.contains("theorem witness_claim_settled")
                    && lean.contains("decide +kernel")
                    && lean.contains("is_pm_one")
            }),
        format!("exit {code}\n{text}"),
    );
    probe.demand(
        "non-exported-claims-named",
        text.contains("not exported: claims.emath `certified_bound_claim`")
            && text.contains("not exported: claims.emath `exhaustion_claim`"),
        format!("exit {code}\n{text}"),
    );

    // The kernel proves the exported claim: `--check` accepts the
    // project with the claim theorem in it.
    if lake_available() {
        let out_checked = std::env::temp_dir().join(format!("emath_lean_claims_check_{}", std::process::id()));
        let checked_arg = out_checked.to_string_lossy().to_string();
        let (text, code) = emath(&workspace(), &["export", "lean", &claims, "--out", &checked_arg, "--check"]);
        probe.demand(
            "kernel-proves-the-witness-claim",
            code == EXIT_PARTIAL as i32 && text.contains("lean: accepted") && text.contains("1 claim theorems"),
            format!("exit {code}\n{text}"),
        );

        // The forged Barker receipt (claim_6, whose checker rejects it)
        // refuses the export by name, before anything is written.
        let forged = workspace()
            .join("tests/fixtures/invention/bench_179/library_forged.emath")
            .to_string_lossy()
            .to_string();
        let forged_out = std::env::temp_dir().join(format!("emath_lean_forged_{}", std::process::id()));
        let forged_arg = forged_out.to_string_lossy().to_string();
        let (text, code) = emath(&workspace(), &["export", "lean", &forged, "--out", &forged_arg]);
        probe.demand(
            "forged-claim-refuses-by-name",
            code == EXIT_REFUSED as i32
                && text.contains("E-VERIFY-REJECTED")
                && text.contains("bench179_claim_6"),
            format!("exit {code}\n{text}"),
        );
        probe.demand(
            "forged-exports-nothing",
            !forged_out.exists(),
            format!("the forged export wrote files: {forged_out:?}"),
        );
    } else {
        eprintln!("export_lean_claims: `lake` is not on PATH; the --check half did not run");
    }
    probe.finish();
}
