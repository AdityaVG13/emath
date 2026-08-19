//! Capability census and backend-selection tests.
//!
//! Moved from #[cfg(test)] in crates/emath-adapter-dew/src/capability.rs.

use emath_adapter_dew::{Backend, provide_capability, select_backend};

#[test]
fn jit_backend_not_in_advertised_inventory() {
    let capability = provide_capability();
    let error = select_backend(&capability, Backend::JitCranelift).unwrap_err();
    assert!(error.contains("E-PROV-031"), "got {error:?}");
}

#[test]
fn census_does_not_claim_linear_operators_for_scalar_backends() {
    // The advertised operators must all be served by the advertised
    // backends; the linear-algebra families exist as mapping APIs
    // only (`map_linear`) and are listed under the no-claim boundary,
    // never in the operators inventory.
    let capability = provide_capability();
    for operator in &capability.operators {
        assert!(
            !matches!(operator.as_str(), "dot" | "matvec" | "scale" | "matadd"),
            "linear operator `{operator}` must not be claimed by scalar backends"
        );
    }
    assert!(
        capability
            .no_claim
            .unimplemented_operators
            .contains(&"dot".to_string()),
        "unimplemented linear operators must be disclosed in the no-claim boundary"
    );
    assert_eq!(
        capability.domains,
        vec!["scalar-strict-f64".to_string()],
        "only the served domain is claimed"
    );
}
