//! Export a run's journal as tables, and list the runs under a root.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use emath_exec_ir::constructor_layer::{render_scratch_value, CValue};

use super::journal::{read_events, JournalEvent, RecordView};
use super::rundir::{pid_alive, RunDir};
use crate::host::HostFault;

/// A value as the export and the dashboard spell it: exact integers,
/// `n/d` rationals (an integer when `d = 1`), booleans, text; anything
/// else in the scratch interchange.
#[must_use]
pub fn display_value(value: &CValue) -> String {
    match value {
        CValue::Int(n) => n.to_string(),
        CValue::Rat { num, den } => {
            if den.to_string() == "1" {
                num.to_string()
            } else {
                format!("{num}/{den}")
            }
        }
        CValue::Bool(b) => b.to_string(),
        CValue::Str(text) => text.to_string(),
        other => render_scratch_value(other).unwrap_or_else(|_| "?".into()),
    }
}

fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

fn join_ints(values: &[i128]) -> String {
    values.iter().map(ToString::to_string).collect::<Vec<_>>().join(";")
}

/// Export a run's `batches` or `records` table as `csv` or `jsonl`.
pub fn export_run(run: &RunDir, table: &str, format: &str) -> Result<String, HostFault> {
    let events = read_events(&run.journal())?;
    let (columns, rows): (Vec<String>, Vec<Vec<String>>) = match table {
        "batches" => {
            let columns = [
                "batch", "verdict", "verdict_name", "used", "delta_used", "watermark", "plateau_streak",
                "incumbent_key", "incumbent_score", "promoted", "quarantined", "case_count", "archive_len",
            ];
            let rows = events
                .iter()
                .filter(|e| e.t == "batch")
                .map(|e| {
                    let incumbent = e.record("incumbent").unwrap_or_default();
                    let field = |name: &str| incumbent.get(name).map(display_value).unwrap_or_default();
                    vec![
                        e.uint("batch").unwrap_or(0).to_string(),
                        e.int("verdict").unwrap_or(0).to_string(),
                        e.str("verdict_name").unwrap_or("").to_string(),
                        e.int("used").unwrap_or(0).to_string(),
                        e.int("delta_used").unwrap_or(0).to_string(),
                        e.int("watermark").unwrap_or(0).to_string(),
                        e.int("plateau_streak").unwrap_or(0).to_string(),
                        field("key"),
                        field("score"),
                        join_ints(&e.ints("promoted")),
                        join_ints(&e.ints("quarantined")),
                        e.ints("case_ids").len().to_string(),
                        e.int("archive_len").unwrap_or(0).to_string(),
                    ]
                })
                .collect();
            (columns.iter().map(|c| (*c).to_string()).collect(), rows)
        }
        "records" => {
            let mut latest: BTreeMap<String, RecordView> = BTreeMap::new();
            let mut order: Vec<String> = Vec::new();
            let mut take = |record: RecordView| {
                let key = record.get("key").map(display_value).unwrap_or_default();
                if !latest.contains_key(&key) {
                    order.push(key.clone());
                }
                latest.insert(key, record);
            };
            for event in &events {
                match event.t.as_str() {
                    "seed" => event.records("archive").into_iter().for_each(&mut take),
                    "records" => {
                        event.records("added").into_iter().for_each(&mut take);
                        event.records("changed").into_iter().for_each(&mut take);
                    }
                    _ => {}
                }
            }
            let names: BTreeSet<String> = latest.values().flat_map(|r| r.keys().cloned()).collect();
            let mut columns: Vec<String> = vec!["key".into()];
            columns.extend(names.into_iter().filter(|name| name != "key"));
            let rows = order
                .iter()
                .map(|key| {
                    let record = &latest[key];
                    columns.iter().map(|c| record.get(c).map(display_value).unwrap_or_default()).collect()
                })
                .collect();
            (columns, rows)
        }
        other => {
            return Err(HostFault::fault("dream_export", format!("unknown table `{other}` (batches or records)")));
        }
    };
    let mut out = String::new();
    match format {
        "csv" => {
            out.push_str(&columns.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
            out.push('\n');
            for row in &rows {
                out.push_str(&row.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
                out.push('\n');
            }
        }
        "jsonl" => {
            for row in &rows {
                let fields: Vec<String> = columns
                    .iter()
                    .zip(row)
                    .map(|(c, v)| format!("{}:{}", emath_core::json_quote(c), emath_core::json_quote(v)))
                    .collect();
                out.push('{');
                out.push_str(&fields.join(","));
                out.push_str("}\n");
            }
        }
        other => {
            return Err(HostFault::fault("dream_export", format!("unknown format `{other}` (csv or jsonl)")));
        }
    }
    Ok(out)
}

/// One `emath dream list` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSummary {
    pub name: String,
    pub target: String,
    /// The outcome, or `running` / `stalled`.
    pub status: String,
    pub last_batch: u64,
    pub incumbent_score: Option<String>,
}

/// Summarise every run under `root`, sorted by name.
#[must_use]
pub fn list_runs(root: &Path) -> Vec<RunSummary> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let run = RunDir::new(&entry.path());
        let Ok(info) = run.info() else {
            continue;
        };
        let events = read_events(&run.journal()).unwrap_or_default();
        let last = events.iter().rev().find(|e| e.t == "batch");
        let end = events.iter().rev().find(|e| e.t == "end");
        let status = match (end, &info.ended) {
            (Some(end), _) => end.str("outcome").unwrap_or("ended").to_string(),
            (None, Some(ended)) => ended.outcome.clone(),
            (None, None) => match run.lock_pid() {
                Some(pid) if pid_alive(pid) => "running".into(),
                _ => "stalled".into(),
            },
        };
        let incumbent_score = last
            .or_else(|| events.iter().find(|e| e.t == "seed"))
            .and_then(|e| e.record("incumbent"))
            .and_then(|r| r.get("score").map(display_value));
        out.push(RunSummary {
            name: run.name(),
            target: info.target,
            status,
            last_batch: last.and_then(JournalEvent::batch).unwrap_or(0),
            incumbent_score,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
