//! Generated human/reference projections of a Language Image.

use std::collections::{BTreeMap, BTreeSet};

use emath_core::{CanonicalField, DistributionHash};
use emath_ir::{CapsuleSlot, FeatureCapsule, Maturity};

pub const GENERATED_REFERENCE_HEADER: &str =
    "<!-- @generated from emath.language-image; DO NOT EDIT -->\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedReferenceViews {
    pub pages: BTreeMap<String, String>,
    pub lock: DistributionHash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceViewError {
    DuplicateFeature(String),
    CatalogClaimedLive(String),
    HiddenHole(String),
    UnknownAuthority(String),
    MissingCoverage(String),
    StaleLock,
    ManualEdit(String),
}

pub fn generate_reference_views(
    capsules: &[FeatureCapsule],
    authority: &BTreeMap<String, String>,
) -> Result<GeneratedReferenceViews, ReferenceViewError> {
    let mut seen = BTreeSet::new();
    let mut index = GENERATED_REFERENCE_HEADER.to_string();
    index.push_str("# Feature index\n\n| FeatureID | class | maturity | authority | source |\n|---|---|---|---|---|\n");
    let mut diagnostics = GENERATED_REFERENCE_HEADER.to_string() + "# Diagnostics\n\n";
    let mut coverage = GENERATED_REFERENCE_HEADER.to_string() + "# Provider and world coverage\n\n";
    let mut gaps = GENERATED_REFERENCE_HEADER.to_string() + "# Gap radar\n\n";
    let mut sorted = capsules.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
    for capsule in sorted {
        let id = capsule.feature_id.to_string();
        if !seen.insert(id.clone()) {
            return Err(ReferenceViewError::DuplicateFeature(id));
        }
        let state = authority
            .get(&id)
            .cloned()
            .unwrap_or_else(|| "unassigned".to_string());
        if !matches!(
            state.as_str(),
            "unassigned"
                | "legacy-active"
                | "capsule-candidate"
                | "legacy-active-dual-run"
                | "capsule-active"
                | "rollback-pending"
                | "retired"
        ) {
            return Err(ReferenceViewError::UnknownAuthority(state));
        }
        if capsule.maturity == Maturity::Cataloged && state == "capsule-active" {
            return Err(ReferenceViewError::CatalogClaimedLive(id));
        }
        if capsule.has_blocking_hole() && state == "capsule-active" {
            return Err(ReferenceViewError::HiddenHole(id));
        }
        index.push_str(&format!(
            "| `{}` | {} | {} | {} | [`{}`](../{}) |\n",
            capsule.feature_id,
            capsule.class,
            capsule.maturity.as_str(),
            state,
            capsule.source,
            capsule.source
        ));
        if capsule.class == emath_ir::FeatureClass::Diagnostic {
            diagnostics.push_str(&format!(
                "- `{}` — {}\n",
                capsule.feature_id, capsule.summary
            ));
        }
        let worlds = rendered(capsule, "worlds");
        let providers = rendered(capsule, "providers");
        if state == "capsule-active" && (worlds.is_none() || providers.is_none()) {
            return Err(ReferenceViewError::MissingCoverage(id));
        }
        coverage.push_str(&format!(
            "- `{}`: worlds {}; providers {}\n",
            capsule.feature_id,
            worlds.as_deref().unwrap_or("not-applicable"),
            providers.as_deref().unwrap_or("not-applicable")
        ));
        if state != "capsule-active" || capsule.has_blocking_hole() {
            gaps.push_str(&format!(
                "- `{}`: maturity {}; authority {}; next: {}\n",
                capsule.feature_id,
                capsule.maturity.as_str(),
                state,
                gap_reason(capsule)
            ));
        }
    }
    let pages = BTreeMap::from([
        ("feature-index.md".to_string(), index),
        ("diagnostics.md".to_string(), diagnostics),
        ("coverage.md".to_string(), coverage),
        ("gap-radar.md".to_string(), gaps),
    ]);
    let body = pages
        .iter()
        .map(|(name, page)| format!("{name}\n{page}"))
        .collect::<String>();
    let lock =
        DistributionHash::new(&[CanonicalField::new("image", body.as_bytes()).expect("fixed")])
            .expect("fixed");
    Ok(GeneratedReferenceViews { pages, lock })
}

impl GeneratedReferenceViews {
    pub fn verify(&self) -> Result<(), ReferenceViewError> {
        for (name, page) in &self.pages {
            if !page.starts_with(GENERATED_REFERENCE_HEADER) {
                return Err(ReferenceViewError::ManualEdit(name.clone()));
            }
        }
        let body = self
            .pages
            .iter()
            .map(|(name, page)| format!("{name}\n{page}"))
            .collect::<String>();
        let computed =
            DistributionHash::new(&[CanonicalField::new("image", body.as_bytes()).expect("fixed")])
                .expect("fixed");
        if computed != self.lock {
            return Err(ReferenceViewError::StaleLock);
        }
        Ok(())
    }
}

fn rendered(capsule: &FeatureCapsule, name: &str) -> Option<String> {
    capsule.slots.get(name).map(CapsuleSlot::canonical)
}

fn gap_reason(capsule: &FeatureCapsule) -> &'static str {
    if capsule.has_blocking_hole() {
        "resolve blocking Spec Hole"
    } else if capsule.maturity == Maturity::Cataloged {
        "supply semantics and conformance"
    } else {
        "complete publication gates"
    }
}
