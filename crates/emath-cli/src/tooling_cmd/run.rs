//! The `emath run` / `test` / `bench` execution commands.

use super::*;

/// `run <file> [--out <dir>]`: build then execute the generated crate.
/// Library crates have no binary; their example tests are the runnable
/// surface, so `emath run` executes them (crate mains run as binaries).
pub(crate) fn run_cmd(file: &Path, out: &Path) -> CliExit {
    if let Some(code) = crate::meaning_cmd::refuse_malformed_project_lock(file) {
        return code;
    }
    let report = match build_file(
        file,
        out,
        BuildOptions {
            verify_generated_crate: false,
            ..BuildOptions::default()
        },
    ) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("error: {error}");
            return classify_build_error(&error);
        }
    };
    // Artifact dir is named by its content id (`fnv1a64:<hash>`); cargo
    // rejects the colon, so stage a colon-free copy under the temp dir.
    let hash = report
        .artifact_id
        .0
        .split(':')
        .next_back()
        .unwrap_or(&report.artifact_id.0);
    let run_dir = std::env::temp_dir().join(format!("emath-run-{hash}"));
    // RAII guard: wipe the staged tree on every exit path (a bare
    // success-path remove leaked it on early returns).
    struct RunDirGuard(PathBuf);
    impl Drop for RunDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _ = std::fs::remove_dir_all(&run_dir);
    let _run_dir_guard = RunDirGuard(run_dir.clone());
    if let Err(error) = std::fs::create_dir_all(run_dir.join("src")).and_then(|()| {
        std::fs::copy(
            report.artifact_dir.join("Cargo.toml"),
            run_dir.join("Cargo.toml"),
        )
        .map(|_| ())
    }) {
        eprintln!("error: cannot stage generated crate for execution: {error}");
        return EXIT_USAGE;
    }
    let src_dir = report.artifact_dir.join("src");
    match std::fs::read_dir(&src_dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        eprintln!(
                            "error: cannot read staged sources under {}: {error}",
                            src_dir.display()
                        );
                        return EXIT_USAGE;
                    }
                };
                let name = entry.file_name();
                if let Err(error) = std::fs::copy(entry.path(), run_dir.join("src").join(&name)) {
                    eprintln!(
                        "error: cannot stage {} for execution: {error}",
                        entry.path().display()
                    );
                    return EXIT_USAGE;
                }
            }
        }
        Err(error) => {
            eprintln!(
                "error: cannot read artifact sources {}: {error}",
                src_dir.display()
            );
            return EXIT_USAGE;
        }
    }
    let manifest = run_dir.join("Cargo.toml");
    let library_crate = !run_dir.join("src/main.rs").exists();
    let mut command = Command::new("cargo");
    if library_crate {
        println!(
            "run: artifact {} crate `{}` is a library; executing its example tests",
            report.artifact_id.0, report.crate_name
        );
        // Skip rustdoc/doctests: generated libs have example `#[test]`s,
        // and bare `cargo test` launches rustdoc anyway.
        command
            .args([
                "test",
                "--lib",
                "--bins",
                "--tests",
                "--quiet",
                "--manifest-path",
            ])
            .arg(&manifest)
            .env("CARGO_TARGET_DIR", generated_crate_target_dir(hash));
    } else {
        command
            .args(["run", "--quiet", "--manifest-path"])
            .arg(&manifest)
            .env("CARGO_TARGET_DIR", generated_crate_target_dir(hash));
    }
    let output = match run_cargo_timed(command, std::time::Duration::from_secs(600)) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("error: cannot execute generated crate: {error}");
            return EXIT_USAGE;
        }
    };
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        EXIT_OK
    } else {
        eprintln!("error: generated crate exited with {}", output.status);
        EXIT_REFUSED
    }
}

/// `test <file> [--out <dir>]`: build and run the generated crate's tests.
pub(crate) fn test_cmd(file: &Path, out: &Path) -> CliExit {
    if let Some(code) = crate::meaning_cmd::refuse_malformed_project_lock(file) {
        return code;
    }
    match build_file(
        file,
        out,
        BuildOptions {
            verify_generated_crate: true,
            ..BuildOptions::default()
        },
    ) {
        Ok(report) => {
            println!(
                "test: artifact {} crate `{}` tests passed",
                report.artifact_id.0, report.crate_name
            );
            EXIT_OK
        }
        Err(error) => {
            eprintln!("error: {error}");
            classify_build_error(&error)
        }
    }
}

/// `bench <file>`: typed refusal. Candidate-vs-baseline CLI comparison
/// needs the keep-gate ruleset (Phase 4+); never `EXIT_OK` with empty
/// output. Use `cargo bench --profile release-perf --bench
/// comprehensive_bench`.
pub(crate) fn bench_cmd(file: &Path) -> CliExit {
    eprintln!(
        "error: E-TLT-004: benchmarking `{}` is not a Phase 1 CLI comparison; measure via `cargo bench --profile release-perf --bench comprehensive_bench` (keep-gate history in .bench-history/)",
        file.display()
    );
    EXIT_REFUSED
}
