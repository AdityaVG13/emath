//! `emath check` and `emath build` pipelines.

use super::{
    CliExit, CompilerSession, EXIT_ADMISSION, EXIT_FAULT, EXIT_OK, EXIT_REFUSED, EXIT_USAGE, Path,
    PathBuf, assign_once, refuse_coded, take_nonflag_value,
};
use emath_core::Diagnostics;

pub(crate) struct CheckedSource {
    pub result: emath_sema::admit::CheckResult,
    pub store: emath_core::SourceStore,
}

impl CheckedSource {
    pub fn source(&self) -> &str {
        self.store.files().first().map_or("", |file| file.text.as_str())
    }
}

pub(crate) fn checked_file(path: &Path) -> CheckedSource {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        let mut diagnostics = Diagnostics::new();
        diagnostics.error(
            "E-PKG-080",
            format!("cannot read source file ({})", path.display()),
            emath_core::Span::default(),
        );
        return CheckedSource {
            result: emath_sema::admit::CheckResult {
                syntax: None,
                parse_diagnostics: Diagnostics::new(),
                package: emath_ir::SemanticPackage::new(),
                diagnostics,
                trace: emath_sema::admit::SemanticTrace::default(),
                units_profiles: Vec::new(),
                prepared: None,
            },
            store: session.store,
        };
    };
    let result = session.check(package.file);
    CheckedSource { result, store: session.store }
}

pub(crate) fn checked_text(name: &str, source: &str) -> CheckedSource {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned(name, source);
    CheckedSource { result, store: session.store }
}

pub fn run_check(path: &Path) -> (Diagnostics, String, Vec<(String, String)>) {
    let checked = checked_file(path);
    if checked.store.files().is_empty() {
        return (checked.result.diagnostics, String::new(), Vec::new());
    }
    let result = checked.result;
    let package_id = result.package.content_id().0;
    (result.diagnostics, package_id, result.units_profiles)
}

/// Stdin variant of [`run_check`] (`check -`): same shape, source read
/// from a string. Deterministic; the package id derives from the source
/// bytes alone, so piped and on-disk checks of identical text agree.
pub fn run_check_source(name: &str, source: &str) -> (Diagnostics, String, Vec<(String, String)>) {
    let result = checked_text(name, source).result;
    let package_id = result.package.content_id().0;
    (result.diagnostics, package_id, result.units_profiles)
}

pub enum BuildRequest {
    Ready {
        spec: PathBuf,
        out: PathBuf,
        dry_run: bool,
        json: bool,
        /// The world list a built `quote.worlds()` returns; empty
        /// leaves every world.
        worlds: Vec<String>,
    },
}

pub(super) fn parse_build_request(args: &[String]) -> Option<BuildRequest> {
    let mut path = None;
    let mut out = None;
    let mut dry_run = false;
    let mut json = false;
    let mut worlds = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" => {
                assign_once(
                    &mut out,
                    PathBuf::from(take_nonflag_value(args, &mut index)?),
                )?;
            }
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--worlds" => {
                let list = take_nonflag_value(args, &mut index)?;
                let names: Vec<String> =
                    list.split(',').map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect();
                assign_once(&mut worlds, names)?;
            }
            other if other.starts_with('-') => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
        index += 1;
    }
    let spec = path?;
    // One-command quick run: `emath build <file>` publishes under
    // target/emath relative to the working directory.
    let out = out.unwrap_or_else(|| PathBuf::from("target/emath"));
    Some(BuildRequest::Ready {
        spec,
        out,
        dry_run,
        json,
        worlds: worlds.unwrap_or_default(),
    })
}

/// `build <file> [--out <dir>] [--dry-run] [--worlds w1,w2,...] [--json]`
/// (default out: `target/emath` under the working directory).
pub fn build(request: BuildRequest) -> CliExit {
    let BuildRequest::Ready {
        spec,
        out,
        dry_run,
        json,
        worlds,
    } = request;
    let mut carrier_worlds = Vec::new();
    for name in &worlds {
        match emath_exec_ir::constructor_layer::CarrierWorld::parse(name) {
            Some(world) => carrier_worlds.push(world),
            None => {
                let known: Vec<&str> = emath_exec_ir::constructor_layer::CarrierWorld::ALL
                    .iter()
                    .map(|w| w.name())
                    .collect();
                return refuse_coded(
                    "build",
                    json,
                    EXIT_USAGE,
                    "unknown_world",
                    &format!("unknown world `{name}` in --worlds; worlds are {}", known.join(", ")),
                );
            }
        }
    }
    if dry_run {
        let mut session = CompilerSession::new(emath_core::limits::Limits::default());
        let Ok(package) = session.load_package(&spec) else {
            return refuse_coded(
                "build",
                json,
                EXIT_USAGE,
                "E-PKG-080",
                &format!("cannot read spec: {}", spec.display()),
            );
        };
        let check_result = session.check(package.file);
        if check_result.diagnostics.has_errors() {
            crate::cli_json::print_diagnostics_from(&check_result.diagnostics, &session.store);
            if json {
                let items: Vec<String> = check_result
                    .diagnostics
                    .items()
                    .iter()
                    .map(|d| {
                        crate::json_diagnostic_entry(
                            d.code,
                            match d.severity {
                                emath_core::Severity::Error => "error",
                                emath_core::Severity::Warning => "warning",
                                emath_core::Severity::Note => "note",
                            },
                            &d.message,
                        )
                    })
                    .collect();
                crate::print_json_diagnostics("build", false, &items);
            }
            return EXIT_REFUSED;
        }
        let package_id = check_result.package.content_id();
        let crate_name = check_result
            .package
            .identity
            .as_ref()
            .map_or_else(|| "package".to_string(), |id| id.name.clone());
        if json {
            let mut obj = emath_artifact::JsonWriter::object();
            obj.string("command", "build");
            obj.string("status", "ok");
            obj.string("schema", "emath.constructor-emission.v1");
            obj.bool("dry_run", true);
            obj.string("package_id", &package_id.0);
            obj.string("crate", &crate_name);
            obj.string("target_dir", &out.display().to_string());
            println!("{}", obj.finish());
        } else {
            println!("dry-run: emath build `{}`", spec.display());
            println!("target directory: {}", out.display());
            println!("crate: {crate_name} (package {})", package_id.0);
        }
        return EXIT_OK;
    }
    let built = if carrier_worlds.is_empty() {
        build_constructor_file(&spec, &out, json)
    } else {
        emath_exec_ir::constructor_layer::with_worlds(&carrier_worlds, || {
            build_constructor_file(&spec, &out, json)
        })
    };
    if let Some(exit) = built {
        return exit;
    }
    refuse_coded(
        "build",
        json,
        EXIT_ADMISSION,
        "E-KIND-GONE",
        "`emath build` emits constructor functions. Write `emath object`, `emath function`, or `emath query`.",
    )
}

fn build_constructor_file(spec: &Path, out: &Path, json: bool) -> Option<CliExit> {
    let checked = checked_file(spec);
    if checked.result.diagnostics.has_errors() {
        crate::cli_json::print_diagnostics_from(&checked.result.diagnostics, &checked.store);
        if json {
            crate::print_json_diagnostics(
                "build", false,
                &crate::cli_json::json_diagnostics_entries_in(&checked.result.diagnostics, Some(&checked.store)),
            );
        }
        return Some(if checked.store.files().is_empty() { EXIT_USAGE } else { EXIT_ADMISSION });
    }
    let Some(module) = checked.result.prepared.as_ref() else {
        return Some(refuse_coded("build", json, EXIT_ADMISSION, "E-KIND-GONE", "checked source has no constructor module"));
    };
    // One emitter, two callers: `emath build` and the loop host's
    // `export-native` share constructor_crate so their artifact
    // crates are byte-identical by construction.
    let emission = match emath_rust_backend::constructor_crate::emit_constructor_crate(module, spec, checked.source())
    {
        Ok(emission) => emission,
        Err(emath_rust_backend::constructor_crate::ConstructorEmitRefusal::NotConstructor(
            message,
        )) => {
            return Some(refuse_coded(
                "build",
                json,
                EXIT_ADMISSION,
                "E-KIND-GONE",
                message,
            ));
        }
        Err(emath_rust_backend::constructor_crate::ConstructorEmitRefusal::ImportsRefused {
            e_code,
            detail,
        }) => {
            return Some(refuse_coded(
                "build",
                json,
                EXIT_ADMISSION,
                &e_code,
                &detail,
            ));
        }
    };
    for (relative, contents) in emission.files() {
        let path = out.join(relative);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| emath_build::write_generated_file(&path, contents.as_bytes()));
        if let Err(err) = written {
            return Some(refuse_coded(
                "build",
                json,
                EXIT_FAULT,
                "E-IO",
                &format!("cannot write {}: {err}", path.display()),
            ));
        }
    }
    let runnable = emission.runnable;
    let functions = emission.functions;
    let unresolved = emission.unresolved;
    let status = if runnable { "runnable" } else { "not-runnable" };
    if json {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("command", "build");
        object.string("status", "ok");
        object.string("schema", "emath.constructor-emission.v1");
        object.bool("runnable", runnable);
        object.strings("functions", &functions);
        object.strings("unresolved", &unresolved);
        object.string("artifact_dir", &out.display().to_string());
        println!("{}", object.finish());
    } else {
        println!(
            "constructor emission {status} → {} ({})",
            out.display(),
            if runnable {
                "runnable"
            } else {
                "symbolic code is not marked runnable"
            }
        );
    }
    Some(EXIT_OK)
}
