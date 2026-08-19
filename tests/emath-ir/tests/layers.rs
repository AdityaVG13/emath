//! Ten-layer IR stack witness: unique layer names and unique versioned
//! schema ids, each binding its base and version explicitly.

use emath_ir::IrLayer;
use std::collections::BTreeSet;

#[test]
fn the_stack_has_ten_layers_with_unique_versioned_schemas() {
    assert_eq!(IrLayer::ALL.len(), 10);
    let names: BTreeSet<&str> = IrLayer::ALL.iter().map(|layer| layer.name()).collect();
    assert_eq!(names.len(), 10, "layer names must be unique");
    let schemas: BTreeSet<String> = IrLayer::ALL
        .iter()
        .map(|layer| layer.versioned_schema().0)
        .collect();
    assert_eq!(schemas.len(), 10, "versioned schema ids must be unique");
    for layer in IrLayer::ALL {
        assert_eq!(
            layer.versioned_schema().0,
            format!("{}.v{}", layer.schema_base(), layer.schema_version()),
            "versioned id binds base and version explicitly"
        );
        assert_eq!(layer.schema_version(), 1, "every layer starts at v1");
    }
}
