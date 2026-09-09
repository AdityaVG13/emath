//! Durable feature-authority locks and transition receipts.

use std::collections::BTreeMap;

use emath_core::{FeatureId, SemanticHash};

pub const AUTHORITY_LOCK_SCHEMA: &str = "emath.feature-authority-lock";
pub const AUTHORITY_RECEIPT_SCHEMA: &str = "emath.feature-authority-receipt";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthorityState {
    LegacyActive,
    CapsuleCandidate,
    LegacyActiveDualRun,
    CapsuleActive,
    RollbackPending,
    Retired,
}

impl AuthorityState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyActive => "legacy-active",
            Self::CapsuleCandidate => "capsule-candidate",
            Self::LegacyActiveDualRun => "legacy-active-dual-run",
            Self::CapsuleActive => "capsule-active",
            Self::RollbackPending => "rollback-pending",
            Self::Retired => "retired",
        }
    }

    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::LegacyActive, Self::CapsuleCandidate)
                | (Self::CapsuleCandidate, Self::LegacyActiveDualRun)
                | (Self::LegacyActiveDualRun, Self::CapsuleActive)
                | (Self::LegacyActiveDualRun, Self::LegacyActive)
                | (Self::CapsuleActive, Self::RollbackPending)
                | (Self::RollbackPending, Self::LegacyActive)
                | (Self::CapsuleActive, Self::Retired)
                | (Self::LegacyActive, Self::Retired)
        )
    }
}

impl std::str::FromStr for AuthorityState {
    type Err = AuthorityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "legacy-active" => Ok(Self::LegacyActive),
            "capsule-candidate" => Ok(Self::CapsuleCandidate),
            "legacy-active-dual-run" => Ok(Self::LegacyActiveDualRun),
            "capsule-active" => Ok(Self::CapsuleActive),
            "rollback-pending" => Ok(Self::RollbackPending),
            "retired" => Ok(Self::Retired),
            _ => Err(AuthorityError::UnknownState(value.to_string())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityEntry {
    pub state: AuthorityState,
    pub active_source: String,
    pub semantic_hash: SemanticHash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityLock {
    pub schema: String,
    pub entries: BTreeMap<FeatureId, AuthorityEntry>,
}

impl Default for AuthorityLock {
    fn default() -> Self {
        Self {
            schema: AUTHORITY_LOCK_SCHEMA.to_string(),
            entries: BTreeMap::new(),
        }
    }
}

impl AuthorityLock {
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = format!("schema={}\n", self.schema);
        for (feature, entry) in &self.entries {
            out.push_str(&format!(
                "{feature}={} {} {}\n",
                entry.state.as_str(),
                entry.active_source,
                entry.semantic_hash
            ));
        }
        out
    }

    pub fn transition(
        &mut self,
        feature: &FeatureId,
        next: AuthorityEntry,
        evidence: AuthorityEvidence,
    ) -> Result<AuthorityReceipt, AuthorityError> {
        let current = self
            .entries
            .get(feature)
            .cloned()
            .ok_or_else(|| AuthorityError::UnknownFeature(feature.clone()))?;
        if !current.state.can_transition_to(next.state) {
            return Err(AuthorityError::IllegalTransition {
                from: current.state,
                to: next.state,
            });
        }
        if !active_source_is_valid(next.state, &next.active_source) {
            return Err(AuthorityError::DualAuthority(feature.clone()));
        }
        let receipt = AuthorityReceipt {
            schema: AUTHORITY_RECEIPT_SCHEMA.to_string(),
            feature_id: feature.clone(),
            from: current.state,
            to: next.state,
            old_hash: current.semantic_hash,
            new_hash: next.semantic_hash.clone(),
            conformance: evidence.conformance,
            generated_views: evidence.generated_views,
            rollback: evidence.rollback,
        };
        self.entries.insert(feature.clone(), next);
        Ok(receipt)
    }
}

fn active_source_is_valid(state: AuthorityState, source: &str) -> bool {
    match state {
        AuthorityState::LegacyActive
        | AuthorityState::LegacyActiveDualRun
        | AuthorityState::RollbackPending => source == "legacy",
        AuthorityState::CapsuleActive | AuthorityState::CapsuleCandidate => source == "capsule",
        AuthorityState::Retired => source == "none",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityEvidence {
    pub conformance: Vec<String>,
    pub generated_views: Vec<String>,
    pub rollback: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityReceipt {
    pub schema: String,
    pub feature_id: FeatureId,
    pub from: AuthorityState,
    pub to: AuthorityState,
    pub old_hash: SemanticHash,
    pub new_hash: SemanticHash,
    pub conformance: Vec<String>,
    pub generated_views: Vec<String>,
    pub rollback: String,
}

impl AuthorityReceipt {
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "schema={}\nfeature_id={}\nfrom={}\nto={}\nold_hash={}\nnew_hash={}\nconformance={}\ngenerated_views={}\nrollback={}\n",
            self.schema,
            self.feature_id,
            self.from.as_str(),
            self.to.as_str(),
            self.old_hash,
            self.new_hash,
            self.conformance.join(","),
            self.generated_views.join(","),
            self.rollback,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorityError {
    UnknownFeature(FeatureId),
    IllegalTransition {
        from: AuthorityState,
        to: AuthorityState,
    },
    DualAuthority(FeatureId),
    UnknownState(String),
}
