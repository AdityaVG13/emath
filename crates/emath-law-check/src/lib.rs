#![forbid(unsafe_code)]

//! Independent world checking (WorldChecker): law obligations over
//! finite worlds, minimized counterexamples, scoped authority, and
//! deterministic answer receipts.
//!
//! Treats provider output as untrusted candidate data and validates
//! claimed obligations against it; the first violation over the sorted
//! carrier is the lexicographically smallest counterexample.

use emath_portfolio::Authority;
use emath_term::SymbolId;
use emath_world_ir::{FittedTable, WorldId, fnv1a64};

/// A law obligation an agent/provider world claims to satisfy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Law {
    /// `op(x, y) == op(y, x)` for all carrier pairs.
    Commutative(SymbolId),
    /// `op(op(x, y), z) == op(x, op(y, z))` for all carrier triples.
    Associative(SymbolId),
    /// `op(x, x) == x` for all carrier elements.
    Idempotent(SymbolId),
    /// The declared element `e` is the identity: `op(x, e) == x` and
    /// `op(e, x) == x` for all carrier elements.
    Identity(SymbolId, SymbolId),
    /// Declarative law text checked by an external oracle; the finite
    /// checker refuses it rather than passing it vacuously.
    Custom {
        /// Stable law name.
        name: String,
        /// Canonical law text.
        text: String,
    },
}

impl Law {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Commutative(op) => format!("commutative:{}", op.0),
            Self::Associative(op) => format!("associative:{}", op.0),
            Self::Idempotent(op) => format!("idempotent:{}", op.0),
            Self::Identity(op, e) => format!("identity:{}:{}", op.0, e.0),
            Self::Custom { name, text } => format!("custom:{name}:{text}"),
        }
    }
}

/// One claimed obligation with a contract identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldObligation {
    /// Contract identity (proposal, artifact, or provider claim).
    pub id: u64,
    /// The claimed law.
    pub law: Law,
}

/// A minimized counterexample: the lexicographically smallest carrier
/// tuple violating the obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimizedCounterexample {
    /// Obligation id that was violated.
    pub obligation_id: u64,
    /// Carrier inputs, in argument order.
    pub inputs: Vec<String>,
    /// Human-readable violation, e.g. `op(1,2)=3 != op(2,1)=4`.
    pub detail: String,
}

/// Verdict for one obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawVerdict {
    /// Obligation checked.
    pub obligation_id: u64,
    /// Whether the law held on the whole carrier.
    pub passed: bool,
    /// Minimized counterexample, when the law failed.
    pub counterexample: Option<MinimizedCounterexample>,
}

/// Scoped authority: a check endorses only the obligations it ran, at
/// most `Tested` (no hidden escalation to Certified/Proved).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedAuthority {
    /// Authority granted by the check.
    pub level: Authority,
    /// Obligation ids covered.
    pub scope: Vec<u64>,
}

/// Deterministic answer receipt of a check run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckerReceipt {
    /// Checker identity.
    pub checker: String,
    /// FNV-1a64 content identity over candidate and verdicts.
    pub id: u64,
}

/// Result of checking a set of obligations against a candidate world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldCheckReport {
    /// The candidate world identity.
    pub candidate: WorldId,
    /// Verdicts in obligation order.
    pub verdicts: Vec<LawVerdict>,
    /// Whether every obligation passed.
    pub passed: bool,
    /// Scoped authority, at most `Tested` over the checked obligations.
    pub scoped_authority: ScopedAuthority,
    /// Deterministic answer receipt.
    pub receipt: CheckerReceipt,
}

/// Why a check could not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckerError {
    /// A checker run must include at least one obligation; an empty set
    /// would let a provider skip verification.
    NoObligations,
    /// The candidate table has no rows: a law over an empty carrier
    /// would pass vacuously, so the check is refused instead of scoring
    /// `passed: true` from zero evidence.
    EmptyTable,
    /// The candidate table has no rows for the obligation's operator.
    UnknownOperator {
        /// Operator the obligation targeted.
        operator: String,
    },
    /// A custom law is not this checker's job and cannot pass vacuously.
    UnsupportedLaw {
        /// Custom law name.
        name: String,
    },
    /// The candidate table is not total for a binary operator; the
    /// obligation is refused rather than vacuously passed.
    Untotal {
        /// First undefined row.
        inputs: Vec<String>,
    },
}

/// Sorted carrier for a table: the distinct values appearing in any
/// input or output row.
fn carrier(table: &FittedTable) -> Vec<String> {
    let mut values = std::collections::BTreeSet::new();
    for (inputs, output) in table.cells() {
        for input in inputs {
            values.insert(input.clone());
        }
        values.insert(output.clone());
    }
    values.into_iter().collect()
}

/// Evaluates `op(a, b)` over the table; `None` when the row is undefined
/// (or the table is not binary).
fn eval(table: &FittedTable, a: &str, b: &str) -> Option<String> {
    table
        .get(&[a.to_string(), b.to_string()])
        .map(str::to_string)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LawCheck {
    /// Law held over the carrier.
    Held,
    /// First-found (minimized) counterexample.
    Violated(MinimizedCounterexample),
    /// Table could not be evaluated; obligation is not checked.
    Untotal {
        /// First undefined row.
        inputs: Vec<String>,
    },
}

/// Runs one law over the sorted carrier, returning on first violation so
/// the counterexample is minimized.
fn check_law(table: &FittedTable, obligation: &WorldObligation, carrier: &[String]) -> LawCheck {
    let op = match &obligation.law {
        Law::Commutative(op)
        | Law::Associative(op)
        | Law::Idempotent(op)
        | Law::Identity(op, _) => op,
        Law::Custom { .. } => {
            return LawCheck::Violated(MinimizedCounterexample {
                obligation_id: obligation.id,
                inputs: Vec::new(),
                detail: "custom laws require an external oracle".to_string(),
            });
        }
    };
    match &obligation.law {
        Law::Commutative(_) => {
            for a in carrier {
                for b in carrier {
                    let Some(left) = eval(table, a, b) else {
                        return LawCheck::Untotal {
                            inputs: vec![a.clone(), b.clone()],
                        };
                    };
                    let Some(right) = eval(table, b, a) else {
                        return LawCheck::Untotal {
                            inputs: vec![b.clone(), a.clone()],
                        };
                    };
                    if left != right {
                        return LawCheck::Violated(MinimizedCounterexample {
                            obligation_id: obligation.id,
                            inputs: vec![a.clone(), b.clone()],
                            detail: format!(
                                "{}({a},{b})={left} != {}({b},{a})={right}",
                                op.0, op.0
                            ),
                        });
                    }
                }
            }
        }
        Law::Associative(_) => {
            for a in carrier {
                for b in carrier {
                    for c in carrier {
                        let Some(ab) = eval(table, a, b) else {
                            return LawCheck::Untotal {
                                inputs: vec![a.clone(), b.clone()],
                            };
                        };
                        let Some(bc) = eval(table, b, c) else {
                            return LawCheck::Untotal {
                                inputs: vec![b.clone(), c.clone()],
                            };
                        };
                        let Some(left) = eval(table, &ab, c) else {
                            return LawCheck::Untotal {
                                inputs: vec![ab.clone(), c.clone()],
                            };
                        };
                        let Some(right) = eval(table, a, &bc) else {
                            return LawCheck::Untotal {
                                inputs: vec![a.clone(), bc.clone()],
                            };
                        };
                        if left != right {
                            return LawCheck::Violated(MinimizedCounterexample {
                                obligation_id: obligation.id,
                                inputs: vec![a.clone(), b.clone(), c.clone()],
                                detail: format!(
                                    "{}({}({a},{b}),{c})={left} != {}({a},{}({b},{c}))={right}",
                                    op.0, op.0, op.0, op.0
                                ),
                            });
                        }
                    }
                }
            }
        }
        Law::Identity(_, e) => {
            // The declared identity element `e` must BE the identity:
            // op(x, e) == x and op(e, x) == x for every carrier element.
            // A different witness does not satisfy the declared law.
            let e = e.0.clone();
            if !carrier.contains(&e) {
                return LawCheck::Violated(MinimizedCounterexample {
                    obligation_id: obligation.id,
                    inputs: Vec::new(),
                    detail: format!("declared identity element {e} is not in the carrier"),
                });
            }
            for x in carrier {
                let Some(left) = eval(table, x, &e) else {
                    return LawCheck::Untotal {
                        inputs: vec![x.clone(), e.clone()],
                    };
                };
                let Some(right) = eval(table, &e, x) else {
                    return LawCheck::Untotal {
                        inputs: vec![e.clone(), x.clone()],
                    };
                };
                if left != *x || right != *x {
                    return LawCheck::Violated(MinimizedCounterexample {
                        obligation_id: obligation.id,
                        inputs: vec![x.clone(), e.clone()],
                        detail: format!("{}({x},{e})={left} != {x}", op.0),
                    });
                }
            }
        }
        Law::Idempotent(_) => {
            for x in carrier {
                let Some(value) = eval(table, x, x) else {
                    return LawCheck::Untotal {
                        inputs: vec![x.clone(), x.clone()],
                    };
                };
                if value != *x {
                    return LawCheck::Violated(MinimizedCounterexample {
                        obligation_id: obligation.id,
                        inputs: vec![x.clone()],
                        detail: format!("{}({x},{x})={value} != {x}", op.0),
                    });
                }
            }
        }
        Law::Custom { name, .. } => {
            return LawCheck::Violated(MinimizedCounterexample {
                obligation_id: obligation.id,
                inputs: Vec::new(),
                detail: format!("custom law `{name}` requires an external oracle"),
            });
        }
    }
    LawCheck::Held
}

/// Independent finite-world law checker: verdicts derive only from the
/// table and claimed obligations (candidate data is untrusted); empty
/// obligation sets and custom laws refuse (vacuous pass is unsound);
/// authority never exceeds `Tested`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FiniteLawChecker;

impl FiniteLawChecker {
    /// Check `obligations` against a candidate table; receipt identity
    /// is deterministic (same inputs replay to the same id).
    pub fn check(
        &self,
        candidate: WorldId,
        table: &FittedTable,
        obligations: &[WorldObligation],
    ) -> Result<WorldCheckReport, CheckerError> {
        if obligations.is_empty() {
            return Err(CheckerError::NoObligations);
        }
        for obligation in obligations {
            match &obligation.law {
                Law::Custom { name, .. } => {
                    return Err(CheckerError::UnsupportedLaw { name: name.clone() });
                }
                Law::Commutative(op)
                | Law::Associative(op)
                | Law::Idempotent(op)
                | Law::Identity(op, _) => {
                    if *op != table.operator {
                        return Err(CheckerError::UnknownOperator {
                            operator: op.0.clone(),
                        });
                    }
                }
            }
        }

        let values = carrier(table);
        // No rows → empty carrier → every law vacuously "holds"; refuse.
        if values.is_empty() {
            return Err(CheckerError::EmptyTable);
        }
        let mut verdicts = Vec::new();
        let mut passed = true;
        for obligation in obligations {
            let outcome = check_law(table, obligation, &values);
            let (verdict_passed, counterexample) = match outcome {
                LawCheck::Held => (true, None),
                LawCheck::Violated(counterexample) => (false, Some(counterexample)),
                LawCheck::Untotal { inputs } => {
                    return Err(CheckerError::Untotal { inputs });
                }
            };
            passed &= verdict_passed;
            verdicts.push(LawVerdict {
                obligation_id: obligation.id,
                passed: verdict_passed,
                counterexample,
            });
        }

        let verdict_canonical = verdicts
            .iter()
            .map(|verdict| format!("{}={}", verdict.obligation_id, verdict.passed))
            .collect::<Vec<_>>()
            .join(",");
        let receipt = CheckerReceipt {
            checker: "finite-law-checker".to_string(),
            id: fnv1a64(
                format!("check:candidate={}:{}", candidate.0, verdict_canonical).as_bytes(),
            ),
        };
        let scope = obligations.iter().map(|obligation| obligation.id).collect();
        Ok(WorldCheckReport {
            candidate,
            verdicts,
            passed,
            scoped_authority: ScopedAuthority {
                level: if passed {
                    Authority::Tested
                } else {
                    Authority::Structural
                },
                scope,
            },
            receipt,
        })
    }
}

/// Convenience: runs the finite checker.
pub fn check_world(
    candidate: WorldId,
    table: &FittedTable,
    obligations: &[WorldObligation],
) -> Result<WorldCheckReport, CheckerError> {
    FiniteLawChecker.check(candidate, table, obligations)
}
