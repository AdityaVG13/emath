//! Shared test harness for every emath test package.
//!
//! One dependency, three tools:
//! - **Table-driven checks** ([`check_all`], [`check_all_close`]): one test
//!   function exercises many inputs — and, via closures, many functions —
//!   against their expected results. Every failure is collected and reported
//!   in one message; nothing stops at the first mismatch.
//! - **Float comparison** ([`Close`]): tolerance-based equality with a
//!   message that shows both sides and the gap.
//! - **Workspace fixtures** ([`workspace_path`], [`workspace_file`]): paths
//!   into the repository, resolved from the calling package's manifest dir.
//!
//! Tests in this repository are INTENT tests: they state what behavior must
//! hold, exercise the public API, and are written to fail before the code
//! that satisfies them exists. A test that cannot fail is deleted, not
//! kept. Prefer one table-driven check over ten near-identical micro-tests.

#![forbid(unsafe_code)]

use std::fmt::Debug;
use std::path::{Path, PathBuf};

/// One row of a table-driven check: a name, an input, the expected result.
pub struct Case<I, O> {
    /// Stable name reported on failure (function or scenario under test).
    pub name: &'static str,
    /// Input handed to the checked function.
    pub input: I,
    /// The result the function must produce.
    pub expected: O,
}

impl<I, O> Case<I, O> {
    /// Shorthand constructor keeping table literals readable.
    pub fn new(name: &'static str, input: I, expected: O) -> Self {
        Self {
            name,
            input,
            expected,
        }
    }
}

/// Run every case through `f`, collecting ALL failures.
///
/// This is the workhorse for "many functions, one intent": pass a closure
/// that dispatches to the function under test and list one case per
/// behavior. Returns `Err` with every failing case named and diffed; an
/// all-green run returns `Ok(())` so callers can add context.
///
/// # Errors
/// One aggregated message listing every failing case (never just the
/// first), formatted for direct use in `assert!`.
pub fn check_all<I, O, F>(cases: &[Case<I, O>], f: F) -> Result<(), String>
where
    I: Debug,
    O: Debug + PartialEq,
    F: Fn(&I) -> O,
{
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|case| {
            let actual = f(&case.input);
            if actual == case.expected {
                None
            } else {
                Some(format!(
                    "[{}] input {input:?}\n  expected: {expected:?}\n  actual:   {actual:?}",
                    case.name,
                    input = case.input,
                    expected = case.expected,
                    actual = actual,
                ))
            }
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} of {} cases failed:\n{}",
            failures.len(),
            cases.len(),
            failures.join("\n"),
        ))
    }
}

/// One row of a tolerance-based table check.
pub struct CloseCase<I> {
    /// Stable name reported on failure.
    pub name: &'static str,
    /// Input handed to the checked function.
    pub input: I,
    /// The value the function must produce within `tol`.
    pub expected: f64,
    /// Maximum absolute deviation from `expected`.
    pub tol: f64,
}

/// Run every case through a scalar-producing `f`, collecting ALL failures.
///
/// The tolerance comparison uses [`Close`]; failures report both sides and
/// the gap.
///
/// # Errors
/// One aggregated message listing every failing case.
pub fn check_all_close<I, F>(cases: &[CloseCase<I>], f: F) -> Result<(), String>
where
    I: Debug,
    F: Fn(&I) -> f64,
{
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|case| {
            let actual = f(&case.input);
            if actual.is_close(case.expected, case.tol) {
                None
            } else {
                Some(format!(
                    "[{}] input {input:?}\n  expected {expected} (tol {tol})\n  actual   {actual} (gap {gap})",
                    case.name,
                    input = case.input,
                    expected = case.expected,
                    tol = case.tol,
                    gap = (actual - case.expected).abs(),
                ))
            }
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} of {} cases failed:\n{}",
            failures.len(),
            cases.len(),
            failures.join("\n"),
        ))
    }
}

/// Tolerance-based comparison for floats, NaN-aware by rejection.
pub trait Close {
    /// True when `|self - other| <= tol` and both sides are finite.
    /// NaN is never close to anything (including itself) — an unexpected
    /// NaN must fail loudly, not silently compare equal.
    #[must_use]
    fn is_close(self, other: f64, tol: f64) -> bool;
}

impl Close for f64 {
    fn is_close(self, other: f64, tol: f64) -> bool {
        if self.is_nan() || other.is_nan() || !self.is_finite() || !other.is_finite() {
            return false;
        }
        (self - other).abs() <= tol
    }
}

/// Panic helper for the table runners: turn the aggregated `Err` into a
/// test failure at the call site (keeps the failure line in the test).
#[track_caller]
pub fn expect_ok(result: Result<(), String>) {
    if let Err(message) = result {
        panic!("{message}");
    }
}

/// Path into the repository, resolved from the calling test package.
///
/// Integration tests run with the package directory as cwd; tests reach
/// shared fixtures and `language/` examples through this helper instead of
/// hard-coded relative walks.
#[must_use]
pub fn workspace_path(relative: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .expect("test packages live two levels below the workspace root")
        .join(relative)
}

/// Whole contents of a workspace-relative file.
///
/// # Panics
/// When the file does not exist — a missing fixture is a test-authoring
/// error, not a runtime condition to handle.
#[must_use]
pub fn workspace_file(relative: &str) -> String {
    let path = workspace_path(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("fixture {} unreadable: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_all_collects_every_failure_not_just_the_first() {
        let cases = [
            Case::new("a", 1, 1),
            Case::new("b", 2, 99),
            Case::new("c", 3, 99),
        ];
        let err = check_all(&cases, |v| *v).unwrap_err();
        assert!(err.contains("[b]"), "second failure named: {err}");
        assert!(err.contains("[c]"), "third failure named: {err}");
        assert!(err.contains("2 of 3"), "aggregate count present: {err}");
    }

    #[test]
    fn check_all_passes_when_every_case_holds() {
        let cases = [Case::new("a", 1, 1), Case::new("b", 2, 2)];
        assert!(check_all(&cases, |v| *v).is_ok());
    }

    #[test]
    fn close_rejects_nan_even_against_itself() {
        assert!(!f64::NAN.is_close(f64::NAN, 0.0));
        assert!(!f64::INFINITY.is_close(f64::INFINITY, 0.0));
        assert!(1.0_f64.is_close(1.0 + 1e-12, 1e-9));
        assert!(!1.0_f64.is_close(1.1, 1e-9));
    }

    #[test]
    fn check_all_close_reports_gap_on_failure() {
        let cases = [CloseCase {
            name: "x",
            input: 1,
            expected: 2.0,
            tol: 0.1,
        }];
        let err = check_all_close(&cases, |v| *v as f64).unwrap_err();
        assert!(err.contains("gap"), "gap reported: {err}");
    }

    #[test]
    fn workspace_path_resolves_from_test_package() {
        let root = workspace_path("Cargo.toml");
        assert!(root.exists(), "workspace root found at {}", root.display());
    }
}
