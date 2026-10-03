//! `emath export lean <file.emath|dir> [--out DIR] [--check]`: translate
//! a user project's emath files into a Lean 4 project (`lean/` beside
//! them by default). A folder exports every `.emath` file under it;
//! sibling files a source `use`s come along. Without `--check` the
//! command only writes the project, so it can be read before anything
//! runs; `--check` then runs `lake build` on it, where the Lean kernel
//! proves every translated example. Declarations Lean cannot take are
//! named with the reason, and the rest still export.
//!
//! Exit: 0 everything translated (and, with `--check`, Lean accepted);
//! 3 something was not translated, or `lake` is missing; 1 Lean
//! rejected the project; 2 a source does not admit; 4 a write failed.

mod lean;

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{CliExit, EXIT_ADMISSION, EXIT_IO, EXIT_OK, EXIT_PARTIAL, EXIT_REFUSED, EXIT_TOOLCHAIN, EXIT_USAGE};

pub(crate) const USAGE: &str = "export lean <file.emath|dir> [--out <dir>] [--check]";

#[derive(Clone, Debug)]
pub(crate) struct ExportRequest {
    pub path: PathBuf,
    pub out: Option<PathBuf>,
    pub check: bool,
}

impl ExportRequest {
    pub(crate) fn parse(rest: &[String]) -> Option<Self> {
        let mut positionals = Vec::new();
        let mut out = None;
        let mut check = false;
        let mut i = 0;
        while i < rest.len() {
            match rest[i].as_str() {
                "--out" | "-o" => {
                    i += 1;
                    out = Some(PathBuf::from(rest.get(i)?));
                }
                "--check" => check = true,
                flag if flag.starts_with('-') => return None,
                positional => positionals.push(positional.to_string()),
            }
            i += 1;
        }
        match positionals.as_slice() {
            [target, path] if target == "lean" => Some(Self { path: PathBuf::from(path), out, check }),
            _ => None,
        }
    }
}

pub(crate) fn run(request: ExportRequest) -> CliExit {
    let path = match request.path.canonicalize() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("error: E-PKG-080: {}: {error}", request.path.display());
            return EXIT_USAGE;
        }
    };
    let base = if path.is_dir() {
        path.clone()
    } else {
        path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
    };
    let out = match &request.out {
        Some(out) if out.is_absolute() => out.clone(),
        Some(out) => std::env::current_dir().map(|cwd| cwd.join(out)).unwrap_or_else(|_| out.clone()),
        None => base.join("lean"),
    };
    let roots = if path.is_dir() {
        let mut roots = Vec::new();
        collect_sources(&path, &out, &mut roots);
        roots.sort();
        roots
    } else {
        vec![path.clone()]
    };
    if roots.is_empty() {
        eprintln!("error: no .emath files under {}", path.display());
        return EXIT_USAGE;
    }
    let export = match lean::export(&roots, &base) {
        Ok(export) => export,
        Err(message) => {
            eprintln!("error: E-EXPORT-SOURCE: {message}");
            return EXIT_ADMISSION;
        }
    };
    for (file, contents) in &export.files {
        let target = out.join(file);
        let written = target
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&target, contents));
        if let Err(error) = written {
            eprintln!("error: E-PKG-080: {}: {error}", target.display());
            return EXIT_IO;
        }
    }
    println!(
        "exported {} source files to {}: {} functions, {} examples, {} library examples",
        export.sources,
        out.display(),
        export.functions,
        export.examples,
        export.library_examples
    );
    for refusal in &export.refusals {
        println!("{refusal}");
    }
    let translated = if export.refusals.is_empty() { EXIT_OK } else { EXIT_PARTIAL };
    if !request.check {
        return translated;
    }
    let output = match Command::new("lake").arg("build").current_dir(&out).output() {
        Ok(output) => output,
        Err(error) => {
            println!("lean: not checked: cannot run `lake` ({error}); install Lean with elan");
            return EXIT_TOOLCHAIN;
        }
    };
    let log = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        println!(
            "lean: accepted ({} examples and {} library examples proved by the kernel)",
            export.examples, export.library_examples
        );
        return translated;
    }
    println!("lean: rejected");
    for site in &export.sites {
        let hit = (site.first_line..=site.last_line)
            .any(|line| log.contains(&format!("{}:{line}:", site.lean)));
        if hit {
            println!("rejected: {}", site.what);
        }
    }
    println!("{log}");
    EXIT_REFUSED
}

/// Every `.emath` file under `dir`, skipping hidden directories (run
/// directories live there) and the output directory.
fn collect_sources(dir: &Path, out: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if path.is_dir() {
            if !hidden && path != out {
                collect_sources(&path, out, found);
            }
        } else if path.extension().is_some_and(|ext| ext == "emath") {
            found.push(path);
        }
    }
}
