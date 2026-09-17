//! The single constructor-fault → language-diagnostic code mapping.
//!
//! Both admission lanes (the sema `check` pass and the CLI build lane)
//! formerly kept private, divergent copies: the same fault could surface
//! as different diagnostic codes depending on which lane reported it
//! (`type` was E-TYPE-010 on the CLI lane and E-TYPE-012 on the sema
//! lane; an E-SEC-101 constructor fault passed through on the CLI lane
//! and fell to the default on the sema lane). This pin holds the one
//! shared table every lane uses.

use emath_exec_ir::constructor_layer::constructor_admit_code;
use emath_test_harness::{Probe, boot};

#[test]
fn probe() {
    boot();
    let mut p = Probe::new(
        "constructor fault codes map to the documented language diagnostics, one table for every lane",
    );
    p.case("already_diagnostic_faults_pass_through", |p| {
        for code in [
            "E-KIND-GONE",
            "E-KIND-011",
            "E-SEC-101",
            "E-NAME-020",
            "E-PKG-050",
            "E-USE-ADMISSION",
            "E-TYPE-002",
            "E-TYPE-003",
            "E-TYPE-010",
        ] {
            p.eq(code, constructor_admit_code(code), code);
        }
    });
    p.case("unbound_names_refuse_e_type_002", |p| {
        for code in ["unbound", "unbound_code"] {
            p.eq(code, constructor_admit_code(code), "E-TYPE-002");
        }
    });
    p.case("unavailable_methods_refuse_e_type_003", |p| {
        for code in [
            "method_unavailable",
            "implementation_unavailable",
            "transformation_rule_unavailable",
            "unresolved",
            "stale_dependency",
        ] {
            p.eq(code, constructor_admit_code(code), "E-TYPE-003");
        }
    });
    p.case("carrier_value_faults_refuse_e_type_012", |p| {
        for code in [
            "type",
            "arity",
            "division_by_zero",
            "overflow",
            "non_finite_scalar",
            "invalid_index",
            "invalid_literal",
            "nonexhaustive_match",
            "invalid_code_construction",
            "object_invariant_failed",
            "unguarded_recursive_binding",
            "recursion_depth_exceeded",
            "incompatible_checkpoint",
        ] {
            p.eq(code, constructor_admit_code(code), "E-TYPE-012");
        }
    });
    p.case("unknown_future_faults_default_to_e_type_012", |p| {
        p.eq(
            "future_fault_slug",
            constructor_admit_code("future_fault_slug"),
            "E-TYPE-012",
        );
    });
    p.finish();
}
