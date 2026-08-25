//! Project-local meaning lock: persist a chosen world fingerprint so
//! later runs are single-world and user-locked.
//!
//! Locks are local-side (per-user, per-project). They are not baked into
//! shared source. The locked identity is the same world fingerprint used
//! by G7 [`crate::WorldCandidate::world_fingerprint`] (`WorldIr::identity`).

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use emath_world_ir::fnv1a64;

use crate::interpretation::{
    evaluate, InterpretationPolicy, LedgerEntry, MetricAxis, PortfolioError, PortfolioReceipt,
};
use crate::record::WorldCandidate;

/// Durable schema id (registry name `emath.meaning-lock`).
pub const LOCK_SCHEMA: &str = "emath.meaning-lock";
/// Layout version. Unknown versions refuse (`E-LOCK-002`).
pub const LOCK_SCHEMA_VERSION: u32 = 1;
/// Directory under the project root that holds the lock file.
pub const LOCK_DIR: &str = ".emath";
/// File name of the project-local lock.
pub const LOCK_FILE_NAME: &str = "meaning.lock";
/// Default candidate cap (the former pin-of-5). Receipted, not hidden.
pub const DEFAULT_PORTFOLIO_CAP: u32 = 5;
/// Whole-term hole id when the lock is not per-symbol.
pub const WHOLE_TERM_HOLE: &str = "*";
/// Provenance token recorded on locked receipts. Never raises authority.
pub const PROVENANCE_USER_LOCKED: &str = "user-locked";

/// How the user chose the locked world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionMethod {
    /// `emath meaning set`.
    CliSet,
    /// Agent-requested update.
    Agent,
    /// Direct edit of the lock file (re-serialized on next write).
    FileEdit,
}

impl SelectionMethod {
    /// Wire name.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::CliSet => "cli-set",
            Self::Agent => "agent",
            Self::FileEdit => "file-edit",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "cli-set" => Some(Self::CliSet),
            "agent" => Some(Self::Agent),
            "file-edit" => Some(Self::FileEdit),
            _ => None,
        }
    }
}

/// Key: declaration/term semantic identity plus hole/symbol id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LockKey {
    /// Term content identity (`Analysis::term_id` / FNV of the term).
    pub declaration_id: u64,
    /// Hole or symbol id; [`WHOLE_TERM_HOLE`] for a whole-term lock.
    pub hole_id: String,
}

/// One locked interpretation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockEntry {
    /// Source path as given at `set` time (drift witness, in identity).
    pub source: String,
    /// Source-byte identity at `set` time (drift witness, in identity).
    pub source_hash: u64,
    /// Locked world fingerprint (`WorldIr::identity` / `world_fingerprint`).
    pub world_fingerprint: u64,
    /// Portfolio receipt the user picked from.
    pub portfolio_receipt_id: u64,
    /// Selection method.
    pub selection_method: SelectionMethod,
    /// Unix seconds when the entry was written. Excluded from identity.
    pub selected_at: u64,
}

/// Project-local meaning lock document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeaningLock {
    /// Configurable portfolio cap; default [`DEFAULT_PORTFOLIO_CAP`].
    pub portfolio_cap: u32,
    /// Entries keyed by (declaration, hole), BTree-ordered.
    pub entries: BTreeMap<LockKey, LockEntry>,
    /// FNV-1a64 of the identity body (excludes `selected_at` and this field).
    pub lock_id: u64,
}

/// Typed meaning-lock refusals. Never silent, never best-effort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    /// Malformed lock file or unreadable document (`E-LOCK-001`).
    Malformed {
        /// Parse/IO detail.
        detail: String,
    },
    /// Unknown `schema_version` (`E-LOCK-002`).
    UnknownVersion {
        /// Version found in the file.
        version: u32,
    },
    /// Stored `lock_id` does not match the identity body (`E-LOCK-003`).
    Tampered {
        /// Fingerprint that failed integrity, when known.
        fingerprint: Option<u64>,
    },
    /// Locked world is missing, drifted, or inadmissible (`E-LOCK-004`).
    Drifted {
        /// Locked fingerprint.
        fingerprint: u64,
        /// Human-readable reason.
        detail: String,
    },
    /// `set` targeted a disqualified world (`E-LOCK-005`).
    Disqualified {
        /// Requested fingerprint.
        fingerprint: u64,
        /// Ledger row from the portfolio receipt.
        ledger: LedgerEntry,
    },
    /// `set` named a fingerprint that is not in the portfolio (`E-LOCK-006`).
    UnknownCandidate {
        /// Requested fingerprint.
        fingerprint: u64,
    },
}

impl LockError {
    /// Stable E-LOCK token.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Malformed { .. } => "E-LOCK-001",
            Self::UnknownVersion { .. } => "E-LOCK-002",
            Self::Tampered { .. } => "E-LOCK-003",
            Self::Drifted { .. } => "E-LOCK-004",
            Self::Disqualified { .. } => "E-LOCK-005",
            Self::UnknownCandidate { .. } => "E-LOCK-006",
        }
    }
}

impl fmt::Display for LockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { detail } => {
                write!(
                    formatter,
                    "error: {}: malformed meaning lock: {detail}",
                    self.code()
                )
            }
            Self::UnknownVersion { version } => write!(
                formatter,
                "error: {}: unknown meaning-lock schema_version {version}; re-open the portfolio with `emath meaning unset`",
                self.code()
            ),
            Self::Tampered { fingerprint } => match fingerprint {
                Some(fingerprint) => write!(
                    formatter,
                    "error: {}: tampered world fingerprint {fingerprint:016x}; re-open the portfolio with `emath meaning unset`",
                    self.code()
                ),
                None => write!(
                    formatter,
                    "error: {}: meaning lock identity does not match contents; re-open the portfolio with `emath meaning unset`",
                    self.code()
                ),
            },
            Self::Drifted {
                fingerprint,
                detail,
            } => write!(
                formatter,
                "error: {}: locked world {fingerprint:016x} is inadmissible ({detail}); re-open the portfolio with `emath meaning unset`",
                self.code()
            ),
            Self::Disqualified {
                fingerprint,
                ledger,
            } => write!(
                formatter,
                "error: {}: refusing lock on disqualified world {fingerprint:016x} (ledger {})",
                self.code(),
                ledger.reason.canonical()
            ),
            Self::UnknownCandidate { fingerprint } => write!(
                formatter,
                "error: {}: world fingerprint {fingerprint:016x} is not in the current portfolio",
                self.code()
            ),
        }
    }
}

impl std::error::Error for LockError {}

impl From<io::Error> for LockError {
    fn from(error: io::Error) -> Self {
        Self::Malformed {
            detail: error.to_string(),
        }
    }
}

impl MeaningLock {
    /// Empty lock with the default cap.
    #[must_use]
    pub fn empty() -> Self {
        Self::with_cap(DEFAULT_PORTFOLIO_CAP)
    }

    /// Empty lock with an explicit cap (receipted configuration).
    #[must_use]
    pub fn with_cap(portfolio_cap: u32) -> Self {
        let mut lock = Self {
            portfolio_cap,
            entries: BTreeMap::new(),
            lock_id: 0,
        };
        lock.lock_id = lock.compute_lock_id();
        lock
    }

    /// Canonical lock path `<root>/.emath/meaning.lock`.
    #[must_use]
    pub fn path(project_root: &Path) -> PathBuf {
        project_root.join(LOCK_DIR).join(LOCK_FILE_NAME)
    }

    /// Walks from `start` to a project root: a directory containing
    /// `.emath/` or `emath-package.toml` wins, else the source-file parent
    /// (so a lock can live next to a lone genesis file).
    #[must_use]
    pub fn discover_project_root(start: &Path) -> PathBuf {
        let mut current = if start.is_file() {
            start
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| start.to_path_buf())
        } else if start.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            start.to_path_buf()
        };
        let fallback = current.clone();
        loop {
            if current.join(LOCK_DIR).is_dir() || current.join("emath-package.toml").is_file() {
                return current;
            }
            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => return fallback,
            }
        }
    }

    /// Loads a lock if the file exists. Missing is `Ok(None)`.
    /// Malformed / unknown version / tamper refuse.
    pub fn load(project_root: &Path) -> Result<Option<Self>, LockError> {
        let path = Self::path(project_root);
        match fs::read_to_string(&path) {
            Ok(body) => Ok(Some(Self::parse(&body)?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(LockError::Malformed {
                detail: format!("cannot read {}: {error}", path.display()),
            }),
        }
    }

    /// Writes the canonical document. Creates `.emath/` as needed.
    pub fn save(&self, project_root: &Path) -> Result<(), LockError> {
        let dir = project_root.join(LOCK_DIR);
        fs::create_dir_all(&dir).map_err(|error| LockError::Malformed {
            detail: format!("cannot create {}: {error}", dir.display()),
        })?;
        let path = Self::path(project_root);
        fs::write(&path, self.encode()).map_err(|error| LockError::Malformed {
            detail: format!("cannot write {}: {error}", path.display()),
        })
    }

    /// Removes the lock file. Missing is success.
    pub fn remove_file(project_root: &Path) -> Result<(), LockError> {
        let path = Self::path(project_root);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(LockError::Malformed {
                detail: format!("cannot remove {}: {error}", path.display()),
            }),
        }
    }

    /// Strict parse. Unknown fields, truncated JSON, and extra data refuse.
    pub fn parse(text: &str) -> Result<Self, LockError> {
        let value = parse_json(text).map_err(|detail| LockError::Malformed { detail })?;
        let object = value.as_object().ok_or_else(|| LockError::Malformed {
            detail: "root must be an object".to_string(),
        })?;
        refuse_unknown_keys(
            object,
            &[
                "schema",
                "schema_version",
                "portfolio_cap",
                "lock_id",
                "entries",
            ],
        )?;
        let schema = required_str(object, "schema")?;
        if schema != LOCK_SCHEMA {
            return Err(LockError::Malformed {
                detail: format!("schema must be {LOCK_SCHEMA}"),
            });
        }
        let version = required_u32(object, "schema_version")?;
        if version != LOCK_SCHEMA_VERSION {
            return Err(LockError::UnknownVersion { version });
        }
        let portfolio_cap = required_u32(object, "portfolio_cap")?;
        if portfolio_cap == 0 {
            return Err(LockError::Malformed {
                detail: "portfolio_cap must be >= 1".to_string(),
            });
        }
        let stored_id = parse_hex(required_str(object, "lock_id")?)?;
        let entries_value = object.get("entries").ok_or_else(|| LockError::Malformed {
            detail: "missing field entries".to_string(),
        })?;
        let raw_entries = entries_value
            .as_array()
            .ok_or_else(|| LockError::Malformed {
                detail: "entries must be an array".to_string(),
            })?;
        let mut entries = BTreeMap::new();
        for item in raw_entries {
            let row = item.as_object().ok_or_else(|| LockError::Malformed {
                detail: "lock entry must be an object".to_string(),
            })?;
            refuse_unknown_keys(
                row,
                &[
                    "declaration_id",
                    "hole_id",
                    "source",
                    "source_hash",
                    "world_fingerprint",
                    "portfolio_receipt_id",
                    "selection_method",
                    "selected_at",
                ],
            )?;
            let declaration_id = parse_hex(required_str(row, "declaration_id")?)?;
            let hole_id = required_str(row, "hole_id")?.to_string();
            if hole_id.is_empty() {
                return Err(LockError::Malformed {
                    detail: "hole_id must be non-empty".to_string(),
                });
            }
            let method = required_str(row, "selection_method")?;
            let selection_method =
                SelectionMethod::parse(method).ok_or_else(|| LockError::Malformed {
                    detail: format!("unknown selection_method `{method}`"),
                })?;
            let key = LockKey {
                declaration_id,
                hole_id,
            };
            let entry = LockEntry {
                source: required_str(row, "source")?.to_string(),
                source_hash: parse_hex(required_str(row, "source_hash")?)?,
                world_fingerprint: parse_hex(required_str(row, "world_fingerprint")?)?,
                portfolio_receipt_id: parse_hex(required_str(row, "portfolio_receipt_id")?)?,
                selection_method,
                selected_at: parse_decimal(required_str(row, "selected_at")?)?,
            };
            if entries.insert(key, entry).is_some() {
                return Err(LockError::Malformed {
                    detail: "duplicate lock key".to_string(),
                });
            }
        }
        let lock = Self {
            portfolio_cap,
            entries,
            lock_id: stored_id,
        };
        let computed = lock.compute_lock_id();
        if computed != stored_id {
            return Err(LockError::Tampered { fingerprint: None });
        }
        Ok(lock)
    }

    /// Deterministic JSON (BTreeMap order, two-space indent, trailing newline).
    #[must_use]
    pub fn encode(&self) -> String {
        let mut entries = String::new();
        for (index, (key, entry)) in self.entries.iter().enumerate() {
            if index > 0 {
                entries.push_str(",\n");
            }
            entries.push_str("    {\n");
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("declaration_id"),
                quote(&hex(key.declaration_id))
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("hole_id"),
                quote(&key.hole_id)
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("source"),
                quote(&entry.source)
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("source_hash"),
                quote(&hex(entry.source_hash))
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("world_fingerprint"),
                quote(&hex(entry.world_fingerprint))
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("portfolio_receipt_id"),
                quote(&hex(entry.portfolio_receipt_id))
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("selection_method"),
                quote(entry.selection_method.as_str())
            ));
            entries.push_str(&format!(
                "      {}: {}\n",
                quote("selected_at"),
                quote(&entry.selected_at.to_string())
            ));
            entries.push_str("    }");
        }
        let entries_body = if self.entries.is_empty() {
            "[]".to_string()
        } else {
            format!("[\n{entries}\n  ]")
        };
        format!(
            "{{\n  {}: {},\n  {}: {},\n  {}: {},\n  {}: {},\n  {}: {}\n}}\n",
            quote("schema"),
            quote(LOCK_SCHEMA),
            quote("schema_version"),
            LOCK_SCHEMA_VERSION,
            quote("portfolio_cap"),
            self.portfolio_cap,
            quote("lock_id"),
            quote(&hex(self.lock_id)),
            quote("entries"),
            entries_body
        )
    }

    /// Identity over schema, version, cap, and entries excluding `selected_at`.
    #[must_use]
    pub fn compute_lock_id(&self) -> u64 {
        fnv1a64(self.identity_body().as_bytes())
    }

    fn identity_body(&self) -> String {
        let mut body = format!(
            "{LOCK_SCHEMA}:{LOCK_SCHEMA_VERSION}:cap={}:",
            self.portfolio_cap
        );
        for (key, entry) in &self.entries {
            body.push_str(&format!(
                "{}|{}|{}|{:016x}|{:016x}|{:016x}|{};",
                hex(key.declaration_id),
                key.hole_id,
                entry.source,
                entry.source_hash,
                entry.world_fingerprint,
                entry.portfolio_receipt_id,
                entry.selection_method.as_str()
            ));
        }
        body
    }

    /// Inserts or replaces an entry and refreshes `lock_id`.
    pub fn upsert(&mut self, key: LockKey, entry: LockEntry) {
        self.entries.insert(key, entry);
        self.lock_id = self.compute_lock_id();
    }

    /// Removes one key. Returns whether it was present.
    pub fn unset(&mut self, key: &LockKey) -> bool {
        let removed = self.entries.remove(key).is_some();
        self.lock_id = self.compute_lock_id();
        removed
    }

    /// Resolves a lock for this run. A miss with a same-source witness is drift.
    pub fn resolve(
        &self,
        declaration_id: u64,
        hole_id: &str,
        source: &str,
    ) -> Result<Option<&LockEntry>, LockError> {
        let key = LockKey {
            declaration_id,
            hole_id: hole_id.to_string(),
        };
        if let Some(entry) = self.entries.get(&key) {
            return Ok(Some(entry));
        }
        for (existing, entry) in &self.entries {
            if existing.hole_id == hole_id && entry.source == source {
                return Err(LockError::Drifted {
                    fingerprint: entry.world_fingerprint,
                    detail: format!(
                        "source `{source}` drifted: locked declaration {} current {}",
                        hex(existing.declaration_id),
                        hex(declaration_id)
                    ),
                });
            }
        }
        Ok(None)
    }
}

/// Refuses `set` when `fingerprint` is on the disqualification ledger or
/// absent from the receipt entirely.
pub fn refuse_disqualified(fingerprint: u64, receipt: &PortfolioReceipt) -> Result<(), LockError> {
    if let Some(entry) = receipt
        .ledger
        .iter()
        .find(|entry| entry.fingerprint == fingerprint)
    {
        match &entry.reason {
            crate::DisqualificationReason::Dominated { .. } => {}
            crate::DisqualificationReason::FailedGuard { .. }
            | crate::DisqualificationReason::Refused { .. } => {
                return Err(LockError::Disqualified {
                    fingerprint,
                    ledger: entry.clone(),
                });
            }
        }
    }
    let known = receipt
        .input
        .candidates
        .iter()
        .any(|candidate| candidate.world_fingerprint == fingerprint);
    if !known {
        return Err(LockError::UnknownCandidate { fingerprint });
    }
    Ok(())
}

/// Commits to `locked` before ranking. The receipt policy is user-locked
/// and authority is copied from the candidate (never raised).
pub fn commit_locked_world(
    locked: WorldCandidate,
    axes: Vec<MetricAxis>,
    lock_id: u64,
    origin_receipt_id: u64,
    method: &SelectionMethod,
) -> Result<PortfolioReceipt, LockError> {
    if locked.labeled_authority > locked.evidence_authority {
        return Err(LockError::Drifted {
            fingerprint: locked.world_fingerprint,
            detail: "locked world would escalate authority".to_string(),
        });
    }
    if locked.guard_failure.is_some() {
        return Err(LockError::Drifted {
            fingerprint: locked.world_fingerprint,
            detail: "locked world is no longer admissible".to_string(),
        });
    }
    let policy = InterpretationPolicy::UserLocked {
        lock_id,
        origin_receipt_id,
        method: method.as_str().to_string(),
    };
    match evaluate(vec![locked.clone()], axes, policy) {
        Ok(receipt) => Ok(receipt),
        Err(PortfolioError::NoViableCandidate | PortfolioError::AmbiguousSingleBest { .. }) => {
            Err(LockError::Drifted {
                fingerprint: locked.world_fingerprint,
                detail: "locked world is no longer admissible".to_string(),
            })
        }
        Err(PortfolioError::AuthorityEscalation { .. }) => Err(LockError::Drifted {
            fingerprint: locked.world_fingerprint,
            detail: "locked world would escalate authority".to_string(),
        }),
        Err(PortfolioError::DuplicateFingerprint { fingerprint }) => Err(LockError::Malformed {
            detail: format!("duplicate fingerprint {fingerprint:016x}"),
        }),
    }
}

/// Caps a ranked candidate list. `cap == 0` is a caller error; genesis
/// maps it to `E-GEN-093`.
#[must_use]
pub fn apply_portfolio_cap<T: Clone>(candidates: &[T], cap: u32) -> Vec<T> {
    let limit = usize::try_from(cap).unwrap_or(usize::MAX);
    candidates.iter().take(limit).cloned().collect()
}

fn hex(value: u64) -> String {
    format!("{value:016x}")
}

fn quote(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn parse_hex(text: &str) -> Result<u64, LockError> {
    if text.len() != 16 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(LockError::Malformed {
            detail: format!("expected 16 hex digits, got `{text}`"),
        });
    }
    u64::from_str_radix(text, 16).map_err(|_| LockError::Malformed {
        detail: format!("invalid hex `{text}`"),
    })
}

fn parse_decimal(text: &str) -> Result<u64, LockError> {
    text.parse::<u64>().map_err(|_| LockError::Malformed {
        detail: format!("expected decimal u64, got `{text}`"),
    })
}

fn required_str<'a>(object: &'a BTreeMap<String, Json>, name: &str) -> Result<&'a str, LockError> {
    match object.get(name) {
        Some(Json::Str(value)) => Ok(value),
        Some(_) => Err(LockError::Malformed {
            detail: format!("field {name} must be a string"),
        }),
        None => Err(LockError::Malformed {
            detail: format!("missing field {name}"),
        }),
    }
}

fn required_u32(object: &BTreeMap<String, Json>, name: &str) -> Result<u32, LockError> {
    match object.get(name) {
        Some(Json::Num(text)) => text.parse::<u32>().map_err(|_| LockError::Malformed {
            detail: format!("field {name} must be a u32"),
        }),
        Some(_) => Err(LockError::Malformed {
            detail: format!("field {name} must be a number"),
        }),
        None => Err(LockError::Malformed {
            detail: format!("missing field {name}"),
        }),
    }
}

fn refuse_unknown_keys(object: &BTreeMap<String, Json>, allowed: &[&str]) -> Result<(), LockError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(LockError::Malformed {
                detail: format!("unknown field `{key}`"),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Str(String),
    Num(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    fn as_object(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Self::Obj(object) => Some(object),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[Json]> {
        match self {
            Self::Arr(items) => Some(items),
            _ => None,
        }
    }
}

fn parse_json(text: &str) -> Result<Json, String> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let value = parse_value(bytes, &mut index).ok_or_else(|| "cannot parse JSON".to_string())?;
    skip_ws(bytes, &mut index);
    if index != bytes.len() {
        return Err("trailing content after JSON document".to_string());
    }
    Ok(value)
}

fn skip_ws(bytes: &[u8], index: &mut usize) {
    while bytes
        .get(*index)
        .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
    {
        *index += 1;
    }
}

fn parse_value(bytes: &[u8], index: &mut usize) -> Option<Json> {
    skip_ws(bytes, index);
    match bytes.get(*index)? {
        b'{' => parse_object(bytes, index),
        b'[' => parse_array(bytes, index),
        b'"' => parse_string(bytes, index).map(Json::Str),
        b'-' | b'0'..=b'9' => parse_number(bytes, index).map(Json::Num),
        _ => None,
    }
}

fn parse_object(bytes: &[u8], index: &mut usize) -> Option<Json> {
    *index += 1;
    let mut object = BTreeMap::new();
    loop {
        skip_ws(bytes, index);
        if bytes.get(*index)? == &b'}' {
            *index += 1;
            return Some(Json::Obj(object));
        }
        if !object.is_empty() {
            if bytes.get(*index)? != &b',' {
                return None;
            }
            *index += 1;
            skip_ws(bytes, index);
        }
        let key = parse_string(bytes, index)?;
        skip_ws(bytes, index);
        if bytes.get(*index)? != &b':' {
            return None;
        }
        *index += 1;
        let value = parse_value(bytes, index)?;
        if object.insert(key, value).is_some() {
            return None;
        }
        skip_ws(bytes, index);
        if bytes.get(*index)? == &b'}' {
            *index += 1;
            return Some(Json::Obj(object));
        }
    }
}

fn parse_array(bytes: &[u8], index: &mut usize) -> Option<Json> {
    *index += 1;
    let mut items = Vec::new();
    loop {
        skip_ws(bytes, index);
        if bytes.get(*index)? == &b']' {
            *index += 1;
            return Some(Json::Arr(items));
        }
        if !items.is_empty() {
            if bytes.get(*index)? != &b',' {
                return None;
            }
            *index += 1;
            skip_ws(bytes, index);
        }
        items.push(parse_value(bytes, index)?);
        skip_ws(bytes, index);
        if bytes.get(*index)? == &b']' {
            *index += 1;
            return Some(Json::Arr(items));
        }
    }
}

fn parse_string(bytes: &[u8], index: &mut usize) -> Option<String> {
    if bytes.get(*index)? != &b'"' {
        return None;
    }
    *index += 1;
    let mut out = String::new();
    loop {
        match bytes.get(*index)? {
            b'"' => {
                *index += 1;
                return Some(out);
            }
            b'\\' => {
                match bytes.get(*index + 1)? {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let digits = bytes.get(*index + 2..*index + 6)?;
                        let text = std::str::from_utf8(digits).ok()?;
                        let code = u32::from_str_radix(text, 16).ok()?;
                        out.push(char::from_u32(code)?);
                        *index += 4;
                    }
                    _ => return None,
                }
                *index += 2;
            }
            byte => {
                out.push(char::from(*byte));
                *index += 1;
            }
        }
    }
}

fn parse_number(bytes: &[u8], index: &mut usize) -> Option<String> {
    let start = *index;
    if bytes.get(*index) == Some(&b'-') {
        *index += 1;
    }
    let digits_start = *index;
    while bytes.get(*index).is_some_and(|byte| byte.is_ascii_digit()) {
        *index += 1;
    }
    if *index == digits_start {
        return None;
    }
    std::str::from_utf8(&bytes[start..*index])
        .ok()
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpretation::{DisqualificationReason, MetricPolarity};
    use crate::record::GuardFailure;
    use crate::Authority;
    use std::collections::BTreeMap as Map;

    fn sample_entry() -> (LockKey, LockEntry) {
        (
            LockKey {
                declaration_id: 0x1111_1111_1111_1111,
                hole_id: WHOLE_TERM_HOLE.to_string(),
            },
            LockEntry {
                source: "glyphs.emath".to_string(),
                source_hash: 0x2222_2222_2222_2222,
                world_fingerprint: 0x3333_3333_3333_3333,
                portfolio_receipt_id: 0x4444_4444_4444_4444,
                selection_method: SelectionMethod::CliSet,
                selected_at: 1_700_000_000,
            },
        )
    }

    fn world(fp: u64, authority: Authority) -> WorldCandidate {
        let mut metrics = Map::new();
        metrics.insert("cost".to_string(), 1);
        WorldCandidate::new(fp, "p", authority, metrics, fp)
    }

    fn axes() -> Vec<MetricAxis> {
        vec![MetricAxis::new("cost", MetricPolarity::Minimize)]
    }

    #[test]
    fn round_trip_is_byte_deterministic() {
        let mut lock = MeaningLock::with_cap(5);
        let (key, entry) = sample_entry();
        lock.upsert(key, entry);
        let first = lock.encode();
        let parsed = MeaningLock::parse(&first).expect("parse");
        let second = parsed.encode();
        assert_eq!(first.as_bytes(), second.as_bytes());
        assert_eq!(parsed.lock_id, lock.lock_id);
        assert_eq!(first, lock.encode());
    }

    #[test]
    fn timestamp_is_excluded_from_lock_id() {
        let mut left = MeaningLock::empty();
        let mut right = MeaningLock::empty();
        let (key, mut entry) = sample_entry();
        left.upsert(key.clone(), entry.clone());
        entry.selected_at = 9;
        right.upsert(key, entry);
        assert_eq!(left.lock_id, right.lock_id);
        assert_ne!(left.encode(), right.encode());
    }

    #[test]
    fn unknown_schema_version_refuses() {
        let body = "{\n  \"schema\": \"emath.meaning-lock\",\n  \"schema_version\": 99,\n  \"portfolio_cap\": 5,\n  \"lock_id\": \"0000000000000000\",\n  \"entries\": []\n}\n";
        match MeaningLock::parse(body) {
            Err(LockError::UnknownVersion { version }) => assert_eq!(version, 99),
            other => panic!("expected UnknownVersion, got {other:?}"),
        }
    }

    #[test]
    fn malformed_file_refuses() {
        match MeaningLock::parse("{not-json") {
            Err(LockError::Malformed { .. }) => {}
            other => panic!("expected Malformed, got {other:?}"),
        }
        match MeaningLock::parse(
            "{\n  \"schema\": \"emath.meaning-lock\",\n  \"schema_version\": 1,\n  \"portfolio_cap\": 5,\n  \"lock_id\": \"0000000000000000\",\n  \"entries\": [],\n  \"extra\": 1\n}\n",
        ) {
            Err(LockError::Malformed { detail }) => {
                assert!(detail.contains("unknown field"), "{detail}")
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn tampered_fingerprint_refuses() {
        let mut lock = MeaningLock::empty();
        let (key, entry) = sample_entry();
        lock.upsert(key, entry);
        let mut body = lock.encode();
        body = body.replace("3333333333333333", "aaaaaaaaaaaaaaaa");
        match MeaningLock::parse(&body) {
            Err(LockError::Tampered { .. }) => {}
            other => panic!("expected Tampered, got {other:?}"),
        }
    }

    #[test]
    fn fingerprint_match_and_source_drift() {
        let mut lock = MeaningLock::empty();
        let (key, entry) = sample_entry();
        lock.upsert(key.clone(), entry.clone());
        let hit = lock
            .resolve(key.declaration_id, WHOLE_TERM_HOLE, "glyphs.emath")
            .expect("resolve")
            .expect("entry");
        assert_eq!(hit.world_fingerprint, entry.world_fingerprint);
        match lock.resolve(0x9999, WHOLE_TERM_HOLE, "glyphs.emath") {
            Err(LockError::Drifted { fingerprint, .. }) => {
                assert_eq!(fingerprint, entry.world_fingerprint);
            }
            other => panic!("expected Drifted, got {other:?}"),
        }
        assert!(lock
            .resolve(0x9999, WHOLE_TERM_HOLE, "other.emath")
            .expect("other source is unlocked")
            .is_none());
    }

    #[test]
    fn commit_locked_world_is_single_world_user_locked() {
        let locked = world(7, Authority::Structural);
        let receipt = commit_locked_world(locked, axes(), 0x10, 0x20, &SelectionMethod::CliSet)
            .expect("commit");
        assert_eq!(receipt.selected, vec![7]);
        assert!(receipt.archived.is_empty());
        match &receipt.input.policy {
            InterpretationPolicy::UserLocked {
                lock_id,
                origin_receipt_id,
                method,
            } => {
                assert_eq!(*lock_id, 0x10);
                assert_eq!(*origin_receipt_id, 0x20);
                assert_eq!(method, "cli-set");
            }
            other => panic!("expected UserLocked, got {other:?}"),
        }
        assert!(receipt.encode().contains("user-locked"));
        assert_eq!(
            receipt
                .input
                .candidates
                .iter()
                .map(|candidate| candidate.labeled_authority)
                .max()
                .expect("candidate"),
            Authority::Structural
        );
    }

    #[test]
    fn lock_on_disqualified_world_is_refused_with_ledger() {
        let mut bad = world(9, Authority::Structural);
        bad.guard_failure = Some(GuardFailure {
            code: "hard-constraint:violated".to_string(),
            detail: "carrier empty".to_string(),
        });
        let good = world(8, Authority::Structural);
        let receipt =
            evaluate(vec![good, bad], axes(), InterpretationPolicy::Portfolio).expect("portfolio");
        match refuse_disqualified(9, &receipt) {
            Err(LockError::Disqualified {
                fingerprint,
                ledger,
            }) => {
                assert_eq!(fingerprint, 9);
                assert_eq!(ledger.fingerprint, 9);
                match ledger.reason {
                    DisqualificationReason::FailedGuard { code, .. } => {
                        assert_eq!(code, "hard-constraint:violated");
                    }
                    other => panic!("expected FailedGuard, got {other:?}"),
                }
            }
            other => panic!("expected Disqualified, got {other:?}"),
        }
    }

    #[test]
    fn drifted_locked_candidate_refuses() {
        let mut locked = world(3, Authority::Structural);
        locked.guard_failure = Some(GuardFailure {
            code: "missing-metric".to_string(),
            detail: "cost".to_string(),
        });
        match commit_locked_world(locked, axes(), 1, 2, &SelectionMethod::CliSet) {
            Err(LockError::Drifted { fingerprint, .. }) => assert_eq!(fingerprint, 3),
            other => panic!("expected Drifted, got {other:?}"),
        }
    }
}
