//! Native-provider registration, Modelica import, artifact and architecture commands.

use super::*;

/// Registers the in-tree static `native.rust` capability
/// (`evaluate.rust.library` → f64, exact, deterministic, E2 ceiling) so
/// the generic planner serves the same goals the native pipeline plans.
/// This mirrors the `provider list` status table, never a new capability.
pub(super) fn register_native_rust(registry: &mut ProviderRegistry) {
    use emath_ir::EvidenceLevel;
    use emath_provider_api::{
        CapabilitySpec, CapabilityTable, ProviderIsolation, ProviderLock, RepresentationSpec,
    };
    let table = CapabilityTable {
        capabilities: vec![
            CapabilitySpec {
                // Exact produce match (`evaluate.rust.library`), not a prefix:
                // a bare `evaluate` capability would serve every evaluate goal
                // and hide unplanned produce targets (CONF-0028).
                name: "evaluate.rust.library".into(),
                semantic_subset: "rust-library".into(),
                representations: vec![RepresentationSpec {
                    name: "f64".into(),
                    exact_relation: "bit-identical".into(),
                    encode_cost: 0,
                }],
                exactness: vec!["exact".into()],
                failure_modes: vec![],
                checker_bindings: vec!["sir-checker".into()],
            },
            CapabilitySpec {
                name: "simplify".into(),
                semantic_subset: "symbolic".into(),
                representations: vec![RepresentationSpec {
                    name: "exact-integer-expression".into(),
                    exact_relation: "structural-checked".into(),
                    encode_cost: 0,
                }],
                exactness: vec!["exact".into()],
                failure_modes: vec!["E-SYM-002".into(), "E-SYM-003".into()],
                checker_bindings: vec!["native-symbolic-v1".into()],
            },
        ],
        isolation: ProviderIsolation::Static,
        lock: ProviderLock::Unlocked,
        maximum_evidence: EvidenceLevel::E2,
        deterministic: true,
    };
    if let Err(error) = registry.register("native.rust", ProviderIsolation::Static, table) {
        // Static bootstrap table is well-formed; a failure is an internal
        // registry defect, not user input. Keep planning usable by logging.
        eprintln!("warning: native.rust capability registration failed: {error:?}");
    }
}

pub fn list_published_artifact_ids(dir: &Path) -> Result<Vec<String>, CliExit> {
    let artifact_root = dir.join("emath");
    if !artifact_root.is_dir() {
        eprintln!(
            "error: E-EVID-105: no `emath/` state directory under {}",
            dir.display()
        );
        return Err(EXIT_USAGE);
    }
    let Ok(entries) = std::fs::read_dir(&artifact_root) else {
        eprintln!(
            "error: E-TLT-005: cannot list artifact state directory {}",
            artifact_root.display()
        );
        return Err(EXIT_USAGE);
    };
    let mut artifact_ids = Vec::new();
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            artifact_ids.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    artifact_ids.sort_unstable();
    if artifact_ids.is_empty() {
        eprintln!(
            "error: E-EVID-105: no published artifacts under {}",
            artifact_root.display()
        );
        return Err(EXIT_REFUSED);
    }
    Ok(artifact_ids)
}

/// `artifact check <dir>`: independent verification of every published
/// artifact under `<dir>/emath/<artifact-id>` via `emath-evidence`'s `checker` module
/// (one identity, one checker; empty state dirs are refused).
pub fn artifact_check(dir: &Path) -> CliExit {
    let artifact_ids = match list_published_artifact_ids(dir) {
        Ok(ids) => ids,
        Err(code) => return code,
    };
    let artifact_root = dir.join("emath");
    let mut ok = true;
    for id in artifact_ids {
        let root = artifact_root.join(&id);
        match emath_evidence::checker::check_artifact_dir(&root) {
            Ok(report) if report.valid() => {
                println!("artifact {id}: verified ({} files)", report.files_verified);
            }
            Ok(report) => {
                for issue in &report.issues {
                    eprintln!("artifact {id}: {}: {}", issue.code, issue.message);
                }
                ok = false;
            }
            Err(error) => {
                eprintln!("artifact {id}: FAILED: {error}");
                ok = false;
            }
        }
    }
    if ok { EXIT_OK } else { EXIT_REFUSED }
}

/// `help` output. Generated from the command catalog so usage and summary
/// cannot drift from `emath help <command>` / `emath capabilities --json`.
pub fn help_text() -> String {
    let mut out = String::from("emath compiler (constructor layer). Usage:\n");
    for command in catalog::COMMANDS {
        let Some(usage) = catalog::command_usage(command) else {
            continue;
        };
        let Some(summary) = catalog::command_summary(command) else {
            continue;
        };
        out.push_str("  emath ");
        out.push_str(usage);
        out.push('\n');
        out.push_str("      ");
        out.push_str(summary);
        out.push('\n');
        let aliases = catalog::command_aliases(command);
        if !aliases.is_empty() {
            out.push_str("      aliases: ");
            out.push_str(&aliases.join(", "));
            out.push('\n');
        }
    }
    out.push_str("\nexit codes: 0 completed, 2 admission, 3 unmet/partial, 4 fault, 5 checkpoint\n");
    out
}
