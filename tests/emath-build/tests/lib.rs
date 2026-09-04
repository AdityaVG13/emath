mod run_cargo_timed_tests {
    use emath_build::run_cargo_timed;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn timeout_kills_a_live_child() {
        let mut command = Command::new("sleep");
        command.arg("30");
        let error =
            run_cargo_timed(command, Duration::from_millis(200)).expect_err("must time out");
        assert!(
            error.contains("E-RES-120"),
            "expected E-RES-120, got {error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_grandchild_process_group() {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let pid_file = std::env::temp_dir().join(format!(
            "emath-pgid-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let script = format!("sleep 30 & echo $! > {}; wait", pid_file.display());
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        let error =
            run_cargo_timed(command, Duration::from_millis(250)).expect_err("must time out");
        assert!(
            error.contains("E-RES-120"),
            "expected E-RES-120, got {error}"
        );
        let grandchild = std::fs::read_to_string(&pid_file)
            .unwrap_or_default()
            .trim()
            .to_string();
        let _ = std::fs::remove_file(&pid_file);
        if !grandchild.is_empty() {
            let still_alive = Command::new("kill")
                .args(["-0", "--", &grandchild])
                .status()
                .is_ok_and(|status| status.success());
            assert!(
                !still_alive,
                "grandchild {grandchild} must die with the process group"
            );
        }
    }

    #[test]
    fn finished_child_is_not_reported_as_timeout() {
        let command = Command::new("true");
        let output = run_cargo_timed(command, Duration::from_secs(5)).expect("true must finish");
        assert!(output.status.success());
    }
}
