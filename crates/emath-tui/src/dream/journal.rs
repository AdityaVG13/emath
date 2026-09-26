//! The dream journal (`emath.dream.journal.v1`): the canonical,
//! append-only record of one `emath dream` run.
//!
//! One JSON object per line; the first two keys are always `seq`
//! (1, 2, 3, ... contiguous) and `t`. The journal holds no timestamps,
//! no paths (except the run-relative `level.file`), no pids, and no
//! measurement: identical module, config, and applied steering produce
//! byte-identical journals (measurement lives in `metrics.jsonl`).
//! Values render through the scratch value interchange, so a record in
//! the journal and in a checkpoint are the same bytes.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use emath_artifact::{parse_json_document, JsonValue};
use emath_core::json_quote;
use emath_exec_ir::constructor_layer::{parse_scratch_value, render_scratch_value, CValue};

use crate::host::{value_int, HostFault};

/// The schema every `start` event declares and every reader demands.
pub const JOURNAL_SCHEMA: &str = "emath.dream.journal.v1";

/// One JSON object under construction: keys in insertion order, values
/// pre-rendered JSON. Compact rendering (`{"k":v,...}`).
#[derive(Clone, Debug, Default)]
pub struct Obj {
    fields: Vec<(&'static str, String)>,
}

impl Obj {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A pre-rendered JSON value.
    #[must_use]
    pub fn raw(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.fields.push((key, value.into()));
        self
    }

    /// An exact integer.
    #[must_use]
    pub fn int(self, key: &'static str, value: impl Display) -> Self {
        self.raw(key, value.to_string())
    }

    #[must_use]
    pub fn str(self, key: &'static str, value: &str) -> Self {
        self.raw(key, json_quote(value))
    }

    #[must_use]
    pub fn bool(self, key: &'static str, value: bool) -> Self {
        self.raw(key, if value { "true" } else { "false" })
    }

    #[must_use]
    pub fn null(self, key: &'static str) -> Self {
        self.raw(key, "null")
    }

    #[must_use]
    pub fn opt_int(self, key: &'static str, value: Option<impl Display>) -> Self {
        match value {
            Some(value) => self.int(key, value),
            None => self.null(key),
        }
    }

    #[must_use]
    pub fn opt_str(self, key: &'static str, value: Option<&str>) -> Self {
        match value {
            Some(value) => self.str(key, value),
            None => self.null(key),
        }
    }

    /// An array of exact integers.
    #[must_use]
    pub fn ints(self, key: &'static str, values: &[i128]) -> Self {
        let inner = values.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
        self.raw(key, format!("[{inner}]"))
    }

    /// An array of pre-rendered values.
    #[must_use]
    pub fn array(self, key: &'static str, values: &[String]) -> Self {
        self.raw(key, format!("[{}]", values.join(",")))
    }

    /// Render the object on one line.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::from("{");
        for (index, (key, value)) in self.fields.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&json_quote(key));
            out.push(':');
            out.push_str(value);
        }
        out.push('}');
        out
    }
}

fn record_fields(record: &CValue) -> Result<&BTreeMap<String, CValue>, HostFault> {
    match record {
        CValue::Record { fields, .. } => Ok(fields),
        other => Err(HostFault::fault(
            "dream_record_contract",
            format!("an archive element must be a record, found {other:?}"),
        )),
    }
}

/// The record projection `R`: every field of the archive element, in
/// field order, rendered through the scratch interchange. Only `key`
/// is required.
pub fn project_record(record: &CValue) -> Result<String, HostFault> {
    let fields = record_fields(record)?;
    if !fields.contains_key("key") {
        return Err(HostFault::fault(
            "dream_record_contract",
            "an archive record carries no `key` field: the dream journal identifies records by key",
        ));
    }
    let mut out = String::from("{");
    for (index, (name, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&json_quote(name));
        out.push(':');
        out.push_str(&render_scratch_value(value).map_err(HostFault::from)?);
    }
    out.push('}');
    Ok(out)
}

/// The key of an archive record.
pub fn record_key(record: &CValue) -> Result<i128, HostFault> {
    value_int(record, "key").map_err(|fault| HostFault::fault("dream_record_contract", fault.message))
}

/// An archive diff by key: `added` holds new keys, `changed` holds keys
/// whose projection differs; both in archive order, projections rendered.
#[derive(Clone, Debug, Default)]
pub struct ArchiveDiff {
    pub added: Vec<String>,
    pub changed: Vec<String>,
}

impl ArchiveDiff {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty()
    }
}

/// Diff two archives by key.
pub fn diff_archive(previous: &[CValue], next: &[CValue]) -> Result<ArchiveDiff, HostFault> {
    let mut before: BTreeMap<i128, String> = BTreeMap::new();
    for record in previous {
        before.insert(record_key(record)?, project_record(record)?);
    }
    let mut diff = ArchiveDiff::default();
    for record in next {
        let key = record_key(record)?;
        let projected = project_record(record)?;
        match before.get(&key) {
            None => diff.added.push(projected),
            Some(old) if *old != projected => diff.changed.push(projected),
            Some(_) => {}
        }
    }
    Ok(diff)
}

/// The journal writer: appends complete lines, one `write` per line.
pub struct JournalWriter {
    file: File,
    seq: u64,
}

impl JournalWriter {
    /// Open `path` for appending; the next event gets `last_seq + 1`.
    pub fn append(path: &Path, last_seq: u64) -> Result<Self, HostFault> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| io_fault("dream_journal_io", path, &err))?;
        Ok(Self { file, seq: last_seq })
    }

    /// The seq of the last written event.
    #[must_use]
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Append one event: `{"seq":n,"t":<t>,<body...>}`.
    pub fn emit(&mut self, t: &str, body: Obj) -> Result<u64, HostFault> {
        let seq = self.seq + 1;
        let mut line = Obj::new().int("seq", seq).str("t", t);
        line.fields.extend(body.fields);
        let mut text = line.render();
        text.push('\n');
        self.file
            .write_all(text.as_bytes())
            .and_then(|()| self.file.flush())
            .map_err(|err| HostFault::fault("dream_journal_io", format!("cannot append to the journal: {err}")))?;
        self.seq = seq;
        Ok(seq)
    }
}

pub(crate) fn io_fault(code: &str, path: &Path, err: &std::io::Error) -> HostFault {
    HostFault::fault(code, format!("{}: {err}", path.display()))
}

/// A decoded record projection: field name to value.
pub type RecordView = BTreeMap<String, CValue>;

/// Decode one `R` object.
#[must_use]
pub fn decode_record(json: &JsonValue) -> Option<RecordView> {
    let JsonValue::Obj(entries) = json else {
        return None;
    };
    let mut out = BTreeMap::new();
    for (name, value) in entries {
        out.insert(name.clone(), parse_scratch_value(value).ok()?);
    }
    Some(out)
}

/// One parsed journal line.
#[derive(Clone, Debug)]
pub struct JournalEvent {
    pub seq: u64,
    pub t: String,
    pub fields: Vec<(String, JsonValue)>,
    /// The line exactly as written (without the newline).
    pub line: String,
}

fn json_int(value: &JsonValue) -> Option<i128> {
    match value {
        JsonValue::Num(text) => text.parse().ok(),
        _ => None,
    }
}

impl JournalEvent {
    /// Parse one complete line.
    pub fn parse(line: &str) -> Result<Self, HostFault> {
        let json = parse_json_document(line).map_err(|err| {
            HostFault::fault("dream_journal_parse", format!("malformed journal line: {err:?}"))
        })?;
        let JsonValue::Obj(fields) = json else {
            return Err(HostFault::fault("dream_journal_parse", "a journal line is not an object"));
        };
        let seq = fields
            .iter()
            .find(|(key, _)| key == "seq")
            .and_then(|(_, value)| json_int(value))
            .and_then(|seq| u64::try_from(seq).ok())
            .ok_or_else(|| HostFault::fault("dream_journal_parse", "a journal line has no `seq`"))?;
        let t = match fields.iter().find(|(key, _)| key == "t") {
            Some((_, JsonValue::Str(t))) => t.clone(),
            _ => return Err(HostFault::fault("dream_journal_parse", "a journal line has no `t`")),
        };
        Ok(Self {
            seq,
            t,
            fields,
            line: line.to_string(),
        })
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        self.fields.iter().find(|(name, _)| name == key).map(|(_, value)| value)
    }

    #[must_use]
    pub fn int(&self, key: &str) -> Option<i128> {
        self.get(key).and_then(json_int)
    }

    #[must_use]
    pub fn uint(&self, key: &str) -> Option<u64> {
        self.int(key).and_then(|value| u64::try_from(value).ok())
    }

    #[must_use]
    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(JsonValue::Str(text)) => Some(text),
            _ => None,
        }
    }

    #[must_use]
    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.get(key) {
            Some(JsonValue::Bool(value)) => Some(*value),
            _ => None,
        }
    }

    /// An array of exact integers (empty when absent).
    #[must_use]
    pub fn ints(&self, key: &str) -> Vec<i128> {
        match self.get(key) {
            Some(JsonValue::Arr(items)) => items.iter().filter_map(json_int).collect(),
            _ => Vec::new(),
        }
    }

    /// The batch this event belongs to, when it names one.
    #[must_use]
    pub fn batch(&self) -> Option<u64> {
        self.uint("batch")
    }

    #[must_use]
    pub fn record(&self, key: &str) -> Option<RecordView> {
        self.get(key).and_then(decode_record)
    }

    #[must_use]
    pub fn records(&self, key: &str) -> Vec<RecordView> {
        match self.get(key) {
            Some(JsonValue::Arr(items)) => items.iter().filter_map(decode_record).collect(),
            _ => Vec::new(),
        }
    }

    /// A nested object's integer field (`config.budget0`).
    #[must_use]
    pub fn nested_int(&self, object: &str, key: &str) -> Option<i128> {
        match self.get(object) {
            Some(JsonValue::Obj(entries)) => entries
                .iter()
                .find(|(name, _)| name == key)
                .and_then(|(_, value)| json_int(value)),
            _ => None,
        }
    }
}

/// Split journal text into complete lines (a trailing partial line,
/// still being written, is not a line yet).
#[must_use]
pub fn complete_lines(text: &str) -> Vec<&str> {
    let end = text.rfind('\n').map_or(0, |index| index + 1);
    text[..end].lines().filter(|line| !line.is_empty()).collect()
}

/// Read every complete event of a journal file (missing file = none).
pub fn read_events(path: &Path) -> Result<Vec<JournalEvent>, HostFault> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(io_fault("dream_journal_io", path, &err)),
    };
    let events = complete_lines(&text)
        .into_iter()
        .map(JournalEvent::parse)
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(first) = events.first() {
        if first.t != "start" || first.str("schema") != Some(JOURNAL_SCHEMA) {
            return Err(HostFault::fault(
                "dream_journal_schema",
                format!(
                    "{} is not an {JOURNAL_SCHEMA} journal (first event `{}`)",
                    path.display(),
                    first.t
                ),
            ));
        }
    }
    Ok(events)
}
