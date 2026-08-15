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
pub mod v6;

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
    fn external_file_imports_are_refused() {
        // Library-path imports are admitted by the front-end; file-style
        // paths (`./x.emath`) are refused by the lexer/parser (E-SYN),
        // and the front-end keeps E-PKG-050 as defense-in-depth for any
        // file-like path that reaches it.
        let bad = "use ./local.emath\nemath custom <X> as function:\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n";
        let result = check(bad);
        assert!(
            result
                .diagnostics
                .items()
                .iter()
                .any(|d| d.code == "E-SYN-110" || d.code == "E-PKG-050"),
            "file-style import must be refused, got {:?}",
            result
                .diagnostics
                .items()
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn v6_file_front_end_records_package_and_imports() {
        let source = "package examples.square\nuse std.numeric.Real\nuse std.units.{Millisecond, Second}\n\nemath function Square:\n    input:\n        x: Real\n\n    output:\n        y: Real\n\n    define:\n        y = x * x\n";
        let result = check(source);
        assert!(
            !result.diagnostics.has_errors(),
            "unexpected diagnostics: {:?}",
            result.diagnostics.items()
        );
        let package = &result.package;
        assert_eq!(
            package.package_path.as_deref(),
            Some(&["examples".to_string(), "square".to_string()][..])
        );
        assert_eq!(package.imports.len(), 2);
        assert_eq!(package.imports[0].path, ["std", "numeric"]);
        assert_eq!(package.declarations.len(), 1);
        assert_eq!(package.declarations[0].kind_label, "function");
    }

    #[test]
    fn v6_imports_are_deterministic_in_identity() {
        let a = "package p.a\nuse std.units.{Mebibyte, Millisecond}\nemath record R:\n    state:\n        x: Millisecond\n";
        let b = "package p.a\nuse std.units.{Millisecond, Mebibyte}\nemath record R:\n    state:\n        x: Millisecond\n";
        let result_a = check(a);
        let result_b = check(b);
        assert!(!result_a.diagnostics.has_errors());
        assert!(!result_b.diagnostics.has_errors());
        assert_eq!(result_a.package.content_id(), result_b.package.content_id());
    }

    #[test]
    fn v6_unknown_kind_is_refused() {
        let bad = "package p.q\nemath unknown_kind Thing:\n    input:\n        x: Real\n";
        let result = check(bad);
        assert!(result
            .diagnostics
            .items()
            .iter()
            .any(|d| d.code == "E-KIND-001"));
    }

    #[test]
    fn v6_unknown_section_is_refused() {
        let bad = "package p.q\nemath function F:\n    input:\n        x: Real\n    telemetry:\n        y = 1\n";
        let result = check(bad);
        assert!(result
            .diagnostics
            .items()
            .iter()
            .any(|d| d.code == "E-SYN-101" && d.message.contains("telemetry")));
    }

    #[test]
    fn v6_kind_application_enforces_schema() {
        let good = "emath kind K:\n    schema:\n        require section input\n        require exactly_one output\n\nemath K App:\n    input:\n        x: Real\n\n    output:\n        y: Real\n";
        let result = check(good);
        assert!(
            !result.diagnostics.has_errors(),
            "unexpected diagnostics: {:?}",
            result.diagnostics.items()
        );
        let bad = "emath kind K:\n    schema:\n        require section input\n        require exactly_one output\n\nemath K App:\n    output:\n        y: Real\n";
        let result = check(bad);
        assert!(result
            .diagnostics
            .items()
            .iter()
            .any(|d| d.code == "E-KIND-003" && d.message.contains("input")));
    }
}
