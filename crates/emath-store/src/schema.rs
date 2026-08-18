//! Feature-independent schema text and claim-status validation (std-only).

/// Name of the artifact table.
pub const ARTIFACTS_TABLE: &str = "artifacts";
/// Name of the evidence table.
pub const EVIDENCE_TABLE: &str = "evidence";

/// Deterministic DDL for the evidence/artifact state store.
///
/// The artifacts table holds one row per evidence-carrying artifact; the
/// evidence table holds per-artifact claim rows. Row order is explicit
/// (seq), never wall-clock. The status CHECK closes the value set to the
/// three documented states.
pub const SCHEMA_SQL: &str = r"CREATE TABLE IF NOT EXISTS artifacts (
  id   TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  path TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS evidence (
  artifact_id TEXT NOT NULL REFERENCES artifacts(id),
  claim       TEXT NOT NULL,
  status      TEXT NOT NULL CHECK (status IN ('ok', 'fail', 'pending')),
  seq         INTEGER NOT NULL,
  PRIMARY KEY (artifact_id, claim, seq)
);";

/// Evidence claim status: claim verified.
pub const CLAIM_STATUS_OK: &str = "ok";
/// Evidence claim status: claim contradicted / verification failed.
pub const CLAIM_STATUS_FAIL: &str = "fail";
/// Evidence claim status: claim not yet adjudicated.
pub const CLAIM_STATUS_PENDING: &str = "pending";

/// The closed set of valid claim statuses.
pub const VALID_CLAIM_STATUSES: [&str; 3] =
    [CLAIM_STATUS_OK, CLAIM_STATUS_FAIL, CLAIM_STATUS_PENDING];

/// True iff `status` is one of `VALID_CLAIM_STATUSES`.
#[must_use]
pub fn valid_claim_status(status: &str) -> bool {
    VALID_CLAIM_STATUSES.contains(&status)
}
