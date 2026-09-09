//! Capability census and backend-selection tests.
use emath_adapter_dew::{Backend, provide_capability, select_backend};
use emath_test_harness::Probe;

#[test]
fn probe() {
    let mut p = Probe::new("dew capability census claims only the served scalar domain and refuses jit");
    match select_backend(&provide_capability(), Backend::JitCranelift) {
        Ok(b) => p.fail("jit-refused", format!("jit must refuse, got {b:?}")),
        Err(e) => p.contains("jit-refused/code", &e, "E-PROV-031"),
    };
    let cap = provide_capability();
    p.case("no-linear-claim", |p| {
        for op in &cap.operators {
            p.demand(format!("op/{op}"), !matches!(op.as_str(), "dot" | "matvec" | "scale" | "matadd"), format!("linear {op} must not be claimed"));
        }
    });
    p.demand("no-claim/dot", cap.no_claim.unimplemented_operators.contains(&"dot".to_string()), "dot must be disclosed");
    p.eq("domains", cap.domains, vec!["scalar-strict-f64".to_string()]);
    p.finish();
}
