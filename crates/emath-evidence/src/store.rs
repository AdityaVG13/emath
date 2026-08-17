//! Content-addressed evidence store.
//!
//! Records are addressed by the FNV-1a of their canonical token, so
//! identical content always maps to one slot and a content-identity
//! mismatch is tamper, not a rebuild. Revocation and supersession are
//! append-only: markers are never deleted and conflicts are refused.
//! A provenance graph records which records each entry derives from.
//!
//! Stable codes:
//! - `E-EVID-501` unknown record id;
//! - `E-EVID-502` duplicate append-only revocation marker;
//! - `E-EVID-503` content-identity mismatch (bootstrap identity);
//! - `E-EVID-504` double supersession (append-only conflict).

use std::collections::{BTreeMap, BTreeSet};

use emath_core::{ContentId, content_id_of_str};

use crate::{EvidenceError, EvidenceKind, EvidenceRecord};

/// Content-addressed evidence store.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvidenceStore {
    records: BTreeMap<String, EvidenceRecord>,
    provenance: BTreeMap<String, Vec<String>>,
    revoked: BTreeSet<String>,
    superseded_by: BTreeMap<String, String>,
}

impl EvidenceStore {
    /// Registers a record under its content address. Registering the
    /// identical content again is idempotent; a record that would land
    /// on an address already holding different content is tamper and
    /// refused (`E-EVID-503`).
    pub fn register(&mut self, record: EvidenceRecord) -> Result<ContentId, EvidenceError> {
        let address = Self::address(&record);
        if let Some(existing) = self.records.get(&address.0) {
            if existing == &record {
                return Ok(address);
            }
            return Err(EvidenceError::new(
                "E-EVID-503",
                format!(
                    "content-identity mismatch at {}: different record for the same address",
                    address.0
                ),
            ));
        }
        self.records.insert(address.0.clone(), record);
        Ok(address)
    }

    /// Registers a record and attaches its provenance sources
    /// (producer/checker records it derives from).
    pub fn register_with_sources(
        &mut self,
        record: EvidenceRecord,
        sources: &[String],
    ) -> Result<ContentId, EvidenceError> {
        let address = self.register(record)?;
        let mut existing = self.provenance.get(&address.0).cloned().unwrap_or_default();
        for source in sources {
            if !existing.contains(source) {
                existing.push(source.clone());
            }
        }
        existing.sort();
        self.provenance.insert(address.0.clone(), existing);
        Ok(address)
    }

    /// Queries a record by content address (`E-EVID-501` when absent).
    pub fn query(&self, id: &str) -> Result<&EvidenceRecord, EvidenceError> {
        self.records.get(id).ok_or_else(|| {
            EvidenceError::new("E-EVID-501", format!("unknown evidence record {id}"))
        })
    }

    /// All records of one evidence kind, in address order.
    #[must_use]
    pub fn query_by_kind(&self, kind: EvidenceKind) -> Vec<&EvidenceRecord> {
        self.records
            .values()
            .filter(|record| record.kind == kind)
            .collect()
    }

    /// All resolved (promotable) records, in address order.
    #[must_use]
    pub fn query_resolved(&self) -> Vec<&EvidenceRecord> {
        self.records
            .values()
            .filter(|record| record.resolved())
            .collect()
    }

    /// Append-only revocation: revoking an already revoked record is
    /// refused (`E-EVID-502`); revocation never deletes content.
    pub fn revoke(&mut self, id: &str) -> Result<(), EvidenceError> {
        self.query(id)?;
        if self.revoked.contains(id) {
            return Err(EvidenceError::new(
                "E-EVID-502",
                format!("record {id} is already revoked (append-only)"),
            ));
        }
        self.revoked.insert(id.to_string());
        Ok(())
    }

    /// Whether the record is revoked.
    #[must_use]
    pub fn is_revoked(&self, id: &str) -> bool {
        self.revoked.contains(id)
    }

    /// Append-only supersession: marks `old` superseded by `new`.
    /// Double supersession is refused (`E-EVID-504`); both records must
    /// exist (`E-EVID-501`).
    pub fn supersede(&mut self, old: &str, new: &str) -> Result<(), EvidenceError> {
        self.query(old)?;
        self.query(new)?;
        if self.superseded_by.contains_key(old) {
            return Err(EvidenceError::new(
                "E-EVID-504",
                format!("record {old} is already superseded (append-only)"),
            ));
        }
        if old == new {
            return Err(EvidenceError::new(
                "E-EVID-504",
                format!("record {old} cannot supersede itself"),
            ));
        }
        self.superseded_by.insert(old.to_string(), new.to_string());
        Ok(())
    }

    /// The replacement of a superseded record, if any.
    #[must_use]
    pub fn superseded_by(&self, id: &str) -> Option<&str> {
        self.superseded_by.get(id).map(String::as_str)
    }

    /// Direct provenance sources of a record (`E-EVID-501` when absent).
    pub fn provenance_of(&self, id: &str) -> Result<&[String], EvidenceError> {
        self.query(id)?;
        Ok(self.provenance.get(id).map_or(&[], Vec::as_slice))
    }

    /// Integrity check for a stored slot: the record content must still
    /// address the slot it lives under. A mismatch is tamper
    /// (`E-EVID-503`).
    pub fn verify_integrity(&self, id: &str) -> Result<(), EvidenceError> {
        let record = self.query(id)?;
        let expected = Self::address(record);
        if expected.0 != id {
            return Err(EvidenceError::new(
                "E-EVID-503",
                format!(
                    "slot {id} no longer matches its record content (address {})",
                    expected.0
                ),
            ));
        }
        Ok(())
    }

    /// Deterministic store token over all slots.
    #[must_use]
    pub fn canonical(&self) -> String {
        let records: Vec<String> = self
            .records
            .iter()
            .map(|(id, record)| {
                let status = if self.revoked.contains(id) {
                    "revoked".to_string()
                } else if let Some(next) = self.superseded_by.get(id) {
                    format!("superseded-by:{next}")
                } else {
                    "active".to_string()
                };
                format!("{id}={}({})", record.verdict.as_str(), status)
            })
            .collect();
        let provenance: Vec<String> = self
            .provenance
            .iter()
            .map(|(id, sources)| format!("{id}<-[{}]", sources.join(",")))
            .collect();
        format!(
            "store:v1:[{}];[{}]",
            records.join(";"),
            provenance.join(";")
        )
    }

    /// Content address of a record (FNV-1a of its canonical token).
    #[must_use]
    pub fn address(record: &EvidenceRecord) -> ContentId {
        content_id_of_str(&record.canonical())
    }
}

/// Free-function accessor matching the module re-export.
pub fn store_address(record: &EvidenceRecord) -> ContentId {
    EvidenceStore::address(record)
}
