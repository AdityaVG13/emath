use std::fs;
use std::path::{Path, PathBuf};

use emath_exec_ir::language_image::{
    LanguageImageError, compile_language_directory, load_language_distribution,
    write_language_distribution,
};

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

fn fixture() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "emath-language-distribution-{}",
        std::process::id()
    ));
    copy_tree(Path::new("../../language/spec"), &root.join("spec"));
    root
}

#[test]
fn production_distribution_builds_loads_and_rejects_drift() {
    let root = fixture();
    let first = compile_language_directory(&root).unwrap();
    let second = compile_language_directory(&root).unwrap();
    assert_eq!(first.image.semantic_hash, second.image.semantic_hash);
    assert_eq!(
        first.image.distribution_hash,
        second.image.distribution_hash
    );
    assert!(first.capsules.len() >= 748);
    write_language_distribution(&root, &first).unwrap();

    let loaded = load_language_distribution(&root).unwrap();
    assert_eq!(loaded.image, first.image);
    assert_eq!(loaded.authority, first.authority);
    assert!(
        loaded
            .authority_map()
            .values()
            .any(|state| state == "capsule-active")
    );

    let lock = root.join("language.lock");
    fs::write(&lock, "tampered\n").unwrap();
    assert!(matches!(
        load_language_distribution(&root),
        Err(LanguageImageError::GeneratedDrift { path }) if path == lock
    ));
}
