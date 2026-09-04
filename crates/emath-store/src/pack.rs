//! Draft `.emlib` pack reader/writer.
//!
//! The portable-pack draft: magic `EMATHLIB\0`, length-framed segments
//! (the house `object_graph::frame` convention), corruption budgets,
//! and a canonical export — entries are written sorted by id, so the
//! same entry set produces identical bytes regardless of insertion
//! order. Thin packs carry a parent reference (the id of the pack they
//! delta against) and REFUSE to read without that parent closure —
//! never a partial silent read.
//!
//! Draft discipline: no compression/decompression yet (the
//! decompression budget is a declared follow-up), invisible to ordinary
//! `emath run` until share/mount, and the format must not stabilize
//! before the capstones. Entry ids are carried VERBATIM — never
//! re-derived from payload bytes (re-derivation would mint a different
//! identity; the lesson).
//!
//! Determinism class: pure sequence. No wall-clock, no randomness; the
//! bytes are a pure function of (entries, parent reference, budgets).

use std::collections::BTreeMap;

const MAGIC: &[u8] = b"EMATHLIB\0";
const FORMAT_VERSION: u8 = 1;

/// Size/ref-count budgets for a pack. Corruption that would blow a
/// budget refuses (`E-EVID-604`) instead of being read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackBudgets {
    pub max_total_bytes: usize,
    pub max_entries: usize,
    pub max_payload_bytes: usize,
}

impl PackBudgets {
    /// Draft defaults: generous for hand-built packs, small enough that
    /// a corrupt length prefix cannot demand gigabytes.
    #[must_use]
    pub fn draft() -> Self {
        Self {
            max_total_bytes: 64 << 20,
            max_entries: 65_536,
            max_payload_bytes: 16 << 20,
        }
    }
}

/// One pack entry: a verbatim identity string and its payload bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackEntry {
    pub id: String,
    pub payload: Vec<u8>,
}

impl PackEntry {
    #[must_use]
    pub fn new(id: &str, payload: &[u8]) -> Self {
        Self {
            id: id.to_string(),
            payload: payload.to_vec(),
        }
    }
}

/// Pack refusals. Codes are the evidence-family corruption codes:
/// `E-EVID-602` bad magic, `E-EVID-603` truncated, `E-EVID-604`
/// oversized, `E-EVID-605` thin pack without parent closure, `E-EVID-606`
/// duplicate entry ids (canonical export needs a set).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackFault {
    BadMagic { code: String },
    Truncated { code: String },
    Oversized { code: String },
    ThinWithoutParent { code: String },
    DuplicateEntry { code: String, id: String },
}

impl std::fmt::Display for PackFault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMagic { code } => write!(
                formatter,
                "{code}: not an .emlib pack (magic mismatch) — a corrupted or foreign file is \
                 never read as a pack"
            ),
            Self::Truncated { code } => write!(
                formatter,
                "{code}: pack is truncated — a length prefix runs past the buffer"
            ),
            Self::Oversized { code } => write!(
                formatter,
                "{code}: pack exceeds a declared budget (total bytes, entry count, or payload \
                 size) — corruption never masquerades as data"
            ),
            Self::ThinWithoutParent { code } => write!(
                formatter,
                "{code}: thin pack has no parent closure — a thin pack references the pack it \
                 deltas against and cannot be read without it"
            ),
            Self::DuplicateEntry { code, id } => write!(
                formatter,
                "{code}: duplicate entry id `{id}` — canonical export requires an id set"
            ),
        }
    }
}

impl std::error::Error for PackFault {}

fn frame(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    bytes.extend_from_slice(value);
}

/// Read one framed segment from `bytes` at `*cursor`, advancing past it.
fn read_frame<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    let length = u64::from_be_bytes(bytes.get(*cursor..*cursor + 8)?.try_into().ok()?) as usize;
    let end = cursor.checked_add(8)?.checked_add(length)?;
    let value = bytes.get(*cursor + 8..end)?;
    *cursor = end;
    Some(value)
}

/// The draft pack writer: canonical export sorted by id, budget-checked.
#[derive(Clone, Copy, Debug)]
pub struct PackWriter {
    budgets: PackBudgets,
}

impl PackWriter {
    #[must_use]
    pub fn new(budgets: PackBudgets) -> Self {
        Self { budgets }
    }

    /// Write a pack: full (no parent reference) or thin (names the
    /// parent pack id it deltas against). Entries are sorted by id —
    /// canonical export — and duplicates refuse.
    pub fn write(&self, entries: &[PackEntry], parent: Option<&str>) -> Result<Vec<u8>, PackFault> {
        if entries.len() > self.budgets.max_entries {
            return Err(PackFault::Oversized {
                code: "E-EVID-604".to_string(),
            });
        }
        let mut by_id = BTreeMap::new();
        for entry in entries {
            if entry.payload.len() > self.budgets.max_payload_bytes {
                return Err(PackFault::Oversized {
                    code: "E-EVID-604".to_string(),
                });
            }
            if by_id.insert(entry.id.as_str(), entry).is_some() {
                return Err(PackFault::DuplicateEntry {
                    code: "E-EVID-606".to_string(),
                    id: entry.id.clone(),
                });
            }
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(FORMAT_VERSION);
        frame(&mut bytes, parent.map_or(&[][..], str::as_bytes));
        frame(
            &mut bytes,
            &u64::try_from(by_id.len()).unwrap_or(u64::MAX).to_be_bytes(),
        );
        for entry in by_id.values() {
            frame(&mut bytes, entry.id.as_bytes());
            frame(&mut bytes, &entry.payload);
        }
        if bytes.len() > self.budgets.max_total_bytes {
            return Err(PackFault::Oversized {
                code: "E-EVID-604".to_string(),
            });
        }
        Ok(bytes)
    }
}

/// The draft pack reader: corruption refuses before any entry is
/// returned.
#[derive(Clone, Copy, Debug)]
pub struct PackReader {
    budgets: PackBudgets,
}

impl PackReader {
    #[must_use]
    pub fn new(budgets: PackBudgets) -> Self {
        Self { budgets }
    }

    /// Read a pack. A thin pack (non-empty parent reference) requires
    /// `parent` to carry the parent pack's bytes — without the parent
    /// closure the read refuses (`E-EVID-605`), never a partial silent
    /// read. With the closure, the result is the merged view: parent
    /// entries overlaid by the thin entries (thin wins on equal ids).
    pub fn read(&self, bytes: &[u8], parent: Option<&[u8]>) -> Result<Vec<PackEntry>, PackFault> {
        if bytes.len() > self.budgets.max_total_bytes {
            return Err(PackFault::Oversized {
                code: "E-EVID-604".to_string(),
            });
        }
        if !bytes.starts_with(MAGIC) {
            return Err(PackFault::BadMagic {
                code: "E-EVID-602".to_string(),
            });
        }
        let mut cursor = MAGIC.len();
        let Some(&version) = bytes.get(cursor) else {
            return Err(PackFault::Truncated {
                code: "E-EVID-603".to_string(),
            });
        };
        cursor += 1;
        if version != FORMAT_VERSION {
            return Err(PackFault::BadMagic {
                code: "E-EVID-602".to_string(),
            });
        }
        let parent_ref = read_frame(bytes, &mut cursor).ok_or(PackFault::Truncated {
            code: "E-EVID-603".to_string(),
        })?;
        let count_frame = read_frame(bytes, &mut cursor).ok_or(PackFault::Truncated {
            code: "E-EVID-603".to_string(),
        })?;
        let Ok(count) = <[u8; 8]>::try_from(count_frame).map(u64::from_be_bytes) else {
            return Err(PackFault::Truncated {
                code: "E-EVID-603".to_string(),
            });
        };
        if count as usize > self.budgets.max_entries {
            return Err(PackFault::Oversized {
                code: "E-EVID-604".to_string(),
            });
        }
        let mut read_entries = Vec::new();
        for _ in 0..count {
            let id = read_frame(bytes, &mut cursor).ok_or(PackFault::Truncated {
                code: "E-EVID-603".to_string(),
            })?;
            let payload = read_frame(bytes, &mut cursor).ok_or(PackFault::Truncated {
                code: "E-EVID-603".to_string(),
            })?;
            if payload.len() > self.budgets.max_payload_bytes {
                return Err(PackFault::Oversized {
                    code: "E-EVID-604".to_string(),
                });
            }
            let Ok(id) = std::str::from_utf8(id) else {
                return Err(PackFault::BadMagic {
                    code: "E-EVID-602".to_string(),
                });
            };
            read_entries.push(PackEntry {
                id: id.to_string(),
                payload: payload.to_vec(),
            });
        }
        if parent_ref.is_empty() {
            return Ok(read_entries);
        }
        // Thin pack: the parent closure is mandatory.
        let Some(parent_bytes) = parent else {
            return Err(PackFault::ThinWithoutParent {
                code: "E-EVID-605".to_string(),
            });
        };
        let parent_entries = self.read(parent_bytes, None)?;
        let mut merged: BTreeMap<String, Vec<u8>> = parent_entries
            .into_iter()
            .map(|entry| (entry.id, entry.payload))
            .collect();
        for entry in read_entries {
            merged.insert(entry.id, entry.payload);
        }
        Ok(merged
            .into_iter()
            .map(|(id, payload)| PackEntry { id, payload })
            .collect())
    }
}
