//! Remaining host commands extracted from production `emath`.

use emath_cli::{CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE, list_published_artifact_ids};
use std::path::Path;

/// `import modelica <file.mo> [--json]`: retain a Modelica subset source as
/// foreign-model declarations with adapter identity. No source rewrite.
pub fn import_modelica_cmd(path: &Path, json: bool) -> CliExit {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", path.display());
            return EXIT_USAGE;
        }
    };
    match emath_adapter_rumoca::import_modelica(&source) {
        Ok(declarations) => {
            if json {
                let mut object = emath_artifact::JsonWriter::object();
                object.string("command", "import modelica");
                object.int("declarations", declarations.len() as u64);
                let names: Vec<String> = declarations
                    .iter()
                    .map(|declaration| declaration.name.clone())
                    .collect();
                object.strings("models", &names);
                println!("{}", object.finish());
            } else {
                for declaration in &declarations {
                    println!(
                        "foreign {} adapter={} parameters={} equations={} identity={:016x}",
                        declaration.name,
                        declaration.adapter,
                        declaration.parameters.join(","),
                        declaration.equations,
                        declaration.content_identity()
                    );
                }
            }
            EXIT_OK
        }
        Err(error) => {
            eprintln!("error: {} {}", error.code, error.message);
            EXIT_REFUSED
        }
    }
}

/// `artifact battery <dir>`: run the seeded negative-control battery over
/// every published artifact.
pub fn artifact_battery(dir: &Path) -> CliExit {
    let artifact_ids = match list_published_artifact_ids(dir) {
        Ok(ids) => ids,
        Err(code) => return code,
    };
    let artifact_root = dir.join("emath");
    let mut ok = true;
    for id in artifact_ids {
        let root = artifact_root.join(&id);
        match emath_evidence::checker::artifact_input_from_dir(&root) {
            Ok(input) => {
                let run = emath_evidence::checker::run_standard_battery(&input);
                for control in &run.refused {
                    println!("artifact {id}: control refused ({control})");
                }
                for (control, detail) in &run.escaped {
                    eprintln!("artifact {id}: control ESCAPED ({control}): {detail}");
                }
                if run.all_refused() {
                    println!(
                        "artifact {id}: battery clean ({} controls, {})",
                        run.refused.len() + run.escaped.len(),
                        run.refused.join(", ")
                    );
                } else {
                    ok = false;
                }
            }
            Err(error) => {
                eprintln!("artifact {id}: battery FAILED: {error}");
                ok = false;
            }
        }
    }
    if ok { EXIT_OK } else { EXIT_REFUSED }
}

/// Stdout document for `emath architecture --json`.
pub fn architecture_json() -> String {
    let pipeline = ".emath -> SIR -> GIR -> resolution plan -> EMIR -> Rust artifact -> protected host promotion";
    let paths: Vec<String> = emath_artifact::required_artifact_paths()
        .iter()
        .map(ToString::to_string)
        .collect();
    let mut object = emath_artifact::JsonWriter::object();
    object.string("schema", "emath.architecture");
    object.string("pipeline", pipeline);
    object.strings("required_paths", &paths);
    object.finish()
}

/// `architecture [--json]`: provider-neutral pipeline description.
pub fn architecture(json: bool) -> CliExit {
    if json {
        print!("{}", architecture_json());
    } else {
        let pipeline = ".emath -> SIR -> GIR -> resolution plan -> EMIR -> Rust artifact -> protected host promotion";
        let paths: Vec<String> = emath_artifact::required_artifact_paths()
            .iter()
            .map(ToString::to_string)
            .collect();
        println!("{pipeline}");
        println!("provider-neutral required paths: {paths:?}");
    }
    EXIT_OK
}

/// `bench <file>`: typed refusal until the keep-gate comparison ruleset lands.
pub fn bench_cmd(file: &Path) -> CliExit {
    eprintln!(
        "error: E-TLT-004: benchmarking `{}` is not a Phase 1 CLI comparison; measure via `cargo bench --profile release-perf --bench comprehensive_bench` (keep-gate history in .bench-history/)",
        file.display()
    );
    EXIT_REFUSED
}
