//! Standard library as executable object packs.
//!
//! `std::{core, ...}` exports cells/theories/evidence as `.emlib` object
//! packs: each element is an object with a MeaningID, canonical semantic
//! payload, and presentation — objects, not catalog markdown. Two
//! workspaces mount one pack without duplicating storage or compiler
//! branches: the pack bytes are the single source, mounting is
//! deterministic, and every artifact is content-addressed so forgery
//! refuses before it can be read.
//!
//! Envelope format (stable tag-prefixed, length-framed, deterministic):
//! entry payloads are `tag ++ <object_graph::frame fields>`. Entry ids
//! in the pack are carried verbatim (`.emlib` discipline) and verified
//! against the content-addressed id the graph mints — a forged payload
//! under a stale id is a typed refusal, never silent data.
//!
//! Determinism class: pure sequence. Canonical export (sorted ids)
//! means the same object set always yields the same bytes.

use crate::evidence_plane::{EvidencePlane, EvidencePlaneError, EvidenceReceipt};
use crate::object_graph::{ObjectDraft, ObjectGraph, ObjectKind, frame};
use crate::pack::{PackBudgets, PackEntry, PackFault, PackReader, PackWriter};
use emath_core::{ObjectId, PackId};
use std::str::FromStr;

/// Envelope schema token, length-framed into every entry payload.
pub const STDLIB_ENVELOPE_V1: &str = "emath.stdlib.envelope.v1";
/// Entry payload tag: a library object.
pub const ENVELOPE_TAG_OBJECT: u8 = 1;
/// Entry payload tag: an evidence receipt attachment.
pub const ENVELOPE_TAG_RECEIPT: u8 = 2;

/// One stdlib object as stored in a pack: kind, MeaningID, canonical
/// semantic payload, presentation. Presentation never enters identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdObject {
    pub kind: ObjectKind,
    pub meaning_id: emath_core::MeaningId,
    pub semantic_payload: Vec<u8>,
    pub presentation: Option<String>,
}

impl StdObject {
    /// The object as an insertable graph draft.
    #[must_use]
    pub fn to_draft(&self) -> ObjectDraft {
        ObjectDraft {
            kind: self.kind.clone(),
            meaning_id: self.meaning_id.clone(),
            semantic_payload: self.semantic_payload.clone(),
            presentation: self.presentation.clone(),
        }
    }

    /// Deterministic envelope bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        frame(&mut bytes, STDLIB_ENVELOPE_V1.as_bytes());
        bytes.push(ENVELOPE_TAG_OBJECT);
        frame(&mut bytes, self.kind.canonical_name().as_bytes());
        frame(&mut bytes, self.meaning_id.as_str().as_bytes());
        frame(&mut bytes, &self.semantic_payload);
        match &self.presentation {
            Some(text) => {
                bytes.push(1);
                frame(&mut bytes, text.as_bytes());
            }
            None => bytes.push(0),
        }
        bytes
    }

    /// Decode envelope bytes; malformed structure refuses typed.
    pub fn decode(payload: &[u8]) -> Result<Self, StdMountError> {
        let mut cursor = 0usize;
        let scheme = read_frame(payload, &mut cursor)?
            .ok_or_else(|| StdMountError::malformed("truncated envelope schema"))?;
        if scheme != STDLIB_ENVELOPE_V1.as_bytes() {
            return Err(StdMountError::malformed("unknown envelope schema"));
        }
        if payload.get(cursor) != Some(&ENVELOPE_TAG_OBJECT) {
            return Err(StdMountError::malformed("expected object tag"));
        }
        cursor += 1;
        let kind = String::from_utf8(
            read_frame(payload, &mut cursor)?
                .ok_or_else(|| StdMountError::malformed("truncated kind"))?
                .to_vec(),
        )
        .map_err(|_| StdMountError::malformed("kind is not utf-8"))?;
        let meaning_id = String::from_utf8(
            read_frame(payload, &mut cursor)?
                .ok_or_else(|| StdMountError::malformed("truncated meaning id"))?
                .to_vec(),
        )
        .map_err(|_| StdMountError::malformed("meaning id is not utf-8"))?;
        let semantic_payload = read_frame(payload, &mut cursor)?
            .ok_or_else(|| StdMountError::malformed("truncated semantic payload"))?
            .to_vec();
        let presentation = match payload.get(cursor) {
            Some(0) => None,
            Some(1) => {
                cursor += 1;
                Some(
                    String::from_utf8(
                        read_frame(payload, &mut cursor)?
                            .ok_or_else(|| StdMountError::malformed("truncated presentation"))?
                            .to_vec(),
                    )
                    .map_err(|_| StdMountError::malformed("presentation is not utf-8"))?,
                )
            }
            _ => return Err(StdMountError::malformed("bad presentation flag")),
        };
        Ok(Self {
            kind: parse_kind(&kind)?,
            meaning_id: emath_core::MeaningId::from_str(&meaning_id)
                .map_err(|_| StdMountError::malformed("meaning id is not a durable id"))?,
            semantic_payload,
            presentation,
        })
    }
}

/// One evidence receipt as stored in a pack: the sealed receipt plus the
/// object it attaches to. Entry id is the receipt's own EvidenceID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdReceipt {
    pub kind: String,
    pub payload: Vec<u8>,
    pub object_id: ObjectId,
}

impl StdReceipt {
    /// Deterministic envelope bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        frame(&mut bytes, STDLIB_ENVELOPE_V1.as_bytes());
        bytes.push(ENVELOPE_TAG_RECEIPT);
        frame(&mut bytes, self.kind.as_bytes());
        frame(&mut bytes, &self.payload);
        frame(&mut bytes, self.object_id.as_str().as_bytes());
        bytes
    }

    /// Decode envelope bytes; malformed structure refuses typed.
    pub fn decode(payload: &[u8]) -> Result<Self, StdMountError> {
        let mut cursor = 0usize;
        let scheme = read_frame(payload, &mut cursor)?
            .ok_or_else(|| StdMountError::malformed("truncated envelope schema"))?;
        if scheme != STDLIB_ENVELOPE_V1.as_bytes() {
            return Err(StdMountError::malformed("unknown envelope schema"));
        }
        if payload.get(cursor) != Some(&ENVELOPE_TAG_RECEIPT) {
            return Err(StdMountError::malformed("expected receipt tag"));
        }
        cursor += 1;
        let kind = String::from_utf8(
            read_frame(payload, &mut cursor)?
                .ok_or_else(|| StdMountError::malformed("truncated receipt kind"))?
                .to_vec(),
        )
        .map_err(|_| StdMountError::malformed("receipt kind is not utf-8"))?;
        let payload_bytes = read_frame(payload, &mut cursor)?
            .ok_or_else(|| StdMountError::malformed("truncated receipt payload"))?
            .to_vec();
        let object_id = String::from_utf8(
            read_frame(payload, &mut cursor)?
                .ok_or_else(|| StdMountError::malformed("truncated object id"))?
                .to_vec(),
        )
        .map_err(|_| StdMountError::malformed("object id is not utf-8"))?;
        Ok(Self {
            kind,
            payload: payload_bytes,
            object_id: ObjectId::from_str(&object_id)
                .map_err(|_| StdMountError::malformed("object id is not a durable id"))?,
        })
    }
}

/// A pack entry as decoded: object or evidence receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StdEntry {
    Object(StdObject),
    Receipt(StdReceipt),
}

impl StdEntry {
    /// Decode a pack entry payload by its envelope schema and tag byte.
    pub fn decode(payload: &[u8]) -> Result<Self, StdMountError> {
        let mut cursor = 0usize;
        let scheme = read_frame(payload, &mut cursor)?
            .ok_or_else(|| StdMountError::malformed("truncated envelope schema"))?;
        if scheme != STDLIB_ENVELOPE_V1.as_bytes() {
            return Err(StdMountError::malformed("unknown envelope schema"));
        }
        match payload.get(cursor) {
            Some(&ENVELOPE_TAG_OBJECT) => StdObject::decode(payload).map(Self::Object),
            Some(&ENVELOPE_TAG_RECEIPT) => StdReceipt::decode(payload).map(Self::Receipt),
            _ => Err(StdMountError::malformed("unknown entry tag")),
        }
    }
}

/// Typed mount refusals. Closed set; every failure has a stable code and
/// nothing is silently dropped:
/// `E-EVID-5xx` evidence-plane codes (forgery refused before storage),
/// `E-EVID-6xx` pack corruption codes, `E-STD-001` malformed envelope,
/// `E-STD-002` forged object (entry id does not re-derive from content).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StdMountError {
    /// Pack-level corruption or budget refusal (`E-EVID-60x`).
    Pack(PackFault),
    /// Malformed envelope structure (`E-STD-001`).
    Malformed { code: String, detail: String },
    /// An entry id that does not re-derive from its content — a forged
    /// object under a stale id (`E-STD-002`).
    ForgedObject {
        code: String,
        entry_id: String,
        recomputed: String,
    },
    /// A receipt whose hash does not re-verify — forged or tampered
    /// evidence (`E-EVID-503`).
    ForgedEvidence { code: String },
    /// Object identity collision on mount (same id, different content).
    Collision { code: String, detail: String },
}

impl StdMountError {
    fn malformed(detail: &str) -> Self {
        Self::Malformed {
            code: "E-STD-001".to_string(),
            detail: detail.to_string(),
        }
    }
}

impl std::fmt::Display for StdMountError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(fault) => write!(formatter, "{fault}"),
            Self::Malformed { code, detail } => {
                write!(formatter, "{code}: malformed stdlib pack entry: {detail}")
            }
            Self::ForgedObject {
                code,
                entry_id,
                recomputed,
            } => write!(
                formatter,
                "{code}: forged object: entry id `{entry_id}` does not re-derive from its \
                 content (recomputed `{recomputed}`) — never read as data"
            ),
            Self::ForgedEvidence { code } => write!(
                formatter,
                "{code}: forged evidence: receipt hash does not re-verify — refused before \
                 anything is stored"
            ),
            Self::Collision { code, detail } => write!(formatter, "{code}: {detail}"),
        }
    }
}

impl std::error::Error for StdMountError {}

/// A mounted stdlib workspace view: the object graph plus its evidence
/// plane, both derived deterministically from one pack's bytes.
#[derive(Clone, Debug)]
pub struct StdMount {
    pub graph: ObjectGraph,
    pub evidence: EvidencePlane,
    pub pack_id: PackId,
}

/// Mount a stdlib pack: read (corruption refuses), re-verify every
/// object id and every evidence hash, and assemble the workspace view.
/// Deterministic: the same bytes always yield the same graph, the same
/// evidence ids, and the same [`PackId`].
pub fn mount_stdlib(bytes: &[u8]) -> Result<StdMount, StdMountError> {
    let entries = PackReader::new(PackBudgets::draft())
        .read(bytes, None)
        .map_err(StdMountError::Pack)?;
    // Decode each entry payload exactly once, then run the two passes
    // over the DECODED entries — objects first, then receipts — so the
    // mount is order-independent without re-parsing every payload twice.
    // Decode errors dominate by position in either design, and no
    // partial state escapes on failure (the graph is discarded), so the
    // observable refusals are unchanged.
    let decoded: Vec<StdEntry> = entries
        .iter()
        .map(|entry| StdEntry::decode(&entry.payload))
        .collect::<Result<Vec<_>, _>>()?;
    let mut graph = ObjectGraph::default();
    let mut evidence = EvidencePlane::default();
    for (entry, decoded) in entries.iter().zip(&decoded) {
        if let StdEntry::Object(object) = decoded {
            let id = graph
                .put(object.to_draft())
                .map_err(|error| StdMountError::Collision {
                    code: "E-STD-003".to_string(),
                    detail: error.to_string(),
                })?;
            if id.as_str() != entry.id {
                return Err(StdMountError::ForgedObject {
                    code: "E-STD-002".to_string(),
                    entry_id: entry.id.clone(),
                    recomputed: id.as_str().to_string(),
                });
            }
        }
    }
    for (entry, decoded) in entries.iter().zip(&decoded) {
        if let StdEntry::Receipt(receipt) = decoded {
            let sealed = EvidenceReceipt::seal(&receipt.kind, &receipt.payload);
            if sealed.evidence_id.as_str() != entry.id {
                // The pack carries an id that does not derive from the
                // receipt content: forged evidence under a stale seal.
                return Err(StdMountError::ForgedEvidence {
                    code: "E-EVID-503".to_string(),
                });
            }
            evidence
                .attach(&graph, &receipt.object_id, sealed)
                .map_err(|error| match error {
                    EvidencePlaneError::ForgedHash(code, _) => {
                        StdMountError::ForgedEvidence { code }
                    }
                    other => StdMountError::Malformed {
                        code: "E-STD-001".to_string(),
                        detail: other.to_string(),
                    },
                })?;
        }
    }
    Ok(StdMount {
        graph,
        evidence,
        pack_id: PackId::from_bytes(bytes),
    })
}

/// Canonical export of a stdlib pack: entries sorted by id, so the same
/// object set always produces identical bytes regardless of insertion
/// order. Duplicate ids refuse.
pub fn export_std_pack(entries: &[PackEntry]) -> Result<Vec<u8>, PackFault> {
    PackWriter::new(PackBudgets::draft()).write(entries, None)
}

fn read_frame<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<Option<&'a [u8]>, StdMountError> {
    let Some(end_len) = cursor.checked_add(8) else {
        return Ok(None);
    };
    let Some(length_bytes) = bytes.get(*cursor..end_len) else {
        return Ok(None);
    };
    let Ok(length_bytes) = <[u8; 8]>::try_from(length_bytes) else {
        return Ok(None);
    };
    let length = u64::from_be_bytes(length_bytes) as usize;
    let Some(end) = end_len.checked_add(length) else {
        return Ok(None);
    };
    let Some(value) = bytes.get(end_len..end) else {
        return Ok(None);
    };
    *cursor = end;
    Ok(Some(value))
}

fn parse_kind(name: &str) -> Result<ObjectKind, StdMountError> {
    Ok(match name {
        "cell" => ObjectKind::Cell,
        "theory" => ObjectKind::Theory,
        "proof" => ObjectKind::Proof,
        "method" => ObjectKind::Method,
        "lesson" => ObjectKind::Lesson,
        "recipe" => ObjectKind::Recipe,
        other => ObjectKind::Custom(
            // Strip the namespace marker exactly once: a custom kind
            // whose own name contains the prefix (`custom:proxy` → wire
            // name `custom:custom:proxy`) must round-trip identically,
            // never collapse nested namespaces onto one kind.
            other.strip_prefix("custom:").unwrap_or(other).to_string(),
        ),
    })
}
