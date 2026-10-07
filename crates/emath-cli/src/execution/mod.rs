//! `emath run`, `step`, and `inspect` on the constructor layer.
//!
//! A run evaluates on the reference VM. A `--work` budget stop writes an
//! immutable `constructor-checkpoint.json`; `step` resumes it and
//! `inspect` reads it without executing.

use crate::{CliExit, EXIT_ADMISSION, EXIT_FAULT, EXIT_OK, EXIT_PARTIAL, EXIT_REFUSED, EXIT_USAGE};
use emath_artifact::{JsonValue, JsonWriter, parse_json_document};
use emath_core::content_id_of_str;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const DEFAULT_WORK: usize = 1024;

#[derive(Clone, Debug)]
pub(crate) struct RunRequest {
    pub path: PathBuf,
    pub out: PathBuf,
    pub function: Option<String>,
    pub given: BTreeMap<String, String>,
    pub set_file: Option<PathBuf>,
    pub work: usize,
    pub work_set: bool,
    pub json: bool,
    /// The world list `quote.worlds()` returns; empty leaves every world.
    pub worlds: Vec<String>,
}

impl RunRequest {
    pub(crate) fn parse(args: &[String], continuation: bool) -> Option<Self> {
        let mut path = None;
        let mut out = None;
        let mut function = None;
        let mut given = BTreeMap::new();
        let mut set_file = None;
        let mut work = None;
        let mut worlds = None;
        let mut json = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--out" | "-o" => {
                    crate::assign_once(
                        &mut out,
                        PathBuf::from(crate::take_nonflag_value(args, &mut index)?),
                    )?;
                }
                "--function" if !continuation => {
                    crate::assign_once(
                        &mut function,
                        crate::take_nonflag_value(args, &mut index)?.to_string(),
                    )?;
                }
                "--set" if !continuation => {
                    let raw = crate::take_nonflag_value(args, &mut index)?;
                    let (name, value) = raw.split_once('=')?;
                    if name.is_empty()
                        || value.is_empty()
                        || given.insert(name.to_string(), value.to_string()).is_some()
                    {
                        return None;
                    }
                }
                "--set-file" if !continuation => {
                    crate::assign_once(
                        &mut set_file,
                        PathBuf::from(crate::take_nonflag_value(args, &mut index)?),
                    )?;
                }
                "--work" => {
                    let value: usize = crate::take_nonflag_value(args, &mut index)?.parse().ok()?;
                    if value == 0 {
                        return None;
                    }
                    crate::assign_once(&mut work, value)?;
                }
                "--worlds" if !continuation => {
                    let list = crate::take_nonflag_value(args, &mut index)?;
                    let names: Vec<String> =
                        list.split(',').map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect();
                    crate::assign_once(&mut worlds, names)?;
                }
                "--json" => json = true,
                flag if flag.starts_with('-') => return None,
                value => crate::assign_once(&mut path, PathBuf::from(value))?,
            }
            index += 1;
        }
        let path = path?;
        let out = out.unwrap_or_else(|| {
            if continuation {
                path.parent().unwrap_or(Path::new(".")).to_path_buf()
            } else {
                PathBuf::from("target/emath/runs")
            }
        });
        Some(Self {
            path,
            out,
            function,
            given,
            set_file,
            work: work.unwrap_or(DEFAULT_WORK),
            work_set: work.is_some(),
            json,
            worlds: worlds.unwrap_or_default(),
        })
    }
}

pub(crate) fn diagnostic(json: bool, exit: CliExit, code: &str, message: &str) -> CliExit {
    diagnostic_with(json, exit, code, message, &format!("error: {code}: {message}"))
}

/// [`diagnostic`] with the stderr text given (for example with a source
/// location from [`crate::cli_json::error_text`]).
pub(crate) fn diagnostic_with(json: bool, exit: CliExit, code: &str, message: &str, text: &str) -> CliExit {
    eprintln!("{text}");
    if json {
        let mut out = JsonWriter::object();
        out.string("schema_version", "emath.constructor.v1");
        out.string("command", "run");
        out.string("admission", "refused");
        out.string("execution", "absent");
        out.string("fulfillment", "unmet");
        out.objects("evidence", &[]);
        out.strings("remaining", &[message.to_string()]);
        out.objects(
            "diagnostics",
            &[crate::json_diagnostic_entry(code, "error", message)],
        );
        println!("{}", out.finish());
    }
    exit
}

use constructor::*;
mod cmd;
mod constructor;
mod run;

pub(crate) use cmd::*;
pub(crate) use run::*;
