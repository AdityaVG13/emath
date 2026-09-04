//! Registry completeness gate.
//!
//! Every stable diagnostic code emitted by the production crates must be
//! named in `implementation/ERROR_CODES.md` (including the generated
//! completeness annex), and a code that appears in more than one
//! predicate is a repurposing. The annex is regenerated with
//! `python3 scripts/dump_error_codes.py`; this test enforces the same
//! extraction rule (`E-<PREFIX>-<3 digits>` tokens) so the two can
//! never drift apart.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Repository root, two levels above the crate directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Every `E-<PREFIX>-<3 digits>` token in `text`, requiring word
/// boundaries so doc-comment family patterns (`E-SYN-2xx`) never match.
fn codes_in(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0usize;
    while i + 2 < bytes.len() {
        if bytes[i] != b'E' || bytes[i + 1] != b'-' {
            i += 1;
            continue;
        }
        let mut j = i + 2;
        while j < bytes.len() && bytes[j].is_ascii_uppercase() {
            j += 1;
        }
        if j == i + 2 || j >= bytes.len() || bytes[j] != b'-' {
            i += 1;
            continue;
        }
        let mut k = j + 1;
        let start = k;
        while k < bytes.len() && bytes[k].is_ascii_digit() {
            k += 1;
        }
        if k - start != 3 {
            i += 1;
            continue;
        }
        let prev_boundary = i == 0
            || !matches!(
                bytes[i - 1],
                b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'-' | b'_'
            );
        let next_boundary = k >= bytes.len()
            || !matches!(
                bytes[k],
                b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'-' | b'_'
            );
        if prev_boundary && next_boundary {
            out.insert(std::str::from_utf8(&bytes[i..k]).unwrap().to_string());
        }
        i += 1;
    }
    out
}

/// Relative paths (repo-root based) of every `.rs` file under a crate.
fn rust_files_under(root: &Path, entry: &Path, files: &mut Vec<String>, all: &mut Vec<PathBuf>) {
    if entry.is_dir() {
        for child in fs::read_dir(entry).expect("read dir") {
            let child = child.expect("dir entry").path();
            let name = child.file_name().unwrap().to_string_lossy().into_owned();
            if name == "target" || name == ".git" {
                continue;
            }
            rust_files_under(root, &child, files, all);
        }
    } else if entry.extension().is_some_and(|e| e == "rs") {
        files.push(
            entry
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        );
        all.push(entry.to_path_buf());
    }
}

/// Codes emitted anywhere in the production crates.
fn emitted_codes() -> (BTreeSet<String>, usize) {
    let root = repo_root();
    let crates = root.join("crates");
    let mut emitted = BTreeSet::new();
    let mut file_count = 0usize;
    for entry in fs::read_dir(&crates).expect("read crates dir") {
        let entry = entry.expect("crates dir entry").path();
        if entry.is_dir() {
            let mut files = Vec::new();
            let mut paths = Vec::new();
            rust_files_under(&root, &entry, &mut files, &mut paths);
            file_count += files.len();
            for path in paths {
                let text = fs::read_to_string(&path).expect("read rust file");
                emitted.extend(codes_in(&text));
            }
        }
    }
    (emitted, file_count)
}

/// Codes named in the registry, annex included.
fn documented_codes() -> BTreeSet<String> {
    let path = repo_root().join("implementation").join("ERROR_CODES.md");
    let text = fs::read_to_string(&path).expect("read ERROR_CODES.md");
    codes_in(&text)
}

/// Every emitted code is documented (registry + generated annex).
#[test]
fn every_emitted_code_is_documented() {
    let (emitted, file_count) = emitted_codes();
    let documented = documented_codes();
    assert!(
        file_count > 100,
        "crate walk looks broken: only {file_count} rust files found"
    );
    let missing: Vec<&String> = emitted
        .iter()
        .filter(|c| !documented.contains(*c))
        .collect();
    assert!(
        missing.is_empty(),
        "emitted codes missing from implementation/ERROR_CODES.md (regenerate the annex \
         with `python3 scripts/dump_error_codes.py`): {}",
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// One code, one predicate: `E-KIND-010` is the sema
/// section×declaration-kind conformance refusal (state/constructors/
/// equations/algebraic only on the kinds that carry them) and nothing
/// else. The emission lives in the admit submodules since the
/// `admit.rs` restructuring (
/// fixed this gate to follow the move; `implementation/ERROR_CODES.md`
/// already lists the submodule files).
#[test]
fn ekind010_is_single_predicate() {
    let root = repo_root();
    let mut emitting = BTreeSet::new();
    for entry in fs::read_dir(root.join("crates")).expect("read crates dir") {
        let entry = entry.expect("crates dir entry").path();
        if entry.is_dir() {
            let mut files = Vec::new();
            let mut paths = Vec::new();
            rust_files_under(&root, &entry, &mut files, &mut paths);
            for (rel, path) in files.iter().zip(paths.iter()) {
                if fs::read_to_string(path)
                    .expect("read rust file")
                    .contains("E-KIND-010")
                {
                    emitting.insert(rel.clone());
                }
            }
        }
    }
    // Files that may mention the literal. Emissions (code fields) live
    // in the admit submodules (declaration.rs, equations.rs — the same
    // kind×section admission predicate, not a repurposing); builder
    // embeds the predicate in a message; the remaining two files are
    // this gate and the (historical) open.rs test module.
    let allowed = BTreeSet::from([
        "crates/emath-sema/src/admit/declaration.rs".to_string(),
        "crates/emath-sema/src/admit/equations.rs".to_string(),
        "crates/emath-sema/src/admit.rs".to_string(),
        "crates/emath-build/src/builder.rs".to_string(),
        "crates/emath-hir/src/open.rs".to_string(),
        "crates/emath-hir/tests/registry_complete.rs".to_string(),
    ]);
    assert!(
        emitting.is_subset(&allowed),
        "new E-KIND-010 emission sites are a repurposing; mint a sibling code instead. \
         Unexpected: {}",
        emitting
            .difference(&allowed)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(
        emitting.contains("crates/emath-sema/src/admit/declaration.rs")
            && emitting.contains("crates/emath-sema/src/admit/equations.rs"),
        "E-KIND-010 must still be emitted by sema admission (the admit \
         submodule predicate sites)"
    );
}

/// The minted sibling is emitted and documented.
#[test]
fn ekind016_split_is_minted() {
    let (emitted, _) = emitted_codes();
    assert!(emitted.contains("E-KIND-016"), "E-KIND-016 must be emitted");
    assert!(
        documented_codes().contains("E-KIND-016"),
        "E-KIND-016 must be in the registry"
    );
}
