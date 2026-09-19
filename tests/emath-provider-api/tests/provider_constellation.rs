//! Fork constellation contract and stable-IR boundary checks.

use std::path::{Path, PathBuf};

use emath_artifact::JsonValue;
use emath_provider_api::{UpstreamPin, fork_adapter_contracts, pinned_fork_adapters};
use emath_test_harness::Probe;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("workspace root")
}

fn scan_rust_sources(p: &mut Probe, dir: &Path, crate_dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("stable crate source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            scan_rust_sources(p, &path, crate_dir);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let source = std::fs::read_to_string(&path).expect("UTF-8 Rust source");
            for native in ["dew::", "rumoca::", "wrenfold::"] {
                p.demand(format!("{}:no-{native}", path.strip_prefix(crate_dir).unwrap_or(&path).display()), !source.contains(native), format!("{} leaks {native}", path.display()));
            }
        }
    }
}

#[test]
fn fork_constellation() {
    let mut p = Probe::new("fork adapters are pinned and provider types stay out of stable IR");
    p.case("contracts-pinned", |p| {
        let root = workspace_root();
        let lock = std::fs::read_to_string(root.join("forks/UPSTREAM_LOCK.json")).expect("committed upstream lock");
        let document = emath_artifact::parse_json_document(&lock).expect("valid upstream lock JSON");
        let JsonValue::Arr(repositories) = document.field("repositories").expect("repositories") else {
            p.fail("repositories", "repositories must be an array");
            return;
        };
        let pins = repositories.iter().map(|r| UpstreamPin { id: r.string_field("id").expect("pin id"), repository: r.string_field("repository").expect("pin repository"), commit: r.string_field("commit").expect("pin commit"), license: r.string_field("license").expect("pin license") }).collect::<Vec<_>>();
        let pinned = pinned_fork_adapters(&pins).expect("valid fork adapter pins");
        p.eq("count", pinned.len(), 3);
        p.eq("order", pinned.iter().map(|a| a.contract.upstream_id).collect::<Vec<_>>(), ["dew", "rumoca", "wrenfold"].to_vec());
        p.demand("locks", pinned.iter().all(|a| a.pin.commit.len() == 40 && !a.pin.license.is_empty()), "40-char commits and licenses");
        p.eq("contracts", fork_adapter_contracts(), &pinned.iter().map(|r| r.contract).collect::<Vec<_>>());
    });
    p.case("stable-ir-boundary", |p| {
        let root = workspace_root();
        for crate_name in ["emath-core", "emath-ir", "emath-plan", "emath-sema", "emath-rt", "emath-provider-api", "emath-artifact"] {
            let crate_dir = root.join("crates").join(crate_name);
            scan_rust_sources(p, &crate_dir.join("src"), &crate_dir);
            let manifest = std::fs::read_to_string(crate_dir.join("Cargo.toml")).expect("stable crate manifest");
            for dependency in ["dew", "rumoca", "wrenfold"] {
                p.demand(format!("{crate_name}:no-{dependency}-dep"), !manifest.lines().any(|line| { let line = line.trim_start(); line.starts_with(&format!("{dependency} =")) || line.starts_with(&format!("{dependency}-")) }), format!("{crate_name} must not depend on {dependency}"));
            }
        }
    });
    p.finish();
}
