//! The evaluation ledger: `<root>/ledger/<hex>.jsonl`, append-only and
//! shared by every run under one root, including runs driven in
//! parallel.
//!
//! One file per fact identity (`LoopHost::fact_id`,
//! `emath:facts:v1:<hex>`). Line 1 is the header
//! `{"schema":"emath.dream.ledger.v1","facts_id":…}`; every later line
//! is one completed fact-function call
//! `{"key":<memo key>,"value":<scratch value>,"run":…,"batch":…}`.
//!
//! A driver holds one [`Ledger`] and tails it by byte offset: `sync`
//! returns the facts appended by anyone since the last sync, `append`
//! writes this driver's fresh facts minus any a sibling already wrote.
//! Writers hold an exclusive OS file lock and readers a shared one, so
//! a reader never sees half a write from a live writer. Locks die with
//! their process, so a crash never leaves the ledger locked.
//!
//! Only complete lines are facts. A writer that finds a torn tail (a
//! process killed mid-write) trims it before appending: the torn bytes
//! never completed a fact, and a file left with no complete line is
//! restarted with its header. Any other unreadable line, a header for a
//! different identity, or a missing header refuses (`dream_ledger`)
//! rather than trusting a damaged store. Values for one key are equal
//! by purity, so the first line for a key wins.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use emath_artifact::{parse_json_document, JsonValue};
use emath_exec_ir::constructor_layer::{parse_scratch_value, render_scratch_value, CValue};

use super::journal::{io_fault, Obj};
use crate::host::HostFault;

/// The schema a ledger header declares.
pub const LEDGER_SCHEMA: &str = "emath.dream.ledger.v1";

fn fault(message: String) -> HostFault {
    HostFault::fault("dream_ledger", message)
}

/// The ledger file for `facts_id` under a runs root.
#[must_use]
pub fn ledger_path(root: &Path, facts_id: &str) -> PathBuf {
    let hex = facts_id.rsplit(':').next().unwrap_or(facts_id);
    root.join("ledger").join(format!("{hex}.jsonl"))
}

fn field<'a>(entries: &'a [(String, JsonValue)], key: &str) -> Option<&'a JsonValue> {
    entries.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

fn text_field(entries: &[(String, JsonValue)], key: &str) -> Option<String> {
    match field(entries, key) {
        Some(JsonValue::Str(text)) => Some(text.clone()),
        _ => None,
    }
}

/// A driver's handle on one ledger file.
#[derive(Debug)]
pub struct Ledger {
    path: PathBuf,
    facts_id: String,
    /// Bytes already read: always 0 or just past a newline.
    offset: u64,
    keys: BTreeSet<String>,
}

impl Ledger {
    #[must_use]
    pub fn new(path: PathBuf, facts_id: &str) -> Self {
        Self { path, facts_id: facts_id.to_string(), offset: 0, keys: BTreeSet::new() }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn io(&self, err: &std::io::Error) -> HostFault {
        io_fault("dream_ledger", &self.path, err)
    }

    /// Facts appended (by any driver) since the last sync or append.
    pub fn sync(&mut self) -> Result<Vec<(String, CValue)>, HostFault> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(self.io(&err)),
        };
        file.lock_shared().map_err(|err| self.io(&err))?;
        self.read_new(&mut file)
    }

    /// Parse the complete lines past `offset`; advance past them.
    fn read_new(&mut self, file: &mut File) -> Result<Vec<(String, CValue)>, HostFault> {
        let len = file.metadata().map_err(|err| self.io(&err))?.len();
        if len < self.offset {
            return Err(fault(format!("{} shrank below what this driver read", self.path.display())));
        }
        file.seek(SeekFrom::Start(self.offset)).map_err(|err| self.io(&err))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|err| self.io(&err))?;
        let Some(end) = bytes.iter().rposition(|b| *b == b'\n').map(|i| i + 1) else {
            return Ok(Vec::new());
        };
        let text = std::str::from_utf8(&bytes[..end])
            .map_err(|_| fault(format!("{} is not UTF-8", self.path.display())))?;
        let mut facts = Vec::new();
        let mut lines = text.lines().filter(|line| !line.is_empty());
        if self.offset == 0 {
            let header = lines.next().unwrap_or("");
            let entries = match parse_json_document(header) {
                Ok(JsonValue::Obj(entries)) => entries,
                _ => return Err(fault(format!("{}: the header is not a JSON object", self.path.display()))),
            };
            if text_field(&entries, "schema").as_deref() != Some(LEDGER_SCHEMA)
                || text_field(&entries, "facts_id").as_deref() != Some(self.facts_id.as_str())
            {
                return Err(fault(format!(
                    "{}: the header does not declare {LEDGER_SCHEMA} for {}",
                    self.path.display(),
                    self.facts_id
                )));
            }
        }
        for line in lines {
            let bad = || fault(format!("{}: `{line}` is not a ledger fact", self.path.display()));
            let JsonValue::Obj(entries) = parse_json_document(line).map_err(|_| bad())? else {
                return Err(bad());
            };
            let key = text_field(&entries, "key").ok_or_else(bad)?;
            let value = parse_scratch_value(field(&entries, "value").ok_or_else(bad)?).map_err(|_| bad())?;
            if self.keys.insert(key.clone()) {
                facts.push((key, value));
            }
        }
        self.offset += end as u64;
        Ok(facts)
    }

    /// Append the facts `run` computed at `batch` that the ledger does
    /// not already hold. Returns the facts siblings appended since the
    /// last sync (they are read under the same lock, before writing).
    pub fn append(
        &mut self,
        facts: &[(String, CValue)],
        run: &str,
        batch: u64,
    ) -> Result<Vec<(String, CValue)>, HostFault> {
        if facts.is_empty() {
            return self.sync();
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|err| io_fault("dream_ledger", dir, &err))?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)
            .map_err(|err| self.io(&err))?;
        file.lock().map_err(|err| self.io(&err))?;
        // A torn tail is past every complete line, hence past `offset`.
        let len = file.metadata().map_err(|err| self.io(&err))?.len();
        file.seek(SeekFrom::Start(self.offset)).map_err(|err| self.io(&err))?;
        let mut tail = Vec::new();
        file.read_to_end(&mut tail).map_err(|err| self.io(&err))?;
        let complete = tail.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1) as u64;
        if self.offset + complete < len {
            file.set_len(self.offset + complete).map_err(|err| self.io(&err))?;
        }
        let siblings = self.read_new(&mut file)?;
        let mut out = String::new();
        if self.offset == 0 {
            out.push_str(&Obj::new().str("schema", LEDGER_SCHEMA).str("facts_id", &self.facts_id).render());
            out.push('\n');
        }
        let mut written = Vec::new();
        for (key, value) in facts {
            if self.keys.contains(key) {
                continue;
            }
            let rendered = render_scratch_value(value).map_err(|err| fault(err.to_string()))?;
            out.push_str(&Obj::new().str("key", key).raw("value", rendered).str("run", run).int("batch", batch).render());
            out.push('\n');
            written.push(key.clone());
        }
        if written.is_empty() {
            return Ok(siblings);
        }
        file.seek(SeekFrom::End(0)).map_err(|err| self.io(&err))?;
        file.write_all(out.as_bytes()).map_err(|err| self.io(&err))?;
        file.flush().map_err(|err| self.io(&err))?;
        self.offset += out.len() as u64;
        self.keys.extend(written);
        Ok(siblings)
    }
}
