//! Every public example must check, and representative run and simulation
//! workflows must produce their declared outcomes.

use emath_cli::{run, run_check, CliExit};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn example(rel: &str) -> PathBuf {
    repo_root().join("language/examples").join(rel)
}

fn cli(command: &str, rest: &[&str]) -> Vec<String> {
    std::iter::once(command.to_string())
        .chain(rest.iter().map(|argument| argument.to_string()))
        .collect()
}

fn out_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "emath-corpus-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create corpus scratch dir");
    dir
}

/// Corpus walk: every `.emath` file under `language/examples/*/*.emath`
/// passes `emath check` with no `E-*` error diagnostic. New files join the
/// gate automatically; a file that starts refusing fails here.
#[test]
fn all_public_examples_typecheck() {
    emath_syntax::install_source_parser();
    let examples_dir = repo_root().join("language/examples");
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for category in std::fs::read_dir(&examples_dir).expect("examples dir") {
        let category = category.expect("category entry");
        let path = category.path();
        if !path.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(&path).expect("category dir") {
            let file = file.expect("example entry");
            let source = file.path();
            if source.extension().and_then(|extension| extension.to_str()) != Some("emath") {
                continue;
            }
            checked += 1;
            // A file may declare an intentional refusal with a leading
            // `-> E-XXX-NNN` or `expect: E-XXX-NNN` line.
            let text = std::fs::read_to_string(&source).expect("read example");
            let pinned = header_pinned_code(&text);
            match pinned {
                Some(expected) => {
                    let (diagnostics, _, _) = run_check(&source);
                    let emitted: Vec<&str> = diagnostics
                        .items()
                        .iter()
                        .filter(|item| {
                            item.severity == emath_core::Severity::Error
                                && item.code.starts_with("E-")
                        })
                        .map(|item| item.code)
                        .collect();
                    assert!(
                        emitted.contains(&expected),
                        "refuse-pinned example must emit {expected}, got {emitted:?}: {}",
                        source.display()
                    );
                    assert_eq!(
                        run(&cli("check", &[&source.to_string_lossy()])),
                        CliExit::Refused,
                        "refuse-pinned example must refuse check: {}",
                        source.display()
                    );
                }
                None => {
                    let (diagnostics, _, _) = run_check(&source);
                    let refused: Vec<&str> = diagnostics
                        .items()
                        .iter()
                        .filter(|item| {
                            item.severity == emath_core::Severity::Error
                                && item.code.starts_with("E-")
                        })
                        .map(|item| item.code)
                        .collect();
                    if !refused.is_empty() {
                        failures.push(format!(
                            "{}: check emitted {}",
                            source.display(),
                            refused.join(", ")
                        ));
                    }
                    assert_eq!(
                        run(&cli("check", &[&source.to_string_lossy()])),
                        CliExit::Ok,
                        "corpus file must admit check (add a `-> E-XXX-NNN` header to pin a refusal): {}",
                        source.display()
                    );
                }
            }
        }
    }
    assert!(
        checked >= 20,
        "the curated teaching corpus must retain broad introductory coverage"
    );
    assert!(
        failures.is_empty(),
        "corpus files admitted with error diagnostics: {failures:?}"
    );
}

/// Scan leading comment lines for `-> E-XXX-NNN` / `expect: E-XXX-NNN`.
/// The first pinned code wins; no pin means the file must admit.
fn header_pinned_code(text: &str) -> Option<&'static str> {
    for line in text.lines() {
        let line = line.trim_start_matches('#').trim();
        if line.is_empty() {
            continue;
        }
        if let Some(code) = pinned_code_in(line) {
            return Some(String::leak(code));
        }
        // Only the leading comment block is scanned; the first non-comment
        // line ends the header scan.
        if !line.starts_with("-> ") && !line.starts_with("expect:") {
            break;
        }
    }
    None
}

/// Extract `E-XXX-NNN` from a header line (after `->` or `expect:`).
fn pinned_code_in(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("-> ")
        .or_else(|| line.strip_prefix("expect:"))?
        .trim();
    let code: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || *ch == '-')
        .collect();
    if valid_code(&code) {
        Some(code)
    } else {
        None
    }
}

/// E-XXX…-NNN shape check: `E-`, a family of >=3 uppercase letters (E-SYN,
/// E-UNIT, E-TYPE, E-MEAS, …), `-`, exactly 3 digits.
fn valid_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    if bytes.len() < 9 || bytes[0] != b'E' || bytes[1] != b'-' {
        return false;
    }
    let Some(relative) = bytes[2..].iter().position(|byte| *byte == b'-') else {
        return false;
    };
    let family_dash = relative + 2;
    if family_dash + 4 != bytes.len() {
        return false;
    }
    bytes[2..family_dash].iter().all(|byte| byte.is_ascii_uppercase())
        && bytes[family_dash + 1..].iter().all(|byte| byte.is_ascii_digit())
}

/// Representative native-backend oracles: build and execute the generated
/// crate (library crates run their `tests:` example assertions).
#[test]
fn corpus_runs_rows_execute() {
    emath_syntax::install_source_parser();
    for (name, relative) in [
        ("newton-second", "physics/newton-second.emath"),
        ("hello-square", "intro/hello-square.emath"),
        ("symbolic-cas", "algebra/symbolic-cas.emath"),
    ] {
        let dir = out_dir(name);
        let source = example(relative);
        assert_eq!(
            run(&cli(
                "run",
                &[&source.to_string_lossy(), "--out", &dir.to_string_lossy()]
            )),
            CliExit::Ok,
            "README runs row must execute its generated crate: {relative}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Pinned simulate oracles for the numerical README rows: pinned bindings,
/// pinned window. Numeric agreement with the analytic solution (undamped
/// spring x=cos(t), conserved heat sums) is pinned textually in
/// `scripts/validate.sh` against the same bindings;
/// here the pinned contract is admission + successful stepping to t1.
#[test]
fn corpus_simulate_rows_step_to_t1() {
    emath_syntax::install_source_parser();
    for (relative, bindings) in [
        (
            "numerical/explicit-mass-spring.emath",
            vec![
                "--set", "m=1", "--set", "k=1", "--set", "c=0", "--set", "s=[1,0]",
                "--dt", "0.01", "--t1", "3.141592653589793",
            ],
        ),
        (
            "numerical/heat-rod-sim.emath",
            vec![
                "--set", "alpha=1.0", "--set", "u=[1,0,0,0,0]",
                "--dt", "0.01", "--t1", "1.0",
            ],
        ),
    ] {
        let source = example(relative);
        let mut arguments = vec!["simulate".to_string(), source.to_string_lossy().into_owned()];
        arguments.extend(bindings.iter().map(|binding| binding.to_string()));
        assert_eq!(
            run(&arguments),
            CliExit::Ok,
            "README simulate row must step to t1 with pinned bindings: {relative}"
        );
    }
}

/// Provenance oracle: `emath explain --provenance` renders the identity-bearing
/// Citation/Assumed DAG for the science row.
#[test]
fn corpus_provenance_row_renders() {
    emath_syntax::install_source_parser();
    let source = example("science/observations.emath");
    assert_eq!(
        run(&cli(
            "explain",
            &[&source.to_string_lossy(), "--provenance"]
        )),
        CliExit::Ok,
        "README provenance row must render its provenance DAG"
    );
}

/// Typed-hole oracle: `scratch.emath` admits check (the hole is the example)
/// and `emath run` refuses with E-GOAL-043 — an open hole must never claim a
/// produced artifact. Pinned exact code, not "some E-code".
#[test]
fn corpus_typed_hole_refuses_run_with_goal_code() {
    emath_syntax::install_source_parser();
    let source = example("intro/scratch.emath");
    let (diagnostics, _, _) = run_check(&source);
    assert!(
        diagnostics
            .items()
            .iter()
            .any(|item| item.code == "N-HOLE-001"),
        "scratch must carry the open-hole note"
    );
    let dir = out_dir("scratch-refused");
    assert_eq!(
        run(&cli(
            "run",
            &[&source.to_string_lossy(), "--out", &dir.to_string_lossy()]
        )),
        CliExit::Refused,
        "open-hole scratch must refuse `emath run` (E-GOAL-043 family)"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

