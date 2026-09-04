#![forbid(unsafe_code)]

use emath_language_model::{FeatureCapsule, FeatureId, Projection};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeaningSpine {
    pub features: BTreeMap<FeatureId, FeatureCapsule>,
}

impl MeaningSpine {
    #[must_use]
    pub fn dependency_closure(&self, root: &FeatureId) -> BTreeSet<FeatureId> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([root.clone()]);
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(feature) = self.features.get(&id) {
                queue.extend(feature.dependencies.iter().cloned());
            }
        }
        seen
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextCapsule {
    pub feature: FeatureId,
    pub dependency_closure: BTreeSet<FeatureId>,
    pub required_projections: BTreeSet<Projection>,
    pub read_order: Vec<String>,
    pub hazards: Vec<String>,
}
