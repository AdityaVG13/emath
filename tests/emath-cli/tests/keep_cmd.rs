//! `emath keep` end-to-end: rediscover sequences with the authored
//! synthesizer, keep the programs into a user library, reuse the
//! library from a second file, and keep a second-tier find that the
//! search built from a kept function (`fibonacci(n + n)`). The written
//! library must match the golden `tests/fixtures/keep/lib/seq.emath`; re-keeping identical
//! content is a no-op; different content under a kept name refuses
//! unless `--overwrite`, and an overwrite really replaces the
//! definition (the reuse file's tests then fail). Last, the automated
//! loop: `search.synthesis.learn` returns every rule it learned, and
//! `keep` without `--name` writes them all (golden
//! `tests/fixtures/keep/lib/learned.emath`), round-2 rules calling
//! round-1 names.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::emath_bin;
use emath_cli::{EXIT_OK, EXIT_REFUSED};
use emath_test_harness::Probe;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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

fn keep(dir: &Path, function: &str, name: &str, extra: &[&str]) -> (String, i32) {
    keep_from(dir, "discover.emath", function, name, extra)
}

fn keep_from(dir: &Path, file: &str, function: &str, name: &str, extra: &[&str]) -> (String, i32) {
    let mut args = vec![
        "keep", file, "--function", function, "--name", name, "--to", "lib/seq.emath",
        "--set", "unused=0", "--work", "50000000",
    ];
    args.extend_from_slice(extra);
    emath(dir, &args)
}

#[test]
fn keep_cmd() {
    let mut probe = Probe::new("emath keep: rediscover, keep, reuse");
    let dir = std::env::temp_dir().join(format!("emath_keep_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let fixtures = workspace().join("tests/fixtures/keep");
    for file in ["discover.emath", "uses.emath", "grows.emath", "learn.emath", "explore.emath"] {
        std::fs::copy(fixtures.join(file), dir.join(file)).expect("copy fixture");
    }
    let library = dir.join("lib/seq.emath");

    for (function, name) in [("Triangular", "triangular"), ("Factorial", "factorial"), ("Fibonacci", "fibonacci")] {
        let (text, code) = keep(&dir, function, name, &[]);
        probe.demand(&format!("keep-{name}"), code == EXIT_OK as i32 && text.contains(&format!("kept {name}")), format!("exit {code}\n{text}"));
    }
    let (text, code) = emath(&dir, &["test", "uses.emath"]);
    probe.demand("reuse-passes", code == EXIT_OK as i32 && text.contains("2 authored tests passed"), format!("exit {code}\n{text}"));

    let (text, code) = keep_from(&dir, "grows.emath", "FibonacciEven", "fibonacci_even", &[]);
    probe.demand("keep-built-from-kept", code == EXIT_OK as i32 && text.contains("kept fibonacci_even"), format!("exit {code}\n{text}"));
    let written = std::fs::read_to_string(&library).unwrap_or_default();
    let golden = std::fs::read_to_string(fixtures.join("lib/seq.emath")).expect("golden");
    probe.demand("library-matches-golden", written == golden, written.clone());

    // A kept rule that calls a shipped module's export carries its `use`
    // into the library, so a later program can import the library alone.
    let (text, code) = emath(
        &dir,
        &["keep", "grows.emath", "--function", "CatalanRule", "--name", "catalan", "--to", "lib/cat.emath", "--set", "unused=0", "--work", "50000000"],
    );
    let cat = std::fs::read_to_string(dir.join("lib/cat.emath")).unwrap_or_default();
    probe.demand(
        "keep-carries-use",
        code == EXIT_OK as i32 && cat.lines().any(|l| l == "use discrete.factorials") && !cat.contains("use search.synthesis"),
        format!("exit {code}\n{text}\n{cat}"),
    );
    std::fs::write(
        dir.join("cat_uses.emath"),
        "use lib.cat\n\nemath function catalan_12:\n    inputs:\n        unused: Int\n    outputs:\n        result: Int\n    definitions:\n        result = catalan(12)\n    tests:\n        example <twelfth>:\n            given unused = 0\n            expect result == 208012\n",
    )
    .expect("write cat_uses");
    let (text, code) = emath(&dir, &["test", "cat_uses.emath"]);
    probe.demand("kept-library-admits", code == EXIT_OK as i32, format!("exit {code}\n{text}"));
    // A program built inside an imported module may call what that
    // module imports; the library needs that `use` although the keeping
    // file never wrote it.
    std::fs::write(
        dir.join("builder.emath"),
        "exports: Squarer\n\nuse analysis.powers\n\nemath function Squarer:\n    inputs:\n        unused: Int\n    outputs:\n        result: Code\n    definitions:\n        result = quote(recur f in Rat -> Rat: function x in Rat: rat_pow(x, 2))\n",
    )
    .expect("write builder");
    std::fs::write(
        dir.join("built.emath"),
        "use builder\n\nemath function Built:\n    inputs:\n        unused: Int\n    outputs:\n        result: Code\n    definitions:\n        result = Squarer(0)\n",
    )
    .expect("write built");
    let (text, code) = emath(
        &dir,
        &["keep", "built.emath", "--function", "Built", "--name", "square", "--to", "lib/square.emath", "--set", "unused=0"],
    );
    let square = std::fs::read_to_string(dir.join("lib/square.emath")).unwrap_or_default();
    let (check, checked) = emath(&dir, &["check", "lib/square.emath"]);
    probe.demand(
        "keep-carries-transitive-use",
        code == EXIT_OK as i32 && square.lines().any(|l| l == "use analysis.powers") && !square.contains("use builder") && checked == EXIT_OK as i32,
        format!("exit {code}\n{text}\n{square}\ncheck exit {checked}\n{check}"),
    );
    // Kept under a name its own `use` exports, the library would not
    // load: refused before anything is written.
    let (text, code) = emath(
        &dir,
        &["keep", "grows.emath", "--function", "CatalanRule", "--name", "binomial", "--to", "lib/clash.emath", "--set", "unused=0", "--work", "50000000"],
    );
    probe.demand(
        "clash-refuses-unwritten",
        code == EXIT_REFUSED as i32 && text.contains("E-NAME-022") && !dir.join("lib/clash.emath").exists(),
        format!("exit {code}\n{text}"),
    );

    // A body unfolded from another module (quote.body) can call that
    // module's private helper, which no library can import: refused by
    // name before anything is written.
    std::fs::write(
        dir.join("hidden.emath"),
        "exports: twice\n\nemath function half:\n    inputs:\n        n: Int\n    outputs:\n        result: Int\n    definitions:\n        result = n\n\nemath function twice:\n    inputs:\n        n: Int\n    outputs:\n        result: Int\n    definitions:\n        result = half(n) + half(n)\n",
    )
    .expect("write hidden");
    std::fs::write(
        dir.join("unfold.emath"),
        "use hidden\n\nemath function Unfolded:\n    inputs:\n        unused: Int\n    outputs:\n        result: Code\n    definitions:\n        result = quote.substitute(quote(recur f in Int -> Int: program), \"program\", quote.body(quote(twice)).fragment)\n",
    )
    .expect("write unfold");
    let (text, code) = emath(
        &dir,
        &["keep", "unfold.emath", "--function", "Unfolded", "--name", "twice_copy", "--to", "lib/hidden_copy.emath", "--set", "unused=0"],
    );
    probe.demand(
        "private-helper-refuses-unwritten",
        code == EXIT_REFUSED as i32 && text.contains("E-KEEP-USE") && text.contains("`half`") && !dir.join("lib/hidden_copy.emath").exists(),
        format!("exit {code}\n{text}"),
    );

    let (text, code) = keep(&dir, "Factorial", "factorial", &[]);
    probe.demand("same-content-unchanged", code == EXIT_OK as i32 && text.contains("unchanged factorial"), format!("exit {code}\n{text}"));

    let (text, code) = keep(&dir, "Fibonacci", "factorial", &[]);
    let after = std::fs::read_to_string(&library).unwrap_or_default();
    probe.demand(
        "different-content-refuses",
        code == EXIT_REFUSED as i32 && text.contains("E-KEEP-EXISTS") && after == golden,
        format!("exit {code}\n{text}"),
    );

    let (text, code) = keep(&dir, "Fibonacci", "factorial", &["--overwrite"]);
    probe.demand("overwrite-replaces", code == EXIT_OK as i32, format!("exit {code}\n{text}"));
    let (text, code) = emath(&dir, &["test", "uses.emath"]);
    probe.demand("reuse-sees-overwrite", code != EXIT_OK as i32, format!("exit {code}\n{text}"));

    // No --name: every record the curriculum learned keeps under its own
    // name, in learning order, so round-2 rules call round-1 names.
    let learn = [
        "keep", "learn.emath", "--function", "Curriculum", "--to", "lib/learned.emath",
        "--set", "unused=0", "--work", "50000000",
    ];
    let (text, code) = emath(&dir, &learn);
    let names = ["triangular", "fibonacci", "sum_of_cubes", "fibonacci_even"];
    probe.demand(
        "keep-all-learned",
        code == EXIT_OK as i32 && names.iter().all(|n| text.contains(&format!("kept {n} "))),
        format!("exit {code}\n{text}"),
    );
    let written = std::fs::read_to_string(dir.join("lib/learned.emath")).unwrap_or_default();
    let golden = std::fs::read_to_string(fixtures.join("lib/learned.emath")).expect("learned golden");
    probe.demand("learned-matches-golden", written == golden, written.clone());
    let (text, code) = emath(&dir, &learn);
    probe.demand(
        "relearn-unchanged",
        code == EXIT_OK as i32 && names.iter().all(|n| text.contains(&format!("unchanged {n} "))),
        format!("exit {code}\n{text}"),
    );

    // Three rounds of self-proposed targets fit a modest budget: every
    // search in a round sees the same blocks, so the block set does not
    // grow target by target, and keeping a rule rebuilds one row of the
    // store instead of every bucket.
    let (text, code) = emath(&dir, &["test", "explore.emath", "--work", "32000000"]);
    probe.demand("explore-three-rounds", code == EXIT_OK as i32, format!("exit {code}\n{text}"));

    // Self-proposed targets keep under their joined names (transform_rule).
    let (text, code) = emath(
        &dir,
        &["keep", "explore.emath", "--function", "Explored", "--to", "lib/explored.emath", "--set", "unused=0", "--work", "50000000"],
    );
    probe.demand("keep-explored", code == EXIT_OK as i32, format!("exit {code}\n{text}"));
    let written = std::fs::read_to_string(dir.join("lib/explored.emath")).unwrap_or_default();
    let golden = std::fs::read_to_string(fixtures.join("lib/explored.emath")).expect("explored golden");
    probe.demand("explored-matches-golden", written == golden, written.clone());

    probe.finish();
}

const CODE_BUILDER: &str = "emath function Build:
    inputs:
        unused: Int
    outputs:
        result: Code
    definitions:
        result = quote(recur f in Int -> Int: function x in Int: x + 1)
";

#[test]
fn keep_preserves_semantic_admission() {
    let dir = std::env::temp_dir().join(format!("emath_keep_admission_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("build.emath"), CODE_BUILDER.replace("Build", "_")).unwrap();
    let (text, code) = emath(&dir, &[
        "keep", "build.emath", "--function", "_", "--name", "next",
        "--to", "library.emath", "--set", "unused=0",
    ]);
    assert_eq!(code, emath_cli::EXIT_ADMISSION as i32, "{text}");
    assert!(text.contains("E-NAME-023"), "{text}");
    assert!(!dir.join("library.emath").exists(), "refused source must not create a library");
}

#[test]
fn kept_library_is_admitted_before_publication() {
    let dir = std::env::temp_dir().join(format!("emath_keep_publication_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("build.emath"), CODE_BUILDER).unwrap();
    let original = "exports: existing
emath function existing:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n
";
    std::fs::write(dir.join("library.emath"), original).unwrap();
    let (text, code) = emath(&dir, &[
        "keep", "build.emath", "--function", "Build", "--name", "_",
        "--to", "library.emath", "--set", "unused=0",
    ]);
    assert_eq!(code, EXIT_REFUSED as i32, "{text}");
    assert!(text.contains("E-NAME-023"), "{text}");
    assert_eq!(
        std::fs::read_to_string(dir.join("library.emath")).unwrap(),
        original,
        "a semantically invalid proposed library must leave the original untouched"
    );
}

#[test]
fn kept_imports_come_from_the_checked_module() {
    use emath_exec_ir::constructor_layer::CValue;
    use std::collections::BTreeMap;
    emath_syntax::install_source_parser();
    let dir = std::env::temp_dir().join(format!("emath_keep_snapshot_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("build.emath");
    let helper = dir.join("helper.emath");
    std::fs::write(&helper, "exports: next
emath function next:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n + 1
").unwrap();
    let source = "use helper
emath function Build:
    inputs:
        unused: Int
    outputs:
        result: Code
    definitions:
        result = quote(recur f in Int -> Int: function x in Int: next(x))
";
    std::fs::write(&path, source).unwrap();
    let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
    let loaded = session.load_package(&path).unwrap();
    let checked = session.check(loaded.file);
    assert!(!checked.diagnostics.has_errors(), "{:?}", checked.diagnostics);
    let module = checked.prepared.unwrap();
    let program = module.evaluate_function(
        "Build", &BTreeMap::from([("unused".into(), CValue::Int(0.into()))]), 1000,
    ).unwrap();
    std::fs::write(&path, CODE_BUILDER).unwrap();
    // Both the import line and its export surface change after checking.
    std::fs::write(&helper, "exports: other
emath function other:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n
").unwrap();
    assert_eq!(
        module.needed_uses(&program, &dir.join("library.emath")).unwrap(),
        ["helper"],
        "keeping must use the import graph whose definitions produced this Code"
    );

    let root = dir.join("language/modules");
    std::fs::create_dir_all(root.join("pkg")).unwrap();
    std::fs::write(root.join("pkg/helper.emath"), "exports: next
emath function next:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n + 1
").unwrap();
    std::fs::write(root.join("pkg/builder.emath"), format!("exports: Build\n{source}")).unwrap();
    std::fs::write(dir.join("discover.emath"), "use pkg.builder
emath function Main:
    inputs:
        unused: Int
    outputs:
        result: Code
    definitions:
        result = Build(0)
").unwrap();
    let local_emath = |args: &[&str]| {
        Command::new(emath_bin()).args(args).current_dir(workspace())
            .env_remove("EMATH_ROOT").output().unwrap()
    };
    let destination = root.join("library.emath");
    let discover = dir.join("discover.emath");
    let output = local_emath(&[
        "keep", discover.to_str().unwrap(), "--function", "Main", "--name", "inc",
        "--to", destination.to_str().unwrap(), "--set", "unused=0",
    ]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let library = std::fs::read_to_string(&destination).unwrap();
    assert!(library.lines().any(|line| line == "use pkg.helper"), "{library}");
    let output = local_emath(&[
        "run", destination.to_str().unwrap(), "--function", "inc", "--set", "x=2", "--json",
    ]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("\"payload\": \"3\""), "{text}");
}

// Substituting a replacement that mentions `x` under a binder of the
// same name mints the hygiene token `#x.cap.1`; a leading `#` is a
// comment, so printing that binder into the library would write
// unparsable source. Keep must refuse by name instead.
const HYGIENE_BUILDER: &str = "emath function Capture:
    inputs:
        unused: Int
    outputs:
        result: Code
    definitions:
        result = quote.substitute(quote(recur f in sequence(Rat) -> Rat: function x in sequence(Rat): body), body, quote(x[0]))
";

#[test]
fn keep_refuses_hygiene_renamed_binders() {
    let dir = std::env::temp_dir().join(format!("emath_keep_hygiene_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("build.emath"), HYGIENE_BUILDER).unwrap();
    let (text, code) = emath(&dir, &[
        "keep", "build.emath", "--function", "Capture", "--name", "captured",
        "--to", "library.emath", "--set", "unused=0",
    ]);
    assert_eq!(code, EXIT_REFUSED as i32, "{text}");
    assert!(text.contains("E-KEEP-SHAPE"), "{text}");
    assert!(!dir.join("library.emath").exists(), "an unwritable kept program must not create a library");
}

// A claim built by substitute over an object value carries Code-typed
// fields (statement, certificate, checker, conditions, refutation).
// Printing those fields as bare expressions writes a library that
// means something else: `checker: accept_all` re-parses as a value
// lookup (unbound at run), `certificate: [1, ...]` as a plain list.
// Faithful value-to-source renders every Code value as `quote(...)`,
// so the kept claim rechecks to status 1 (bead emath-zi6bj).
const CLAIM_BUILDER: &str = "use search.claim

emath function accept_all:
    inputs:
        cert: Code
    outputs:
        result: Bool
    definitions:
        result = true

emath function Claimed:
    inputs:
        unused: Int
    outputs:
        result: Claim
    definitions:
        result = quote.substitute(quote(recur me in Int -> Claim: function unused in Int: v), \"v\", make_claim(0, quote(function s in sequence(Int): length(s) == 5), quote([1, 0 - 1, 1, 1, 0 - 1]), quote(accept_all), quote(5), 5, quote(function s in sequence(Int): length(s) != 5)))
";

#[test]
fn kept_claim_round_trips_through_the_library() {
    let dir = std::env::temp_dir().join(format!("emath_keep_claim_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("claim.emath"), CLAIM_BUILDER).unwrap();
    let (text, code) = keep_from(&dir, "claim.emath", "Claimed", "claimed", &[]);
    assert_eq!(code, EXIT_OK as i32, "{text}");
    let library = std::fs::read_to_string(dir.join("lib/seq.emath")).unwrap();
    assert!(library.contains("checker: quote(accept_all)"), "{library}");
    assert!(
        library.contains("statement: quote(function s in sequence(Int): length(s) == 5)"),
        "{library}"
    );
    std::fs::write(
        dir.join("recheck.emath"),
        "use lib.seq\nuse search.claim\n\nemath function Verdict:\n    inputs:\n        unused: Int\n    outputs:\n        result: Int\n    definitions:\n        result = check_claim(claimed(0), [Checker: {name: quote(accept_all), run: function cert in Code: true}]).status\n    tests:\n        example <kept_claim_accepted>:\n            given unused = 0\n            expect result == 1\n",
    )
    .unwrap();
    let (text, code) = emath(&dir, &["test", "recheck.emath"]);
    assert_eq!(code, EXIT_OK as i32, "{text}");
    assert!(text.contains("1 authored tests passed"), "{text}");
}
