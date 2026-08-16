//! Canonical content identity negative tests (bug-hunt ``).
//!
//! Two packages that differ only in the type of a single declaration input
//! must produce distinct content identity. On the degenerated identity the
//! field loop dropped name and type, so both packages hashed equal and
//! shared one `source_package` in build artifacts.

#[cfg(test)]
mod canonical_identity {
    use emath_core::{QualifiedName, Span};
    use emath_ir::canonical::canonical_package;
    use emath_ir::constructor::{Field, Visibility};
    use emath_ir::goal::CompileSpec;
    use emath_ir::ids::DeclarationId;
    use emath_ir::package::{Declaration, SemanticPackage};
    use emath_ir::types::TypeNode;
    use std::collections::BTreeMap;

    fn package_with_input(ty: TypeNode) -> SemanticPackage {
        let mut package = SemanticPackage::new();
        let ty_id = package.push_type(ty);
        package.declarations.push(Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("model"),
            kind: QualifiedName::single("model"),
            kind_label: String::new(),
            inputs: vec![Field {
                name: "x".to_string(),
                ty: ty_id,
                visibility: Visibility::Public,
                source: Span::default(),
            }],
            outputs: Vec::new(),
            state: Vec::new(),
            constructors: Vec::new(),
            definitions: BTreeMap::new(),
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: Vec::new(),
            exports: Vec::new(),
            compile_spec: CompileSpec::default(),
            source: Span::default(),
        });
        package
    }

    #[test]
    fn distinct_field_types_produce_distinct_content_ids() {
        let float_id = canonical_package(&package_with_input(TypeNode::Float64));
        let bool_id = canonical_package(&package_with_input(TypeNode::Bool));
        assert_ne!(
            float_id, bool_id,
            "canonical identity must bind field types"
        );
    }

    #[test]
    fn same_field_types_produce_same_content_id() {
        let left = canonical_package(&package_with_input(TypeNode::Float64));
        let right = canonical_package(&package_with_input(TypeNode::Float64));
        assert_eq!(left, right, "canonical identity must be deterministic");
    }
}
