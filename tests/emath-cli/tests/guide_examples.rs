//! The user guide in `docs/guide/` must not go stale. Every program it
//! shows lives under `docs/guide/examples/`: each must admit and pass
//! its own `tests:`, and each folder there is a project the guide
//! exports to Lean, which must translate with nothing refused. Every
//! `docs/guide/examples/...` path a page names must exist, so a page
//! never points at a file that moved. Lean itself is not run here
//! (`export_lean` covers `--check`).

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::emath_bin;
use emath_cli::EXIT_OK;
use emath_test_harness::Probe;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn emath(dir: &Path, args: &[&str]) -> (String, i32) {
    let out = Command::new(emath_bin())
        .args(args)
        .current_dir(dir)
        .env("EMATH_ROOT", workspace())
        .output()
        .expect("run emath");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (text, out.status.code().unwrap_or(-1))
}

fn files(dir: &Path, extension: &str, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| !name.to_string_lossy().starts_with('.')) {
                files(&path, extension, found);
            }
        } else if path.extension().is_some_and(|ext| ext == extension) {
            found.push(path);
        }
    }
}

#[test]
fn guide_examples() {
    let mut probe = Probe::new("docs/guide: every example admits, passes its tests, and exports to Lean");
    let guide = workspace().join("docs/guide");
    let examples = guide.join("examples");

    let mut programs = Vec::new();
    files(&examples, "emath", &mut programs);
    programs.sort();
    probe.demand("examples-exist", !programs.is_empty(), examples.display().to_string());
    for program in &programs {
        let name = program.strip_prefix(&workspace()).unwrap_or(program).display().to_string();
        let path = program.to_string_lossy().to_string();
        let (text, code) = emath(&workspace(), &["check", &path]);
        probe.demand(&format!("check-{name}"), code == EXIT_OK as i32, format!("exit {code}\n{text}"));
        let (text, code) = emath(&workspace(), &["test", &path]);
        probe.demand(&format!("test-{name}"), code == EXIT_OK as i32, format!("exit {code}\n{text}"));
    }

    let out = std::env::temp_dir().join(format!("emath_guide_examples_{}", std::process::id()));
    let mut projects: Vec<PathBuf> = std::fs::read_dir(&examples)
        .map(|entries| entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect())
        .unwrap_or_default();
    projects.sort();
    for project in &projects {
        let name = project.file_name().unwrap_or_default().to_string_lossy().to_string();
        let target = out.join(&name).to_string_lossy().to_string();
        let (text, code) = emath(&workspace(), &["export", "lean", &project.to_string_lossy(), "--out", &target]);
        probe.demand(
            &format!("export-{name}"),
            code == EXIT_OK as i32 && !text.contains("not translated"),
            format!("exit {code}\n{text}"),
        );
    }

    let mut pages = Vec::new();
    files(&guide, "md", &mut pages);
    probe.demand("pages-exist", pages.iter().any(|page| page.ends_with("docs/guide/README.md")), format!("{pages:?}"));
    for page in &pages {
        let text = std::fs::read_to_string(page).unwrap_or_default();
        for (start, _) in text.match_indices("docs/guide/examples/") {
            let path: String =
                text[start..].chars().take_while(|c| c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-')).collect();
            let path = path.trim_end_matches('.');
            probe.demand(
                &format!("named-path-exists-{path}"),
                workspace().join(path).exists(),
                page.display().to_string(),
            );
        }
    }
    probe.finish();
}
