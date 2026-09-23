//! Failure collector. A probe that stops at the first mismatch is a bug.
//!
//! Write the intended contract into a [`Probe`], run every related function
//! against it, then [`Probe::finish`]. Every mismatch is kept. The panic
//! lists them all so one run exposes many upgrade sites.

use std::fmt::Debug;
use std::panic::Location;

/// One recorded contract violation.
#[derive(Clone, Debug)]
pub struct Failure {
    /// Case or check name.
    pub name: String,
    /// What was demanded vs what happened.
    pub message: String,
    /// Call site that recorded the failure.
    pub site: &'static Location<'static>,
}

/// Accumulates every failure for one intent. Never skip. Never XFAIL.
pub struct Probe {
    intent: &'static str,
    failures: Vec<Failure>,
    checks: usize,
    finished: bool,
}

impl Probe {
    /// Start a probe. `intent` is the contract this test exists to enforce.
    #[must_use]
    pub fn new(intent: &'static str) -> Self {
        Self {
            intent,
            failures: Vec::new(),
            checks: 0,
            finished: false,
        }
    }

    /// How many checks have been recorded (pass or fail).
    #[must_use]
    pub const fn checks(&self) -> usize {
        self.checks
    }

    /// Failures recorded so far.
    #[must_use]
    pub fn failures(&self) -> &[Failure] {
        &self.failures
    }

    /// Record an unconditional failure. Use when a required path is missing.
    #[track_caller]
    pub fn fail(&mut self, name: impl Into<String>, message: impl Into<String>) -> &mut Self {
        self.checks += 1;
        self.failures.push(Failure {
            name: name.into(),
            message: message.into(),
            site: Location::caller(),
        });
        self
    }

    /// Demand `cond`. Records, does not return early.
    #[track_caller]
    pub fn demand(
        &mut self,
        name: impl Into<String>,
        cond: bool,
        message: impl Into<String>,
    ) -> &mut Self {
        self.checks += 1;
        if !cond {
            self.failures.push(Failure {
                name: name.into(),
                message: message.into(),
                site: Location::caller(),
            });
        }
        self
    }

    /// Demand `actual == expected`.
    #[track_caller]
    pub fn eq<A, E>(
        &mut self,
        name: impl Into<String>,
        actual: A,
        expected: E,
    ) -> &mut Self
    where
        A: Debug + PartialEq<E>,
        E: Debug,
    {
        self.checks += 1;
        if actual != expected {
            self.failures.push(Failure {
                name: name.into(),
                message: format!("expected {expected:?}, got {actual:?}"),
                site: Location::caller(),
            });
        }
        self
    }

    /// Demand `actual != unexpected` (the mutant / silent-success value).
    #[track_caller]
    pub fn ne<A, E>(
        &mut self,
        name: impl Into<String>,
        actual: A,
        unexpected: E,
    ) -> &mut Self
    where
        A: Debug + PartialEq<E>,
        E: Debug,
    {
        self.checks += 1;
        if actual == unexpected {
            self.failures.push(Failure {
                name: name.into(),
                message: format!("got forbidden silent-success value {unexpected:?}"),
                site: Location::caller(),
            });
        }
        self
    }

    /// Demand `|actual - expected| <= tol`. NaN is a failure.
    #[track_caller]
    pub fn close(
        &mut self,
        name: impl Into<String>,
        actual: f64,
        expected: f64,
        tol: f64,
    ) -> &mut Self {
        self.checks += 1;
        let ok = actual.is_finite() && expected.is_finite() && (actual - expected).abs() <= tol;
        if !ok {
            self.failures.push(Failure {
                name: name.into(),
                message: format!(
                    "expected {expected} ± {tol}, got {actual} (gap {})",
                    (actual - expected).abs()
                ),
                site: Location::caller(),
            });
        }
        self
    }

    /// Demand `haystack` contains `needle`.
    #[track_caller]
    pub fn contains(
        &mut self,
        name: impl Into<String>,
        haystack: &str,
        needle: &str,
    ) -> &mut Self {
        self.checks += 1;
        if !haystack.contains(needle) {
            self.failures.push(Failure {
                name: name.into(),
                message: format!("missing {needle:?} in {haystack:?}"),
                site: Location::caller(),
            });
        }
        self
    }

    /// Nested case: run `body` and prefix its failure names with `name`.
    pub fn case(&mut self, name: &str, body: impl FnOnce(&mut Self)) -> &mut Self {
        let start = self.failures.len();
        body(self);
        for failure in &mut self.failures[start..] {
            failure.name = format!("{name}/{}", failure.name);
        }
        self
    }

    /// Panic with every failure, or succeed. A probe with zero checks is itself
    /// a failure: a test that asserts nothing is fluff.
    #[track_caller]
    pub fn finish(mut self) {
        self.finished = true;
        assert!(self.checks != 0, 
            "PROBE `{}` recorded zero checks — that is fluff, not a test",
            self.intent
        );
        if self.failures.is_empty() {
            return;
        }
        let mut out = format!(
            "PROBE `{}` failed {}/{} checks (all sites, not first-only):\n",
            self.intent,
            self.failures.len(),
            self.checks
        );
        for (index, failure) in self.failures.iter().enumerate() {
            out.push_str(&format!(
                "  {}. {} @ {}: {}\n",
                index + 1,
                failure.name,
                failure.site,
                failure.message
            ));
        }
        panic!("{out}");
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        if self.finished || std::thread::panicking() {
            return;
        }
        assert!(self.failures.is_empty(), 
            "PROBE `{}` dropped with {} unreported failures — call finish()",
            self.intent,
            self.failures.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::Probe;

    #[test]
    fn probe_collects_every_mismatch() {
        let mut probe = Probe::new("collector keeps going");
        probe.eq("a", 1, 1);
        probe.eq("b", 1, 2);
        probe.eq("c", 3, 4);
        probe.demand("d", true, "ok");
        assert_eq!(probe.checks(), 4);
        assert_eq!(probe.failures().len(), 2);
        assert_eq!(probe.failures()[0].name, "b");
        assert_eq!(probe.failures()[1].name, "c");
        probe.finished = true;
    }
}
