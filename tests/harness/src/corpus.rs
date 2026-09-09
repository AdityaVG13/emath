//! Workspace corpora. Every `.emath` fixture is a demand, not a snapshot.
//!
//! `tests/valid` and `language/examples` must admit and evaluate their
//! `expect` rows to Passed. `tests/invalid` must emit every pinned `E-*`
//! code. One probe reports every broken file.

use std::fs;
use std::path::PathBuf;

use crate::pipeline::{Source, boot};
use crate::probe::Probe;
use crate::table::workspace_path;

/// Walk the four on-disk corpora and record every broken contract.
pub fn demand_workspace_corpora(probe: &mut Probe) {
    boot();
    demand_tree(probe, "tests/valid", Kind::Valid);
    demand_tree(probe, "tests/invalid", Kind::Invalid);
    demand_tree(probe, "tests/fixtures/language", Kind::Valid);
    demand_tree(probe, "language/examples", Kind::Valid);
}

#[derive(Clone, Copy)]
enum Kind {
    Valid,
    Invalid,
}

fn demand_tree(probe: &mut Probe, relative: &str, kind: Kind) {
    let root = workspace_path(relative);
    let mut files = Vec::new();
    collect_emath(&root, &mut files);
    probe.demand(
        format!("{relative}:nonempty"),
        !files.is_empty(),
        format!("corpus {relative} has no .emath files"),
    );
    for path in files {
        let rel = path_relative(&path);
        let text = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("read {}: {error}", path.display());
        });
        let source = Source::from_str(&rel, text.clone());
        match kind {
            Kind::Invalid => {
                if header_admits(&text) {
                    source.must_admit(probe);
                } else {
                    let codes = pinned_codes(&text);
                    probe.demand(
                        format!("{rel}:pin"),
                        !codes.is_empty(),
                        "invalid fixture must pin expect: E-XXX or expect: admit in the header",
                    );
                    if !codes.is_empty() {
                        let refs: Vec<&str> = codes.iter().map(String::as_str).collect();
                        source.must_refuse(probe, &refs);
                    }
                }
            }
            Kind::Valid => {
                if runnable_math(&text) {
                    source.eval_tests(probe);
                } else {
                    source.must_admit(probe);
                }
            }
        }
    }
}

fn collect_emath(dir: &PathBuf, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    names.sort();
    for path in names {
        if path.is_dir() {
            collect_emath(&path, out);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("emath") {
            out.push(path);
        }
    }
}

fn path_relative(path: &PathBuf) -> String {
    let root = workspace_path("");
    path.strip_prefix(&root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn header_admits(text: &str) -> bool {
    text.lines().take(40).any(|line| {
        let body = line
            .trim_start()
            .trim_start_matches('#')
            .trim_start_matches("//")
            .trim();
        body.starts_with("expect: admit")
    })
}

fn pinned_codes(text: &str) -> Vec<String> {
    let mut codes = Vec::new();
    for line in text.lines().take(40) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if !(trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.starts_with("->")) {
            if !trimmed.starts_with("emath") && !trimmed.starts_with("package") && !trimmed.starts_with("use ") {
                continue;
            }
            if codes.is_empty() && !trimmed.starts_with('#') {
                // Past the header once we hit real syntax after scanning comments.
                if trimmed.starts_with("emath") || trimmed.starts_with("package") || trimmed.starts_with("use ") {
                    break;
                }
            }
        }
        let body = trimmed
            .trim_start_matches('#')
            .trim_start_matches("//")
            .trim();
        for token in body.split(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')) {
            if token.starts_with("E-") && token.chars().filter(|c| *c == '-').count() >= 2 {
                if !codes.iter().any(|have| have == token) {
                    codes.push(token.to_string());
                }
            }
        }
    }
    codes
}

fn runnable_math(text: &str) -> bool {
    text.contains("emath function")
        || text.contains("emath law")
        || text.contains("emath policy")
}

/// Hard language gaps: contracts the compiler must grow into.
///
/// These are supposed to fail in multiple places until the engine upgrades.
/// Do not weaken them to match today's honest no-claims.
pub fn demand_language_gaps(probe: &mut Probe) {
    boot();
    Source::from_str(
        "gap/rk45-identity",
        r#"emath function Rk45Identity:
    inputs:
        y: Vector<Float64>
        h: Float64
    outputs:
        next: Vector<Float64>
    definitions:
        next = step(y, h, "rk45")
    tests:
        example <identity_rate>:
            given y = [1.0]
            given h = 1.0
            expect next == [2.0]
"#,
    )
    .eval_tests(probe);

    Source::from_str(
        "gap/range-slice",
        r#"emath function RangeSlice:
    outputs:
        r: Float64
    definitions:
        v = [1.0, 2.0, 3.0, 4.0]
        s = v[1..3]
        r = s[0]
    tests:
        example <middle>:
            expect r == 2.0
"#,
    )
    .eval_tests(probe);

    Source::from_str(
        "gap/interval-sum",
        r#"emath function IntervalSum:
    outputs:
        s: Interval<Float64>
    definitions:
        s = interval(1.0, 2.0) + interval(3.0, 4.0)
    tests:
        example <endpoints>:
            expect s == interval(4.0, 6.0)
"#,
    )
    .eval_tests(probe);

    Source::from_str(
        "gap/index-oob",
        r#"emath function IndexOob:
    outputs:
        r: Float64
    definitions:
        v = [3.0, 4.0]
        r = v[9]
    tests:
        example <oob>:
            expect r == 0.0
"#,
    )
    .must_refuse(probe, &["E-SHAPE-006"]);
}
