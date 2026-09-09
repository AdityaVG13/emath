//! Ten-layer IR stack witness: unique layer names and unique versioned
//! schema ids, each binding its base and version explicitly.

use emath_ir::IrLayer;
use std::collections::BTreeSet;
use emath_test_harness::Probe;

#[test]
fn intent() {
    let mut p = Probe::new("Ten-layer IR stack witness: unique layer names and unique versioned");
    p.case("the_stack_has_ten_layers_with_unique_versioned_schemas", |p| {
    p.eq("the_stack_has_ten_layers_with_unique_versioned_schemas#1", IrLayer::ALL.len(), 10);
    let names: BTreeSet<&str> = IrLayer::ALL.iter().map(|layer| layer.name()).collect();
    p.eq("layer names must be unique", names.len(), 10);
    let schemas: BTreeSet<String> = IrLayer::ALL
        .iter()
        .map(|layer| layer.versioned_schema().0)
        .collect();
    p.eq("versioned schema ids must be unique", schemas.len(), 10);
    for layer in IrLayer::ALL {
        p.eq("versioned id binds base and version explicitly", layer.versioned_schema().0, format!("{}.v{}", layer.schema_base(), layer.schema_version()));
        p.eq("every layer starts at v1", layer.schema_version(), 1);
    }

    });
    p.finish();
}

