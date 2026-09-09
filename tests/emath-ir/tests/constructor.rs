//! Construction-obligation witnesses: every textual obligation classifies
//! as runtime, receipt composition never drops an obligation, receipt
//! identity is deterministic and content-bound.

use emath_core::Span;
use emath_ir::{ConstructionReceipt, Constructor, ExprId, ObligationClass, ObligationKind};
use std::collections::BTreeMap;
use emath_test_harness::Probe;

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
fn intent() {
    let mut p = Probe::new("Construction-obligation witnesses: every textual obligation classifies");
    p.case("obligation_matrix_classifies_every_textual_obligation_as_runtime", |p| {
    let matrix = constructor(&[1, 2], &[3]).obligation_matrix();
    p.eq("obligation_matrix_classifies_every_textual_obligation_as_runtime#1", matrix.len(), 3);
    p.demand("obligation_matrix_classifies_every_textual_obligation_as_runtime#2", matrix
            .iter()
            .all(|obligation| obligation.class == ObligationClass::Runtime), "obligation_matrix_classifies_every_textual_obligation_as_runtime#2: matrix\n            .iter()\n            .all(|obligation| obligation.class == ObligationClass::Runtim");
    p.eq("obligation_matrix_classifies_every_textual_obligation_as_runtime#3", matrix[0].kind, ObligationKind::Precondition);
    p.eq("obligation_matrix_classifies_every_textual_obligation_as_runtime#4", matrix[2].kind, ObligationKind::Postcondition);

    });
    p.case("receipt_composition_never_drops_an_obligation", |p| {
    let delegating = constructor(&[1], &[]).receipt("Outer");
    let delegate = constructor(&[2], &[3]).receipt("Inner");
    let composed = ConstructionReceipt::compose(&delegating, &delegate);
    p.eq("receipt_composition_never_drops_an_obligation#1", composed.obligations.len(), 3);
    // Delegate obligations run first at runtime.
    p.eq("receipt_composition_never_drops_an_obligation#2", composed.obligations[0].expression, ExprId(2));
    p.eq("receipt_composition_never_drops_an_obligation#3", composed.obligations[2].expression, ExprId(1));
    p.demand("receipt_composition_never_drops_an_obligation#4", composed.declaration == "Outer", "receipt declaration stays Outer");

    });
    p.case("receipt_identity_is_deterministic_and_content_bound", |p| {
    let receipt = constructor(&[1], &[2]).receipt("Scorer");
    p.eq("receipt_identity_is_deterministic_and_content_bound#1", receipt.identity(), receipt.identity());
    let different = constructor(&[1], &[9]).receipt("Scorer");
    p.ne("receipt_identity_is_deterministic_and_content_bound#2", receipt.identity(), different.identity());
    // Only deferred obligations remain open; runtime ones are discharged.
    p.demand("receipt_identity_is_deterministic_and_content_bound#3", receipt.open_obligations().is_empty(), "receipt_identity_is_deterministic_and_content_bound#3: receipt.open_obligations().is_empty()");

    });
    p.finish();
}





