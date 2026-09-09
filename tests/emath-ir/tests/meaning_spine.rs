use std::str::FromStr;

use emath_core::{FeatureId, SemanticHash};
use emath_ir::{
    CapsuleProjection, CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity,
    MeaningEdge, MeaningEdgeKind, MeaningResource, MeaningSpine, MeaningSpineError,
    ProjectionDisposition,
};
use emath_test_harness::Probe;

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
fn intent() {
    let mut p = Probe::new("tests/emath-ir/tests/meaning_spine.rs");
    p.case("twelve_edge_kinds_load_as_one_hundred_fifty_seven_canonical_seeds", |p| {

    p.eq("twelve_edge_kinds_load_as_one_hundred_fifty_seven_canonical_seeds#1", MeaningEdgeKind::ALL.len(), 12);
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
    p.eq("twelve_edge_kinds_load_as_one_hundred_fifty_seven_canonical_seeds#2", first, second);
    p.eq("twelve_edge_kinds_load_as_one_hundred_fifty_seven_canonical_seeds#3", graph.canonical_edges().len(), 157);

    });
    p.case("endpoint_cycle_duplicate_and_resource_boundaries_refuse", |p| {

    let mut graph = graph();
    let dep = edge(
        "std.capability.math.add",
        MeaningEdgeKind::DependsOn,
        feature("std.type.int"),
    );
    graph.insert(dep.clone()).unwrap();
    p.demand("endpoint_cycle_duplicate_and_resource_boundaries_refuse#1", matches!(
        graph.insert(dep),
        Err(MeaningSpineError::Duplicate(_))
    ), "endpoint_cycle_duplicate_and_resource_boundaries_refuse#1: matches!(\n        graph.insert(dep),\n        Err(MeaningSpineError::Duplicate(_))\n    )");

    let reverse = edge(
        "std.type.int",
        MeaningEdgeKind::DependsOn,
        feature("std.capability.math.add"),
    );
    p.demand("endpoint_cycle_duplicate_and_resource_boundaries_refuse#2", matches!(
        graph.insert(reverse),
        Err(MeaningSpineError::Cycle { .. })
    ), "endpoint_cycle_duplicate_and_resource_boundaries_refuse#2: matches!(\n        graph.insert(reverse),\n        Err(MeaningSpineError::Cycle { .. })\n    )");

    p.demand("endpoint_cycle_duplicate_and_resource_boundaries_refuse#3", matches!(
        graph.insert(edge(
            "std.capability.math.add",
            MeaningEdgeKind::RequiresWorld,
            feature("std.type.int")
        )),
        Err(MeaningSpineError::EndpointMismatch { .. })
    ), "endpoint_cycle_duplicate_and_resource_boundaries_refuse#3: matches!(\n        graph.insert(edge(\n            \"std.capability.math.add\",\n            MeaningEdgeK");
    p.demand("endpoint_cycle_duplicate_and_resource_boundaries_refuse#4", matches!(
        MeaningResource::parse("file://tmp/add"),
        Err(MeaningSpineError::AmbiguousResource(_))
    ), "endpoint_cycle_duplicate_and_resource_boundaries_refuse#4: matches!(\n        MeaningResource::parse(\"file://tmp/add\"),\n        Err(MeaningSpineError::Ambiguous");
    p.demand("endpoint_cycle_duplicate_and_resource_boundaries_refuse#5", matches!(
        graph.insert(edge(
            "std.capability.math.add",
            MeaningEdgeKind::ConformsTo,
            feature("std.diagnostic.missing")
        )),
        Err(MeaningSpineError::Unresolved(_))
    ), "endpoint_cycle_duplicate_and_resource_boundaries_refuse#5: matches!(\n        graph.insert(edge(\n            \"std.capability.math.add\",\n            MeaningEdgeK");

    });
    p.case("closures_and_reverse_impact_are_exact_and_sorted", |p| {

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

    p.eq("closures_and_reverse_impact_are_exact_and_sorted#1", graph.transitive_build_dependencies(&id("std.capability.math.add")), vec![
            feature("std.type.int"),
            feature("std.world.exact.int"),
            MeaningResource::parse("ir://vm/add").unwrap(),
        ]);
    p.eq("closures_and_reverse_impact_are_exact_and_sorted#2", graph.reverse_impact(&feature("std.capability.math.add")), vec![
            MeaningResource::parse("ir://runtime/table/add").unwrap(),
            MeaningResource::parse("test://conformance/add-exact").unwrap(),
            MeaningResource::parse("doc://reference/math/add").unwrap(),
        ]);

    });
    p.case("minimum_agent_context_contains_only_owned_edit_information", |p| {

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
    p.demand("minimum_agent_context_contains_only_owned_edit_information#1", context.owner_contract == "language/spec/capabilities/add.emath", format!("expected {:?}, got {:?}", "language/spec/capabilities/add.emath", context.owner_contract));
    p.demand("minimum_agent_context_contains_only_owned_edit_information#2", context.hazards == "exactness", format!("expected {:?}, got {:?}", "exactness", context.hazards));
    p.eq("minimum_agent_context_contains_only_owned_edit_information#3", context.conformance, vec![MeaningResource::parse("test://conformance/add-exact").unwrap()]);

    });
    p.finish();
}







