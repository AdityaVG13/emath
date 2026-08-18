//! Evidence/artifact state store (frankensqlite lane, pass 1).
//!
//! Feature-gated adapter crate (CUTOVER_PLAN.md section 5.2 / 9.10): the
//! default build is std-only with zero third-party dependencies and only
//! first-party workspace crates for the manifest schema (emath-artifact).
//! The sqlite-store feature adds the pinned fsqlite git facade plus the
//! asupersync runtime that drives it, exposing a blocking Store over the
//! async engine.
//!
//! Determinism class: pure sequence. No wall-clock timestamps exist anywhere
//! in the schema or API; row order is explicit via seq + claim, and identical
//! writes produce identical rows (verified by tests).

#![forbid(unsafe_code)]

pub mod schema;

#[cfg(feature = "sqlite-store")]
pub mod store;

#[cfg(feature = "sqlite-store")]
pub use store::{ArtifactRecord, EvidenceRow, Store, StoreError, StoreOp};

#[cfg(test)]
mod tests {
    use super::schema::{
        self, CLAIM_STATUS_FAIL, CLAIM_STATUS_OK, CLAIM_STATUS_PENDING, SCHEMA_SQL,
    };

    #[test]
    fn schema_is_deterministic_ddl() {
        // DDL is a compile-time constant: the schema text is stable across
        // builds. Both tables and the closed status CHECK must be present.
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS artifacts ("));
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS evidence ("));
        assert!(SCHEMA_SQL.contains("status IN ('ok', 'fail', 'pending')"));
        assert!(SCHEMA_SQL.contains("PRIMARY KEY (artifact_id, claim, seq)"));
    }

    #[test]
    fn claim_status_validation_boundaries() {
        assert!(schema::valid_claim_status(CLAIM_STATUS_OK));
        assert!(schema::valid_claim_status(CLAIM_STATUS_FAIL));
        assert!(schema::valid_claim_status(CLAIM_STATUS_PENDING));
        assert!(!schema::valid_claim_status(""));
        assert!(!schema::valid_claim_status("okay"));
        assert!(!schema::valid_claim_status("OK"));
        assert!(!schema::valid_claim_status(" pending"));
    }
}
