#![forbid(unsafe_code)]

use emath_language_model::{FeatureCapsule, FeatureClass, FeatureId, Maturity, Projection};
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    let feature = FeatureCapsule {
        id: FeatureId("std.kind.cipher@1".into()),
        class: FeatureClass::Kind,
        maturity: Maturity::Proposed,
        summary: "schema-defined cipher declaration".into(),
        dependencies: BTreeSet::new(),
        required_projections: BTreeSet::from([
            Projection::Surface,
            Projection::Lowering,
            Projection::Conformance,
            Projection::AgentView,
        ]),
        metadata: BTreeMap::new(),
    };
    let present = BTreeSet::from([Projection::Surface, Projection::Lowering]);
    let missing = feature.missing_projections(&present);
    assert!(missing.contains(&Projection::Conformance));
    assert!(missing.contains(&Projection::AgentView));
    println!("projection closure detects incomplete realization");
}
