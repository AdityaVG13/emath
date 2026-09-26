//! The run inbox: evidence from outside the run, read at batch
//! boundaries only.
//!
//! A file `<run>/inbox/<name>.jsonl` holds one observation per line:
//! `{"subject": <int>, "case": <int>, "value": <number | "n/d">}`.
//! Values are exact: a decimal number is read as its exact rational,
//! never through a float. Writers create the file under another name
//! and rename it into place; a file whose text does not end in a
//! newline is still being written and is skipped until it does.
//!
//! Files are taken in name order, once each. A taken file becomes a
//! journal `evidence` event carrying every observation it held (or an
//! `evidence_rejected` event naming why it was refused; a rejected
//! file is never retried). The journal, not the inbox, is the
//! authority: resume and fork rebuild the evidence from kept events,
//! and the driver never deletes or edits an inbox file.
//!
//! An observation becomes an `Observation` record (`subject`,
//! `case_id`, `value`, `source`), `source` being the 1-based ordinal
//! of the evidence event that brought it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use emath_artifact::{parse_json_document, JsonValue};
use emath_exec_ir::constructor_layer::{parse_scratch_value, render_scratch_value, CValue};
use emath_exec_ir::exact_int::ExactInt;

use super::journal::{io_fault, JournalEvent};
use crate::host::HostFault;

/// The record type the evidence input carries.
pub const OBSERVATION_TYPE: &str = "Observation";

/// One inbox file ready to be taken.
pub struct InboxFile {
    pub name: String,
    pub text: String,
}

/// The inbox files not yet taken, in name order.
pub fn pending(inbox: &Path, taken: &BTreeSet<String>) -> Result<Vec<InboxFile>, HostFault> {
    let entries = match fs::read_dir(inbox) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(io_fault("dream_inbox_io", inbox, &err)),
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".jsonl") && !name.starts_with('.') && !taken.contains(name))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        let path = inbox.join(&name);
        let text = fs::read_to_string(&path).map_err(|err| io_fault("dream_inbox_io", &path, &err))?;
        if !text.is_empty() && !text.ends_with('\n') {
            continue;
        }
        out.push(InboxFile { name, text });
    }
    Ok(out)
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.abs()
}

fn reduce(num: i128, den: i128) -> Result<(i128, i128), String> {
    if den == 0 {
        return Err("a zero denominator".into());
    }
    let sign = if den < 0 { -1 } else { 1 };
    let g = gcd(num, den).max(1);
    let num = (num / g).checked_mul(sign).ok_or("a numerator beyond machine scale")?;
    Ok((num, (den / g).abs()))
}

/// An exact decimal (`-12`, `2.8316`); exponents are refused rather
/// than rounded.
fn parse_decimal(text: &str) -> Result<(i128, i128), String> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(format!("`{text}` is not an exact decimal"));
    }
    let scale = u32::try_from(frac.len()).ok().and_then(|k| 10i128.checked_pow(k));
    let num = format!("{whole}{frac}").parse::<i128>().ok();
    let (Some(num), Some(den)) = (num, scale) else {
        return Err(format!("`{text}` is beyond machine scale"));
    };
    reduce(if negative { -num } else { num }, den)
}

fn parse_value(value: &JsonValue) -> Result<(i128, i128), String> {
    match value {
        JsonValue::Num(text) => parse_decimal(text),
        JsonValue::Str(text) => {
            let (num, den) = text.split_once('/').ok_or_else(|| format!("`{text}` is not n/d"))?;
            let num = num.trim().parse::<i128>().map_err(|_| format!("`{text}` is not n/d"))?;
            let den = den.trim().parse::<i128>().map_err(|_| format!("`{text}` is not n/d"))?;
            reduce(num, den)
        }
        other => Err(format!("value {other:?} is neither a number nor \"n/d\"")),
    }
}

fn observation(subject: i128, case_id: i128, (num, den): (i128, i128), source: i128) -> CValue {
    let int = |n: i128| CValue::Int(ExactInt::from(n));
    CValue::Record {
        type_name: OBSERVATION_TYPE.into(),
        fields: BTreeMap::from([
            ("subject".to_string(), int(subject)),
            ("case_id".to_string(), int(case_id)),
            (
                "value".to_string(),
                CValue::Rat {
                    num: ExactInt::from(num),
                    den: ExactInt::from(den),
                },
            ),
            ("source".to_string(), int(source)),
        ])
        .into(),
    }
}

/// Read one file into `Observation` records stamped with `source`, or
/// the reason it is refused (the whole file is refused on any bad line).
pub fn parse_file(text: &str, source: i128) -> Result<Vec<CValue>, String> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let bad = |why: String| format!("line {line_no}: {why}");
        let Ok(JsonValue::Obj(entries)) = parse_json_document(line) else {
            return Err(bad("not a JSON object".into()));
        };
        let mut subject = None;
        let mut case_id = None;
        let mut value = None;
        for (key, item) in &entries {
            let int = || match item {
                JsonValue::Num(text) => text.parse::<i128>().map_err(|_| bad(format!("`{key}` is not an integer"))),
                _ => Err(bad(format!("`{key}` is not an integer"))),
            };
            match key.as_str() {
                "subject" if subject.is_none() => subject = Some(int()?),
                "case" if case_id.is_none() => case_id = Some(int()?),
                "value" if value.is_none() => value = Some(parse_value(item).map_err(bad)?),
                other => return Err(bad(format!("unexpected or repeated key `{other}`"))),
            }
        }
        match (subject, case_id, value) {
            (Some(subject), Some(case_id), Some(value)) => out.push(observation(subject, case_id, value, source)),
            _ => return Err(bad("needs `subject`, `case`, and `value`".into())),
        }
    }
    if out.is_empty() {
        return Err("the file holds no observations".into());
    }
    Ok(out)
}

/// Render observations for the journal.
pub fn render(observations: &[CValue]) -> Result<Vec<String>, HostFault> {
    observations
        .iter()
        .map(|o| render_scratch_value(o).map_err(HostFault::from))
        .collect()
}

/// The evidence and the taken file names a kept journal holds.
pub fn replay(kept: &[&JournalEvent]) -> Result<(Vec<CValue>, BTreeSet<String>), HostFault> {
    let mut evidence = Vec::new();
    let mut taken = BTreeSet::new();
    for event in kept {
        if event.t != "evidence" && event.t != "evidence_rejected" {
            continue;
        }
        if let Some(file) = event.str("file") {
            taken.insert(file.to_string());
        }
        if event.t == "evidence" {
            let Some(JsonValue::Arr(items)) = event.get("observations") else {
                return Err(HostFault::fault("dream_journal_parse", "an evidence event has no observations"));
            };
            for item in items {
                evidence.push(parse_scratch_value(item).map_err(HostFault::from)?);
            }
        }
    }
    Ok((evidence, taken))
}
