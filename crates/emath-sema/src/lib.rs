//! Semantic admission (Phase 1): syntax tree → typed neutral SIR.
//!
//! Orchestrates field checks, constructor/invariant admission, definition
//! typing, goal elaboration and plan construction, mirroring the frozen
//! `CompilerSession` surface from `implementation/PUBLIC_API_INVENTORY.md`.
//! Everything outside the Phase 1 subset receives a typed capability
//! refusal; nothing is silently dropped.

#![forbid(unsafe_code)]

pub mod admit;
pub mod session;

pub use admit::{CheckResult, SemanticTrace, TraceEntry};
pub use session::{
    CompilerPolicy, CompilerSession, EmittedAnchor, GeneratedCrate, PlanResult, SourcePackage,
};

#[cfg(test)]
mod tests {
    use super::*;
    use emath_core::limits::Limits;

    const AFFINE: &str = include_str!("../../../implementation/tests/valid/stateful.emath");
    const MINIMAL: &str = include_str!("../../../implementation/tests/valid/minimal.emath");

    fn check(text: &str) -> CheckResult {
        let mut session = CompilerSession::new(Limits::default());
        session.check_owned("test.emath", text)
    }

    fn plan(text: &str) -> PlanResult {
        let mut session = CompilerSession::new(Limits::default());
        let file = session.load_text("test.emath", text.to_string());
        session.plan(file)
    }

    #[test]
    fn affine_policy_admits() {
        // Goal attachment happens in `plan`; `check` returns the admitted
        // package and diagnostics without adding GIR.
        let result = plan(AFFINE);
        assert!(
            !result.diagnostics.has_errors(),
            "unexpected diagnostics: {:?}",
            result.diagnostics.items()
        );
        let package = &result.package;
        assert_eq!(package.declarations.len(), 1);
        let decl = &package.declarations[0];
        assert_eq!(decl.name.leaf(), "AffinePolicy");
        assert_eq!(decl.constructors.len(), 1);
        assert_eq!(decl.constructors[0].preconditions.len(), 3);
        assert_eq!(decl.definitions.len(), 1);
        assert!(decl.definitions.contains_key("score"));
        assert_eq!(decl.goals.len(), 1);
        assert_eq!(package.goals.len(), 1);
        assert_eq!(decl.exports.len(), 2);
        // identity is sealed and deterministic
        let again = plan(AFFINE);
        assert_eq!(package.content_id(), again.package.content_id());
    }

    #[test]
    fn square_function_admits_with_test() {
        let result = check(MINIMAL);
        assert!(
            !result.diagnostics.has_errors(),
            "unexpected diagnostics: {:?}",
            result.diagnostics.items()
        );
        let package = &result.package;
        assert_eq!(package.tests.len(), 1);
        assert_eq!(package.tests[0].name, "three_squared");
    }

    #[test]
    fn unknown_output_variable_is_rejected() {
        let bad = "emath custom <Bad> as function:\n    outputs:\n        y: Float64\n    definitions:\n        y = mystery + 1\n";
        let result = check(bad);
        assert!(result
            .diagnostics
            .items()
            .iter()
            .any(|d| d.code == "E-TYPE-002"));
    }

    #[test]
    fn duplicate_output_field_is_rejected() {
        let bad = "emath custom <Bad> as function:\n    outputs:\n        y: Float64\n        y: Float64\n    definitions:\n        y = 1\n";
        let result = check(bad);
        assert!(result
            .diagnostics
            .items()
            .iter()
            .any(|d| d.code == "E-NAME-020"));
    }

    #[test]
    fn missing_state_assignment_is_rejected() {
        let bad = "emath custom <Bad> as policy:\n    state:\n        scale: Float64\n    constructors:\n        public fn new() -> Self:\n            Self:\n";
        let result = check(bad);
        assert!(result
            .diagnostics
            .items()
            .iter()
            .any(|d| d.code == "E-CTOR-030"));
    }

    #[test]
    fn kinds_outside_subset_are_refused() {
        let bad = "emath custom <X> as pde_model:\n    outputs:\n        t: Float64\n";
        let result = check(bad);
        assert!(result
            .diagnostics
            .items()
            .iter()
            .any(|d| d.code == "E-KIND-100"));
    }

    #[test]
    fn imports_are_refused_in_phase1() {
        let bad = "use core::math::*\nemath custom <X> as function:\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n";
        let result = check(bad);
        assert!(result
            .diagnostics
            .items()
            .iter()
            .any(|d| d.code == "E-PKG-050"));
    }
}
