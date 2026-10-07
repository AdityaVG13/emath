//! One bounded external process run: the host side of an experiment trial.
//!
//! The runner owns the child from spawn to reap. It enforces the wall-clock
//! limit, the stdout byte limit, and cancellation through stdio completion,
//! even when descendants retain handles after the child exits. It never
//! interprets the output. Wall time is host-clocked with the monotonic
//! `Instant` from just before spawn to the first reap poll that observes the
//! exit, so it includes process start-up (for both arms alike) and carries
//! up to one poll interval of quantization.
//!
//! Isolation is a separate, optional layer: on macOS the program runs under
//! `sandbox-exec` with a generated deny profile. Without it a subprocess is
//! only a subprocess - callers must report that, not call it a sandbox.

#[cfg(unix)]
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Poll interval of the reap loop; the timing quantum of every sample.
pub const POLL_INTERVAL: Duration = Duration::from_micros(50);

/// A cooperative stop button shared with an embedding caller. Setting it
/// kills the running child at the next poll and stops the experiment at
/// the next commit boundary.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// What one run produced. Every non-`Success` arm is a distinct host fact;
/// none of them carries outputs that could be mistaken for a result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Success {
        stdout: Vec<u8>,
        wall_ns: u128,
    },
    /// The run hit its wall-clock limit and was killed.
    Timeout {
        wall_ns: u128,
    },
    /// The cancel token fired and the child was killed.
    Cancelled {
        wall_ns: u128,
    },
    /// Non-zero exit or death by signal.
    Crash {
        code: Option<i32>,
        signal: Option<i32>,
        stderr_tail: String,
    },
    /// stdout exceeded the byte limit; the child was killed.
    OutputLimit {
        limit: usize,
    },
    /// The program could not be started at all.
    SpawnFailed {
        detail: String,
    },
}

/// The macOS sandbox front end, when present on this host.
#[must_use]
pub fn sandbox_exec() -> Option<PathBuf> {
    let path = PathBuf::from("/usr/bin/sandbox-exec");
    (cfg!(target_os = "macos") && path.is_file()).then_some(path)
}

/// The Linux resource-limit front end, when present on this host.
#[must_use]
pub fn prlimit() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    ["/usr/bin/prlimit", "/bin/prlimit"]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

fn sandbox_literal(path: &Path) -> String {
    let text = path.display().to_string();
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Paths are canonicalized because the sandbox matches resolved paths
/// (`/tmp` is `/private/tmp` on macOS). A missing file is resolved through
/// its parent so a not-yet-written ledger is still covered.
#[must_use]
pub fn resolved(path: &Path) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map_or_else(|_| path.to_path_buf(), |real| real.join(name)),
        _ => path.to_path_buf(),
    }
}

/// What a trial process may touch.
#[derive(Clone, Debug)]
pub struct TrialAccess {
    pub network: bool,
    pub filesystem_write: bool,
    /// Files the child must never read (audit workload, ledger, checkpoint).
    pub protected: Vec<PathBuf>,
}

/// The sandbox profile for one trial binary: no forks, no exec other than
/// the binary itself, no reads of protected files, and the declared
/// network/write denials.
#[must_use]
pub fn trial_profile(program: &Path, access: &TrialAccess) -> String {
    let mut profile = String::from("(version 1)(allow default)");
    if !access.network {
        profile.push_str("(deny network*)");
    }
    if !access.filesystem_write {
        profile.push_str("(deny file-write*)(allow file-write-data (literal \"/dev/null\"))");
    }
    for path in &access.protected {
        profile.push_str(&format!(
            "(deny file-read* (literal {}))",
            sandbox_literal(path)
        ));
    }
    profile.push_str("(deny process-fork)(deny process-exec)");
    profile.push_str(&format!(
        "(allow process-exec (literal {}))",
        sandbox_literal(program)
    ));
    profile
}

/// The sandbox profile for a build: builds need to write their target dir
/// and run rustc, so only network and protected reads are denied. This keeps
/// build scripts and proc macros away from audit contents.
#[must_use]
pub fn build_profile(protected: &[PathBuf]) -> String {
    let mut profile = String::from("(version 1)(allow default)(deny network*)");
    for path in protected {
        profile.push_str(&format!(
            "(deny file-read* (literal {}))",
            sandbox_literal(path)
        ));
    }
    profile
}

/// One run request.
pub struct RunSpec<'a> {
    pub program: &'a Path,
    pub stdin: &'a [u8],
    pub cwd: &'a Path,
    pub timeout: Duration,
    pub output_limit: usize,
    /// `Some(profile)` runs under `sandbox-exec -p profile`.
    pub sandbox_profile: Option<&'a str>,
    /// `Some(bytes)` runs under `prlimit --as=bytes` (Linux).
    pub address_space_limit: Option<u64>,
}

fn command_for(spec: &RunSpec<'_>) -> Command {
    let mut command = if let (Some(profile), Some(sandbox)) = (spec.sandbox_profile, sandbox_exec())
    {
        let mut command = Command::new(sandbox);
        command.arg("-p").arg(profile).arg(spec.program);
        command
    } else if let (Some(bytes), Some(prlimit)) = (spec.address_space_limit, prlimit()) {
        let mut command = Command::new(prlimit);
        command
            .arg(format!("--as={bytes}"))
            .arg("--")
            .arg(spec.program);
        command
    } else {
        Command::new(spec.program)
    };
    // The child inherits nothing from the host environment (API keys,
    // tokens, CARGO_* settings): its only input is stdin.
    command.env_clear().current_dir(spec.cwd);
    command
}

/// Kill an unreaped child and its direct children, best effort.
/// Never call this after `try_wait` returns its status: pid reuse could
/// otherwise make the sweep target an unrelated process.
pub fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("pkill")
            .args(["-9", "-P", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

fn crash_of(status: ExitStatus, stderr: &[u8]) -> RunOutcome {
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    };
    #[cfg(not(unix))]
    let signal = None;
    let text = String::from_utf8_lossy(stderr);
    let tail: String = text
        .chars()
        .rev()
        .take(400)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    RunOutcome::Crash {
        code: status.code(),
        signal,
        stderr_tail: tail,
    }
}

/// Run one program to completion or to a limit. The child is always reaped
/// before this returns, whatever the outcome.
#[must_use]
#[cfg(unix)]
pub fn run_bounded(spec: &RunSpec<'_>, cancel: &CancelToken) -> RunOutcome {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;

    let stream_pair = || -> std::io::Result<(UnixStream, Stdio)> {
        let (host, child) = UnixStream::pair()?;
        host.set_nonblocking(true)?;
        Ok((host, Stdio::from(OwnedFd::from(child))))
    };
    let streams =
        (|| -> std::io::Result<_> { Ok((stream_pair()?, stream_pair()?, stream_pair()?)) })();
    let ((stdin, child_stdin), (mut stdout, child_stdout), (mut stderr, child_stderr)) =
        match streams {
            Ok(streams) => streams,
            Err(error) => {
                return RunOutcome::SpawnFailed {
                    detail: format!("cannot create child stdio: {error}"),
                };
            }
        };
    let mut command = command_for(spec);
    command
        .stdin(child_stdin)
        .stdout(child_stdout)
        .stderr(child_stderr);
    let started = Instant::now();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return RunOutcome::SpawnFailed {
                detail: format!("cannot start {}: {err}", spec.program.display()),
            };
        }
    };
    // Drop Command's retained child endpoints so they cannot mask EOF.
    drop(command);
    let mut stdin = Some(stdin);
    let mut written = 0;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let (mut out_done, mut err_done) = (false, false);
    let mut chunk = [0u8; 8192];
    let limit = spec.output_limit;
    let mut status = None;
    let mut wall_ns = 0;
    loop {
        if status.is_none() {
            match child.try_wait() {
                Ok(Some(exit)) => {
                    status = Some(exit);
                    // Preserve the measurement endpoint: reap, not pipe EOF.
                    wall_ns = started.elapsed().as_nanos();
                }
                Ok(None) => {}
                Err(error) => {
                    kill_tree(&mut child);
                    let _ = child.wait();
                    return RunOutcome::SpawnFailed {
                        detail: format!("cannot wait on child: {error}"),
                    };
                }
            }
        }
        let mut progress = false;
        if written == spec.stdin.len() {
            stdin = None;
        }
        if let Some(pipe) = &mut stdin {
            let remaining = &spec.stdin[written..];
            match pipe.write(&remaining[..remaining.len().min(chunk.len())]) {
                Ok(0) => stdin = None,
                Ok(n) => {
                    written += n;
                    progress = true;
                    if written == spec.stdin.len() {
                        stdin = None;
                    }
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                // A child is allowed not to consume the input (EPIPE).
                Err(_) => stdin = None,
            }
        }
        let mut overflow = false;
        for (pipe, buffer, done, cap, is_stdout) in [
            (&mut stdout, &mut out, &mut out_done, limit, true),
            (&mut stderr, &mut err, &mut err_done, 64 * 1024, false),
        ] {
            if *done {
                continue;
            }
            match pipe.read(&mut chunk) {
                Ok(0) => *done = true,
                Ok(n) => {
                    let room = cap.saturating_sub(buffer.len());
                    overflow |= is_stdout && n > room;
                    buffer.extend_from_slice(&chunk[..n.min(room)]);
                    // Keep draining stderr after its capture cap: closing it
                    // or ignoring it can crash or block a verbose child.
                    progress = true;
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                Err(_) => *done = true,
            }
        }
        let elapsed = started.elapsed();
        let reason = if cancel.is_cancelled() {
            Some(RunOutcome::Cancelled {
                wall_ns: elapsed.as_nanos(),
            })
        } else if overflow {
            Some(RunOutcome::OutputLimit { limit })
        } else if status.is_some() && out_done && err_done && stdin.is_none() {
            let status = status.expect("exit was observed");
            return if status.success() {
                RunOutcome::Success {
                    stdout: out,
                    wall_ns,
                }
            } else {
                crash_of(status, &err)
            };
        } else if elapsed >= spec.timeout {
            Some(RunOutcome::Timeout {
                wall_ns: elapsed.as_nanos(),
            })
        } else {
            None
        };
        if let Some(reason) = reason {
            if status.is_none() {
                kill_tree(&mut child);
                let _ = child.wait();
            }
            return reason;
        }
        if !progress {
            std::thread::sleep(POLL_INTERVAL.min(spec.timeout - elapsed.min(spec.timeout)));
        }
    }
}

/// Refuse before spawn when safe interruptible stdio is unavailable.
#[must_use]
#[cfg(not(unix))]
pub fn run_bounded(_spec: &RunSpec<'_>, _cancel: &CancelToken) -> RunOutcome {
    RunOutcome::SpawnFailed {
        detail: "bounded process stdio requires Unix socket pairs on this host".into(),
    }
}
