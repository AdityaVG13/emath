//! Evidence IR: claims with producer, checker, verdict, level and references.

use crate::goal::EvidenceLevel;
use emath_core::{ContentId, SchemaId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceClaim {
    pub id: String,
    pub statement: String,
    pub class: String,
    pub scope: String,
    pub assumptions: Vec<String>,
    pub producer: String,
    pub checker: Option<String>,
    pub verdict: ClaimVerdict,
    pub level: EvidenceLevel,
    pub falsifiers: Vec<String>,
    pub artifacts: Vec<String>,
    pub fresh_until: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimVerdict {
    Pass,
    Fail,
    Inconclusive,
    NotRun,
}

impl ClaimVerdict {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Inconclusive => "inconclusive",
            Self::NotRun => "not-run",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceBundle {
    pub schema: SchemaId,
    pub bundle_id: ContentId,
    pub source_package: ContentId,
    pub resolution_plan: ContentId,
    pub claims: Vec<EvidenceClaim>,
    pub artifacts: std::collections::BTreeMap<String, String>,
    pub reproduction: Vec<String>,
}
