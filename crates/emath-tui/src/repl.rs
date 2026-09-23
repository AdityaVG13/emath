//! The line-REPL core (bead emath-q3cqx).
//!
//! Drives an opened session surface over any line reader (stdin or a
//! script file) with a deterministic transcript: no timestamps, no
//! timings, ordered maps, state-derived lines only. The same session
//! script produces byte-identical output on every run. Interactive
//! mode adds a `> ` prompt before each read; scripted mode adds
//! nothing.
//!
//! Command grammar (one command per line; blank lines and `#` comments
//! are ignored):
//!   - `help`                       list the commands
//!   - `step [budget]`              run one batch (default budget from
//!                                  the config)
//!   - `run [n]`                    run up to n batches; without n, run
//!                                  until the session closes
//!                                  (`goal_attained` or
//!                                  `domain_exhausted`; cap 1000 batches)
//!   - `show`                       the state and incumbent summary
//!   - `grow-case <id>`             freeze one more case ordinal; the
//!                                  next batch re-scores the archive
//!   - `save <path>`                write an emath.scratch.v1 checkpoint
//!   - `load <path>`                replace the session from a checkpoint
//!   - `export-native <dir>`        emit + build the native epoch host
//!                                  (artifact crate + sibling bin over
//!                                  the artifact ABI; cross-lane
//!                                  scratch parity is its acceptance)
//!   - `quit`                       end the session
//!
//! Errors print `error <code>: <message>` and the session continues;
//! only a broken stream or a failed session start ends the run with a
//! fault. `export-native` is an honest boundary: the artifact ABI is
//! proven, the epoch host bin is bead emath-8k3zw's deliverable, and
//! this line says so instead of faking an export.

use std::io::{BufRead, Write};

use super::host::{
    value_bool, value_int, value_rational, verdict_name, HostFault, LoopHost, LoopSession,
};
use emath_exec_ir::constructor_layer::BatchLedgerEntry;

/// The hard cap for `run` without a count: a non-terminating loop must
/// refuse by name, not hang the session.
const RUN_CAP: u64 = 1000;

/// REPL configuration.
#[derive(Clone, Debug)]
pub struct ReplConfig {
    /// The budget a `step`/`run` uses when the command omits one.
    pub default_budget: i128,
    /// Print a `> ` prompt before each read (interactive sessions only;
    /// never in scripted mode, so transcripts stay clean).
    pub interactive: bool,
}

/// How the session ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplOutcome {
    /// Real commands executed (comments and blank lines excluded).
    pub commands: usize,
    /// Batches committed in this run (the final ledger revision).
    pub batches: u64,
    /// The final state verdict (the vocabulary number).
    pub verdict: i128,
}

fn write_line(out: &mut dyn Write, line: &str) -> Result<(), HostFault> {
    out.write_all(line.as_bytes())
        .and_then(|()| out.write_all(b"\n"))
        .map_err(|err| HostFault::fault("loop_stream", format!("cannot write transcript: {err}")))
}

fn i128_list(items: &[i128]) -> String {
    let parts: Vec<String> = items.iter().map(std::string::ToString::to_string).collect();
    format!("[{}]", parts.join(", "))
}

fn error_line(out: &mut dyn Write, fault: &HostFault) -> Result<(), HostFault> {
    write_line(out, &format!("error {}: {}", fault.code, fault.message))
}

fn seed_line(out: &mut dyn Write, session: &LoopSession) -> Result<(), HostFault> {
    let batch = session.state_int("batch")?;
    let verdict = session.state_int("verdict")?;
    let used = session.state_int("used")?;
    let mode = session.state_int("mode")?;
    let incumbent = session.incumbent_record()?;
    let key = value_int(&incumbent, "key")?;
    let (num, den) = value_rational(&incumbent, "score")?;
    write_line(
        out,
        &format!(
            "seed batch {batch} verdict {} used {used} incumbent key {key} score {num}/{den} mode {mode}",
            verdict_name(verdict)
        ),
    )
}

fn batch_line(out: &mut dyn Write, entry: &BatchLedgerEntry) -> Result<(), HostFault> {
    write_line(
        out,
        &format!(
            "batch {} verdict {} used {} incumbent key {} score {}/{} promoted {} quarantined {} cases {} archive {}",
            entry.batch,
            verdict_name(entry.verdict),
            entry.used,
            entry.incumbent_key,
            entry.incumbent_score_num,
            entry.incumbent_score_den,
            i128_list(&entry.promoted),
            i128_list(&entry.quarantined),
            i128_list(&entry.case_ids),
            entry.archive_len,
        ),
    )
}

fn show_lines(out: &mut dyn Write, session: &LoopSession) -> Result<(), HostFault> {
    let batch = session.state_int("batch")?;
    let verdict = session.state_int("verdict")?;
    let used = session.state_int("used")?;
    let mode = session.state_int("mode")?;
    let archive = super::host::value_sequence(session.state(), "archive")?;
    let cases = session.case_ids()?;
    write_line(
        out,
        &format!(
            "state batch {batch} verdict {} used {used} mode {mode} archive {} cases {}",
            verdict_name(verdict),
            archive.len(),
            i128_list(&cases),
        ),
    )?;
    let incumbent = session.incumbent_record()?;
    let key = value_int(&incumbent, "key")?;
    let (value_num, value_den) = value_rational(&incumbent, "value")?;
    let (score_num, score_den) = value_rational(&incumbent, "score")?;
    let tier = value_int(&incumbent, "tier")?;
    let accepted = value_bool(&incumbent, "accepted")?;
    write_line(
        out,
        &format!(
            "incumbent key {key} value {value_num}/{value_den} score {score_num}/{score_den} tier {tier} accepted {accepted}"
        ),
    )
}

const HELP: &str = "commands:
  help                 list the commands
  step [budget]        run one batch (default from --budget)
  run [n]              run up to n batches; without n, until the session closes
  show                 the state and incumbent summary
  grow-case <id>       freeze one more case ordinal; the next batch re-scores
  save <path>          write an emath.scratch.v1 checkpoint
  load <path>          replace the session from a checkpoint
  export-native <dir>  emit + build the native epoch host (cross-lane parity)
  quit                 end the session";

/// Run the line-REPL over `input`, writing the deterministic
/// transcript to `out`. Returns the outcome, or the fault that ended
/// the run (a failed session start or a broken stream).
pub fn run_repl(
    host: &LoopHost,
    config: &ReplConfig,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<ReplOutcome, HostFault> {
    write_line(out, &format!("module {}", host.module_path().display()))?;
    write_line(
        out,
        &format!(
            "target {} state {}",
            host.surface().step,
            host.surface().state_type
        ),
    )?;
    write_line(out, &format!("meaning {}", host.identity().meaning_id))?;

    let mut session = host.begin()?;
    seed_line(out, &session)?;

    let mut commands = 0;
    let mut line = String::new();
    loop {
        line.clear();
        if config.interactive {
            out.write_all(b"> ")
                .map_err(|err| HostFault::fault("loop_stream", format!("cannot prompt: {err}")))?;
            out.flush()
                .map_err(|err| HostFault::fault("loop_stream", format!("cannot prompt: {err}")))?;
        }
        match input.read_line(&mut line) {
            Ok(0) => break, // EOF ends the session like quit.
            Ok(_) => {}
            Err(err) => {
                return Err(HostFault::fault("loop_stream", format!("cannot read: {err}")));
            }
        }
        let text = line.trim().to_string();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        commands += 1;
        let mut words = text.split_whitespace();
        let Some(word) = words.next() else {
            continue;
        };
        match word {
            "help" => {
                write_line(out, HELP)?;
            }
            "step" => {
                let budget = match words.next() {
                    Some(raw) => if let Ok(value) = raw.parse::<i128>() { value } else {
                        error_line(
                            out,
                            &HostFault::fault(
                                "loop_command",
                                format!("`{raw}` is not a budget (integer)"),
                            ),
                        )?;
                        continue;
                    },
                    None => config.default_budget,
                };
                match session.step(host, budget) {
                    Ok(entry) => batch_line(out, entry)?,
                    Err(fault) => error_line(out, &fault)?,
                }
            }
            "run" => {
                let count = match words.next() {
                    Some(raw) => if let Ok(value) = raw.parse::<u64>() { Some(value) } else {
                        error_line(
                            out,
                            &HostFault::fault(
                                "loop_command",
                                format!("`{raw}` is not a batch count"),
                            ),
                        )?;
                        continue;
                    },
                    None => None,
                };
                // The authored verdict vocabulary: 1 goal_attained and
                // 2 domain_exhausted close the session; 3 plateau and
                // 4 budget_exhausted are per-batch labels the host
                // resumes across (a bigger budget re-runs a batch that
                // ran out; a plateau batch keeps its stepping stones).
                let limit = count.unwrap_or(RUN_CAP);
                let mut closed = false;
                for _ in 0..limit {
                    match session.step(host, config.default_budget) {
                        Ok(entry) => {
                            batch_line(out, entry)?;
                            if entry.verdict == 1 || entry.verdict == 2 {
                                closed = true;
                                break;
                            }
                        }
                        Err(fault) => {
                            error_line(out, &fault)?;
                            closed = true;
                            break;
                        }
                    }
                }
                if count.is_none() && !closed {
                    error_line(
                        out,
                        &HostFault::fault(
                            "loop_run_cap",
                            format!("{RUN_CAP} batches without a closing verdict (goal_attained or domain_exhausted): pass a count or investigate the target"),
                        ),
                    )?;
                }
            }
            "show" => show_lines(out, &session)?,
            "grow-case" => {
                let Some(raw) = words.next() else {
                    error_line(
                        out,
                        &HostFault::fault("loop_command", "grow-case needs a case ordinal"),
                    )?;
                    continue;
                };
                match raw.parse::<i128>() {
                    Ok(case_id) => match session.grow_case(case_id) {
                        Ok(()) => write_line(out, &format!("case {case_id} frozen"))?,
                        Err(fault) => error_line(out, &fault)?,
                    },
                    Err(_) => error_line(
                        out,
                        &HostFault::fault(
                            "loop_command",
                            format!("`{raw}` is not a case ordinal (integer)"),
                        ),
                    )?,
                }
            }
            "save" => {
                let Some(path) = words.next() else {
                    error_line(out, &HostFault::fault("loop_command", "save needs a path"))?;
                    continue;
                };
                match session.save(host, std::path::Path::new(path)) {
                    Ok(()) => write_line(
                        out,
                        &format!("saved revision {} {}", session.revision(), path),
                    )?,
                    Err(fault) => error_line(out, &fault)?,
                }
            }
            "load" => {
                let Some(path) = words.next() else {
                    error_line(out, &HostFault::fault("loop_command", "load needs a path"))?;
                    continue;
                };
                match LoopSession::load(host, std::path::Path::new(path)) {
                    Ok(loaded) => {
                        session = loaded;
                        write_line(out, &format!("loaded revision {} {}", session.revision(), path))?;
                    }
                    Err(fault) => error_line(out, &fault)?,
                }
            }
            "export-native" => {
                let Some(dir) = words.next() else {
                    error_line(
                        out,
                        &HostFault::fault("loop_command", "export-native needs an output directory"),
                    )?;
                    continue;
                };
                match super::export::export_native(host, std::path::Path::new(dir)) {
                    Ok(report) => {
                        write_line(
                            out,
                            &format!("exported artifact {}", report.artifact_dir.display()),
                        )?;
                        write_line(
                            out,
                            &format!("exported epoch-host {}", report.host_dir.display()),
                        )?;
                        write_line(out, &format!("built {}", report.binary_path.display()))?;
                        write_line(
                            out,
                            &format!(
                                "run {} --batches N --budget 60 --scratch out.json (its checkpoint is byte-identical to this session's save)",
                                report.binary_path.display()
                            ),
                        )?;
                    }
                    Err(fault) => error_line(out, &fault)?,
                }
            }
            "quit" => break,
            other => {
                error_line(
                    out,
                    &HostFault::fault(
                        "loop_command",
                        format!("unknown command `{other}` (help lists the commands)"),
                    ),
                )?;
            }
        }
    }
    let verdict = session.state_int("verdict")?;
    write_line(
        out,
        &format!("end batches {} verdict {}", session.revision(), verdict_name(verdict)),
    )?;
    if config.interactive {
        out.flush()
            .map_err(|err| HostFault::fault("loop_stream", format!("cannot flush: {err}")))?;
    }
    Ok(ReplOutcome {
        commands,
        batches: session.revision(),
        verdict,
    })
}
