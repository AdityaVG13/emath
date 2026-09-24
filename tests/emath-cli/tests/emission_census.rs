//! Emission census gate (emath-nwmm6): every RUNNABLE module in
//! `language/modules` must compile offline as an emitted crate.
//!
//! The dogfooding census, made repeatable: walk every authored module,
//! `emath build --json` each one, and offline-compile every crate that
//! reports `runnable: true`. The gate law is `runnable implies compiles`
//! - a module the emitter claims must actually build with no network and
//! no dependencies. Not-runnable modules are the named refusal lanes
//! (unknown-value carriers, quote.view, non-authored records); their
//! COUNT is pinned so a module silently losing runnable status fails the
//! gate and forces a conscious update, with the drift named.
//!
//! Determinism: modules walk in sorted order and report sorted; scratch
//! dirs are unique per run and never deleted (repo law); the four
//! compile lanes take disjoint sorted chunks. Runtime is the sweep's
//! (~7 minutes on one machine) - this is the DSR census lane, not part
//! of a fast feedback loop.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;

mod common;

/// The pinned not-runnable census (2026-09-24, commit 13691db): 15
/// modules across the named refusal lanes. Update this number ONLY with
/// a bead that names the lane that changed.
const NOT_RUNNABLE_PINNED: usize = 15;

/// Fixed compile-lane count (the sweep's `-P 4`).
const LANES: usize = 4;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every authored module under `language/modules`, sorted for
/// deterministic walking and reporting.
fn census_modules() -> Vec<PathBuf> {
    let root = repo_root().join("language/modules");
    let mut modules = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("emath") {
                modules.push(path);
            }
        }
    }
    modules.sort();
    modules
}

/// One census row: the module, whether the emitter claimed it runnable,
/// and the first `error` lines when the emitted crate fails to compile
/// offline.
#[derive(Debug)]
struct Row {
    module: String,
    runnable: bool,
    error: Option<String>,
}

/// Emit one module and, when the emitter claims it runnable, compile
/// the emitted crate offline.
fn census_row(bin: &Path, module: &Path, out: &Path) -> Row {
    let build = Command::new(bin)
        .args([
            "build",
            module.to_str().expect("utf8 module path"),
            "--out",
            out.to_str().expect("utf8 scratch path"),
            "--json",
        ])
        .output()
        .expect("run emath build");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let row = Row {
        module: module.display().to_string(),
        runnable: text.contains("\"runnable\": true"),
        error: None,
    };
    if !row.runnable {
        return row;
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let compiled = Command::new(&cargo)
        .args([
            "build",
            "--offline",
            "--manifest-path",
            out.join("Cargo.toml").to_str().expect("utf8 manifest"),
        ])
        .env("CARGO_TARGET_DIR", out.join("target"))
        .output()
        .expect("run cargo build for emitted crate");
    if compiled.status.success() {
        row
    } else {
        let stderr = String::from_utf8_lossy(&compiled.stderr);
        let first = stderr
            .lines()
            .filter(|line| line.starts_with("error"))
            .take(4)
            .collect::<Vec<_>>()
            .join("\n");
        Row {
            error: Some(first),
            ..row
        }
    }
}

#[test]
fn emission_census_every_runnable_module_compiles_offline() {
    let bin = common::emath_bin();
    let modules = census_modules();
    assert!(
        !modules.is_empty(),
        "census found no modules under language/modules"
    );

    let run_id = format!(
        "{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let scratch = |module: &Path| -> PathBuf {
        let stem = module.file_stem().and_then(|s| s.to_str()).expect("stem");
        let parent = module
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|p| p.to_str())
            .expect("parent");
        std::env::temp_dir().join(format!(
            "emath_census_{run_id}_{}__{parent}",
            stem.replace(['-', '.'], "_")
        ))
    };

    // Disjoint sorted chunks: lane i takes modules i, i+LANES, ... -
    // each lane's cargo runs in its own per-module target dir, so the
    // lanes never contend on a build lock.
    let (tx, rx) = mpsc::channel::<Row>();
    std::thread::scope(|scope| {
        for lane in 0..LANES {
            let tx = tx.clone();
            let chunk: Vec<PathBuf> = modules
                .iter()
                .enumerate()
                .filter(|(index, _)| index % LANES == lane)
                .map(|(_, module)| module.clone())
                .collect();
            let bin = bin.to_path_buf();
            scope.spawn(move || {
                for module in chunk {
                    let out = scratch(&module);
                    let row = census_row(&bin, &module, &out);
                    let _ = tx.send(row);
                }
            });
        }
    });
    drop(tx);

    let mut rows: Vec<Row> = rx.iter().collect();
    rows.sort_by(|a, b| a.module.cmp(&b.module));

    let failures: Vec<&Row> = rows
        .iter()
        .filter(|row| row.runnable && row.error.is_some())
        .collect();
    assert!(
        failures.is_empty(),
        "runnable modules must compile offline ({} of {} failed):\n{}",
        failures.len(),
        rows.len(),
        failures
            .iter()
            .map(|row| {
                format!(
                    "{}\n{}",
                    row.module,
                    row.error.as_deref().unwrap_or("<no error lines>")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    );

    let not_runnable: Vec<&Row> = rows.iter().filter(|row| !row.runnable).collect();
    assert_eq!(
        not_runnable.len(),
        NOT_RUNNABLE_PINNED,
        "the not-runnable census drifted (pinned {NOT_RUNNABLE_PINNED}); name the lane that changed:\n{}",
        not_runnable
            .iter()
            .map(|row| row.module.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
