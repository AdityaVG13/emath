//! The interactive REPL and world-name resolution.

use super::*;

/// The genesis lane (unchanged surface): evaluate a genesis-format
/// reference file on the semantic VM. `--world` selects one admitted
/// world; a lock commits the locked fingerprint; plain evals use the
/// default world.
pub(super) fn eval_genesis(path: &Path, world_name: Option<&str>, json: bool) -> CliExit {
    let analysis = match genesis_cmd::analyze(path) {
        Ok(analysis) => analysis,
        Err(error) => return refuse_eval(&error, json),
    };
    let all_worlds = genesis_cmd::builtin_worlds(&analysis.inference.signature);
    let selection = match crate::meaning_cmd::resolve_locked_worlds(path, &analysis, all_worlds) {
        Ok(selection) => selection,
        Err(error) => return refuse_eval(&error, json),
    };
    if let Some(lock) = &selection.lock {
        if let Some(wanted) = world_name {
            match evaluate_named(&analysis, wanted) {
                Ok(receipt) if receipt.world_id == lock.fingerprint => {
                    emit_receipt(&receipt.with_lock(lock.lock_id), json);
                    return EXIT_OK;
                }
                Ok(_) => {
                    return refuse_eval(
                        &format!(
                            "E-LOCK-004: --world `{wanted}` disagrees with locked fingerprint {:016x}; re-open the portfolio with `emath meaning unset`",
                            lock.fingerprint
                        ),
                        json,
                    );
                }
                Err(error) => return refuse_eval(&error, json),
            }
        }
        match evaluate_world(&analysis, &selection.worlds[0]) {
            receipt => {
                emit_receipt(&receipt.with_lock(lock.lock_id), json);
                EXIT_OK
            }
        }
    } else {
        let wanted = world_name.unwrap_or(DEFAULT_WORLD);
        match evaluate_named(&analysis, wanted) {
            Ok(receipt) => {
                emit_receipt(&receipt, json);
                EXIT_OK
            }
            Err(error) => refuse_eval(&error, json),
        }
    }
}

pub(super) fn repl_cmd(path: &Path) -> CliExit {
    let analysis = match genesis_cmd::analyze(path) {
        Ok(analysis) => analysis,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    let all_worlds = genesis_cmd::builtin_worlds(&analysis.inference.signature);
    let selection =
        match crate::meaning_cmd::resolve_locked_worlds(path, &analysis, all_worlds.clone()) {
            Ok(selection) => selection,
            Err(error) => {
                eprintln!("{error}");
                return EXIT_REFUSED;
            }
        };
    let worlds = if selection.lock.is_some() {
        selection.worlds.clone()
    } else {
        all_worlds
    };
    let default = worlds
        .first()
        .map(|world| world.name.as_str())
        .unwrap_or(DEFAULT_WORLD);
    let Ok(mut last) = evaluate_in_world(&analysis, &worlds, default) else {
        eprintln!("{}", unknown_world_error(default));
        return EXIT_REFUSED;
    };
    if let Some(lock) = &selection.lock {
        last = last.with_lock(lock.lock_id);
    }
    emit_receipt(&last, false);

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            return EXIT_REFUSED;
        };
        match parse_repl_line(&line) {
            ReplOp::Empty => {}
            ReplOp::Quit => return EXIT_OK,
            ReplOp::Unknown => println!("{UNKNOWN_REPL}"),
            ReplOp::Portfolio => print_portfolio(&analysis, &worlds),
            ReplOp::Explain => print_explain(&last),
            ReplOp::World(name) => match resolve_world_name(&name) {
                Ok(label) => {
                    if let Some(lock) = &selection.lock {
                        let locked_name = worlds[0].name.as_str();
                        if label != locked_name {
                            eprintln!(
                                "error: E-LOCK-004: :world `{label}` disagrees with locked `{locked_name}` ({:016x}); re-open the portfolio with `emath meaning unset`",
                                lock.fingerprint
                            );
                            continue;
                        }
                    }
                    match evaluate_in_world(&analysis, &worlds, label) {
                        Ok(receipt) => {
                            let receipt = match &selection.lock {
                                Some(lock) => receipt.with_lock(lock.lock_id),
                                None => receipt,
                            };
                            emit_receipt(&receipt, false);
                            last = receipt;
                        }
                        Err(error) => eprintln!("{error}"),
                    }
                }
                Err(error) => eprintln!("{error}"),
            },
        }
    }
    EXIT_OK
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReplOp {
    Empty,
    Quit,
    Portfolio,
    Explain,
    World(String),
    Unknown,
}

pub(super) fn parse_repl_line(line: &str) -> ReplOp {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ReplOp::Empty;
    }
    if trimmed == ":quit" {
        return ReplOp::Quit;
    }
    if trimmed == ":portfolio" {
        return ReplOp::Portfolio;
    }
    if trimmed == ":explain" {
        return ReplOp::Explain;
    }
    if let Some(rest) = trimmed.strip_prefix(":world") {
        let mut parts = rest.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some(name), None) => ReplOp::World(name.to_string()),
            _ => ReplOp::Unknown,
        }
    } else {
        ReplOp::Unknown
    }
}

pub(super) fn resolve_world_name(name: &str) -> Result<&'static str, String> {
    ADMITTED_WORLDS
        .iter()
        .chain(WORLD_IR_WORLD_NAMES.iter())
        .copied()
        .find(|label| *label == name)
        .ok_or_else(|| unknown_world_error(name))
}

pub(super) fn unknown_world_error(name: &str) -> String {
    format!("error: E-GEN-092: unknown world `{name}`")
}
