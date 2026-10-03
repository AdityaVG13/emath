//! `emath export lean <file.emath|dir> [--out DIR] [--check]`: translate
//! a user project's emath files into a Lean 4 project (`lean/` beside
//! them by default). A folder exports every `.emath` file under it;
//! sibling files a source `use`s come along. Without `--check` the
//! command only writes the project, so it can be read before anything
//! runs; `--check` then runs `lake build` on it, where the Lean kernel
//! proves every translated example. Declarations Lean cannot take are
//! named with the reason, and the rest still export.
//!
//! A function that calls `certify_equal` or `certify_equal_from`
//! becomes laws: theorems that the two quoted sequences agree at every
//! `n`, proved by the law library (`Law.lean`) on Mathlib. Such a
//! project uses one shared, prebuilt Mathlib (see `mathlib_packages`);
//! `--check` fetches Mathlib's cache there once and never compiles it.
//! A law Lean proves counts only if emath certifies it too: Lean's
//! arithmetic is total (`x / 0 = 0`) where emath's faults. Likewise an
//! example that faults in emath is not exported.
//!
//! Exit: 0 everything translated (and, with `--check`, Lean accepted);
//! 3 something was not translated, a law was not proven, or `lake` or
//! prebuilt Mathlib is missing; 1 Lean rejected the project (a wrong
//! example or law); 2 a source does not admit; 4 a write failed.

mod lean;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
    let mathlib = mathlib_packages();
    let export = match lean::export(&roots, &base, &mathlib) {
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
        "exported {} source files to {}: {} functions, {} examples, {} library examples, {} laws",
        export.sources,
        out.display(),
        export.functions,
        export.examples,
        export.library_examples,
        export.laws
    );
    for refusal in &export.refusals {
        println!("{refusal}");
    }
    let translated = if export.refusals.is_empty() { EXIT_OK } else { EXIT_PARTIAL };
    if !request.check {
        return translated;
    }
    if export.laws > 0
        && let Err(reason) = prebuilt_mathlib(&out, &mathlib)
    {
        println!("lean: not checked: {reason}");
        return EXIT_TOOLCHAIN;
    }
    let lake = Command::new("lake")
        .arg("build")
        .current_dir(&out)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let lake = match lake {
        Ok(child) => child,
        Err(error) => {
            println!("lean: not checked: cannot run `lake` ({error}); install Lean with elan");
            return EXIT_TOOLCHAIN;
        }
    };
    // emath certifies every law while Lean builds; only the verdicts on
    // laws Lean proves are read.
    let certified = certified(&export.law_sites);
    let output = match lake.wait_with_output() {
        Ok(output) => output,
        Err(error) => {
            println!("lean: not checked: `lake build` did not finish ({error})");
            return EXIT_TOOLCHAIN;
        }
    };
    let log = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let at = |lean: &str, line: usize| log.contains(&format!("{lean}:{line}:"));
    let uncertified: Vec<String> = export
        .law_sites
        .iter()
        .zip(&certified)
        .filter(|(site, certified)| {
            !**certified
                && (output.status.success() || !(site.first_line..=site.last_line).any(|line| at(&site.lean, line)))
        })
        .map(|(site, _)| {
            format!(
                "{}: Lean proves it, but emath does not certify it (Lean's arithmetic is total where emath's faults)",
                site.what
            )
        })
        .collect();
    if output.status.success() && uncertified.is_empty() {
        println!(
            "lean: accepted ({} examples, {} library examples and {} laws proved by the kernel)",
            export.examples, export.library_examples, export.laws
        );
        return translated;
    }
    let mut rejected = Vec::new();
    let mut unproven = uncertified;
    for site in &export.sites {
        if (site.first_line..=site.last_line).any(|line| at(&site.lean, line)) {
            rejected.push(site.what.clone());
        }
    }
    // An order bound that fails leaves the check without its point
    // count, so the check's own error then says nothing about the law.
    // A check only refutes when `emath_check` names a point where the
    // kernel proved the sides differ (`emath_kcheck`: a parameter point
    // the hypotheses allow).
    for site in &export.law_sites {
        let check = format!("{}:{}:", site.lean, site.check_line);
        let counterexample = log
            .lines()
            .filter(|line| line.contains(&check))
            .find_map(|line| line.split_once("the two sides differ at ").map(|(_, at)| at.trim().to_string()));
        if site.order_lines.iter().any(|&line| at(&site.lean, line)) {
            unproven.push(format!("{}: Lean found no order bound for a side", site.what));
        } else if site.bound_lines.iter().any(|&line| at(&site.lean, line)) {
            unproven.push(format!(
                "{}: Lean could not bound a side as a ratio of polynomials in the parameters under the hypotheses",
                site.what
            ));
        } else if let Some(point) = counterexample {
            rejected.push(format!("{}: the two sides differ at {point}", site.what));
        } else if at(&site.lean, site.check_line) {
            unproven.push(format!("{}: Lean could not evaluate the sides at the checked points", site.what));
        } else if (site.first_line..=site.last_line).any(|line| at(&site.lean, line)) {
            rejected.push(format!("{}: Lean does not accept the law's statement", site.what));
        }
    }
    if rejected.is_empty() && !unproven.is_empty() {
        println!("lean: not proven");
    } else if rejected.is_empty() {
        println!("lean: failed outside the translated examples and laws; see the log below");
    } else {
        println!("lean: rejected");
    }
    for what in &rejected {
        println!("rejected: {what}");
    }
    for what in &unproven {
        println!("unproven: {what}");
    }
    println!("{log}");
    if rejected.is_empty() && !unproven.is_empty() { EXIT_PARTIAL } else { EXIT_REFUSED }
}

/// Whether emath certifies each law, in `sites` order. Lean's arithmetic
/// is total (`x / 0 = 0`, a list read past its end gives 0) where emath
/// faults, so Lean can prove a statement emath refuses; a law Lean
/// proves counts only with emath's verdict, so Lean never claims more
/// than emath.
fn certified(sites: &[lean::LawSite]) -> Vec<bool> {
    let mut verdicts = vec![false; sites.len()];
    let mut by_source: Vec<(&Path, Vec<usize>)> = Vec::new();
    for (index, site) in sites.iter().enumerate() {
        match by_source.iter_mut().find(|(source, _)| *source == site.source.as_path()) {
            Some((_, indices)) => indices.push(index),
            None => by_source.push((&site.source, vec![index])),
        }
    }
    for (source, indices) in by_source {
        let calls: Vec<&str> = indices.iter().map(|&index| sites[index].emath_call.as_str()).collect();
        for (&index, certified) in indices.iter().zip(lean::emath_certifies(source, &calls)) {
            verdicts[index] = certified;
        }
    }
    verdicts
}

/// The shared Mathlib packages: `EMATH_LEAN_MATHLIB`, else
/// `~/.cache/emath/lean-mathlib`, so every export reuses one download.
/// Absolute, because Lake reads the path from the project folder.
fn mathlib_packages() -> PathBuf {
    let dir = match std::env::var_os("EMATH_LEAN_MATHLIB") {
        Some(dir) => PathBuf::from(dir),
        None => std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".cache/emath/lean-mathlib"),
    };
    if dir.is_absolute() {
        dir
    } else {
        std::env::current_dir().map(|cwd| cwd.join(&dir)).unwrap_or(dir)
    }
}

/// Makes sure the Mathlib modules the law library imports are built,
/// fetching Mathlib's prebuilt cache once if they are not. Compiling
/// Mathlib from source takes hours of every core, so this refuses
/// rather than let `lake build` start it.
fn prebuilt_mathlib(out: &Path, packages: &Path) -> Result<(), String> {
    let built = || {
        Command::new("lake")
            .args(["build", "--no-build"])
            .args(lean::law_imports())
            .current_dir(out)
            .output()
            .map(|output| output.status.success())
            .map_err(|error| format!("cannot run `lake` ({error}); install Lean with elan"))
    };
    // A missing Mathlib is cloned by the first `lake` call; say so first.
    let present = packages.join("mathlib").is_dir();
    if !present {
        println!("lean: fetching Mathlib and its prebuilt cache into {} (once, several GB)", packages.display());
    }
    if present && built()? {
        return Ok(());
    }
    if present {
        println!("lean: fetching Mathlib's prebuilt cache into {}", packages.display());
    }
    let fetched = Command::new("lake")
        .args(["exe", "cache", "get"])
        .env(lean::FETCH_ENV, "1")
        .current_dir(out)
        .status()
        .map_err(|error| format!("cannot run `lake` ({error}); install Lean with elan"))?;
    if fetched.success() && built()? {
        return Ok(());
    }
    Err(format!(
        "Mathlib in {} is not prebuilt and its cache could not be fetched; run `lake exe cache get` in {}",
        packages.display(),
        out.display()
    ))
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
