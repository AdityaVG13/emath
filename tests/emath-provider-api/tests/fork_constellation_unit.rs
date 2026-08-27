//! Fork constellation contract and stable-IR boundary checks.

use std::path::{Path, PathBuf};

use emath_artifact::JsonValue;
use emath_provider_api::{UpstreamPin, fork_adapter_contracts, pinned_fork_adapters};

#[test]
fn fork_constellation_contracts_have_license_and_source_locks() {
    let root = workspace_root();
    let lock = std::fs::read_to_string(root.join("forks/UPSTREAM_LOCK.json"))
        .expect("committed upstream lock");
    let document = emath_artifact::parse_json_document(&lock).expect("valid upstream lock JSON");
    let JsonValue::Arr(repositories) = document.field("repositories").expect("repositories") else {
        panic!("repositories must be an array");
    };
    let pins = repositories
        .iter()
        .map(|repository| UpstreamPin {
            id: repository.string_field("id").expect("pin id"),
            repository: repository
                .string_field("repository")
                .expect("pin repository"),
            commit: repository.string_field("commit").expect("pin commit"),
            license: repository.string_field("license").expect("pin license"),
        })
        .collect::<Vec<_>>();
    let pinned = pinned_fork_adapters(&pins).expect("valid fork adapter pins");

    assert_eq!(pinned.len(), 3);
    assert_eq!(
        pinned
            .iter()
            .map(|adapter| adapter.contract.upstream_id)
            .collect::<Vec<_>>(),
        ["dew", "rumoca", "wrenfold"]
    );
    assert!(
        pinned
            .iter()
            .all(|adapter| adapter.pin.commit.len() == 40 && !adapter.pin.license.is_empty())
    );
    assert_eq!(
        fork_adapter_contracts(),
        pinned.iter().map(|row| row.contract).collect::<Vec<_>>()
    );
}

#[test]
fn fork_constellation_provider_types_do_not_leak_into_stable_ir() {
    let root = workspace_root();
    for crate_name in [
        "emath-core",
        "emath-ir",
        "emath-goal",
        "emath-plan",
        "emath-sema",
        "emath-runtime",
        "emath-provider-api",
        "emath-artifact",
    ] {
        let crate_dir = root.join("crates").join(crate_name);
        scan_rust_sources(&crate_dir.join("src"), &crate_dir);
        let manifest =
            std::fs::read_to_string(crate_dir.join("Cargo.toml")).expect("stable crate manifest");
        for dependency in ["dew", "rumoca", "wrenfold"] {
            assert!(
                !manifest.lines().any(|line| {
                    let line = line.trim_start();
                    line.starts_with(&format!("{dependency} ="))
                        || line.starts_with(&format!("{dependency}-"))
                }),
                "{crate_name} must not depend on provider crate `{dependency}`"
            );
        }
    }
}

fn scan_rust_sources(dir: &Path, crate_dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("stable crate source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            scan_rust_sources(&path, crate_dir);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = std::fs::read_to_string(&path).expect("UTF-8 Rust source");
            for provider_path in ["dew::", "rumoca::", "wrenfold::"] {
                assert!(
                    !source.contains(provider_path),
                    "{} leaks provider-native path `{provider_path}`",
                    path.strip_prefix(crate_dir).unwrap_or(&path).display()
                );
            }
        }
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}
