//! Construction-obligation witnesses: every textual obligation classifies
//! as runtime, receipt composition never drops an obligation, receipt
//! identity is deterministic and content-bound.

use emath_core::Span;
use emath_ir::{
    Constructor, ConstructionReceipt, ExprId, ObligationClass, ObligationKind,
};
use std::collections::BTreeMap;

fn constructor(preconditions: &[u32], postconditions: &[u32]) -> Constructor {
    Constructor {
        name: "new".to_string(),
        parameters: vec![],
        preconditions: preconditions.iter().map(|id| ExprId(*id)).collect(),
        assignments: BTreeMap::new(),
        postconditions: postconditions.iter().map(|id| ExprId(*id)).collect(),
        defaults: BTreeMap::new(),
        error_type: None,
        is_public: true,
        source: Span::default(),
    }
}

#[test]
fn obligation_matrix_classifies_every_textual_obligation_as_runtime() {
    let matrix = constructor(&[1, 2], &[3]).obligation_matrix();
    assert_eq!(matrix.len(), 3);
    assert!(
        matrix
            .iter()
            .all(|obligation| obligation.class == ObligationClass::Runtime)
    );
    assert_eq!(matrix[0].kind, ObligationKind::Precondition);
    assert_eq!(matrix[2].kind, ObligationKind::Postcondition);
}

#[test]
fn receipt_composition_never_drops_an_obligation() {
    let delegating = constructor(&[1], &[]).receipt("Outer");
    let delegate = constructor(&[2], &[3]).receipt("Inner");
    let composed = ConstructionReceipt::compose(&delegating, &delegate);
    assert_eq!(composed.obligations.len(), 3);
    // Delegate obligations run first at runtime.
    assert_eq!(composed.obligations[0].expression, ExprId(2));
    assert_eq!(composed.obligations[2].expression, ExprId(1));
    assert_eq!(composed.declaration, "Outer");
}

#[test]
fn receipt_identity_is_deterministic_and_content_bound() {
    let receipt = constructor(&[1], &[2]).receipt("Scorer");
    assert_eq!(receipt.identity(), receipt.identity());
    let different = constructor(&[1], &[9]).receipt("Scorer");
    assert_ne!(receipt.identity(), different.identity());
    // Only deferred obligations remain open; runtime ones are discharged.
    assert!(receipt.open_obligations().is_empty());
}
