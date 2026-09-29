//! `emath keep`: promote a closed program returned by an authored
//! function into a user-owned library file as an ordinary
//! `emath function`, so later programs `use` it. Without `--name` the
//! function returns a sequence of `{name, program}` records (a quoted
//! identifier and its program) and each keeps under its own name, in
//! order, all or nothing.
//!
//! The library is plain source: a header, an `exports:` line, and one
//! definition per kept name. Each definition's summary line records
//! where it came from and the content id of its text. Keeping a name
//! that already holds different text refuses `E-KEEP-EXISTS` unless
//! `--overwrite`; keeping identical text is a no-op.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use emath_core::content_id_of_str;
use emath_exec_ir::constructor_layer::{
    evaluate_function_budgeted_at, named_programs, parse_constructor_scalar, place_definition, Placed,
    DEFAULT_WORK,
};

use crate::{CliExit, EXIT_FAULT, EXIT_IO, EXIT_OK, EXIT_REFUSED};

pub(crate) const USAGE: &str =
    "keep <file.emath> --function F [--name NAME] --to <library.emath> [--set name=value] [--work N] [--overwrite]";

#[derive(Clone, Debug)]
pub(crate) struct KeepRequest {
    pub path: PathBuf,
    pub function: String,
    pub name: Option<String>,
    pub to: PathBuf,
    pub given: Vec<(String, String)>,
    pub work: u64,
    pub overwrite: bool,
}

impl KeepRequest {
    pub(crate) fn parse(rest: &[String]) -> Option<Self> {
        let (mut path, mut function, mut name, mut to) = (None, None, None, None);
        let mut given = Vec::new();
        let mut work = DEFAULT_WORK;
        let mut overwrite = false;
        let mut i = 0;
        while i < rest.len() {
            match rest[i].as_str() {
                "--function" => {
                    i += 1;
                    function = Some(rest.get(i)?.clone());
                }
                "--name" => {
                    i += 1;
                    name = Some(rest.get(i)?.clone());
                }
                "--to" => {
                    i += 1;
                    to = Some(PathBuf::from(rest.get(i)?));
                }
                "--set" => {
                    i += 1;
                    let (k, v) = rest.get(i)?.split_once('=')?;
                    given.push((k.to_string(), v.to_string()));
                }
                "--work" => {
                    i += 1;
                    work = rest.get(i)?.parse().ok()?;
                }
                "--overwrite" => overwrite = true,
                other => {
                    if other.starts_with('-') || path.is_some() {
                        return None;
                    }
                    path = Some(PathBuf::from(other));
                }
            }
            i += 1;
        }
        Some(Self { path: path?, function: function?, name, to: to?, given, work, overwrite })
    }
}

fn refuse(code: &str, message: impl std::fmt::Display, exit: CliExit) -> CliExit {
    eprintln!("error {code}: {message}");
    exit
}

pub(crate) fn run(request: KeepRequest) -> CliExit {
    let source = match fs::read_to_string(&request.path) {
        Ok(text) => text,
        Err(err) => return refuse("E-CLI-IO", format!("cannot read {}: {err}", request.path.display()), EXIT_IO),
    };
    let (tree, diagnostics) = emath_syntax::parse_str(&source);
    if diagnostics.has_errors() {
        crate::print_diagnostics(&diagnostics);
        return EXIT_REFUSED;
    }
    let inputs: BTreeMap<_, _> = request
        .given
        .iter()
        .map(|(k, v)| (k.clone(), parse_constructor_scalar(v)))
        .collect();
    let value = match evaluate_function_budgeted_at(
        &tree,
        &request.function,
        &inputs,
        request.work,
        None,
        &content_id_of_str(&source).0,
        Some(&request.path),
        &source,
    ) {
        Ok(value) => value,
        Err((err, _)) => return refuse(&err.code, err.message, EXIT_FAULT),
    };
    let entries = match &request.name {
        Some(name) => vec![(name.clone(), value)],
        None => match named_programs(&value) {
            Ok(entries) => entries,
            Err(err) => return refuse(&err.code, err.message, EXIT_REFUSED),
        },
    };
    let setting: Vec<String> = request.given.iter().map(|(k, v)| format!("{k}={v}")).collect();
    let origin = format!("{} {}({})", request.path.display(), request.function, setting.join(", "));

    let mut library = match fs::read_to_string(&request.to) {
        Ok(text) => Some(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return refuse("E-CLI-IO", format!("cannot read {}: {err}", request.to.display()), EXIT_IO),
    };
    // Every entry is placed before anything is written: one refusal
    // leaves the library untouched.
    let mut reports = Vec::new();
    let mut changed = false;
    for (name, program) in &entries {
        match place_definition(library.as_deref(), name, program, &origin, request.overwrite) {
            Ok((Placed::Unchanged, content)) => {
                reports.push(format!("unchanged {name} in {} (content {content})", request.to.display()));
            }
            Ok((Placed::Kept(text), content)) => {
                library = Some(text);
                changed = true;
                reports.push(format!("kept {name} in {} (content {content})", request.to.display()));
            }
            Err(err) => {
                let hint = if err.code == "E-KEEP-EXISTS" { "; pass --overwrite to replace it" } else { "" };
                return refuse(&err.code, format!("{}: {}{hint}", request.to.display(), err.message), EXIT_REFUSED);
            }
        }
    }
    let library = match library {
        Some(text) if changed => text,
        _ => {
            reports.iter().for_each(|line| println!("{line}"));
            return EXIT_OK;
        }
    };
    let (_, library_diagnostics) = emath_syntax::parse_str(&library);
    if library_diagnostics.has_errors() {
        crate::print_diagnostics(&library_diagnostics);
        return refuse("E-KEEP-SHAPE", "the kept library would not parse; nothing was written", EXIT_REFUSED);
    }
    if let Some(dir) = request.to.parent().filter(|d| !d.as_os_str().is_empty()) {
        if let Err(err) = fs::create_dir_all(dir) {
            return refuse("E-CLI-IO", format!("cannot create {}: {err}", dir.display()), EXIT_IO);
        }
    }
    if let Err(err) = fs::write(&request.to, library) {
        return refuse("E-CLI-IO", format!("cannot write {}: {err}", request.to.display()), EXIT_IO);
    }
    reports.iter().for_each(|line| println!("{line}"));
    EXIT_OK
}
