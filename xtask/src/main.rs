#![forbid(unsafe_code)]

//! emath capstone demos: `cargo xtask demo cache-policy` and
//! `cargo xtask demo semantic-genesis`.
//!
//! Both demos run the real compiler pipeline through the `emath` CLI and
//! verify deterministic artifacts plus negative controls. Std-only.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const REFERENCE_SOURCE: &str = "language/examples/01_arbitrary_glyphs.emath";
const REPLAY_DIR: &str = "validation/semantic-genesis/replay";
const GENERATED_DIR: &str = "examples/generated/semantic-genesis-worlds";

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let code = if args.first().map(String::as_str) == Some("demo") {
        match args.get(1).map(String::as_str) {
            Some("cache-policy") => demo_cache_policy(),
            Some("semantic-genesis") => {
                demo_semantic_genesis(args.iter().any(|a| a == "--update-replay"))
            }
            Some("all") => {
                let cache = demo_cache_policy();
                if cache != 0 {
                    cache
                } else {
                    demo_semantic_genesis(false)
                }
            }
            other => {
                eprintln!(
                    "unknown demo {other:?}; usage: cargo xtask demo <cache-policy|semantic-genesis|all> [--update-replay]"
                );
                2
            }
        }
    } else {
        eprintln!("usage: cargo xtask demo <cache-policy|semantic-genesis|all> [--update-replay]");
        2
    };
    std::process::exit(i32::from(code));
}

fn demo_cache_policy() -> u8 {
    println!("== demo cache-policy ==");
    let work = temp_dir("emath-xtask-cache");
    match run_demo_cache_policy(&work) {
        Ok(()) => {
            println!("cache-policy demo: ok");
            0
        }
        Err(error) => {
            eprintln!("cache-policy demo FAILED: {error}");
            1
        }
    }
}

fn run_demo_cache_policy(work: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(work)
        .map_err(|error| format!("cannot create {}: {error}", work.display()))?;
    // Full V5 pipeline with verification.
    check(
        cargo_run(&[
            "run",
            "-q",
            "-p",
            "emath-cli",
            "--",
            "build",
            "implementation/tests/valid/stateful.emath",
            "--out",
            &work.display().to_string(),
            "--verify",
        ]),
        "artifact build --verify",
    )?;
    // Host promotion with constructor invariants + negative control.
    let host = check(
        cargo_run(&["run", "-q", "-p", "demo-host"]),
        "demo-host run",
    )?;
    let host_out = String::from_utf8(host.stdout).map_err(|error| error.to_string())?;
    require(&host_out, "host integration ok", "demo-host final line")?;
    require(&host_out, "score(3.0) = 7", "demo-host score print")?;
    require(&host_out, "negative control", "demo-host negative control")?;
    Ok(())
}

fn demo_semantic_genesis(update_replay: bool) -> u8 {
    println!("== demo semantic-genesis ==");
    let work = temp_dir("emath-xtask-sg");
    match run_demo_semantic_genesis(&work, update_replay) {
        Ok(()) => {
            println!("semantic-genesis demo: ok");
            0
        }
        Err(error) => {
            eprintln!("semantic-genesis demo FAILED: {error}");
            1
        }
    }
}

const GENESIS_ARTIFACTS: [&str; 7] = [
    "parse-forest.json",
    "signature.json",
    "free-term.json",
    "meaning-problem.json",
    "interpretation-portfolio.json",
    "world-admission.jsonl",
    "answer-receipt.json",
];

fn run_demo_semantic_genesis(work: &Path, update_replay: bool) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(work)
        .map_err(|error| format!("cannot create {}: {error}", work.display()))?;
    let a = work.join("a");
    let b = work.join("b");
    let generated = work.join("generated");
    let source_arg = REFERENCE_SOURCE;

    // Full analysis pipeline, twice, on identical sources.
    check(
        cargo_run(&[
            "run",
            "-q",
            "-p",
            "emath-cli",
            "--",
            "genesis",
            source_arg,
            "--out",
            &a.display().to_string(),
        ]),
        "genesis run #1",
    )?;
    check(
        cargo_run(&[
            "run",
            "-q",
            "-p",
            "emath-cli",
            "--",
            "genesis",
            source_arg,
            "--out",
            &b.display().to_string(),
        ]),
        "genesis run #2",
    )?;
    for name in GENESIS_ARTIFACTS {
        require_bytes(&a.join(name), "genesis artifact")?;
    }
    let candidates = a.join("world-candidates");
    let entry_count = std::fs::read_dir(&candidates)
        .map_err(|error| error.to_string())?
        .count();
    if entry_count != 3 {
        return Err(format!("expected 3 world candidates, got {entry_count}"));
    }
    diff_dirs(&a, &b, "genesis determinism")?;

    // Replay bundle: committed expected artifacts must match regeneration.
    let replay = Path::new(REPLAY_DIR);
    if update_replay {
        let _ = std::fs::remove_dir_all(replay);
        copy_dir_all(&a, replay)?;
        println!("replay bundle refreshed at {REPLAY_DIR}");
    } else {
        if !replay.join("answer-receipt.json").is_file() {
            return Err(format!(
                "replay bundle missing at {REPLAY_DIR}; run `cargo xtask demo semantic-genesis --update-replay` once"
            ));
        }
        diff_dirs(&a, replay, "replay fidelity")?;
    }

    // Parametric codegen onto a fresh directory.
    check(
        cargo_run(&[
            "run",
            "-q",
            "-p",
            "emath-cli",
            "--",
            "compile",
            "--parametric",
            source_arg,
            "--out",
            &generated.display().to_string(),
        ]),
        "compile --parametric",
    )?;
    for name in [
        "Cargo.toml",
        "src/lib.rs",
        "src/main.rs",
        "manifest.json",
        "source-map.json",
    ] {
        require_bytes(&generated.join(name), "generated file")?;
    }
    // The committed generated crate must be byte-identical (the CLI-only
    // manifest/source-map artifacts are not part of the committed crate).
    diff_dirs_excluding(
        &generated,
        Path::new(GENERATED_DIR),
        "committed generated crate fidelity",
        &["manifest.json", "source-map.json"],
    )?;

    // Generated crate tests: fixtures + wrong-world negative control.
    let test = Command::new("cargo")
        .args(["test", "--quiet", "--manifest-path"])
        .arg(generated.join("Cargo.toml"))
        .output()
        .map_err(|error| error.to_string())?;
    if !test.status.success() {
        let stderr = String::from_utf8_lossy(&test.stderr);
        return Err(format!("generated crate tests failed:\n{stderr}"));
    }

    // Runtime differential evaluation through the generated binary.
    let run = Command::new("cargo")
        .args(["run", "--quiet", "--manifest-path"])
        .arg(generated.join("Cargo.toml"))
        .output()
        .map_err(|error| error.to_string())?;
    if !run.status.success() {
        let stderr = String::from_utf8_lossy(&run.stderr);
        return Err(format!("generated crate run failed:\n{stderr}"));
    }
    let stdout = String::from_utf8(run.stdout).map_err(|error| error.to_string())?;
    let mut free = None;
    let mut boolean = None;
    let mut modular = None;
    let mut swapped = None;
    for line in stdout.lines() {
        if let Some(value) = line.strip_prefix("free: ") {
            free = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("boolean: ") {
            boolean = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("modular-17: ") {
            modular = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("swapped-modular-17: ") {
            swapped = Some(value.to_string());
        }
    }
    let free = free.ok_or("missing free: line")?;
    let boolean = boolean.ok_or("missing boolean: line")?;
    let modular = modular.ok_or("missing modular-17: line")?;
    let swapped = swapped.ok_or("missing swapped-modular-17: line")?;
    if free != "apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))" {
        return Err(format!("free-symbolic result wrong: {free}"));
    }
    if boolean != "false" {
        return Err(format!("boolean result wrong: {boolean}"));
    }
    if modular != "6" {
        return Err(format!("modular-17 result wrong: {modular}"));
    }
    if swapped == "6" {
        return Err(format!("wrong world not rejected: swapped = {swapped}"));
    }
    println!("free: {free}");
    println!("boolean: {boolean}");
    println!("modular-17: {modular}");
    println!("swapped-modular-17: {swapped} (rejected)");
    Ok(())
}

fn cargo_run(args: &[&str]) -> Output {
    Command::new("cargo")
        .args(args)
        .output()
        .expect("cargo spawns")
}

fn check(output: Output, what: &str) -> Result<Output, String> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "{what} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn require(haystack: &str, needle: &str, what: &str) -> Result<(), String> {
    if haystack.contains(needle) {
        Ok(())
    } else {
        Err(format!("{what}: missing `{needle}` in:\n{haystack}"))
    }
}

fn require_bytes(path: &Path, what: &str) -> Result<(), String> {
    if std::fs::read(path).is_ok_and(|bytes| !bytes.is_empty()) {
        Ok(())
    } else {
        Err(format!("{what}: missing or empty {}", path.display()))
    }
}

fn temp_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{label}-{}", std::process::id()))
}

fn diff_dirs(left: &Path, right: &Path, what: &str) -> Result<(), String> {
    diff_dirs_excluding(left, right, what, &[])
}

fn diff_dirs_excluding(
    left: &Path,
    right: &Path,
    what: &str,
    exclude: &[&str],
) -> Result<(), String> {
    let mut left_files = Vec::new();
    collect_files(left, &mut left_files)?;
    let mut right_files = Vec::new();
    collect_files(right, &mut right_files)?;
    let keep = |relative: &String| !exclude.contains(&relative.as_str());
    let left_files = left_files.into_iter().filter(keep).collect::<Vec<_>>();
    let right_files = right_files.into_iter().filter(keep).collect::<Vec<_>>();
    if left_files != right_files {
        return Err(format!(
            "{what}: file sets differ\nleft:  {left_files:?}\nright: {right_files:?}"
        ));
    }
    for relative in left_files {
        let l = std::fs::read(left.join(&relative)).map_err(|e| e.to_string())?;
        let r = std::fs::read(right.join(&relative)).map_err(|e| e.to_string())?;
        if l != r {
            return Err(format!("{what}: {relative} differs"));
        }
    }
    Ok(())
}

fn collect_files(root: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let mut queue = vec![root.to_path_buf()];
    while let Some(dir) = queue.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                queue.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into_owned();
                out.push(relative);
            }
        }
    }
    out.sort();
    Ok(())
}

fn copy_dir_all(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let mut queue = vec![from.to_path_buf()];
    while let Some(dir) = queue.pop() {
        let relative = dir.strip_prefix(from).map_err(|e| e.to_string())?;
        let to_dir = if relative.as_os_str().is_empty() {
            to.to_path_buf()
        } else {
            to.join(relative)
        };
        std::fs::create_dir_all(&to_dir).map_err(|e| e.to_string())?;
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                queue.push(path);
            } else {
                let target = to_dir.join(entry.file_name());
                std::fs::copy(&path, &target).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
