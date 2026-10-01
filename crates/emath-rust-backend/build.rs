//! Embeds the reference engine's source into this crate so an artifact
//! that runs functions in the engine (`quote.cost`, `quote.world`,
//! `quote.worlds`) can carry it under `vendor/` and build without this
//! repository.
//!
//! The crate set is the `emath-*` dependency closure of `emath-exec-ir`,
//! read from the manifests, so it follows the engine as it changes. Each
//! manifest is rewritten without workspace inheritance; the workspace
//! lockfile is trimmed to the packages that closure reaches, so the
//! artifact builds against the same third-party versions as this tree.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const ENGINE: &str = "emath-exec-ir";

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.join("../..");
    let workspace_toml = read(&workspace.join("Cargo.toml"));
    let package = section(&workspace_toml, "workspace.package");
    let dependencies = section(&workspace_toml, "workspace.dependencies");

    let mut manifests = BTreeMap::new();
    let mut queue = vec![ENGINE.to_string()];
    while let Some(name) = queue.pop() {
        if manifests.contains_key(&name) {
            continue;
        }
        let dir = workspace.join("crates").join(&name);
        let (manifest, local) = standalone_manifest(&read(&dir.join("Cargo.toml")), &package, &dependencies);
        queue.extend(local);
        manifests.insert(name, manifest);
    }

    let mut out = String::from("pub(crate) static ENGINE_FILES: &[(&str, &str)] = &[\n");
    let license = workspace.join("LICENSE");
    rerun(&license);
    writeln!(out, "    (\"LICENSE\", include_str!({:?})),", absolute(&license)).unwrap();
    for (name, manifest) in &manifests {
        let dir = workspace.join("crates").join(name);
        rerun(&dir.join("Cargo.toml"));
        rerun(&dir.join("src"));
        writeln!(out, "    ({:?}, {manifest:?}),", format!("{name}/Cargo.toml")).unwrap();
        let mut files = Vec::new();
        collect(&dir.join("src"), &mut files);
        files.sort();
        for file in files {
            let relative = file.strip_prefix(&dir).expect("file under crate").to_string_lossy().replace('\\', "/");
            writeln!(out, "    ({:?}, include_str!({:?})),", format!("{name}/{relative}"), absolute(&file)).unwrap();
        }
    }
    out.push_str("];\n\n");

    let lock_path = workspace.join("Cargo.lock");
    rerun(&lock_path);
    let (header, packages) = trimmed_lock(&read(&lock_path), manifests.keys().cloned().collect());
    writeln!(out, "pub(crate) static ENGINE_LOCK_HEADER: &str = {header:?};").unwrap();
    out.push_str("pub(crate) static ENGINE_LOCK_PACKAGES: &[(&str, &str)] = &[\n");
    for (name, block) in packages {
        writeln!(out, "    ({name:?}, {block:?}),").unwrap();
    }
    out.push_str("];\n");

    let generated = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("engine_vendor.rs");
    std::fs::write(generated, out).expect("write engine_vendor.rs");
    rerun(&workspace.join("Cargo.toml"));
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn absolute(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|error| panic!("cannot resolve {}: {error}", path.display()))
        .display()
        .to_string()
}

fn rerun(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|error| panic!("cannot list {}: {error}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}

/// `key = value` lines of one `[name]` table.
fn section(toml: &str, name: &str) -> BTreeMap<String, String> {
    let header = format!("[{name}]");
    let mut inside = false;
    let mut entries = BTreeMap::new();
    for line in toml.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == header;
        } else if inside && !line.is_empty() && !line.starts_with('#') {
            if let Some((key, value)) = line.split_once('=') {
                entries.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    entries
}

/// A crate manifest with workspace inheritance resolved (package fields
/// inlined, `emath-*` dependencies as sibling paths under `vendor/`,
/// lints dropped), plus the `emath-*` crates it depends on.
fn standalone_manifest(
    manifest: &str,
    package: &BTreeMap<String, String>,
    dependencies: &BTreeMap<String, String>,
) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut local = Vec::new();
    let mut table = String::new();
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            table = trimmed.to_string();
        }
        if table == "[lints]" || table == "[dev-dependencies]" {
            continue;
        }
        if trimmed.starts_with('[') || trimmed.starts_with('#') || !trimmed.contains("workspace") {
            if table == "[dependencies]" && !trimmed.starts_with('#') {
                if let Some(name) = trimmed.split('=').next().map(str::trim).filter(|name| name.starts_with("emath-")) {
                    panic!("engine dependency `{name}` must inherit from the workspace to vendor");
                }
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let key = trimmed.split(['.', '=', ' ']).next().expect("manifest key");
        let inherits = trimmed == format!("{key}.workspace = true") || trimmed == format!("{key} = {{ workspace = true }}");
        assert!(inherits, "cannot vendor manifest line `{trimmed}`");
        match table.as_str() {
            "[package]" => {
                let value = package.get(key).unwrap_or_else(|| panic!("workspace.package has no `{key}`"));
                writeln!(out, "{key} = {value}").unwrap();
            }
            "[dependencies]" => {
                let spec = dependencies.get(key).unwrap_or_else(|| panic!("workspace.dependencies has no `{key}`"));
                if spec.contains("path = \"crates/") {
                    writeln!(out, "{key} = {{ path = \"../{key}\" }}").unwrap();
                    local.push(key.to_string());
                } else {
                    writeln!(out, "{key} = {spec}").unwrap();
                }
            }
            other => panic!("cannot vendor workspace inheritance in {other}"),
        }
    }
    (out, local)
}

/// The lockfile header and the `[[package]]` blocks (name, text) the
/// given crates reach, in lockfile order.
fn trimmed_lock(lock: &str, roots: Vec<String>) -> (String, Vec<(String, String)>) {
    let mut parts = lock.split("[[package]]\n");
    let header = parts.next().unwrap_or_default().to_string();
    let blocks: Vec<(String, String, Vec<String>, String)> = parts
        .map(|block| {
            let field = |key: &str| {
                block
                    .lines()
                    .find_map(|line| line.strip_prefix(&format!("{key} = \"")))
                    .map(|value| value.trim_end_matches('"').to_string())
                    .unwrap_or_default()
            };
            let needs = block
                .split_once("dependencies = [\n")
                .map(|(_, rest)| {
                    rest.lines()
                        .take_while(|line| *line != "]")
                        .map(|line| line.trim().trim_end_matches(',').trim_matches('"').to_string())
                        .collect()
                })
                .unwrap_or_default();
            (field("name"), field("version"), needs, format!("[[package]]\n{}\n", block.trim_end()))
        })
        .collect();
    let mut kept = BTreeSet::new();
    let mut queue = roots;
    while let Some(wanted) = queue.pop() {
        // A dependency entry is `name`, or `name version [(source)]` when
        // the lockfile holds several versions of it.
        let mut words = wanted.split_whitespace();
        let name = words.next().unwrap_or_default();
        let version = words.next();
        for (index, (block_name, block_version, needs, _)) in blocks.iter().enumerate() {
            if block_name == name && version.is_none_or(|version| version == block_version) && kept.insert(index) {
                queue.extend(needs.iter().cloned());
            }
        }
    }
    let packages = kept
        .into_iter()
        .map(|index| (blocks[index].0.clone(), blocks[index].3.clone()))
        .collect();
    (header, packages)
}
