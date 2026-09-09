mod run_cargo_timed_tests {
    use emath_build::run_cargo_timed;
    use emath_test_harness::Probe;
    use std::process::Command;
    use std::time::Duration;

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
                        format!("process group must time out, got status {:?}", output.status),
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
