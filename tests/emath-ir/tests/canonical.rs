//! Canonical content identity must discriminate record and opaque node
//! kinds that share a display name (`m`); the field-type discrimination
//! cases live in `src/lib.rs`.

#[cfg(test)]
mod canonical_identity {
    use emath_core::{ContentId, QualifiedName, Span};
    use emath_ir::canonical::canonical_package;
    use emath_ir::constructor::{Field, Visibility};
    use emath_ir::goal::CompileSpec;
    use emath_ir::ids::DeclarationId;
    use emath_ir::package::{Declaration, SemanticPackage};
    use emath_ir::types::TypeNode;
    use std::collections::BTreeMap;

    fn package_with_input(ty: TypeNode) -> ContentId {
        let mut package = SemanticPackage::new();
        let ty_id = package.push_type(ty);
        package.declarations.push(Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("linear"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
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
        canonical_package(&package)
    }

    #[test]
    fn record_and_opaque_do_not_collide_in_identity() {
        // display_name() renders both as `m`; structural identity must
        // still discriminate the node kinds.
        let record = package_with_input(TypeNode::Record(QualifiedName::single("m")));
        let opaque = package_with_input(TypeNode::Opaque {
            name: QualifiedName::single("m"),
            provider_contract: None,
        });
        assert_ne!(record, opaque);
    }
}
