//! Tooling commands: `new`, `fmt`, `explain`, `run`, `test`, `bench`,
//! `verify`, `inspect`, `diff`, `doctor`, `vendor`, `provider`, `fork`,
//! and the structured `agent` API envelope.
//!
//! Implemented commands exercise the real pipeline (check/plan/build,
//! artifact verification). Capabilities outside the Phase 1 subset are
//! typed refusals with stable codes (`E-TLT-*`, `E-PROV-*`); nothing is
//! silently accepted.

use std::path::{Path, PathBuf};
use std::process::Command;

use emath_artifact::JsonWriter;
use emath_build::{BuildOptions, build_file, generated_crate_target_dir, run_cargo_timed};
use emath_core::content_id_of_str;
use emath_sema::session::CompilerSession;

use crate::catalog;
use crate::{EXIT_OK, EXIT_REFUSED, EXIT_USAGE, artifact_check, print_diagnostics, usage};

/// Relative path of the committed upstream lock file (repo layout).
const UPSTREAM_LOCK_REL: &str = "forks/UPSTREAM_LOCK.json";

/// Absolute lock path: crate-relative so `vendor`/`fork` work regardless of
/// the caller's working directory.
fn upstream_lock_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(UPSTREAM_LOCK_REL)
}

/// Built-in provider descriptors: (id, capability, status). Status must
/// match in-tree reality: std-only native stand-ins `implemented`;
/// upstream lanes (Dew JIT/GPU, full Rumoca, Wrenfold, Franken*) always
/// `planned`.
const PROVIDERS: [(&str, &str, &str); 10] = [
    (
        "native.rust",
        "scalar codegen + checked constructors",
        "implemented",
    ),
    (
        "evidence.rust",
        "artifact verification + negative controls",
        "implemented",
    ),
    (
        "dew.scalar",
        "scalar strict-f64 mapping; Rust source + token backends (in-tree, std-only)",
        "implemented",
    ),
    (
        "native.causal",
        "neutral DAE plan: structural gate + native lowering",
        "implemented",
    ),
    (
        "native.euler",
        "forward-Euler simulation of causal DAE plans",
        "implemented",
    ),
    (
        "rumoca.subset-import",
        "Modelica subset scanner -> retained declarations (no upstream parse)",
        "implemented",
    ),
    (
        "phase2.expression",
        "upstream Dew engine: optimization, JIT, GPU backends",
        "planned",
    ),
    (
        "phase3.structural",
        "upstream Rumoca engine: full parse, flattening, DAE analysis",
        "planned",
    ),
    (
        "phase4.symbolic",
        "cross-engine differential corpus against a real Dew/Rumoca checkout",
        "planned",
    ),
    ("phase5.numerics", "solvers and optimization", "planned"),
];

/// Dispatch for all tooling subcommands added by the tooling slice.
pub fn tooling_dispatch(command: &str, args: &[String]) -> u8 {
    match command {
        "new" => new_cmd(args),
        "fmt" => fmt_cmd(args),
        "explain" => explain_cmd(args),
        "run" => run_cmd(args),
        "test" => test_cmd(args),
        "bench" => bench_cmd(args),
        "verify" => verify_cmd(args),
        "inspect" => inspect_cmd(args),
        "diff" => diff_cmd(args),
        "doctor" => doctor_cmd(args),
        "vendor" => vendor_cmd(args),
        "provider" => provider_cmd(args),
        "fork" => fork_cmd(args),
        "agent" => crate::agent_cmd::agent_cmd(args),
        _ => EXIT_USAGE,
    }
}

/// `new <name> --out <dir>`: deterministic project scaffold.
fn new_cmd(args: &[String]) -> u8 {
    let Some(name) = args.first() else {
        return usage("new <name> [--out <dir>]");
    };
    if !is_valid_name(name) {
        eprintln!("error: invalid package name `{name}` (E-TLT-010)");
        return EXIT_USAGE;
    }
    // Default project dir is ./<name>; --out moves it.
    let out = flag_value("--out", args)
        .or_else(|| flag_value("-o", args))
        .map_or_else(|| PathBuf::from(name), PathBuf::from);
    let main = out.join("src/main.emath");
    let manifest = out.join("emath-package.toml");
    if main.exists() || manifest.exists() {
        eprintln!(
            "error: refusing to overwrite existing project at {} (E-TLT-011)",
            out.display()
        );
        return EXIT_REFUSED;
    }
    if std::fs::create_dir_all(out.join("src")).is_err() {
        eprintln!("error: cannot create {}", out.display());
        return EXIT_USAGE;
    }
    let gitignore_body = "# Local interpretation lock (per-user, per-project).\n# Teams MAY commit .emath/meaning.lock to share one interpretation.\n.emath/meaning.lock\n";
    if let Err(error) = std::fs::create_dir_all(out.join(".emath")) {
        eprintln!(
            "error: cannot create {}: {error}",
            out.join(".emath").display()
        );
        return EXIT_USAGE;
    }
    if let Err(error) = std::fs::write(out.join(".gitignore"), gitignore_body) {
        eprintln!(
            "error: cannot write {}: {error}",
            out.join(".gitignore").display()
        );
        return EXIT_USAGE;
    }
    let manifest_body = format!(
        "# Generated by emath new (deterministic template; edit freely).\nschema = \"emath.package\"\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"0.1\"\nmain = \"src/main.emath\"\n",
    );
    let main_body = "emath function Greeter:\n    inputs:\n        x: Float64\n    definitions:\n        y = x\n";
    let result =
        std::fs::write(&manifest, &manifest_body).and_then(|()| std::fs::write(&main, main_body));
    if result.is_err() {
        eprintln!("error: cannot write scaffold under {}", out.display());
        return EXIT_USAGE;
    }
    println!(
        "created {} (content id {})",
        manifest.display(),
        content_id_of_str(&manifest_body).0
    );
    println!(
        "created {} (content id {})",
        main.display(),
        content_id_of_str(main_body).0
    );
    println!("next: emath run {}", main.display());
    EXIT_OK
}

fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

/// `fmt <file>`: canonical-form check via the lossless formatter;
/// canonical only on byte-for-byte round-trip, else refusal + diff.
fn fmt_cmd(args: &[String]) -> u8 {
    let Some(file) = first_positional(args) else {
        return usage("fmt <file.emath>");
    };
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(&file) else {
        eprintln!("error: cannot read {file}");
        return EXIT_USAGE;
    };
    let result = session.check(package.file);
    print_diagnostics(&result.diagnostics);
    if result.diagnostics.has_errors() {
        return EXIT_REFUSED;
    }
    let limits = emath_core::limits::Limits::default();
    let lossless = emath_syntax::parse_lossless(&package.text, package.file, &limits);
    let canonical = emath_syntax::formatter::format(&lossless.tree, &lossless.comments);
    if canonical == package.text {
        println!("fmt: {file}: canonical form (lossless round-trip)");
        EXIT_OK
    } else {
        eprintln!("fmt: {file}: NOT canonical; expected lossless formatter output");
        for (line_no, (expected, actual)) in canonical
            .lines()
            .zip(package.text.lines())
            .enumerate()
            .filter(|(_, (expected, actual))| expected != actual)
            .take(10)
        {
            eprintln!(
                "  line {}: expected `{expected}`, found `{actual}`",
                line_no + 1
            );
        }
        EXIT_REFUSED
    }
}

/// `explain <file> [<symbol>]` or `explain E-LAW-001`: plan-level or checker witness.
fn explain_cmd(args: &[String]) -> u8 {
    let Some(file) = first_positional(args) else {
        return usage(
            "explain <file.emath> [<symbol>] [--provenance] | explain E-LAW-001 [--json]",
        );
    };
    if file.starts_with("E-LAW-") || file == emath_diagnostics::E_LAW_001 {
        return explain_law_cmd(args);
    }
    if args.iter().any(|arg| arg == "--provenance") {
        return match crate::provenance_explanation(Path::new(&file), catalog::wants_json(args)) {
            Ok(explanation) => {
                print!("{explanation}");
                EXIT_OK
            }
            Err(code) => code,
        };
    }
    let symbol = args
        .iter()
        .filter(|arg| !arg.starts_with('-'))
        .nth(1)
        .cloned();
    let inspections = match crate::explain_inspections(Path::new(&file)) {
        Ok(inspections) => inspections,
        Err(code) => return code,
    };
    let json = catalog::wants_json(args);
    if json {
        for inspection in &inspections {
            println!("{}", inspection.to_json());
        }
        return EXIT_OK;
    }
    for inspection in &inspections {
        println!("{}", inspection.explain());
    }
    if let Some(symbol) = symbol {
        println!(
            "explain: symbol `{symbol}`: declaration indexing is Phase 4+; goals above are the available evidence"
        );
    }
    EXIT_OK
}

fn explain_law_cmd(args: &[String]) -> u8 {
    let json = catalog::wants_json(args);
    let (report, explanations) = emath_diagnostics::e_law_001_demo();
    if report.passed {
        eprintln!("error: E-LAW-001 demo table unexpectedly held");
        return EXIT_REFUSED;
    }
    let Some(explanation) = explanations.first() else {
        eprintln!("error: checker produced no witness");
        return EXIT_REFUSED;
    };
    if let Err(error) = emath_diagnostics::tutor_check_v1(explanation) {
        eprintln!("error: tutor-check/v1 refused ({})", error.as_str());
        return EXIT_REFUSED;
    }
    if json {
        print!("{}", emath_diagnostics::explanation_json(explanation));
        return EXIT_OK;
    }
    println!("{} {}", explanation.code, explanation.kind.as_str());
    println!("{}", explanation.structured_narrative);
    if let Some(witness) = &explanation.witness {
        print!("{}", emath_diagnostics::render_cayley_ascii(witness));
    }
    EXIT_OK
}

/// `run <file> [--out <dir>]`: build then execute the generated crate.
/// Library crates have no binary; their example tests are the runnable
/// surface, so `emath run` executes them (crate mains run as binaries).
fn run_cmd(args: &[String]) -> u8 {
    let Some(file) = first_positional(args) else {
        return usage("run <file.emath> [--out <dir>]");
    };
    if let Some(code) = crate::meaning_cmd::refuse_malformed_project_lock(Path::new(&file)) {
        return code;
    }
    let out = flag_value("--out", args)
        .or_else(|| flag_value("-o", args))
        .map_or_else(|| PathBuf::from("target/emath"), PathBuf::from);
    let report = match build_file(
        &file,
        &out,
        BuildOptions {
            verify_generated_crate: false,
        },
    ) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("error: {error}");
            return classify_build_error(&error);
        }
    };
    // Artifact dir is named by its content id (`fnv1a64:<hash>`); cargo
    // rejects the colon, so stage a colon-free copy under the temp dir.
    let hash = report
        .artifact_id
        .0
        .split(':')
        .next_back()
        .unwrap_or(&report.artifact_id.0);
    let run_dir = std::env::temp_dir().join(format!("emath-run-{hash}"));
    // RAII guard: wipe the staged tree on every exit path (a bare
    // success-path remove leaked it on early returns).
    struct RunDirGuard(PathBuf);
    impl Drop for RunDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _ = std::fs::remove_dir_all(&run_dir);
    let _run_dir_guard = RunDirGuard(run_dir.clone());
    if let Err(error) = std::fs::create_dir_all(run_dir.join("src")).and_then(|()| {
        std::fs::copy(
            report.artifact_dir.join("Cargo.toml"),
            run_dir.join("Cargo.toml"),
        )
        .map(|_| ())
    }) {
        eprintln!("error: cannot stage generated crate for execution: {error}");
        return EXIT_USAGE;
    }
    let src_dir = report.artifact_dir.join("src");
    match std::fs::read_dir(&src_dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        eprintln!(
                            "error: cannot read staged sources under {}: {error}",
                            src_dir.display()
                        );
                        return EXIT_USAGE;
                    }
                };
                let name = entry.file_name();
                if let Err(error) = std::fs::copy(entry.path(), run_dir.join("src").join(&name)) {
                    eprintln!(
                        "error: cannot stage {} for execution: {error}",
                        entry.path().display()
                    );
                    return EXIT_USAGE;
                }
            }
        }
        Err(error) => {
            eprintln!(
                "error: cannot read artifact sources {}: {error}",
                src_dir.display()
            );
            return EXIT_USAGE;
        }
    }
    let manifest = run_dir.join("Cargo.toml");
    let library_crate = !run_dir.join("src/main.rs").exists();
    let mut command = Command::new("cargo");
    if library_crate {
        println!(
            "run: artifact {} crate `{}` is a library; executing its example tests",
            report.artifact_id.0, report.crate_name
        );
        // Skip rustdoc/doctests: generated libs have example `#[test]`s,
        // and bare `cargo test` launches rustdoc anyway.
        command
            .args([
                "test",
                "--lib",
                "--bins",
                "--tests",
                "--quiet",
                "--manifest-path",
            ])
            .arg(&manifest)
            .env("CARGO_TARGET_DIR", generated_crate_target_dir(hash));
    } else {
        command
            .args(["run", "--quiet", "--manifest-path"])
            .arg(&manifest)
            .env("CARGO_TARGET_DIR", generated_crate_target_dir(hash));
    }
    let output = match run_cargo_timed(command, std::time::Duration::from_secs(600)) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("error: cannot execute generated crate: {error}");
            return EXIT_USAGE;
        }
    };
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        EXIT_OK
    } else {
        eprintln!("error: generated crate exited with {}", output.status);
        EXIT_REFUSED
    }
}

/// `test <file> [--out <dir>]`: build and run the generated crate's tests.
fn test_cmd(args: &[String]) -> u8 {
    let Some(file) = first_positional(args) else {
        return usage("test <file.emath> [--out <dir>]");
    };
    if let Some(code) = crate::meaning_cmd::refuse_malformed_project_lock(Path::new(&file)) {
        return code;
    }
    let out = flag_value("--out", args)
        .or_else(|| flag_value("-o", args))
        .map_or_else(|| PathBuf::from("target/emath"), PathBuf::from);
    match build_file(
        &file,
        out,
        BuildOptions {
            verify_generated_crate: true,
        },
    ) {
        Ok(report) => {
            println!(
                "test: artifact {} crate `{}` tests passed",
                report.artifact_id.0, report.crate_name
            );
            EXIT_OK
        }
        Err(error) => {
            eprintln!("error: {error}");
            classify_build_error(&error)
        }
    }
}

/// `bench <file>`: typed refusal. Candidate-vs-baseline CLI comparison
/// needs the keep-gate ruleset (Phase 4+); never `EXIT_OK` with empty
/// output. Use `cargo bench --profile release-perf --bench
/// comprehensive_bench`.
fn bench_cmd(args: &[String]) -> u8 {
    let Some(file) = first_positional(args) else {
        return usage("bench <file.emath>");
    };
    eprintln!(
        "error: E-TLT-004: benchmarking `{file}` is not a Phase 1 CLI comparison; measure via `cargo bench --profile release-perf --bench comprehensive_bench` (keep-gate history in .bench-history/)"
    );
    EXIT_REFUSED
}

/// `verify <dir>`: independent artifact re-verification.
fn verify_cmd(args: &[String]) -> u8 {
    let Some(dir) = first_positional(args) else {
        return usage("verify <artifact-dir>");
    };
    artifact_check(&PathBuf::from(dir))
}

/// `inspect <dir>`: print the committed artifact manifest; refuses
/// non-UTF-8 manifests instead of substituting lossy text.
fn inspect_cmd(args: &[String]) -> u8 {
    let Some(dir) = first_positional(args) else {
        return usage("inspect <artifact-dir>");
    };
    let dir = PathBuf::from(dir);
    let root = dir.join("emath");
    let Ok(entries) = std::fs::read_dir(&root) else {
        eprintln!(
            "error: E-TLT-005: no `emath/` state directory under {}",
            dir.display()
        );
        return EXIT_USAGE;
    };
    let mut inspected: u64 = 0;
    let mut manifests = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let manifest = entry.path().join("emath/artifact-manifest.json");
        let bytes = match std::fs::read(&manifest) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!(
                    "error: E-TLT-005: cannot read manifest at {}: {error}",
                    manifest.display()
                );
                return EXIT_REFUSED;
            }
        };
        let Ok(text) = String::from_utf8(bytes) else {
            eprintln!(
                "error: E-EVID-114: manifest is not valid UTF-8 at {}",
                manifest.display()
            );
            return EXIT_REFUSED;
        };
        if catalog::wants_json(args) {
            manifests.push(text);
        } else {
            println!("artifact {}:", entry.file_name().to_string_lossy());
            println!("{text}");
        }
        inspected += 1;
    }
    if inspected == 0 {
        eprintln!("error: E-TLT-005: no artifacts under {}", root.display());
        EXIT_USAGE
    } else if catalog::wants_json(args) {
        let mut object = JsonWriter::object();
        object.string("schema", "emath.inspect");
        object.string("dir", &dir.display().to_string());
        object.int("count", inspected);
        object.objects("manifests", &manifests);
        println!("{}", object.finish());
        EXIT_OK
    } else {
        EXIT_OK
    }
}

/// `diff <a.emath> <b.emath>`: fingerprint comparison of parse-admitted sources.
fn diff_cmd(args: &[String]) -> u8 {
    let positional = positional_args(args);
    let Some(a) = positional.first() else {
        return usage("diff <a.emath> <b.emath>");
    };
    let Some(b) = positional.get(1) else {
        return usage("diff <a.emath> <b.emath>");
    };
    let id_a = fingerprint(a);
    let id_b = fingerprint(b);
    match (id_a, id_b) {
        (Ok(id_a), Ok(id_b)) => {
            let identical = id_a == id_b;
            if catalog::wants_json(args) {
                let mut object = JsonWriter::object();
                object.string("schema", "emath.diff");
                object.string("a", a);
                object.string("a_id", &id_a.0);
                object.string("b", b);
                object.string("b_id", &id_b.0);
                object.bool("identical", identical);
                println!("{}", object.finish());
            } else {
                println!("diff: {a} {}", id_a.0);
                println!("diff: {b} {}", id_b.0);
                println!("diff: {}", if identical { "identical" } else { "differ" });
            }
            if identical { EXIT_OK } else { EXIT_REFUSED }
        }
        (Err(()), _) | (_, Err(())) => EXIT_REFUSED,
    }
}

/// Content id of a parse-admitted source; diagnostics printed on refusal.
fn fingerprint(file: &str) -> Result<emath_core::ContentId, ()> {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(file) else {
        eprintln!("error: cannot read {file}");
        return Err(());
    };
    let result = session.check(package.file);
    print_diagnostics(&result.diagnostics);
    if result.diagnostics.has_errors() {
        return Err(());
    }
    let Ok(bytes) = std::fs::read(file) else {
        eprintln!("error: cannot read {file}");
        return Err(());
    };
    // Bind the bytes, not a lossy decode: non-UTF-8 stays distinct.
    Ok(emath_core::bootstrap_content_id(&bytes))
}

pub(crate) struct DoctorProbe {
    pub(crate) name: &'static str,
    pub(crate) ok: bool,
    pub(crate) version: Option<String>,
}

pub(crate) fn doctor_probes() -> Vec<DoctorProbe> {
    [
        ("rustc", "rustc --version"),
        ("cargo", "cargo --version"),
        ("rustfmt", "rustfmt --version"),
        ("clippy", "cargo clippy --version"),
    ]
    .into_iter()
    .map(|(name, probe)| match probe_program(probe) {
        Some(version) => DoctorProbe {
            name,
            ok: true,
            version: Some(version),
        },
        None => DoctorProbe {
            name,
            ok: false,
            version: None,
        },
    })
    .collect()
}

/// `doctor`: toolchain presence checks.
fn doctor_cmd(args: &[String]) -> u8 {
    let probes = doctor_probes();
    let lock = upstream_lock_path();
    let fork_lock = std::fs::read_to_string(&lock)
        .map_err(|error| format!("cannot read {}: {error}", lock.display()))
        .and_then(|text| {
            parse_upstream_pins(&text)
                .and_then(|pins| {
                    emath_provider_api::pinned_fork_adapters(&pins)
                        .map_err(|error| error.to_string())
                })
                .map(|adapters| (content_id_of_str(&text).0, adapters))
        });
    let ok = probes.iter().all(|probe| probe.ok) && fork_lock.is_ok();
    if catalog::wants_json(args) {
        let mut rows = Vec::new();
        for probe in &probes {
            let mut row = JsonWriter::object();
            row.string("name", probe.name);
            row.bool("ok", probe.ok);
            if let Some(version) = &probe.version {
                row.string("version", version);
            }
            rows.push(row.finish());
        }
        let mut object = JsonWriter::object();
        object.string("schema", "emath.doctor");
        object.bool("ok", ok);
        object.objects("checks", &rows);
        object.string("fork_lock_source", UPSTREAM_LOCK_REL);
        match &fork_lock {
            Ok((lock_id, adapters)) => {
                object.string("fork_lock_id", lock_id);
                let mut fork_rows = Vec::new();
                for adapter in adapters {
                    let mut row = JsonWriter::object();
                    row.string("provider_id", adapter.contract.provider_id);
                    row.string("upstream_id", adapter.contract.upstream_id);
                    row.string(
                        "adapter_crate",
                        adapter.contract.adapter_crate.unwrap_or("oracle-only"),
                    );
                    row.string("status", adapter.contract.status);
                    row.string("repository", &adapter.pin.repository);
                    row.string("source_lock", &adapter.pin.commit);
                    row.string("license", &adapter.pin.license);
                    fork_rows.push(row.finish());
                }
                object.objects("fork_adapters", &fork_rows);
            }
            Err(error) => {
                object.string("fork_lock_error", error);
            }
        }
        println!("{}", object.finish());
    } else {
        for probe in &probes {
            match &probe.version {
                Some(version) => println!("doctor: {}: ok ({version})", probe.name),
                None => println!("doctor: {}: MISSING", probe.name),
            }
        }
        match &fork_lock {
            Ok((lock_id, adapters)) => {
                for adapter in adapters {
                    println!(
                        "doctor: fork {}: pinned {} license={} (lock {lock_id})",
                        adapter.contract.provider_id, adapter.pin.commit, adapter.pin.license
                    );
                }
            }
            Err(error) => println!("doctor: fork lock: INVALID ({error})"),
        }
    }
    if ok { EXIT_OK } else { EXIT_REFUSED }
}

fn parse_upstream_pins(text: &str) -> Result<Vec<emath_provider_api::UpstreamPin>, String> {
    let document = emath_artifact::parse_json_document(text).map_err(|error| error.to_string())?;
    let repositories = match document
        .field("repositories")
        .map_err(|error| error.to_string())?
    {
        emath_artifact::JsonValue::Arr(repositories) => repositories,
        _ => return Err("`repositories` is not an array".into()),
    };
    repositories
        .iter()
        .map(|repository| {
            Ok(emath_provider_api::UpstreamPin {
                id: repository
                    .string_field("id")
                    .map_err(|error| error.to_string())?,
                repository: repository
                    .string_field("repository")
                    .map_err(|error| error.to_string())?,
                commit: repository
                    .string_field("commit")
                    .map_err(|error| error.to_string())?,
                license: repository
                    .string_field("license")
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

fn probe_program(command: &str) -> Option<String> {
    let mut parts = command.split_whitespace();
    let program = parts.next()?;
    // ubs:ignore — executable/args are only the static doctor_probes() literals
    // (`rustc --version`, `cargo --version`, `rustfmt --version`, `cargo clippy --version`)
    let output = Command::new(program).args(parts).output().ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Some(text.lines().next().unwrap_or(&text).to_string())
    } else {
        None
    }
}

/// `vendor --out <dir>`: offline dependency snapshot (zero third-party deps).
fn vendor_cmd(args: &[String]) -> u8 {
    let Some(out) = flag_value("--out", args).or_else(|| flag_value("-o", args)) else {
        return usage("vendor --out <dir>");
    };
    let lock = upstream_lock_path();
    let Ok(bytes) = std::fs::read(&lock) else {
        eprintln!(
            "error: E-TLT-007: upstream lock missing at {}",
            lock.display()
        );
        return EXIT_USAGE;
    };
    if bytes.is_empty() {
        eprintln!(
            "error: E-TLT-007: upstream lock is empty at {}",
            lock.display()
        );
        return EXIT_USAGE;
    }
    let Ok(text) = String::from_utf8(bytes) else {
        eprintln!(
            "error: E-TLT-007: upstream lock is not valid UTF-8 at {}",
            lock.display()
        );
        return EXIT_USAGE;
    };
    let entry_count = text.matches("      \"id\": \"").count();
    let mut object = JsonWriter::object();
    object.string("schema", "emath.vendor");
    object.string("source", UPSTREAM_LOCK_REL);
    let source_id = content_id_of_str(&text).0;
    object.string("source_id", &source_id);
    object.int("upstream_pins", u64::try_from(entry_count).unwrap_or(0));
    object.int("third_party_deps", 0);
    object.bool("offline", true);
    let out_dir = PathBuf::from(out);
    if std::fs::create_dir_all(&out_dir).is_err() {
        eprintln!("error: cannot create {}", out_dir.display());
        return EXIT_USAGE;
    }
    let target = out_dir.join("vendor-manifest.json");
    let body = object.finish();
    if std::fs::write(&target, body.clone()).is_err() {
        eprintln!("error: cannot write {}", target.display());
        return EXIT_USAGE;
    }
    println!("vendor: wrote {body}");
    EXIT_OK
}

/// `provider list|inspect <id>|test <id>`.
fn provider_cmd(args: &[String]) -> u8 {
    let Some(sub) = args.first() else {
        return usage("provider list|inspect <id>|test <id>");
    };
    match sub.as_str() {
        "list" => {
            if catalog::wants_json(args) {
                let mut rows = Vec::new();
                for (id, capability, status) in PROVIDERS {
                    let mut row = JsonWriter::object();
                    row.string("id", id);
                    row.string("capability", capability);
                    row.string("status", status);
                    rows.push(row.finish());
                }
                let mut object = JsonWriter::object();
                object.string("schema", "emath.provider-list");
                object.objects("providers", &rows);
                println!("{}", object.finish());
            } else {
                for (id, capability, status) in PROVIDERS {
                    println!("provider {id}: {capability} [{status}]");
                }
            }
            EXIT_OK
        }
        "inspect" => {
            let Some(id) = args.get(1) else {
                return usage("provider inspect <id>");
            };
            let Some((_, capability, status)) =
                PROVIDERS.iter().find(|(candidate, _, _)| candidate == id)
            else {
                eprintln!("error: E-TLT-016: unknown provider `{id}`");
                if let Some(hint) = suggest_provider(id) {
                    eprintln!("did you mean `emath provider inspect {hint}`?");
                }
                return EXIT_USAGE;
            };
            let mut object = JsonWriter::object();
            object.string("schema", "emath.provider-descriptor");
            object.string("id", id);
            object.string("capability", capability);
            object.string("status", status);
            println!("{}", object.finish());
            EXIT_OK
        }
        "test" => {
            let Some(id) = args.get(1) else {
                return usage("provider test <id>");
            };
            let Some((_, _, status)) = PROVIDERS.iter().find(|(candidate, _, _)| candidate == id)
            else {
                eprintln!("error: E-TLT-016: unknown provider `{id}`");
                return EXIT_USAGE;
            };
            let _ = (status, id);
            // No in-CLI battery exists; printing "ok" without running
            // anything would be a fake success (same as bench E-TLT-004).
            eprintln!(
                "error: E-TLT-013: provider `{id}` has no in-CLI negative-control battery; run `cargo test` against tests/emath-adapter-rumoca in the workspace"
            );
            EXIT_REFUSED
        }
        _ => usage("provider list|inspect <id>|test <id>"),
    }
}

/// `fork status|sync [--dry-run]`.
fn fork_cmd(args: &[String]) -> u8 {
    let Some(sub) = args.first() else {
        return usage("fork status|sync [--dry-run]");
    };
    let lock = upstream_lock_path();
    let Ok(bytes) = std::fs::read(&lock) else {
        eprintln!(
            "error: E-TLT-007: upstream lock missing at {}",
            lock.display()
        );
        return EXIT_USAGE;
    };
    if bytes.is_empty() {
        eprintln!(
            "error: E-TLT-007: upstream lock is empty at {}",
            lock.display()
        );
        return EXIT_USAGE;
    }
    let Ok(text) = String::from_utf8(bytes) else {
        eprintln!(
            "error: E-TLT-007: upstream lock is not valid UTF-8 at {}",
            lock.display()
        );
        return EXIT_USAGE;
    };
    match sub.as_str() {
        "status" => {
            let ids = lock_ids(&text);
            let lock_id = content_id_of_str(&text).0;
            if catalog::wants_json(args) {
                let mut object = JsonWriter::object();
                object.string("schema", "emath.fork-status");
                object.string("lock_id", &lock_id);
                object.int("pins", ids.len() as u64);
                object.strings("ids", &ids);
                object.bool("offline", true);
                println!("{}", object.finish());
            } else {
                for id in ids {
                    println!("fork {id}: pinned (lock {lock_id})");
                }
            }
            EXIT_OK
        }
        "sync" => {
            if args.iter().any(|arg| arg == "--dry-run") {
                println!(
                    "sync: dry-run: {} upstream pins unchanged (offline)",
                    lock_ids(&text).len()
                );
                EXIT_OK
            } else {
                eprintln!(
                    "error: E-TLT-006: network/source sync is disabled in Phase 1 (offline-first); use --dry-run"
                );
                EXIT_REFUSED
            }
        }
        _ => usage("fork status|sync [--dry-run]"),
    }
}

/// Extracts quoted `"id": "..."` values from the lock document.
fn lock_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("\"id\": \"") else {
            continue;
        };
        if let Some(end) = rest.find('"') {
            ids.push(rest[..end].to_string());
        }
    }
    ids
}

fn suggest_provider(unknown: &str) -> Option<&'static str> {
    let mut best: Option<(&'static str, usize)> = None;
    for (id, _, _) in PROVIDERS {
        if id == unknown {
            return Some(id);
        }
        let distance = id
            .chars()
            .zip(unknown.chars())
            .filter(|(a, b)| a != b)
            .count()
            + id.len().abs_diff(unknown.len());
        if distance <= 4 && best.is_none_or(|(_, current)| distance < current) {
            best = Some((id, distance));
        }
    }
    best.map(|(id, _)| id)
}

/// Maps a build error to the conventional exit class.
pub(crate) fn classify_build_error(error: &dyn std::fmt::Display) -> u8 {
    let text = error.to_string();
    if text.contains("admission refused") {
        EXIT_REFUSED
    } else {
        EXIT_USAGE
    }
}

pub(crate) fn positional_args(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|arg| !arg.starts_with('-'))
        .cloned()
        .collect()
}

fn first_positional(args: &[String]) -> Option<String> {
    positional_args(args).first().cloned()
}

pub(crate) fn flag_value(flag: &str, args: &[String]) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}
