//! The independent evidence plane: proofs/tests attach to
//! stored objects as content-addressed receipts WITHOUT touching the
//! object graph — attaching evidence never changes ObjectID or
//! MeaningID (evidence is not a second meaning identity; authority
//! comes from the receipt content, never from method popularity).
//!
//! Receipts reuse the house content-addressing convention
//! (`fnv1a64` over length-framed canonical bytes, schema-fenced like
//! every store id). A receipt whose recorded `evidence_id` does not
//! match the hash recomputed from its `(kind, payload)` is FORGED —
//! `E-EVID-503`, the EvidenceStore tamper code, refused before any
//! view mutation. Attachment is idempotent per (object, receipt);
//! views are derived (count/iterate), never stored separately, so the
//! plane cannot drift from its own receipts.

use std::collections::{BTreeMap, BTreeSet};

use std::str::FromStr;

use emath_core::{EvidenceId, ObjectId};

use crate::object_graph::ObjectGraph;

const EVIDENCE_PLANE_SCHEMA_V1: &str = "emath.store.evidence-attachment.v1";

/// A sealed evidence receipt: an evidence kind (what checked what —
/// `test-receipt`, `proof-receipt`, …) and the canonical payload bytes
/// that were checked. `evidence_id` is the content address of the
/// (kind, payload) pair; `seal` computes it and `attach` re-verifies
/// it, so a tampered payload carrying a stale id is a forgery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceReceipt {
    pub evidence_id: EvidenceId,
    pub kind: String,
    pub payload: Vec<u8>,
}

impl EvidenceReceipt {
    /// Seal a receipt: hash the framed canonical content.
    pub fn seal(kind: &str, payload: &[u8]) -> EvidenceReceipt {
        let mut bytes = Vec::new();
        crate::object_graph::frame(&mut bytes, EVIDENCE_PLANE_SCHEMA_V1.as_bytes());
        crate::object_graph::frame(&mut bytes, kind.as_bytes());
        crate::object_graph::frame(&mut bytes, payload);
        EvidenceReceipt {
            evidence_id: EvidenceId::from_bytes(&bytes),
            kind: kind.to_string(),
            payload: payload.to_vec(),
        }
    }

    /// The receipt's own canonical bytes (what `attach` re-hashes).
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        crate::object_graph::frame(&mut bytes, EVIDENCE_PLANE_SCHEMA_V1.as_bytes());
        crate::object_graph::frame(&mut bytes, self.kind.as_bytes());
        crate::object_graph::frame(&mut bytes, &self.payload);
        bytes
    }
}

/// Plane-level refusals. `ForgedHash` carries the house tamper code
/// `E-EVID-503` plus the mismatched address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidencePlaneError {
    /// Attachment target is not in the object graph.
    UnknownObject(ObjectId),
    /// Recorded evidence id does not match the recomputed content
    /// address — tampered receipt content (`E-EVID-503`).
    ForgedHash(String, String),
    /// An empty evidence kind is not checkable evidence.
    EmptyKind,
}

impl std::fmt::Display for EvidencePlaneError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownObject(id) => {
                write!(formatter, "evidence attachment target `{id}` is not stored")
            }
            Self::ForgedHash(code, address) => {
                write!(
                    formatter,
                    "{code}: evidence content-identity mismatch at {address}: \
                     recorded hash does not match the receipt content (forgery)"
                )
            }
            Self::EmptyKind => formatter.write_str("evidence receipt kind must not be empty"),
        }
    }
}

impl std::error::Error for EvidencePlaneError {}

/// The independent evidence plane: receipts by content address, plus
/// the per-object attachment index. Deliberately separate from
/// [`crate::object_graph::ObjectGraph`]: the graph owns meaning
/// identity, the plane owns evidence — attaching mutates neither the
/// graph nor any id it minted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvidencePlane {
    receipts: BTreeMap<String, (String, Vec<u8>)>,
    attachments: BTreeMap<ObjectId, BTreeSet<String>>,
}

impl EvidencePlane {
    /// Attach a sealed receipt to a stored object. Requires the
    /// object to exist; re-verifies the receipt's content hash
    /// (forgery refuses `E-EVID-503` before anything is stored);
    /// idempotent per (object, receipt content).
    pub fn attach(
        &mut self,
        graph: &ObjectGraph,
        object: &ObjectId,
        receipt: EvidenceReceipt,
    ) -> Result<EvidenceId, EvidencePlaneError> {
        if receipt.kind.trim().is_empty() {
            return Err(EvidencePlaneError::EmptyKind);
        }
        if graph.object(object).is_none() {
            return Err(EvidencePlaneError::UnknownObject(object.clone()));
        }
        let canonical = receipt.canonical_bytes();
        let recomputed = EvidenceId::from_bytes(&canonical);
        if recomputed != receipt.evidence_id {
            // The recorded id does not re-derive from the content:
            // forged (tampered payload under a stale seal).
            return Err(EvidencePlaneError::ForgedHash(
                "E-EVID-503".to_string(),
                recomputed.as_str().to_string(),
            ));
        }
        let address = receipt.evidence_id.as_str().to_string();
        match self.receipts.get(&address) {
            Some((existing_kind, existing_payload)) => {
                if existing_kind != &receipt.kind || existing_payload != &receipt.payload {
                    // Same id, different content: tamper even if the
                    // caller forged a *colliding* pair. The tamper MUST
                    // be refused loudly, not silently absorbed — the
                    // existing receipt under this address is the
                    // evidence another consumer may have already read.
                    return Err(EvidencePlaneError::ForgedHash(
                        "E-EVID-503".to_string(),
                        address,
                    ));
                }
            }
            None => {
                self.receipts
                    .insert(address.clone(), (receipt.kind, receipt.payload));
            }
        }
        self.attachments
            .entry(object.clone())
            .or_default()
            .insert(address);
        Ok(receipt.evidence_id)
    }

    /// The receipts attached to an object (ascending content order).
    /// The materialized Vec keeps lifetimes simple: ids are tiny
    /// content-address values, and the view is derived on demand.
    /// Addresses are stored as canonical id strings and parsed back —
    /// never re-hashed (re-hashing an id string would mint a DIFFERENT
    /// identity: the id is the content address of the receipt, not
    /// bytes to digest again).
    pub fn attachments_of(&self, object: &ObjectId) -> Vec<EvidenceId> {
        self.attachments
            .get(object)
            .into_iter()
            .flatten()
            .filter_map(|address| EvidenceId::from_str(address).ok())
            .collect()
    }

    /// Query a receipt by its evidence id (`None` when not attached).
    pub fn receipt(&self, id: &EvidenceId) -> Option<AttachedReceipt<'_>> {
        let (kind, payload) = self.receipts.get(id.as_str())?;
        Some(AttachedReceipt { kind, payload })
    }
}

/// A receipt as seen through the plane's evidence views.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachedReceipt<'a> {
    pub kind: &'a str,
    pub payload: &'a [u8],
}
