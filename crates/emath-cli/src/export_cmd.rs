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

pub(crate) const USAGE: &str = "export lean <file.emath|dir> [--out <dir>] [--check] [--work N]";

#[derive(Clone, Debug)]
pub(crate) struct ExportRequest {
    pub path: PathBuf,
    pub out: Option<PathBuf>,
    pub check: bool,
    /// Each example's emath work budget, as `emath test --work`.
    pub work: Option<u64>,
}

impl ExportRequest {
    pub(crate) fn parse(rest: &[String]) -> Option<Self> {
        let mut positionals = Vec::new();
        let mut out = None;
        let mut check = false;
        let mut work = None;
        let mut i = 0;
        while i < rest.len() {
            match rest[i].as_str() {
                "--out" | "-o" => {
                    i += 1;
                    out = Some(PathBuf::from(rest.get(i)?));
                }
                "--check" => check = true,
                "--work" => {
                    i += 1;
                    work = Some(rest.get(i)?.parse::<u64>().ok()?);
                }
                flag if flag.starts_with('-') => return None,
                positional => positionals.push(positional.to_string()),
            }
            i += 1;
        }
        match positionals.as_slice() {
            [target, path] if target == "lean" => Some(Self { path: PathBuf::from(path), out, check, work }),
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
    let export = match lean::export(&roots, &base, &mathlib, request.work) {
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
            .and_then(|()| emath_build::write_generated_file(&target, contents));
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
    let certified = certified(&export);
    let output = match lake.wait_with_output() {
        Ok(output) => output,
        Err(error) => {
            println!("lean: not checked: `lake build` did not finish ({error})");
            return EXIT_TOOLCHAIN;
        }
    };
    let log = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    // Only errors place a verdict on a site; a linter warning on a
    // proved theorem's line says nothing about it.
    let error_log: String = log.lines().filter(|line| line.starts_with("error:")).collect::<Vec<_>>().join("\n");
    let at = |lean: &str, line: usize| error_log.contains(&format!("{lean}:{line}:"));
    // A file whose imports failed is never elaborated, so no error at a
    // site there says nothing about it. Lake names each file it builds
    // or replays.
    let elaborated = |lean: &str| {
        let module = lean.trim_end_matches(".lean").replace('/', ".");
        log.lines().any(|line| {
            let mut words = line.split_whitespace();
            words.any(|word| ["Built", "Building", "Replayed"].contains(&word)) && words.next() == Some(module.as_str())
        })
    };
    let uncertified: Vec<String> = export
        .law_sites
        .iter()
        .zip(&certified)
        .filter(|(site, certified)| {
            !**certified
                && (output.status.success() || !(site.first_line..=site.last_line).any(|line| at(&site.lean, line)))
        })
        .map(|(site, _)| {
            if output.status.success() || elaborated(&site.lean) {
                format!(
                    "{}: Lean proves it, but emath does not certify it (emath found no bound for a side, or a side faults in emath where Lean's arithmetic is total)",
                    site.what
                )
            } else {
                format!("{}: emath does not certify it, and Lean did not check it (a file it imports failed)", site.what)
            }
        })
        .collect();
    if output.status.success() && uncertified.is_empty() {
        println!(
            "lean: accepted ({} examples, {} library examples and {} laws proved by the kernel)",
            export.examples, export.library_examples, export.laws
        );
        return translated;
    }
    // Lean's errors located in a site's lines; when every one is a spent
    // time or depth budget, Lean never decided the claim either way.
    let errors = |lean: &str, lines: std::ops::RangeInclusive<usize>| -> Vec<&str> {
        let marks: Vec<String> = lines.map(|line| format!("{lean}:{line}:")).collect();
        error_log.lines().filter(|text| marks.iter().any(|mark| text.contains(mark.as_str()))).collect()
    };
    let out_of_budget = |errors: &[&str]| {
        !errors.is_empty()
            && errors.iter().all(|text| {
                ["deterministic timeout", "(deterministic) timeout", "deep recursion", "maximum recursion depth"]
                    .iter()
                    .any(|budget| text.contains(budget))
            })
    };
    const OUT_OF_BUDGET: &str = "Lean ran out of time or recursion depth before deciding it";
    let mut rejected = Vec::new();
    let mut unproven = uncertified;
    for site in &export.sites {
        let errors = errors(&site.lean, site.first_line..=site.last_line);
        if out_of_budget(&errors) {
            unproven.push(format!("{}: {OUT_OF_BUDGET}", site.what));
        } else if !errors.is_empty() {
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
        let counterexample = error_log
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
        } else if out_of_budget(&errors(&site.lean, site.first_line..=site.last_line)) {
            unproven.push(format!("{}: {OUT_OF_BUDGET}", site.what));
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
fn certified(export: &lean::Export) -> Vec<bool> {
    let sites = &export.law_sites;
    let mut verdicts = vec![false; sites.len()];
    let mut by_source: Vec<(&Path, Vec<usize>)> = Vec::new();
    for (index, site) in sites.iter().enumerate() {
        match by_source.iter_mut().find(|(source, _)| *source == site.source.as_path()) {
            Some((_, indices)) => indices.push(index),
            None => by_source.push((&site.source, vec![index])),
        }
    }
    for (source, indices) in by_source {
        let Some(module) = export.snapshots.get(source) else {
            continue;
        };
        let calls: Vec<&str> = indices.iter().map(|&index| sites[index].emath_call.as_str()).collect();
        for (&index, certified) in indices.iter().zip(lean::emath_certifies(source, module, &calls)) {
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
    std::fs::create_dir_all(packages)
        .map_err(|error| format!("cannot create shared Mathlib packages ({error})"))?;
    let lock = std::fs::File::options().read(true).write(true).create(true).truncate(false)
        .open(packages.join(".emath-setup.lock"))
        .map_err(|error| format!("cannot open shared Mathlib setup lock ({error})"))?;
    lock.lock().map_err(|error| format!("cannot lock shared Mathlib setup ({error})"))?;
    compatible_mathlib(packages)?;
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
        println!("lean: preparing Mathlib's prebuilt cache in {} (reusing downloaded files)", packages.display());
    }
    let setup = mathlib_setup(packages)?;
    let fetched = Command::new("lake")
        .args(["exe", "cache", "get"])
        .args(lean::law_imports())
        .env(lean::FETCH_ENV, "1")
        .env("GIT_HTTP_USER_AGENT", "OpenAI File Downloader, XaiImageApiFetch/1.0")
        .env("CURL_HOME", &setup)
        .current_dir(&setup)
        .status()
        .map_err(|error| format!("cannot run `lake` ({error}); install Lean with elan"))?;
    if fetched.success() {
        compatible_mathlib(packages)?;
    }
    if fetched.success() && built()? {
        return Ok(());
    }
    Err(format!(
        "Mathlib in {} is not prebuilt and its cache could not be fetched; run `lake exe cache get` in {}",
        packages.display(),
        setup.display()
    ))
}

/// Never let Lake update a user-supplied checkout to our pinned revision.
fn compatible_mathlib(packages: &Path) -> Result<(), String> {
    let manifest = emath_artifact::parse_json_document(lean::MANIFEST).map_err(|error| error.to_string())?;
    let emath_artifact::JsonValue::Arr(entries) = manifest.field("packages").map_err(|error| error.to_string())? else {
        return Err("invalid embedded Mathlib package manifest".into());
    };
    for entry in entries {
        let name = entry.string_field("name").map_err(|error| error.to_string())?;
        let dir = packages.join(&name);
        if !dir.exists() {
            continue;
        }
        let expected = entry.string_field("rev").map_err(|error| error.to_string())?;
        let git = Command::new("git").arg("-C").arg(&dir).args(["rev-parse", "HEAD"]).output()
            .map_err(|error| format!("cannot inspect Mathlib package `{name}` ({error})"))?;
        if !git.status.success() || String::from_utf8_lossy(&git.stdout).trim() != expected {
            return Err(format!(
                "Mathlib package {} is not at the required revision {expected}; leave this checkout unchanged and set EMATH_LEAN_MATHLIB to compatible packages",
                dir.display()
            ));
        }
    }
    let root = packages.join("mathlib");
    if root.exists() {
        let toolchain = std::fs::read_to_string(root.join("lean-toolchain"))
            .map_err(|error| format!("cannot read Mathlib's lean-toolchain ({error})"))?;
        if toolchain.trim() != lean::TOOLCHAIN.trim() {
            return Err(format!(
                "Mathlib in {} needs {}, but this export needs {}; set EMATH_LEAN_MATHLIB to compatible packages",
                root.display(), toolchain.trim(), lean::TOOLCHAIN.trim()
            ));
        }
    }
    Ok(())
}

fn mathlib_setup(packages: &Path) -> Result<PathBuf, String> {
    // A stable workspace aligns Mathlib's extraction path with Lake's
    // resolver. No dependency artifacts belong to an individual export.
    let setup = packages.join(".emath-setup-v4.34.1");
    let linked = setup.join(".lake/packages");
    std::fs::create_dir_all(linked.parent().unwrap())
        .map_err(|error| format!("cannot create shared Mathlib setup ({error})"))?;
    if std::fs::symlink_metadata(&linked).is_ok() {
        if linked.canonicalize().ok() != packages.canonicalize().ok() {
            return Err(format!("{} does not point at {}; leave it unchanged", linked.display(), packages.display()));
        }
    } else {
        #[cfg(unix)]
        std::os::unix::fs::symlink(packages, &linked)
            .map_err(|error| format!("cannot link shared Mathlib packages ({error})"))?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(packages, &linked)
            .map_err(|error| format!("cannot link shared Mathlib packages ({error})"))?;
        #[cfg(not(any(unix, windows)))]
        return Err("shared Mathlib setup requires directory link support".into());
    }
    for (file, text) in [
        ("lakefile.lean", lean::cache_lakefile(packages)),
        ("lean-toolchain", lean::TOOLCHAIN.to_string()),
        ("lake-manifest.json", lean::manifest(packages)),
        (".curlrc", "user-agent = \"OpenAI File Downloader, XaiImageApiFetch/1.0\"\n".to_string()),
    ] {
        emath_build::write_generated_file(setup.join(file), text)
            .map_err(|error| format!("cannot write shared Mathlib setup ({error})"))?;
    }
    Ok(setup)
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
