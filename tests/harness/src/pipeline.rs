//! Source pipeline: parse → admit → eval. Hard by default.
//!
//! Stopping at "it typechecks" is fluff. A valid program with `tests:` must
//! evaluate those examples to `Passed`. A computed-without-expect run is not
//! a test. A symbolic fallback on a numeric example is a failure.

use crate::probe::Probe;
use crate::table::workspace_path;
use emath_core::limits::Limits;
use emath_core::Diagnostics;
use emath_exec_ir::runner::{run_package, TestVerdict};
use emath_sema::{CheckResult, CompilerSession};
use emath_syntax::parse_str;

/// Install the source parser and the authored language image on this thread.
///
/// Capsule admission is thread-local. Call once at the start of every test
/// that parses, admits, or evaluates `.emath`.
pub fn boot() {
    emath_syntax::install_source_parser();
    let root = workspace_path("language");
    let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
        .unwrap_or_else(|error| {
            panic!(
                "boot: language image failed to load from {}: {error:?}",
                root.display()
            )
        });
    emath_sema::language::install_language_distribution(&distribution).unwrap_or_else(|error| {
        panic!("boot: language distribution failed to install: {error:?}")
    });
}

/// One `.emath` source under a hard contract.
#[derive(Clone, Debug)]
pub struct Source {
    name: String,
    text: String,
}

impl Source {
    /// In-memory source.
    #[must_use]
    pub fn from_str(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
        }
    }

    /// Workspace-relative path.
    #[must_use]
    pub fn from_workspace(relative: &str) -> Self {
        Self {
            name: relative.to_string(),
            text: crate::table::workspace_file(relative),
        }
    }

    /// Display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Source bytes.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Parse. Does not treat errors as probe failures (use [`Self::must_admit`]
    /// or [`Self::must_refuse`]).
    pub fn parse(&self) -> emath_core::tree::SyntaxTree {
        parse_str(&self.text).0
    }

    /// Parse + admit without recording. Use the `must_*` methods to demand.
    pub fn check(&self) -> CheckResult {
        let mut session = CompilerSession::new(Limits::default());
        session.check_owned(&self.name, &self.text)
    }

    /// Demand admission with zero error diagnostics.
    pub fn must_admit(&self, probe: &mut Probe) -> CheckResult {
        let result = self.check();
        let errors: Vec<&str> = error_codes(&result.diagnostics);
        probe.demand(
            format!("{}:admit", self.name),
            errors.is_empty(),
            format!("must admit, errors {errors:?}"),
        );
        result
    }

    /// Demand the source is refused with every `codes` present as Error.
    /// Extra errors are allowed. Missing codes are failures.
    pub fn must_refuse(&self, probe: &mut Probe, codes: &[&str]) {
        let result = self.check();
        let errors = error_codes(&result.diagnostics);
        probe.demand(
            format!("{}:refused", self.name),
            result.diagnostics.has_errors(),
            format!("must refuse, got errors {errors:?}"),
        );
        for code in codes {
            probe.demand(
                format!("{}:code:{code}", self.name),
                errors.iter().any(|got| *got == *code),
                format!("must emit {code}, got {errors:?}"),
            );
        }
    }

    /// Admit, then run every `tests:` example. Each example with an `expect`
    /// must be [`TestVerdict::Passed`]. Computed / symbolic / refused are
    /// failures — they are how a compiler pretends a numeric program works.
    pub fn eval_tests(&self, probe: &mut Probe) {
        let result = self.must_admit(probe);
        if result.diagnostics.has_errors() {
            probe.fail(
                format!("{}:eval", self.name),
                "cannot evaluate a source that did not admit",
            );
            return;
        }
        let report = run_package(&result.package);
        let mut saw_expect = false;
        for declaration in &report.declarations {
            for test in &declaration.tests {
                match &test.verdict {
                    TestVerdict::Passed => {
                        saw_expect = true;
                        probe.demand(
                            format!("{}:{}/{}", self.name, declaration.name, test.name),
                            true,
                            "passed",
                        );
                    }
                    TestVerdict::Computed => {
                        if test.name == emath_exec_ir::runner::PANE_TEST_NAME {
                            continue;
                        }
                        probe.fail(
                            format!("{}:{}/{}", self.name, declaration.name, test.name),
                            "computed without expect — that is not a test",
                        );
                    }
                    other => {
                        saw_expect = true;
                        probe.fail(
                            format!("{}:{}/{}", self.name, declaration.name, test.name),
                            format!("expected Passed, got {other}"),
                        );
                    }
                }
            }
        }
        probe.demand(
            format!("{}:has-expect", self.name),
            saw_expect,
            "source has no tests: example with expect — admission-only is fluff",
        );
    }
}

/// Error diagnostic codes, source order.
#[must_use]
pub fn error_codes(diagnostics: &Diagnostics) -> Vec<&str> {
    diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect()
}
