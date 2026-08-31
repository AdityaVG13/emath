//! Arena identifiers. IDs are meaningful only together with the owning
//! `SemanticPackage`; they are never serialized as durable values.

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);

        impl $name {
            #[must_use]
            pub fn index(self) -> usize {
                usize::try_from(self.0).unwrap_or(usize::MAX)
            }
        }
    };
}

id_type!(DeclarationId);
id_type!(TypeId);
id_type!(ExprId);
id_type!(GoalId);
id_type!(TestId);
id_type!(PlanNodeId);
id_type!(EvidenceClaimId);
// Stable identity of one admitted capability cell. Cells are arena data on
// `SemanticPackage`; adding a cell never adds an `ExprNode` variant. IDs
// index the `capabilities` arena and, like every id here, are meaningful
// only with the owning package.
id_type!(CapabilityId);
