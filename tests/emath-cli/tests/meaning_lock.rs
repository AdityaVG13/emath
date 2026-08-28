//! Meaning-lock E2E: portfolio → set → single-world → unset → portfolio,
//! two-user isolation, and typed refusals.

use emath_cli::{CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE, run};
use std::fs;
use std::path::{Path, PathBuf};

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

fn glyphs_source() -> &'static str {
    "emath custom AlienGlyphs:\n    body:\n        ⧖(a ⋈ b) ⊛ ζ\n\n    construct meaning:\n        explore:\n            free_symbolic\n            Boolean_algebra\n            modular_numeric\n\n        protect:\n            total\n            deterministic\n\n        keep:\n            pareto 8\n\n    answer:\n        return interpretation_portfolio\n"
}

fn drifted_source() -> &'static str {
    "emath custom AlienGlyphs:\n    body:\n        ⧖(b ⋈ a) ⊛ ζ\n\n    construct meaning:\n        explore:\n            free_symbolic\n            Boolean_algebra\n            modular_numeric\n\n        protect:\n            total\n            deterministic\n\n        keep:\n            pareto 8\n\n    answer:\n        return interpretation_portfolio\n"
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "emath-meaning-lock-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0),
        name
    ));
    fs::create_dir_all(&root).expect("scratch dir");
    root
}

fn write_glyphs(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("glyphs.emath");
    fs::write(&path, body).expect("write glyphs");
    path
}

fn world_id_count(body: &str) -> usize {
    body.matches("\"world_id\"").count()
}

fn genesis(file: &Path, out: &Path) -> CliExit {
    run(&args(&[
        "genesis",
        &file.display().to_string(),
        "--out",
        &out.display().to_string(),
    ]))
}

#[test]
fn meaning_help_is_catalogued() {
    assert_eq!(run(&args(&["meaning", "--help"])), EXIT_OK);
    assert_eq!(run(&args(&["help", "meaning"])), EXIT_OK);
    assert_eq!(run(&args(&["meaning"])), EXIT_USAGE);
}

#[test]
fn portfolio_set_lock_rerun_unset_restores_portfolio() {
    let project = scratch("roundtrip");
    let file = write_glyphs(&project, glyphs_source());
    let out1 = project.join("out1");
    let out2 = project.join("out2");
    let out3 = project.join("out3");

    assert_eq!(genesis(&file, &out1), EXIT_OK);
    let portfolio1 = fs::read_to_string(out1.join("interpretation-portfolio.json")).unwrap();
    assert!(
        world_id_count(&portfolio1) >= 5,
        "unlocked genesis keeps the portfolio: {portfolio1}"
    );
    let receipt1 = fs::read_to_string(out1.join("answer-receipt.json")).unwrap();
    assert!(
        !receipt1.contains("user-locked"),
        "unlocked receipt must not claim user-lock: {receipt1}"
    );

    assert_eq!(
        run(&args(&[
            "meaning",
            "set",
            &file.display().to_string(),
            "--world",
            "Boolean_algebra",
            "--dir",
            &project.display().to_string(),
        ])),
        EXIT_OK
    );
    assert!(project.join(".emath/meaning.lock").is_file());

    assert_eq!(genesis(&file, &out2), EXIT_OK);
    let portfolio2 = fs::read_to_string(out2.join("interpretation-portfolio.json")).unwrap();
    assert_eq!(
        world_id_count(&portfolio2),
        1,
        "locked genesis is single-world: {portfolio2}"
    );
    let receipt2 = fs::read_to_string(out2.join("answer-receipt.json")).unwrap();
    assert!(
        receipt2.contains("\"meaning_provenance\": \"user-locked\""),
        "locked receipt provenance: {receipt2}"
    );
    assert!(
        receipt2.contains("\"authority\": \"structural\""),
        "lock must not escalate authority: {receipt2}"
    );

    assert_eq!(
        run(&args(&[
            "meaning",
            "unset",
            "--dir",
            &project.display().to_string(),
        ])),
        EXIT_OK
    );
    assert_eq!(genesis(&file, &out3), EXIT_OK);
    let portfolio3 = fs::read_to_string(out3.join("interpretation-portfolio.json")).unwrap();
    assert!(
        world_id_count(&portfolio3) >= 5,
        "unset restores the portfolio: {portfolio3}"
    );
    let receipt3 = fs::read_to_string(out3.join("answer-receipt.json")).unwrap();
    assert!(
        !receipt3.contains("user-locked"),
        "unset receipt is ranked, not user-locked: {receipt3}"
    );
}

#[test]
fn two_projects_same_source_different_locks() {
    let left = scratch("user-a");
    let right = scratch("user-b");
    let left_file = write_glyphs(&left, glyphs_source());
    let right_file = write_glyphs(&right, glyphs_source());
    assert_eq!(
        run(&args(&[
            "meaning",
            "set",
            &left_file.display().to_string(),
            "--world",
            "Boolean_algebra",
            "--dir",
            &left.display().to_string(),
        ])),
        EXIT_OK
    );
    assert_eq!(
        run(&args(&[
            "meaning",
            "set",
            &right_file.display().to_string(),
            "--world",
            "modular_numeric",
            "--dir",
            &right.display().to_string(),
        ])),
        EXIT_OK
    );
    let left_out = left.join("out");
    let right_out = right.join("out");
    assert_eq!(genesis(&left_file, &left_out), EXIT_OK);
    assert_eq!(genesis(&right_file, &right_out), EXIT_OK);
    let left_receipt = fs::read_to_string(left_out.join("answer-receipt.json")).unwrap();
    let right_receipt = fs::read_to_string(right_out.join("answer-receipt.json")).unwrap();
    assert!(left_receipt.contains("user-locked"));
    assert!(right_receipt.contains("user-locked"));
    let left_fp = lock_world(&left_receipt);
    let right_fp = lock_world(&right_receipt);
    assert_ne!(
        left_fp, right_fp,
        "two users lock different worlds: {left_fp} vs {right_fp}"
    );
}

#[test]
fn empty_nested_emath_dir_does_not_shadow_parent_lock() {
    let project = scratch("shadow");
    let file = write_glyphs(&project, glyphs_source());
    assert_eq!(
        run(&args(&[
            "meaning",
            "set",
            &file.display().to_string(),
            "--world",
            "Boolean_algebra",
            "--dir",
            &project.display().to_string(),
        ])),
        EXIT_OK
    );
    let nested = project.join("nested");
    fs::create_dir_all(nested.join(".emath")).expect("decoy .emath");
    let nested_file = write_glyphs(&nested, glyphs_source());
    let out = nested.join("out");
    assert_eq!(genesis(&nested_file, &out), EXIT_OK);
    let portfolio = fs::read_to_string(out.join("interpretation-portfolio.json")).unwrap();
    assert_eq!(
        world_id_count(&portfolio),
        1,
        "empty nested .emath/ must not bypass the parent lock: {portfolio}"
    );
}

#[test]
fn tampered_lock_and_source_drift_refuse() {
    let project = scratch("negatives");
    let file = write_glyphs(&project, glyphs_source());
    assert_eq!(
        run(&args(&[
            "meaning",
            "set",
            &file.display().to_string(),
            "--world",
            "Boolean_algebra",
            "--dir",
            &project.display().to_string(),
        ])),
        EXIT_OK
    );
    let lock_path = project.join(".emath/meaning.lock");
    let original = fs::read_to_string(&lock_path).unwrap();
    let tampered = original.replacen(&extract_fingerprint(&original), "aaaaaaaaaaaaaaaa", 1);
    fs::write(&lock_path, tampered).unwrap();
    assert_eq!(genesis(&file, &project.join("tamper-out")), EXIT_REFUSED);

    fs::write(&lock_path, original).unwrap();
    fs::write(&file, drifted_source()).unwrap();
    assert_eq!(genesis(&file, &project.join("drift-out")), EXIT_REFUSED);

    fs::write(&lock_path, "{not json").unwrap();
    assert_eq!(genesis(&file, &project.join("malformed-out")), EXIT_REFUSED);
}

#[test]
fn meaning_duplicate_valued_flags_are_usage() {
    assert_eq!(
        run(&args(&[
            "meaning",
            "set",
            "a.emath",
            "--world",
            "one_point",
            "--world",
            "Boolean_algebra",
        ])),
        EXIT_USAGE
    );
    assert_eq!(
        run(&args(&["meaning", "list", "--dir", "a", "--dir", "b"])),
        EXIT_USAGE
    );
    assert_eq!(
        run(&args(&["meaning", "unset", "--hole", "x", "--hole", "y"])),
        EXIT_USAGE
    );
    assert_eq!(
        run(&args(&[
            "meaning", "explain", "a.emath", "--dir", "a", "--dir", "b"
        ])),
        EXIT_USAGE
    );
}

fn extract_fingerprint(lock: &str) -> String {
    let marker = "\"world_fingerprint\": \"";
    let start = lock.find(marker).expect("fingerprint field") + marker.len();
    lock[start..start + 16].to_string()
}

fn lock_world(receipt: &str) -> String {
    let marker = "\"lock_world\": \"";
    let start = receipt.find(marker).expect("lock_world") + marker.len();
    receipt[start..start + 16].to_string()
}
