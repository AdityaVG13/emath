//! /06-032: resolution algebra.
//!
//! Q-states, partial provider-capability steps, and serial/parallel/alt/
//! fallback/portfolio composition; inapplicable steps refuse with stable
//! reason codes, and `apply_total` lifts any partial step to an explicit
//! refusal — never a panic or silent skip.

use std::collections::BTreeSet;

/// One facet of a resolution question. The five facets mirror the
/// compatibility axes of `emath_provider_api::filter_goal`
/// (`E-PROV-512`..`E-PROV-516`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Facet {
    /// Goal kind / produce subset service (`E-PROV-512`).
    Kind,
    /// Evidence ceiling and checker bindings (`E-PROV-513`).
    Evidence,
    /// Target family (`E-PROV-514`).
    Target,
    /// Exactness offering (`E-PROV-515`).
    Exactness,
    /// Determinism (`E-PROV-516`).
    Determinism,
}

impl Facet {
    /// Every facet, in canonical order.
    pub const ALL: [Self; 5] = [
        Self::Kind,
        Self::Evidence,
        Self::Target,
        Self::Exactness,
        Self::Determinism,
    ];

    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Kind => "kind",
            Self::Evidence => "evidence",
            Self::Target => "target",
            Self::Exactness => "exactness",
            Self::Determinism => "determinism",
        }
    }
}

/// Q-state: the residual resolution question, i.e. which facets of the goal
/// remain unresolved. The planner starts from `QState::full()` and a plan is
/// admissible only when the applied composition resolves every facet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QState {
    open: BTreeSet<Facet>,
}

impl QState {
    /// The initial question: every facet open.
    #[must_use]
    pub fn full() -> Self {
        Self {
            open: Facet::ALL.iter().copied().collect(),
        }
    }

    /// The terminal state: nothing left to resolve.
    #[must_use]
    pub fn resolved() -> Self {
        Self {
            open: BTreeSet::new(),
        }
    }

    /// Whether every facet has been resolved.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        self.open.is_empty()
    }

    /// The facets still open, in canonical order.
    #[must_use]
    pub fn open_facets(&self) -> Vec<Facet> {
        self.open.iter().copied().collect()
    }

    /// The state with `facets` discharged.
    #[must_use]
    fn discharge(&self, facets: &BTreeSet<Facet>) -> Self {
        Self {
            open: self.open.difference(facets).copied().collect(),
        }
    }
}

/// Result of applying a step: the residual state, the providers applied in
/// order, and whether a fallback arm was taken (degradation is explicit,
/// never silent).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Application {
    /// Residual Q-state after the step.
    pub state: QState,
    /// Provider ids applied, in application order.
    pub trace: Vec<String>,
    /// Whether any fallback arm was taken.
    pub degraded: bool,
}

/// Total application: either an applied step or an explicit refusal with the
/// stable reasons that made every arm inapplicable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lifted {
    /// The step applied.
    Applied(Application),
    /// The step was inapplicable; every collected refusal reason retained.
    Refused {
        /// Stable refusal reasons (`code: detail`).
        reasons: Vec<String>,
    },
}

/// A step in the resolution algebra: a (possibly partial) transformation
/// over Q-states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// The identity transformation (neutral element of serial composition).
    Id,
    /// A provider capability as a partial transformation: applicable iff
    /// `refusals` is empty, in which case it discharges `discharges`.
    Capability {
        /// Provider id.
        provider: String,
        /// Facets discharged on application.
        discharges: BTreeSet<Facet>,
        /// Stable reasons the capability is inapplicable (empty = applicable).
        refusals: Vec<String>,
    },
    /// Serial composition: apply left, then right on the residual state.
    Serial(Box<Step>, Box<Step>),
    /// Parallel composition: both arms apply to the same state; the results
    /// join (a facet resolved by either arm is resolved).
    Parallel(Box<Step>, Box<Step>),
    /// Ordered alternatives: the first applicable arm wins (left bias).
    Alt(Vec<Step>),
    /// Fallback ladder: primary, else the fallback arm marked as degraded.
    Fallback(Box<Step>, Box<Step>),
    /// Portfolio: every applicable member is retained and applied to the
    /// same state; the results join. Inapplicable iff no member applies.
    Portfolio(Vec<Step>),
}

impl Step {
    /// A fully-discharging capability (a provider the compatibility filter
    /// admitted on every axis).
    #[must_use]
    pub fn compatible(provider: &str) -> Self {
        Self::Capability {
            provider: provider.to_string(),
            discharges: Facet::ALL.iter().copied().collect(),
            refusals: Vec::new(),
        }
    }

    /// An inapplicable capability carrying its stable refusal reasons.
    #[must_use]
    pub fn refused(provider: &str, reasons: Vec<String>) -> Self {
        Self::Capability {
            provider: provider.to_string(),
            discharges: BTreeSet::new(),
            refusals: if reasons.is_empty() {
                vec!["unspecified refusal".to_string()]
            } else {
                reasons
            },
        }
    }

    /// Partial application: `None` when the step does not apply at `state`.
    #[must_use]
    pub fn apply(&self, state: &QState) -> Option<Application> {
        match self {
            Self::Id => Some(Application {
                state: state.clone(),
                trace: Vec::new(),
                degraded: false,
            }),
            Self::Capability {
                provider,
                discharges,
                refusals,
            } => {
                if refusals.is_empty() {
                    Some(Application {
                        state: state.discharge(discharges),
                        trace: vec![provider.clone()],
                        degraded: false,
                    })
                } else {
                    None
                }
            }
            Self::Serial(left, right) => {
                let first = left.apply(state)?;
                let second = right.apply(&first.state)?;
                Some(Application {
                    state: second.state,
                    trace: [first.trace, second.trace].concat(),
                    degraded: first.degraded || second.degraded,
                })
            }
            Self::Parallel(left, right) => {
                let first = left.apply(state)?;
                let second = right.apply(state)?;
                Some(Application {
                    state: join(&first.state, &second.state),
                    trace: [first.trace, second.trace].concat(),
                    degraded: first.degraded || second.degraded,
                })
            }
            Self::Alt(arms) => arms.iter().find_map(|arm| arm.apply(state)),
            Self::Fallback(primary, fallback) => primary.apply(state).or_else(|| {
                fallback.apply(state).map(|application| Application {
                    degraded: true,
                    ..application
                })
            }),
            Self::Portfolio(members) => {
                let applications: Vec<Application> = members
                    .iter()
                    .filter_map(|member| member.apply(state))
                    .collect();
                if applications.is_empty() {
                    return None;
                }
                let mut joined = applications[0].state.clone();
                let mut trace = Vec::new();
                let mut degraded = false;
                for application in &applications {
                    joined = join(&joined, &application.state);
                    trace.extend(application.trace.iter().cloned());
                    degraded = degraded || application.degraded;
                }
                Some(Application {
                    state: joined,
                    trace,
                    degraded,
                })
            }
        }
    }

    /// Lifting: the total application. Inapplicability becomes an explicit
    /// refusal carrying every collected reason; nothing panics or skips.
    #[must_use]
    pub fn apply_total(&self, state: &QState) -> Lifted {
        match self.apply(state) {
            Some(application) => Lifted::Applied(application),
            None => Lifted::Refused {
                reasons: self.collect_refusals(),
            },
        }
    }

    /// Every refusal reason reachable in this step, in deterministic order.
    fn collect_refusals(&self) -> Vec<String> {
        match self {
            Self::Id => Vec::new(),
            Self::Capability {
                provider, refusals, ..
            } => refusals
                .iter()
                .map(|reason| format!("{provider}: {reason}"))
                .collect(),
            Self::Serial(left, right)
            | Self::Parallel(left, right)
            | Self::Fallback(left, right) => {
                [left.collect_refusals(), right.collect_refusals()].concat()
            }
            Self::Alt(arms) | Self::Portfolio(arms) => {
                arms.iter().flat_map(Self::collect_refusals).collect()
            }
        }
    }
}

/// Join of two residual states: a facet is open only when both leave it open.
#[must_use]
fn join(left: &QState, right: &QState) -> QState {
    QState {
        open: left.open.intersection(&right.open).copied().collect(),
    }
}

/// Serial composition helper.
#[must_use]
pub fn serial(left: Step, right: Step) -> Step {
    Step::Serial(Box::new(left), Box::new(right))
}

/// Parallel composition helper.
#[must_use]
pub fn parallel(left: Step, right: Step) -> Step {
    Step::Parallel(Box::new(left), Box::new(right))
}

/// Fallback ladder helper.
#[must_use]
pub fn fallback(primary: Step, secondary: Step) -> Step {
    Step::Fallback(Box::new(primary), Box::new(secondary))
}

// Resolution-algebra tests moved to `tests/emath-plan/tests/algebra.rs`.
