//! Host helpers for generated Rust crates: a stable Cargo target dir,
//! write-if-changed emission, and a wall-clock-bounded Cargo run.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

/// Persistent cargo target dir for generated crates (`$CWD/target/emath-cargo/<key>`).
/// Incremental rustc survives across `emath test` / `--verify` because those
/// paths wipe their source staging dirs after each invocation.
#[must_use]
pub fn generated_crate_target_dir(key: &str) -> PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    dir.push("target");
    dir.push("emath-cargo");
    dir.push(key.replace(['/', ':'], "-"));
    dir
}

/// Write a generated source or manifest without invalidating unchanged Cargo inputs.
pub fn write_generated_file(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> std::io::Result<()> {
    let path = path.as_ref();
    let contents = contents.as_ref();
    if std::fs::read(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    std::fs::write(path, contents)
}

/// Run a Cargo command under a wall-clock budget, including stdio drain.
/// Inherited output handles cannot extend the budget after Cargo exits.
#[cfg(unix)]
pub fn run_cargo_timed(
    mut command: std::process::Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output, String> {
    use std::io::{ErrorKind, Read};
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let started = Instant::now();
    // Only the host endpoints are nonblocking. Socket pairs provide safe,
    // cancellable byte streams without unsafe fcntl or reader threads that
    // can outlive the run when descendants retain inherited descriptors.
    let output_pair = || -> std::io::Result<(UnixStream, Stdio)> {
        let (host, child) = UnixStream::pair()?;
        host.set_nonblocking(true)?;
        Ok((host, Stdio::from(OwnedFd::from(child))))
    };
    let (mut stdout_pipe, stdout_stdio) =
        output_pair().map_err(|error| format!("cannot create stdout: {error}"))?;
    let (mut stderr_pipe, stderr_stdio) =
        output_pair().map_err(|error| format!("cannot create stderr: {error}"))?;
    // Generated crates can live outside the workspace's Cargo configuration.
    // Bound their default fan-out without overriding an explicit caller choice.
    for key in ["CARGO_BUILD_JOBS", "RUST_TEST_THREADS"] {
        if !command.get_envs().any(|(name, _)| name == key) {
            command.env(key, std::env::var_os(key).unwrap_or_else(|| "1".into()));
        }
    }
    // Stay in the terminal's foreground group so Ctrl-C / SIGTERM reach
    // cargo and rustc. On timeout, kill cargo's children first (rustc),
    // then cargo: Child::kill alone leaves the compiler holding
    // CARGO_TARGET_DIR.
    let mut child = command
        .stdout(stdout_stdio)
        .stderr(stderr_stdio)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot spawn cargo: {error}"))?;
    // Command retains caller-supplied descriptors for reuse. Closing those
    // copies here is necessary for EOF when the child closes its own copies.
    drop(command);
    let mut status = None;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let (mut stdout_done, mut stderr_done) = (false, false);
    let mut chunk = [0u8; 8192];
    loop {
        if status.is_none() {
            match child.try_wait() {
                Ok(exit) => status = exit,
                Err(error) => {
                    kill_timed_child(&mut child);
                    let _ = child.wait();
                    return Err(format!("cannot wait on cargo: {error}"));
                }
            }
        }
        let mut progress = false;
        for (pipe, buffer, done) in [
            (&mut stdout_pipe, &mut stdout, &mut stdout_done),
            (&mut stderr_pipe, &mut stderr, &mut stderr_done),
        ] {
            if *done {
                continue;
            }
            match pipe.read(&mut chunk) {
                Ok(0) => *done = true,
                Ok(n) => {
                    buffer.extend_from_slice(&chunk[..n]);
                    progress = true;
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                Err(_) => *done = true,
            }
        }
        if let Some(status) = status.filter(|_| stdout_done && stderr_done) {
            return Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
        }
        if started.elapsed() >= timeout {
            // Never sweep a reaped pid: it may already belong to an unrelated
            // process. Open descendants' descriptors are simply closed here.
            if status.is_none() {
                kill_timed_child(&mut child);
                let _ = child.wait();
            }
            return Err(format!(
                "E-RES-120: cargo exceeded the {timeout:?} wall-clock budget"
            ));
        }
        if !progress {
            std::thread::sleep(
                (timeout - started.elapsed().min(timeout)).min(Duration::from_millis(5)),
            );
        }
    }
}

/// Refuse rather than spawn when safe interruptible stdio is unavailable.
#[cfg(not(unix))]
pub fn run_cargo_timed(
    _command: std::process::Command,
    _timeout: std::time::Duration,
) -> Result<std::process::Output, String> {
    Err("bounded process stdio requires Unix socket pairs on this host".into())
}

fn kill_timed_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // try_wait already proved this pid is still our child. Kill rustc
        // (and other direct children) first; then SIGKILL cargo. A new
        // process group would orphan the same tree on Ctrl-C.
        let pid = child.id();
        let _ = std::process::Command::new("pkill")
            .args(["-9", "-P", &pid.to_string()])
            .status();
        let _ = child.kill();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}
