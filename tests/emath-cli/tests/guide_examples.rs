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
use std::time::{Duration, Instant};

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

#[test]
fn guide_dream_and_simulations() {
    let mut probe = Probe::new("guide: discovery keeps a reusable formula, simulations report actual endpoints");
    let root = workspace();
    let decay = "docs/guide/examples/decay.emath";
    for (end, budget, expected) in [
        ("1", "2", ["accepted_t=1/1", "complete=true", "n_steps=2", "y=[1/4]"]),
        ("2", "1", ["accepted_t=1/2", "complete=false", "n_steps=1", "y=[1/2]"]),
        ("3/4", "2", ["accepted_t=3/4", "complete=true", "n_steps=2", "y=[3/8]"]),
    ] {
        let (text, code) = emath(
            &root,
            &[
                "run",
                decay,
                "--function",
                "decay",
                "--set",
                &format!("end_time={end}"),
                "--set",
                "step_size=1/2",
                "--set",
                &format!("budget={budget}"),
            ],
        );
        probe.demand(
            &format!("decay-{end}-{budget}"),
            code == 0 && expected.iter().all(|field| text.contains(field)),
            format!("exit {code}\n{text}"),
        );
    }
    let (text, code) = emath(&root, &["run", decay, "--function", "trace", "--set", "step_size=1/2", "--set", "steps=2"]);
    probe.demand("decay-trace", code == 0 && text.trim() == "[[0/1, 1/1], [1/2, 1/2], [1/1, 1/4]]", text);

    let out = std::env::temp_dir().join(format!("emath_guide_dream_{}", std::process::id()));
    std::fs::create_dir_all(&out).expect("guide run root");
    let out_s = out.display().to_string();
    let (text, code) = emath(
        &root,
        &[
            "dream",
            "docs/guide/examples/dream.emath",
            "--target",
            "StepFormula",
            "--run",
            "formula",
            "--root",
            &out_s,
            "--detach",
            "--budget",
            "4",
            "--max-batches",
            "4",
            "--checkpoint-every",
            "1",
        ],
    );
    probe.demand("dream-launch", code == 0 && text.lines().next() == Some("formula"), format!("exit {code}\n{text}"));
    let run = out.join("formula");
    let deadline = Instant::now() + Duration::from_secs(30);
    let journal = loop {
        let text = std::fs::read_to_string(run.join("journal.jsonl")).unwrap_or_default();
        if text.contains("\"t\":\"end\"") || code != 0 || Instant::now() >= deadline {
            break text;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    probe.demand(
        "dream-finds-formula",
        journal.contains("\"outcome\":\"goal_attained\"")
            && journal.lines().filter(|line| line.contains("\"t\":\"batch\"")).count() == 2
            && journal.contains("\"t\":\"kept\""),
        &journal,
    );
    let (text, code) = emath(&root, &["dream", "list", "--root", &out_s]);
    probe.demand("dream-list", code == 0 && text.contains("formula  StepFormula  goal_attained  b2"), text);
    let (text, code) = emath(&root, &["dream", "export", "formula", "--root", &out_s, "--format", "csv"]);
    probe.demand("dream-export", code == 0 && text.lines().count() == 3 && text.contains("goal_attained"), text);
    // This is the run's authored library, not a hand-written substitute.
    let library = run.join("world/kept.emath");
    let (text, code) = emath(&root, &["check", &library.to_string_lossy()]);
    probe.demand("kept-admits", code == 0, text);
    let caller = out.join("use_kept.emath");
    std::fs::write(
        &caller,
        r#"use formula.world.kept

emath function ten:
    inputs:
        unused: Int
    outputs:
        result: Rat
    definitions:
        result = triangular(10)
"#,
    )
    .expect("kept-library caller");
    let (text, code) = emath(&root, &["run", &caller.to_string_lossy(), "--function", "ten", "--set", "unused=0"]);
    probe.demand("kept-reused", code == 0 && text.trim() == "55/1", format!("exit {code}\n{text}"));
    probe.finish();
}
