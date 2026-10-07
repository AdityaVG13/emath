//! The `emath doctor` environment probes.

use super::{CliExit, Path, PathBuf, content_id_of_str, JsonWriter, EXIT_OK, EXIT_TOOLCHAIN, Command};

/// Relative path of the committed upstream lock file (repo layout).
const UPSTREAM_LOCK_REL: &str = "forks/UPSTREAM_LOCK.json";

/// Crate-relative, so the probe works from any working directory.
fn upstream_lock_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(UPSTREAM_LOCK_REL)
}

pub struct DoctorProbe {
    pub name: &'static str,
    pub ok: bool,
    pub version: Option<String>,
}

pub fn doctor_probes() -> Vec<DoctorProbe> {
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
    .chain(std::iter::once(source_date_epoch_probe()))
    .chain(std::iter::once(language_root_probe()))
    .collect()
}

/// Language-root probe: most doctor consumers are about to run
/// keep-commands that all gate on a verified Language Image, so the
/// probe reports whether `language/spec` is discoverable from the
/// working directory (MISSING elsewhere: the same condition the
/// E-LANG-IMAGE refusal enforces).
fn language_root_probe() -> DoctorProbe {
    match crate::locate_language_root(None) {
        Ok(root) => DoctorProbe {
            name: "language-root",
            ok: true,
            version: Some(root.display().to_string()),
        },
        Err(_) => DoctorProbe {
            name: "language-root",
            ok: false,
            version: None,
        },
    }
}

/// `SOURCE_DATE_EPOCH` probe: unset is fine (artifacts are
/// content-addressed, never stamped with wall-clock time); set must be a
/// decimal UNIX timestamp, or reproducible-build drivers are feeding
/// garbage into the environment contract.
fn source_date_epoch_probe() -> DoctorProbe {
    match std::env::var("SOURCE_DATE_EPOCH") {
        Err(_) => DoctorProbe {
            name: "SOURCE_DATE_EPOCH",
            ok: true,
            version: Some("unset (artifacts are content-addressed)".to_string()),
        },
        Ok(value) => {
            let valid = !value.is_empty()
                && value.chars().all(|c| c.is_ascii_digit())
                && value.parse::<u64>().is_ok();
            if valid {
                DoctorProbe {
                    name: "SOURCE_DATE_EPOCH",
                    ok: true,
                    version: Some(format!("set to {value} (valid UNIX timestamp)")),
                }
            } else {
                DoctorProbe {
                    name: "SOURCE_DATE_EPOCH",
                    ok: false,
                    version: Some(format!(
                        "invalid: `{value}` (must be a decimal UNIX timestamp)"
                    ),
                    ),
                }
            }
        }
    }
}

/// `doctor`: toolchain presence checks.
pub(crate) fn doctor_cmd(json: bool) -> CliExit {
    let probes = doctor_probes();
    let lock = upstream_lock_path();
    let fork_lock = std::fs::read_to_string(&lock)
        .map_err(|error| format!("cannot read {}: {error}", lock.display()))
        .and_then(|text| {
            parse_upstream_pins(&text)
                .and_then(|pins| validate_upstream_pins(&pins).map(|()| pins))
                .map(|pins| (content_id_of_str(&text).0, pins))
        });
    let ok = probes.iter().all(|probe| probe.ok) && fork_lock.is_ok();
    if json {
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
        object.string("command", "doctor");
        object.string("status", if ok { "ok" } else { "refused" });
        object.bool("ok", ok);
        object.objects("checks", &rows);
        object.string("fork_lock_source", UPSTREAM_LOCK_REL);
        match &fork_lock {
            Ok((lock_id, pins)) => {
                object.string("fork_lock_id", lock_id);
                let mut fork_rows = Vec::new();
                for pin in pins {
                    let mut row = JsonWriter::object();
                    row.string("id", &pin.id);
                    row.string("repository", &pin.repository);
                    row.string("source_lock", &pin.commit);
                    row.string("license", &pin.license);
                    fork_rows.push(row.finish());
                }
                object.objects("fork_pins", &fork_rows);
            }
            Err(error) => {
                object.string("fork_lock_error", error);
            }
        }
        println!("{}", object.finish());
    } else {
        for probe in &probes {
            match (&probe.version, probe.ok) {
                (Some(version), true) => {
                    let status = crate::terminal::stdout_green("ok");
                    println!("doctor: {}: {status} ({version})", probe.name);
                }
                (Some(version), false) => {
                    let status = crate::terminal::stdout_bold_red("INVALID");
                    println!("doctor: {}: {status} ({version})", probe.name);
                }
                (None, _) => {
                    let status = crate::terminal::stdout_bold_red("MISSING");
                    println!("doctor: {}: {status}", probe.name);
                }
            }
        }
        match &fork_lock {
            Ok((lock_id, pins)) => {
                for pin in pins {
                    println!(
                        "doctor: fork {}: pinned {} license={} (lock {lock_id})",
                        pin.id, pin.commit, pin.license
                    );
                }
            }
            Err(error) => {
                let status = crate::terminal::stdout_bold_red("INVALID");
                println!("doctor: fork lock: {status} ({error})");
            }
        }
    }
    if ok { EXIT_OK } else { EXIT_TOOLCHAIN }
}

/// Reproducibility fields for one repository in the upstream lock.
struct UpstreamPin {
    id: String,
    repository: String,
    commit: String,
    license: String,
}

/// Every pin must be unique, fetchable over https, locked to an immutable
/// commit, and carry a declared license.
fn validate_upstream_pins(pins: &[UpstreamPin]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for pin in pins {
        if !seen.insert(&pin.id) {
            return Err(format!("upstream lock contains duplicate pin `{}`", pin.id));
        }
        if !pin.repository.starts_with("https://") {
            return Err(format!("pin `{}` repository is not an https URL", pin.id));
        }
        let hex = pin
            .commit
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if pin.commit.len() != 40 || !hex {
            return Err(format!(
                "pin `{}` commit is not a lowercase 40-hex source lock",
                pin.id
            ));
        }
        if pin.license.trim().is_empty() {
            return Err(format!("pin `{}` license is empty", pin.id));
        }
    }
    Ok(())
}

fn parse_upstream_pins(text: &str) -> Result<Vec<UpstreamPin>, String> {
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
            Ok(UpstreamPin {
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

pub(super) fn probe_program(command: &str) -> Option<String> {
    let mut parts = command.split_whitespace();
    let program = parts.next()?;
    // The executable/args here are only the static doctor_probes() literals
    // (`rustc --version`, `cargo --version`, `rustfmt --version`,
    // `cargo clippy --version`), no user data reaches Command::new.
    let output = Command::new(program).args(parts).output().ok()?; // ubs:ignore static-literal probes
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Some(text.lines().next().unwrap_or(&text).to_string())
    } else {
        None
    }
}
