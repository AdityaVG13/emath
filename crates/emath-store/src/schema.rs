//! Feature-independent schema text and claim-status validation (std-only).

pub const ARTIFACTS_TABLE: &str = "artifacts";
pub const EVIDENCE_TABLE: &str = "evidence";

/// Deterministic DDL: artifact rows plus per-artifact claim rows ordered by
/// seq; the status CHECK closes the value set to three states.
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

pub const CLAIM_STATUS_OK: &str = "ok";
pub const CLAIM_STATUS_FAIL: &str = "fail";
pub const CLAIM_STATUS_PENDING: &str = "pending";

/// The closed set of valid claim statuses.
pub const VALID_CLAIM_STATUSES: [&str; 3] =
    [CLAIM_STATUS_OK, CLAIM_STATUS_FAIL, CLAIM_STATUS_PENDING];

#[must_use]
pub fn valid_claim_status(status: &str) -> bool {
    VALID_CLAIM_STATUSES.contains(&status)
}
