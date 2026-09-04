use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create destination");
    for entry in fs::read_dir(source).expect("read source directory") {
        let entry = entry.expect("read directory entry");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_tree(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path).expect("copy fixture file");
        }
    }
}

fn fixture() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "emath-production-language-gate-{}-{nonce}",
        std::process::id()
    ));
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    copy_tree(&workspace.join("language"), &root.join("language"));
    root
}

fn check(root: &Path) -> Output {
    Command::new(common::emath_bin())
        .args(["check", "language/examples/intro/add-exact.emath", "--json"])
        .current_dir(root)
        .output()
        .expect("run emath check")
}

fn refusal_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn semantic_commands_require_the_verified_checked_in_language_distribution() {
    let root = fixture();
    let clean = check(&root);
    assert!(clean.status.success(), "{}", refusal_text(&clean));
    assert!(String::from_utf8_lossy(&clean.stdout).contains("\"admitted\": true"));

    let lock = root.join("language/language.lock");
    let clean_lock = fs::read_to_string(&lock).expect("read lock");
    fs::write(&lock, format!("{clean_lock}tampered=true\n")).expect("tamper lock");
    let tampered = check(&root);
    assert_eq!(tampered.status.code(), Some(1));
    assert!(refusal_text(&tampered).contains("E-LANG-IMAGE"));
    fs::write(&lock, clean_lock).expect("restore lock");

    let source_map = root.join("language/generated/source-map.lock");
    let hidden_source_map = root.join("language/generated/source-map.lock.missing");
    fs::rename(&source_map, &hidden_source_map).expect("hide source map");
    let missing = check(&root);
    assert_eq!(missing.status.code(), Some(1));
    assert!(refusal_text(&missing).contains("E-LANG-IMAGE"));
    fs::rename(&hidden_source_map, &source_map).expect("restore source map");

    let add_capsule = root.join("language/spec/capabilities/core/add.emath");
    let clean_capsule = fs::read_to_string(&add_capsule).expect("read add capsule");
    let hidden_hole = clean_capsule.replace(
        "projection: \"semantics -> provided\"",
        "projection: \"semantics -> hole(hidden-active-hole | seeded refusal)\"",
    );
    assert_ne!(hidden_hole, clean_capsule);
    fs::write(&add_capsule, hidden_hole).expect("seed active hole");
    let hole = check(&root);
    assert_eq!(hole.status.code(), Some(1));
    assert!(refusal_text(&hole).contains("E-LANG-IMAGE"));
    fs::write(&add_capsule, clean_capsule).expect("restore add capsule");

    let restored = check(&root);
    assert!(restored.status.success(), "{}", refusal_text(&restored));
}
