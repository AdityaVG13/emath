use std::fs;
use std::path::{Path, PathBuf};

use emath_exec_ir::language_image::{
    LanguageImageError, compile_language_directory, load_language_distribution,
    write_language_distribution,
};

use emath_test_harness::Probe;

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


/// assert_eq/assert_ne semantics over references: borrows both operands
/// (like the macros) and allows PartialEq between distinct types.
fn eq_ref<T: ?Sized + std::fmt::Debug, U: ?Sized + std::fmt::Debug>(
    ph: &mut Probe,
    name: impl Into<String>,
    actual: &T,
    expected: &U,
) where
    T: PartialEq<U>,
{
    ph.demand(name, actual == expected, format!("expected {expected:?}, got {actual:?}"));
}

fn ne_ref<T: ?Sized + std::fmt::Debug, U: ?Sized + std::fmt::Debug>(
    ph: &mut Probe,
    name: impl Into<String>,
    actual: &T,
    unexpected: &U,
) where
    T: PartialEq<U>,
{
    ph.demand(name, actual != unexpected, format!("got forbidden value {unexpected:?}"));
}

fn production_distribution_builds_loads_and_rejects_drift(ph: &mut Probe) {
    ph.case("production_distribution_builds_loads_and_rejects_drift", |ph| {
    let root = fixture();
    let first = compile_language_directory(&root).unwrap();
    let second = compile_language_directory(&root).unwrap();
    eq_ref(ph, "1", &(first.image.semantic_hash), &( second.image.semantic_hash));
    eq_ref(ph, "2", &(
        first.image.distribution_hash), &(
        second.image.distribution_hash
    ));
    ph.demand("3", first.capsules.len() >= 748, "assertion failed: first.capsules.len() >= 748");
    write_language_distribution(&root, &first).unwrap();

    let loaded = load_language_distribution(&root).unwrap();
    eq_ref(ph, "4", &(loaded.image), &( first.image));
    eq_ref(ph, "5", &(loaded.authority), &( first.authority));
    ph.demand("6", 
        loaded
            .authority_map()
            .values()
            .any(|state| state == "capsule-active")
    , "assertion failed: loaded\n            .authority_map()\n            .values()\n            .any(|stat");

    let lock = root.join("language.lock");
    fs::write(&lock, "tampered\n").unwrap();
    ph.demand("7", matches!(
        load_language_distribution(&root),
        Err(LanguageImageError::GeneratedDrift { path }) if path == lock
    ), "assertion failed: matches!(\n        load_language_distribution(&root),\n        Err(LanguageImageEr");

    });
}

#[test]
fn production_distribution_contracts() {
    let mut ph = Probe::new("production language distribution builds, loads, and rejects drift deterministically");
    production_distribution_builds_loads_and_rejects_drift(&mut ph);
    ph.finish();
}
