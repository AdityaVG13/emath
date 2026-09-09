//! Table runner: one test function, many inputs, many functions.
//!
//! Prefer this over a `#[test]` per row. Every failing row is reported.

use std::fmt::Debug;
use std::path::{Path, PathBuf};

/// One row of a table-driven check: a name, an input, the expected result.
#[derive(Clone, Debug)]
pub struct Case<I, O> {
    /// Row name shown in the aggregated failure.
    pub name: &'static str,
    /// Input to the function under test.
    pub input: I,
    /// Independent expected result (never derived from the code under test).
    pub expected: O,
}

impl<I, O> Case<I, O> {
    /// Construct a named case.
    #[must_use]
    pub const fn new(name: &'static str, input: I, expected: O) -> Self {
        Self {
            name,
            input,
            expected,
        }
    }
}

/// Run every case through `f`, collecting ALL failures.
///
/// Pass a closure that dispatches to the function under test. One table
/// can cover many functions by encoding the function identity in `I`.
///
/// # Errors
/// One aggregated message listing every failing case.
pub fn check_all<I, O, F>(cases: &[Case<I, O>], f: F) -> Result<(), String>
where
    I: Debug,
    O: Debug + PartialEq,
    F: Fn(&I) -> O,
{
    let mut failures = Vec::new();
    for case in cases {
        let actual = f(&case.input);
        if actual != case.expected {
            failures.push(format!(
                "{}: input {:?} expected {:?} got {:?}",
                case.name, case.input, case.expected, actual
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{}/{} cases failed:\n  {}",
            failures.len(),
            cases.len(),
            failures.join("\n  ")
        ))
    }
}

/// One row of a tolerance-based table check.
#[derive(Clone, Debug)]
pub struct CloseCase<I> {
    /// Row name.
    pub name: &'static str,
    /// Input.
    pub input: I,
    /// Independent expected scalar.
    pub expected: f64,
    /// Absolute tolerance.
    pub tol: f64,
}

/// Run every case through a scalar-producing `f`, collecting ALL failures.
///
/// # Errors
/// One aggregated message listing every failing case.
pub fn check_all_close<I, F>(cases: &[CloseCase<I>], f: F) -> Result<(), String>
where
    I: Debug,
    F: Fn(&I) -> f64,
{
    let mut failures = Vec::new();
    for case in cases {
        let actual = f(&case.input);
        let ok = actual.is_finite()
            && case.expected.is_finite()
            && (actual - case.expected).abs() <= case.tol;
        if !ok {
            failures.push(format!(
                "{}: input {:?} expected {} ± {} got {}",
                case.name, case.input, case.expected, case.tol, actual
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{}/{} cases failed:\n  {}",
            failures.len(),
            cases.len(),
            failures.join("\n  ")
        ))
    }
}

/// Tolerance-based comparison for floats. NaN is never close.
pub trait Close {
    /// True when `self` is within `tol` of `expected`.
    fn close_to(self, expected: Self, tol: f64) -> bool;
}

impl Close for f64 {
    fn close_to(self, expected: f64, tol: f64) -> bool {
        self.is_finite() && expected.is_finite() && (self - expected).abs() <= tol
    }
}

/// Panic helper for the table runners. Keeps the failure line in the test.
#[track_caller]
pub fn expect_ok(result: Result<(), String>) {
    if let Err(message) = result {
        panic!("{message}");
    }
}

/// Path into the repository. The harness lives at `tests/harness`, so two
/// parents is the workspace root regardless of which test crate calls this.
#[must_use]
pub fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// Whole contents of a workspace-relative file.
///
/// # Panics
/// When the file does not exist — a missing fixture is a test-authoring error.
#[must_use]
pub fn workspace_file(relative: &str) -> String {
    let path = workspace_path(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("missing workspace file {}: {error}", path.display()))
}
