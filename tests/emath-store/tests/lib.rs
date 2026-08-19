//! `emath-store` evidence-state schema tests (migrated from
//! `crates/emath-store/src/lib.rs`).

use emath_store::schema::{
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
