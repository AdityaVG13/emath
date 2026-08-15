//! emath CLI: `check`, `plan`, `build`, `artifact`, `architecture`, and the
//! Semantic Genesis commands (`parse`, `signature`, `genesis`, `compile
//! --parametric`, `world show`, `portfolio show`).
//! Exit codes: 0 success, 1 refusal/diagnostic, 2 usage or io error.

#![forbid(unsafe_code)]

mod genesis_cmd;

use emath_artifact::{verify_artifact, StagedFile, Staging};
use emath_build::{build_file, BuildOptions};
use emath_core::Diagnostics;
use emath_sema::session::CompilerSession;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const EXIT_OK: u8 = 0;
pub const EXIT_REFUSED: u8 = 1;
pub const EXIT_USAGE: u8 = 2;

pub fn print_diagnostics(diagnostics: &Diagnostics) {
    for item in diagnostics.items() {
        println!(
            "{} {} ({}:{})",
            item.code, item.message, item.primary.file.0, item.primary.start
        );
    }
}

/// `check <file> [--json]`: parse + admit, no codegen.
pub fn check(path: &Path, json: bool) -> u8 {
    let path = path.to_path_buf();
    let (diagnostics, package_id) = run_check(&path);
    print_diagnostics(&diagnostics);
    if json {
        let mut out = emath_artifact::JsonWriter::object();
        out.string("command", "check");
        out.bool("admitted", !diagnostics.has_errors());
        out.int("diagnostics", diagnostics.len() as u64);
        out.string("package", &package_id);
        println!("{}", out.finish());
    }
    if diagnostics.has_errors() {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
}

fn run_check(path: &PathBuf) -> (Diagnostics, String) {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        let mut diagnostics = Diagnostics::new();
        diagnostics.error(
            "E-PKG-080",
            "cannot read source file",
            emath_core::Span::default(),
        );
        return (diagnostics, String::new());
    };
    let result = session.check(package.file);
    let package_id = result.package.content_id().0;
    (result.diagnostics, package_id)
}

/// `plan <file> [--json]`: check + goals + plans, no artifact.
pub fn plan(path: &PathBuf, json: bool) -> u8 {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        eprintln!("error: cannot read {}", path.display());
        return EXIT_USAGE;
    };
    let result = session.plan(package.file);
    if !result.diagnostics.is_empty() {
        print_diagnostics(&result.diagnostics);
    }
    if json {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("command", "plan");
        object.bool("admitted", !result.diagnostics.has_errors());
        object.int("goals", result.package.goals.len() as u64);
        object.int("plans", result.plans.len() as u64);
        let mut goals = String::new();
        for goal in &result.package.goals {
            let entry = format!("{} {} ", goal.kind.as_str(), goal.target);
            goals.push_str(&entry);
        }
        object.string("goals", &goals);
        println!("{}", object.finish());
    }
    for plan in &result.plans {
        println!(
            "plan {} goal={} policy={} class={}",
            plan.plan_id.0,
            plan.goal.index(),
            plan.policy,
            plan.artifact_class
        );
    }
    if result.diagnostics.has_errors() {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
}

/// `build <file> --out <dir> [--verify] [--json]`.
pub fn build(spec: &PathBuf, out: &PathBuf, verify: bool, json: bool) -> u8 {
    let options = BuildOptions {
        verify_generated_crate: verify,
    };
    match build_file(spec, out, options) {
        Ok(report) => {
            println!(
                "artifact {} (crate {}) → {}",
                report.artifact_id.0,
                report.crate_name,
                report.artifact_dir.display()
            );
            for assumption in &report.assumptions {
                println!("  assumption: {assumption}");
            }
            if json {
                let mut object = emath_artifact::JsonWriter::object();
                object.string("command", "build");
                object.string("artifact_id", &report.artifact_id.0);
                object.string("package_id", &report.package_id.0);
                object.string("crate", &report.crate_name);
                object.string("artifact_dir", &report.artifact_dir.display().to_string());
                object.strings("plan_ids", &report.plan_ids);
                object.strings("exports", &report.exports);
                println!("{}", object.finish());
            }
            EXIT_OK
        }
        Err(error) => {
            eprintln!("error: {error}");
            if error.to_string().contains("admission refused") {
                EXIT_REFUSED
            } else {
                EXIT_USAGE
            }
        }
    }
}

/// `import modelica <file.mo> [--json]`: retain a Modelica subset source as
/// foreign-model declarations with adapter identity. No source rewrite.
pub fn import_modelica_cmd(path: &Path, json: bool) -> u8 {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", path.display());
            return EXIT_USAGE;
        }
    };
    match emath_adapter_rumoca::import::import_modelica(&source) {
        Ok(declarations) => {
            if json {
                let mut object = emath_artifact::JsonWriter::object();
                object.string("command", "import modelica");
                object.int("declarations", declarations.len() as u64);
                let mut names = String::new();
                for declaration in &declarations {
                    let entry = format!("{} ", declaration.name);
                    names.push_str(&entry);
                }
                object.string("models", &names);
                println!("{}", object.finish());
            }
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
            EXIT_OK
        }
        Err(error) => {
            eprintln!("error: {} {}", error.code, error.message);
            EXIT_REFUSED
        }
    }
}

/// `artifact <dir> check`: re-verify fingerprints from the published
/// artifact directory (`<dir>/emath/<artifact-id>`).
pub fn artifact_check(dir: &Path) -> u8 {
    let artifact_root = dir.join("emath");
    if !artifact_root.is_dir() {
        eprintln!("error: no `emath/` state directory under {}", dir.display());
        return EXIT_USAGE;
    }
    let Ok(entries) = std::fs::read_dir(&artifact_root) else {
        eprintln!("error: cannot list {}", artifact_root.display());
        return EXIT_USAGE;
    };
    let mut ok = true;
    for entry in entries.flatten() {
        let id = entry.file_name().to_string_lossy().into_owned();
        if !entry.path().is_dir() {
            continue;
        }
        let mut files = Vec::new();
        for root in required_local(&entry.path()) {
            let Ok(bytes) = std::fs::read(&root) else {
                eprintln!("error: unreadable {}", root.display());
                ok = false;
                continue;
            };
            files.push(StagedFile {
                relative_path: root
                    .strip_prefix(entry.path())
                    .map_or_else(|_| "?".to_string(), |p| p.to_string_lossy().into_owned()),
                bytes,
            });
        }
        match verify_one_artifact(&entry.path()) {
            Ok(()) => println!("artifact {id}: verified"),
            Err(detail) => {
                eprintln!("artifact {id}: FAILED: {detail}");
                ok = false;
            }
        }
    }
    if ok {
        EXIT_OK
    } else {
        EXIT_REFUSED
    }
}

fn required_local(root: &std::path::Path) -> Vec<PathBuf> {
    emath_artifact::required_artifact_paths()
        .iter()
        .map(|p| root.join(p))
        .collect()
}

fn verify_one_artifact(root: &std::path::Path) -> Result<(), String> {
    let mut files = Vec::new();
    for path in required_local(root) {
        let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
        files.push(StagedFile {
            relative_path: path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            bytes,
        });
    }
    let staging: Staging =
        emath_artifact::stage(&files, None).map_err(|error| error.to_string())?;
    verify_artifact(root, &staging).map_err(|error| error.to_string())
}

/// `architecture`: provider-neutral pipeline description.
pub fn architecture() -> u8 {
    println!(".emath -> SIR -> GIR -> resolution plan -> EMIR -> Rust artifact -> protected host promotion");
    println!(
        "provider-neutral required paths: {:?}",
        emath_artifact::required_artifact_paths()
    );
    EXIT_OK
}

/// `help` output.
pub fn help_text() -> String {
    "\
emath compiler (Phase 1 + Semantic Genesis G0-G3)

usage:
  emath check <file.emath> [--json]
      parse + admit, no codegen
  emath plan <file.emath> [--json]
      admit + goals + deterministic native resolution plan
  emath build <file.emath> --out <dir> [--verify] [--json]
      full pipeline; publishes artifact under <dir>/emath/<artifact-id>
      --verify runs `cargo test` on the staged crate before publish
  emath artifact <dir> check
      re-verify every published artifact's fingerprints (independent checker)
  emath import modelica <file.mo> [--json]
      retain a Modelica subset source as foreign-model declarations with
      adapter identity (no silent source rewrite)
  emath parse --forest <file.emath> [--out <dir>]
      genesis glyphs + bounded parse forest (parse-forest.json)
  emath signature <file.emath> [--out <dir>]
      arity/fixity/type-variable signature inference (signature.json)
  emath genesis <file.emath> --out <dir>
      full analysis: forest, signature, free term, meaning problem,
      world candidates + admission log, portfolio, answer receipt
  emath compile --parametric <file.emath> --out <dir> [--world LABEL]
      emit the parametric generated crate (free_symbolic, boolean_algebra,
      modular_numeric) + manifest + source map
  emath world show WORLD_ID --dir <dir>
      print one world candidate artifact
  emath portfolio show PORTFOLIO_ID --dir <dir>
      print one interpretation portfolio artifact
  emath architecture
      describe the provider-neutral pipeline
  emath help
      this text

exit codes: 0 ok, 1 refused/admission diagnostics, 2 usage or io error
"
    .to_string()
}

/// Entry used by main; keeps the CLI testable.
pub fn run(args: &[String]) -> u8 {
    let Some(command) = args.first() else {
        print!("{}", help_text());
        return EXIT_OK;
    };
    match command.as_str() {
        "check" => {
            let (path, json) = parse_file_args(&args[1..]);
            match path {
                Some(path) => check(&path, json),
                None => usage("check <file.emath> [--json]"),
            }
        }
        "plan" => {
            let (path, json) = parse_file_args(&args[1..]);
            match path {
                Some(path) => plan(&path, json),
                None => usage("plan <file.emath> [--json]"),
            }
        }
        "build" => {
            let mut path = None;
            let mut out = None;
            let mut verify = false;
            let mut json = false;
            let mut index = 1;
            while index < args.len() {
                match args[index].as_str() {
                    "--out" | "-o" => {
                        index += 1;
                        if index < args.len() {
                            out = Some(PathBuf::from(&args[index]));
                        }
                    }
                    "--verify" => verify = true,
                    "--json" => json = true,
                    other if other.starts_with('-') => {
                        return usage("build <file.emath> --out <dir> [--verify] [--json]")
                    }
                    other => path = Some(PathBuf::from(other)),
                }
                index += 1;
            }
            match (path, out) {
                (Some(spec), Some(out)) => build(&spec, &out, verify, json),
                _ => usage("build <file.emath> --out <dir> [--verify] [--json]"),
            }
        }
        "parse" => {
            let (path, out, _) = parse_genesis_args(&args[1..]);
            let forest_only = args[1..].iter().any(|arg| arg == "--forest");
            match path {
                Some(path) => genesis_cmd::parse_cmd(&path, out.as_ref(), forest_only),
                None => usage("parse --forest <file.emath> [--out <dir>]"),
            }
        }
        "signature" => {
            let (path, out, _) = parse_genesis_args(&args[1..]);
            match path {
                Some(path) => genesis_cmd::signature_cmd(&path, out.as_ref()),
                None => usage("signature <file.emath> [--out <dir>]"),
            }
        }
        "genesis" => {
            let (path, out, _) = parse_genesis_args(&args[1..]);
            match (path, out) {
                (Some(path), Some(out)) => genesis_cmd::genesis_cmd(&path, &out),
                _ => usage("genesis <file.emath> --out <dir>"),
            }
        }
        "compile" => {
            let (path, out, worlds) = parse_genesis_args(&args[1..]);
            let parametric = args[1..].iter().any(|arg| arg == "--parametric");
            match (path, out, parametric) {
                (Some(path), Some(out), true) => genesis_cmd::compile_cmd(&path, &out, &worlds),
                _ => usage("compile --parametric <file.emath> --out <dir> [--world LABEL]"),
            }
        }
        "world" => {
            if args.get(1).is_some_and(|sub| sub == "show") && args.len() >= 3 {
                let (_, dir, _) = parse_genesis_args(&args[3..]);
                let id = &args[2];
                match dir {
                    Some(dir) => genesis_cmd::world_show_cmd(id, &dir),
                    None => usage("world show WORLD_ID --dir <dir>"),
                }
            } else {
                usage("world show WORLD_ID --dir <dir>")
            }
        }
        "portfolio" => {
            if args.get(1).is_some_and(|sub| sub == "show") && args.len() >= 3 {
                let (_, dir, _) = parse_genesis_args(&args[3..]);
                let id = &args[2];
                match dir {
                    Some(dir) => genesis_cmd::portfolio_show_cmd(id, &dir),
                    None => usage("portfolio show PORTFOLIO_ID --dir <dir>"),
                }
            } else {
                usage("portfolio show PORTFOLIO_ID --dir <dir>")
            }
        }
        "import" => {
            if args.get(1).is_some_and(|sub| sub == "modelica") && args.len() >= 3 {
                let json = args[2..].iter().any(|arg| arg == "--json");
                import_modelica_cmd(&PathBuf::from(&args[2]), json)
            } else {
                usage("import modelica <file.mo> [--json]")
            }
        }
        "artifact" => {
            if args.get(1).is_some_and(|c| c == "check") && args.len() >= 3 {
                artifact_check(&PathBuf::from(&args[2]))
            } else {
                usage("artifact <dir> check")
            }
        }
        "architecture" => architecture(),
        "help" | "--help" | "-h" => {
            print!("{}", help_text());
            EXIT_OK
        }
        other => {
            eprintln!("error: unknown command `{other}`");
            print!("{}", help_text());
            EXIT_USAGE
        }
    }
}

/// Shared arg scan for genesis commands: positional file, `--out`, `--world`.
fn parse_genesis_args(args: &[String]) -> (Option<PathBuf>, Option<PathBuf>, Vec<String>) {
    let mut path = None;
    let mut out = None;
    let mut worlds = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" | "--dir" => {
                index += 1;
                if index < args.len() {
                    out = Some(PathBuf::from(&args[index]));
                }
            }
            "--world" => {
                index += 1;
                if index < args.len() {
                    worlds.push(args[index].clone());
                }
            }
            "--parametric" | "--forest" | "--json" => {}
            other if other.starts_with('-') => {}
            other => path = Some(PathBuf::from(other)),
        }
        index += 1;
    }
    (path, out, worlds)
}

fn parse_file_args(args: &[String]) -> (Option<PathBuf>, bool) {
    let mut path = None;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            other if other.starts_with('-') && other != "-" => {}
            other => path = Some(PathBuf::from(other)),
        }
    }
    (path, json)
}

fn usage(message: &str) -> u8 {
    eprintln!("usage: emath {message}");
    eprintln!("run `emath help` for the full command list");
    EXIT_USAGE
}

/// JSON pretty-helper used by tests.
#[allow(dead_code)]
pub(crate) fn write_json(out: &mut impl Write, fields: &[(&str, String)]) -> std::io::Result<()> {
    let mut object = emath_artifact::JsonWriter::object();
    for (name, value) in fields {
        object.string(name, value);
    }
    write!(out, "{}", object.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_is_positive() {
        assert_eq!(run(&["help".to_string()]), EXIT_OK);
    }

    #[test]
    fn unknown_command_is_usage() {
        assert_eq!(run(&["bogus".to_string()]), EXIT_USAGE);
    }

    #[test]
    fn architecture_is_positive() {
        assert_eq!(run(&["architecture".to_string()]), EXIT_OK);
    }

    #[test]
    fn check_minimal_file_admits() {
        let dir = std::env::temp_dir().join(format!("emath-cli-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("minimal.emath");
        std::fs::write(
            &file,
            "emath custom <Square> as function:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
",
        )
        .unwrap();
        assert_eq!(
            run(&["check".to_string(), file.display().to_string()]),
            EXIT_OK
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_rejects_duplicate_input() {
        let dir = std::env::temp_dir().join(format!("emath-cli-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("dup.emath");
        std::fs::write(
            &file,
            "emath custom <Dup> as function:
    inputs:
        x: Float64
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
",
        )
        .unwrap();
        assert_eq!(
            run(&["check".to_string(), file.display().to_string()]),
            EXIT_REFUSED
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_id_is_typed() {
        let file = emath_core::FileId(0);
        assert_eq!(file.0, 0);
    }

    #[test]
    fn import_modelica_retains_foreign_declaration() {
        let dir = std::env::temp_dir().join(format!("emath-cli-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("mass_spring.mo");
        std::fs::write(
            &file,
            "model MassSpring\n  parameter Real m = 1;\n  Real x;\nequation\n  der(x) = 0;\nend MassSpring;\n",
        )
        .unwrap();
        assert_eq!(
            run(&[
                "import".to_string(),
                "modelica".to_string(),
                file.display().to_string()
            ]),
            EXIT_OK
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_modelica_unsupported_construct_refused() {
        let dir = std::env::temp_dir().join(format!("emath-cli-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("sampled.mo");
        std::fs::write(
            &file,
            "model Sampled\n  Real x;\nequation\n  x = sample(0, 1);\nend Sampled;\n",
        )
        .unwrap();
        assert_eq!(
            run(&[
                "import".to_string(),
                "modelica".to_string(),
                file.display().to_string()
            ]),
            EXIT_REFUSED
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
