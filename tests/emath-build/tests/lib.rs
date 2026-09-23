mod run_cargo_timed_tests {
    use emath_build::run_cargo_timed;
    use emath_test_harness::Probe;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn child_resource_defaults_and_overrides() {
        for (jobs, threads) in [(None, None), (Some("3"), Some("2"))] {
            let mut command = Command::new("sh");
            command.env_clear().args([
                "-c",
                "printf '%s:%s' \"$CARGO_BUILD_JOBS\" \"$RUST_TEST_THREADS\"",
            ]);
            if let Some(jobs) = jobs {
                command.env("CARGO_BUILD_JOBS", jobs);
            }
            if let Some(threads) = threads {
                command.env("RUST_TEST_THREADS", threads);
            }
            let output = run_cargo_timed(command, Duration::from_secs(5)).unwrap();
            assert!(output.status.success());
            let expected_jobs = jobs.map(str::to_owned).unwrap_or_else(|| {
                std::env::var("CARGO_BUILD_JOBS").unwrap_or_else(|_| "1".into())
            });
            let expected_threads = threads.map(str::to_owned).unwrap_or_else(|| {
                std::env::var("RUST_TEST_THREADS").unwrap_or_else(|_| "1".into())
            });
            let expected = format!("{expected_jobs}:{expected_threads}");
            assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        }
    }

    #[test]
    fn unchanged_generated_source_preserves_mtime() {
        let path =
            std::env::temp_dir().join(format!("emath-generated-write-{}", std::process::id()));
        emath_build::write_generated_file(&path, b"first").unwrap();
        let old_time = std::time::UNIX_EPOCH + Duration::from_secs(1_000_000);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old_time)
            .unwrap();
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        emath_build::write_generated_file(&path, b"first").unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before,
            "identical generated content must not dirty Cargo's input fingerprint"
        );
        emath_build::write_generated_file(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert_ne!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before
        );
    }

    #[test]
    fn probe() {
        let mut p = Probe::new(
            "run_cargo_timed kills live children with E-RES-120 and never reports a finished child as timeout",
        );
        p.case("timeout", |p| {
            let mut command = Command::new("sleep");
            command.arg("30");
            match run_cargo_timed(command, Duration::from_millis(200)) {
                Err(error) => {
                    p.contains("code", &error, "E-RES-120");
                }
                Ok(output) => {
                    p.fail(
                        "timeout",
                        format!("sleep 30 must time out, got status {:?}", output.status),
                    );
                }
            }
        });
        #[cfg(unix)]
        p.case("process-group", |p| {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let pid_file = std::env::temp_dir().join(format!(
                "emath-pgid-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let script = format!("sleep 30 & echo $! > {}; wait", pid_file.display());
            let mut command = Command::new("sh");
            command.arg("-c").arg(script);
            match run_cargo_timed(command, Duration::from_millis(250)) {
                Err(error) => {
                    p.contains("code", &error, "E-RES-120");
                }
                Ok(output) => {
                    p.fail(
                        "timeout",
                        format!(
                            "process group must time out, got status {:?}",
                            output.status
                        ),
                    );
                }
            }
            let grandchild = std::fs::read_to_string(&pid_file)
                .unwrap_or_default()
                .trim()
                .to_string();
            let _ = std::fs::remove_file(&pid_file);
            if grandchild.is_empty() {
                p.fail("pid", "grandchild pid file must be written before timeout");
            } else {
                let still_alive = Command::new("kill")
                    .args(["-0", "--", &grandchild])
                    .status()
                    .is_ok_and(|status| status.success());
                p.demand(
                    "dead",
                    still_alive == false,
                    format!("grandchild {grandchild} must die with the process group"),
                );
            }
        });
        p.case("finished", |p| {
            let command = Command::new("true");
            match run_cargo_timed(command, Duration::from_secs(5)) {
                Ok(output) => {
                    p.demand(
                        "success",
                        output.status.success(),
                        format!("true must finish successfully, got {:?}", output.status),
                    );
                }
                Err(error) => {
                    p.fail("finish", format!("true must finish, got timeout {error}"));
                }
            }
        });
        p.finish();
    }
}
