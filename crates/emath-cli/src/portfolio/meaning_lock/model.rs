//! Meaning-lock data model: keys, entries, errors.

use super::*;

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

    pub(super) fn parse(value: &str) -> Option<Self> {
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
