use std::str::FromStr;

use emath_core::{FeatureId, SemanticHash};
use emath_ir::{
    CapsuleProjection, CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity,
    MeaningEdge, MeaningEdgeKind, MeaningResource, MeaningSpine, MeaningSpineError,
    ProjectionDisposition,
};

fn id(value: &str) -> FeatureId {
    FeatureId::from_str(value).unwrap()
}

fn feature(value: &str) -> MeaningResource {
    MeaningResource::Feature(id(value))
}

fn edge(source: &str, kind: MeaningEdgeKind, target: MeaningResource) -> MeaningEdge {
    MeaningEdge {
        source: feature(source),
        kind,
        target,
    }
}

fn graph() -> MeaningSpine {
    let mut graph = MeaningSpine::default();
    for (name, class) in [
        ("std.capability.math.add", FeatureClass::Capability),
        ("std.type.int", FeatureClass::Type),
        ("std.world.exact.int", FeatureClass::World),
        ("std.provider.reference", FeatureClass::Provider),
        ("std.artifact.value", FeatureClass::Artifact),
        ("std.migration.math.add", FeatureClass::Migration),
    ] {
        graph.register_feature(id(name), class);
    }
    for resource in [
        "ir://runtime/table/add",
        "ir://vm/add",
        "test://conformance/add-exact",
        "doc://reference/math/add",
    ] {
        graph
            .register_external(MeaningResource::parse(resource).unwrap())
            .unwrap();
    }
    graph
}

#[test]
fn twelve_edge_kinds_load_as_one_hundred_fifty_seven_canonical_seeds() {
    assert_eq!(MeaningEdgeKind::ALL.len(), 12);
    let mut graph = graph();
    let sources = [
        "std.capability.math.add",
        "std.type.int",
        "std.world.exact.int",
        "std.provider.reference",
        "std.artifact.value",
        "std.migration.math.add",
    ];
    for ordinal in 0..157 {
        let source = sources[ordinal % sources.len()];
        let target = format!("doc://seed/{ordinal:03}");
        let resource = MeaningResource::parse(&target).unwrap();
        graph.register_external(resource.clone()).unwrap();
        graph
            .insert(edge(source, MeaningEdgeKind::Documents, resource))
            .unwrap();
    }
    let first = graph.canonical();
    let second = graph.canonical();
    assert_eq!(first, second);
    assert_eq!(graph.canonical_edges().len(), 157);
}

#[test]
fn endpoint_cycle_duplicate_and_resource_boundaries_refuse() {
    let mut graph = graph();
    let dep = edge(
        "std.capability.math.add",
        MeaningEdgeKind::DependsOn,
        feature("std.type.int"),
    );
    graph.insert(dep.clone()).unwrap();
    assert!(matches!(
        graph.insert(dep),
        Err(MeaningSpineError::Duplicate(_))
    ));

    let reverse = edge(
        "std.type.int",
        MeaningEdgeKind::DependsOn,
        feature("std.capability.math.add"),
    );
    assert!(matches!(
        graph.insert(reverse),
        Err(MeaningSpineError::Cycle { .. })
    ));

    assert!(matches!(
        graph.insert(edge(
            "std.capability.math.add",
            MeaningEdgeKind::RequiresWorld,
            feature("std.type.int")
        )),
        Err(MeaningSpineError::EndpointMismatch { .. })
    ));
    assert!(matches!(
        MeaningResource::parse("file://tmp/add"),
        Err(MeaningSpineError::AmbiguousResource(_))
    ));
    assert!(matches!(
        graph.insert(edge(
            "std.capability.math.add",
            MeaningEdgeKind::ConformsTo,
            feature("std.diagnostic.missing")
        )),
        Err(MeaningSpineError::Unresolved(_))
    ));
}

#[test]
fn closures_and_reverse_impact_are_exact_and_sorted() {
    let mut graph = graph();
    graph
        .insert(edge(
            "std.capability.math.add",
            MeaningEdgeKind::DependsOn,
            feature("std.type.int"),
        ))
        .unwrap();
    graph
        .insert(edge(
            "std.capability.math.add",
            MeaningEdgeKind::RequiresWorld,
            feature("std.world.exact.int"),
        ))
        .unwrap();
    graph
        .insert(edge(
            "std.capability.math.add",
            MeaningEdgeKind::Implements,
            MeaningResource::parse("ir://vm/add").unwrap(),
        ))
        .unwrap();
    graph
        .insert(edge(
            "std.capability.math.add",
            MeaningEdgeKind::ConformsTo,
            MeaningResource::parse("test://conformance/add-exact").unwrap(),
        ))
        .unwrap();
    graph
        .insert(edge(
            "std.capability.math.add",
            MeaningEdgeKind::Documents,
            MeaningResource::parse("doc://reference/math/add").unwrap(),
        ))
        .unwrap();
    graph
        .insert(edge(
            "std.capability.math.add",
            MeaningEdgeKind::ProjectsTo,
            MeaningResource::parse("ir://runtime/table/add").unwrap(),
        ))
        .unwrap();

    assert_eq!(
        graph.transitive_build_dependencies(&id("std.capability.math.add")),
        vec![
            feature("std.type.int"),
            feature("std.world.exact.int"),
            MeaningResource::parse("ir://vm/add").unwrap(),
        ]
    );
    assert_eq!(
        graph.reverse_impact(&feature("std.capability.math.add")),
        vec![
            MeaningResource::parse("ir://runtime/table/add").unwrap(),
            MeaningResource::parse("test://conformance/add-exact").unwrap(),
            MeaningResource::parse("doc://reference/math/add").unwrap(),
        ]
    );
}

#[test]
fn minimum_agent_context_contains_only_owned_edit_information() {
    let mut graph = graph();
    graph
        .insert(edge(
            "std.capability.math.add",
            MeaningEdgeKind::ConformsTo,
            MeaningResource::parse("test://conformance/add-exact").unwrap(),
        ))
        .unwrap();
    let capsule = FeatureCapsule {
        schema: FEATURE_CAPSULE_SCHEMA.to_string(),
        feature_id: id("std.capability.math.add"),
        semantic_hash: SemanticHash::from_str(&format!("sha256:{}", "0".repeat(64))).unwrap(),
        class: FeatureClass::Capability,
        maturity: Maturity::Proposed,
        summary: "add".to_string(),
        source: "catalog.add".to_string(),
        edges: vec![],
        slots: [(
            "agent".to_string(),
            CapsuleSlot::Value(
                "owners=language/spec/capabilities/add.emath;hazards=exactness".to_string(),
            ),
        )]
        .into(),
        projections: vec![CapsuleProjection {
            name: "semantics".to_string(),
            disposition: ProjectionDisposition::Required,
        }],
    };
    let context = graph.minimum_agent_context(&capsule);
    assert_eq!(
        context.owner_contract,
        "language/spec/capabilities/add.emath"
    );
    assert_eq!(context.hazards, "exactness");
    assert_eq!(
        context.conformance,
        vec![MeaningResource::parse("test://conformance/add-exact").unwrap()]
    );
}
