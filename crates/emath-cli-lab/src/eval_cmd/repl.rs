//! World-name resolution for the eval lane.

use super::*;

/// The genesis lane (unchanged surface): evaluate a genesis-format
/// reference file on the semantic VM. `--world` selects one admitted
/// world; a lock commits the locked fingerprint; plain evals use the
/// default world.
#[allow(unreachable_code, unused_variables)]
pub(super) fn eval_genesis(path: &Path, world_name: Option<&str>, json: bool) -> CliExit {
    return refuse_eval_coded(
        "E-KIND-GONE",
        "genesis worlds are not a constructor. Write an ordinary `emath function` and `emath run`.",
        json,
    );
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
