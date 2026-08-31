//! SG-10 scoped binders: capture-safe binding forms over the first-order
//! term IR.
//!
//! Families: `Structural` expands to its finite range; `FiniteAnalogue`
//! expands like structural but claims no continuum meaning; `OpaqueSeeded`
//! carries only a seeded identity; `Conventional` refuses expansion.
//! Substitution alpha-renames to avoid capture; canonical identity is
//! alpha-invariant (de Bruijn indices). Expansion is budgeted; the budget
//! is an execution parameter, excluded from [`binder_id`].

use std::collections::BTreeSet;
use std::fmt::Write as _;

use emath_term::{SymbolId, Term, VariableId};
use emath_world_ir::fnv1a64;

/// Scoped-binder schema id for artifacts and receipts.
pub const BINDER_SCHEMA: &str = "emath.binder";
/// Scoped-binder schema version. Bump on any change to the canonical
/// encoding or family semantics; consumers refuse versions they do not know.
pub const BINDER_VERSION: u32 = 1;

/// Refuses schema versions this module does not understand.
pub fn check_version(version: u32) -> Result<(), BinderError> {
    if version == BINDER_VERSION {
        Ok(())
    } else {
        Err(BinderError::UnknownVersion { version })
    }
}

/// The four binder families and their meaning claims (see module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BinderFamily {
    /// Defined as its finite structural expansion.
    Structural,
    /// No structural claim; deterministic seeded identity only.
    OpaqueSeeded,
    /// Finite analogue of a conventional infinite form; no continuum claim.
    FiniteAnalogue,
    /// Conventional notation preserved; expansion refuses.
    Conventional,
}

impl BinderFamily {
    /// All families in canonical order.
    pub const ALL: [Self; 4] = [
        Self::Structural,
        Self::OpaqueSeeded,
        Self::FiniteAnalogue,
        Self::Conventional,
    ];

    /// Canonical family name.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Structural => "structural",
            Self::OpaqueSeeded => "opaque-seeded",
            Self::FiniteAnalogue => "finite-analogue",
            Self::Conventional => "conventional",
        }
    }
}

/// Binder kinds: the conventional big operators plus custom binders.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BinderKind {
    /// Σ-style summation.
    Sum,
    /// Π-style product.
    Product,
    /// ∫-style integral.
    Integral,
    /// d/dx-style derivative.
    Derivative,
    /// lim-style limit.
    Limit,
    /// A named custom binder.
    Custom(String),
}

impl BinderKind {
    /// Canonical kind name.
    #[must_use]
    pub fn canonical_name(&self) -> &str {
        match self {
            Self::Sum => "sum",
            Self::Product => "product",
            Self::Integral => "integral",
            Self::Derivative => "derivative",
            Self::Limit => "limit",
            Self::Custom(name) => name,
        }
    }
}

/// The domain a binder ranges over.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BinderDomain {
    /// An explicit inclusive integer range.
    FiniteRange {
        /// Inclusive lower bound.
        lower: i64,
        /// Inclusive upper bound.
        upper: i64,
    },
    /// A symbolic anchor (limit point, measure, ...) with no finite
    /// enumeration; only opaque or conventional families may use it.
    Symbolic {
        /// Anchor text preserved byte-exactly.
        anchor: String,
    },
}

impl BinderDomain {
    fn write_canonical(&self, out: &mut String) {
        match self {
            Self::FiniteRange { lower, upper } => {
                let _ = write!(out, "range({lower},{upper})");
            }
            Self::Symbolic { anchor } => {
                let _ = write!(out, "symbolic({})", escape(anchor));
            }
        }
    }
}

/// Expansion budget: the maximum number of body instantiations one
/// [`ScopedBinder::expand`] call may perform, across nesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinderBudget {
    /// Maximum body instantiations.
    pub max_terms: u32,
}

impl Default for BinderBudget {
    fn default() -> Self {
        Self { max_terms: 64 }
    }
}

/// Typed refusals for binder operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinderError {
    /// Schema version this module does not understand.
    UnknownVersion {
        /// Offending version.
        version: u32,
    },
    /// Expansion requested for a family that never expands.
    NotExpandable {
        /// Binder kind name.
        kind: String,
        /// Refusing family.
        family: BinderFamily,
    },
    /// Expansion requested over a non-finite domain.
    NonFiniteDomain {
        /// Binder kind name.
        kind: String,
        /// Family that required a finite range.
        family: BinderFamily,
    },
    /// Finite range with `lower > upper`; no neutral element is invented.
    EmptyDomain {
        /// Inclusive lower bound.
        lower: i64,
        /// Inclusive upper bound.
        upper: i64,
    },
    /// The expansion needed more instantiations than the budget allows.
    BudgetExceeded {
        /// Budget limit that was exhausted.
        limit: u32,
    },
    /// Operation reserved for a different family (e.g. opaque identity on a
    /// structural binder).
    WrongFamily {
        /// Operation name.
        operation: &'static str,
        /// Actual family.
        family: BinderFamily,
    },
}

/// A term that may contain scoped binders.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BinderTerm {
    /// A binder-free first-order term.
    Leaf(Term),
    /// A nested scoped binder.
    Bind(Box<ScopedBinder>),
}

/// One scoped binder: `kind` over `domain`, binding `bound` in `body`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScopedBinder {
    /// Binder kind.
    pub kind: BinderKind,
    /// Meaning-claim family.
    pub family: BinderFamily,
    /// Domain the bound variable ranges over.
    pub domain: BinderDomain,
    /// The bound variable.
    pub bound: VariableId,
    /// Body in which `bound` is in scope.
    pub body: BinderTerm,
}

impl BinderTerm {
    /// Free variables (bound occurrences excluded).
    #[must_use]
    pub fn free_variables(&self) -> BTreeSet<VariableId> {
        match self {
            Self::Leaf(term) => term_free_variables(term),
            Self::Bind(binder) => {
                let mut free = binder.body.free_variables();
                free.remove(&binder.bound);
                free
            }
        }
    }

    /// Alpha-invariant canonical form: bound occurrences render as de
    /// Bruijn indices, so alpha-equivalent terms share one string.
    /// Budgets are execution parameters and never appear.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        let mut stack: Vec<VariableId> = Vec::new();
        write_binder_term(self, &mut stack, &mut out);
        out
    }

    /// Capture-avoiding substitution of `replacement` for free occurrences
    /// of `variable`. Substitution under a binder whose bound variable
    /// would capture a free variable of `replacement` alpha-renames the
    /// bound variable first (deterministic `name#N` fresh names).
    #[must_use]
    pub fn substitute(&self, variable: &VariableId, replacement: &Term) -> Self {
        match self {
            Self::Leaf(term) => Self::Leaf(substitute_term(term, variable, replacement)),
            Self::Bind(binder) => {
                if binder.bound == *variable {
                    // Shadowed: the substitution stops at this scope.
                    return self.clone();
                }
                let replacement_free = term_free_variables(replacement);
                let binder = if replacement_free.contains(&binder.bound) {
                    let mut avoid = binder.body.free_variables();
                    avoid.extend(replacement_free);
                    avoid.insert(variable.clone());
                    let fresh = fresh_name(&binder.bound, &avoid);
                    let renamed_body = binder
                        .body
                        .substitute(&binder.bound, &Term::Variable(fresh.clone()));
                    Box::new(ScopedBinder {
                        kind: binder.kind.clone(),
                        family: binder.family,
                        domain: binder.domain.clone(),
                        bound: fresh,
                        body: renamed_body,
                    })
                } else {
                    binder.clone()
                };
                Self::Bind(Box::new(ScopedBinder {
                    kind: binder.kind.clone(),
                    family: binder.family,
                    domain: binder.domain.clone(),
                    bound: binder.bound.clone(),
                    body: binder.body.substitute(variable, replacement),
                }))
            }
        }
    }
}

impl ScopedBinder {
    /// Alpha-invariant canonical form of this binder (see
    /// [`BinderTerm::canonical`]).
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        let mut stack: Vec<VariableId> = Vec::new();
        write_binder(self, &mut stack, &mut out);
        out
    }

    /// Structural expansion: instantiate the body per domain value and
    /// left-fold with `combine`; only `Structural`/`FiniteAnalogue` over
    /// a finite range expand, else a typed refusal.
    pub fn expand(&self, combine: &SymbolId, budget: BinderBudget) -> Result<Term, BinderError> {
        let mut remaining = budget.max_terms;
        self.expand_inner(combine, &mut remaining, budget.max_terms)
    }

    fn expand_inner(
        &self,
        combine: &SymbolId,
        remaining: &mut u32,
        limit: u32,
    ) -> Result<Term, BinderError> {
        match self.family {
            BinderFamily::OpaqueSeeded | BinderFamily::Conventional => {
                Err(BinderError::NotExpandable {
                    kind: self.kind.canonical_name().to_string(),
                    family: self.family,
                })
            }
            BinderFamily::Structural | BinderFamily::FiniteAnalogue => {
                let BinderDomain::FiniteRange { lower, upper } = self.domain else {
                    return Err(BinderError::NonFiniteDomain {
                        kind: self.kind.canonical_name().to_string(),
                        family: self.family,
                    });
                };
                if lower > upper {
                    return Err(BinderError::EmptyDomain { lower, upper });
                }
                let mut accumulator: Option<Term> = None;
                for value in lower..=upper {
                    if *remaining == 0 {
                        return Err(BinderError::BudgetExceeded { limit });
                    }
                    *remaining -= 1;
                    let instance = self
                        .body
                        .substitute(&self.bound, &Term::Constant(SymbolId(value.to_string())));
                    let flat = match instance {
                        BinderTerm::Leaf(term) => term,
                        BinderTerm::Bind(nested) => {
                            nested.expand_inner(combine, remaining, limit)?
                        }
                    };
                    accumulator = Some(match accumulator {
                        None => flat,
                        Some(previous) => Term::Apply {
                            operator: combine.clone(),
                            arguments: vec![previous, flat],
                        },
                    });
                }
                accumulator.ok_or(BinderError::EmptyDomain { lower, upper })
            }
        }
    }

    /// Deterministic seeded identity for `OpaqueSeeded` binders: FNV-1a64
    /// over the versioned canonical form mixed with `seed`. Refuses other
    /// families — they carry structural or conventional claims instead.
    pub fn opaque_identity(&self, seed: u64) -> Result<u64, BinderError> {
        if self.family != BinderFamily::OpaqueSeeded {
            return Err(BinderError::WrongFamily {
                operation: "opaque_identity",
                family: self.family,
            });
        }
        let base = binder_id(self);
        Ok(fnv1a64(format!("{base:016x}:{seed:016x}").as_bytes()))
    }
}

/// Alpha-invariant binder identity: FNV-1a64 over the versioned canonical
/// form.
#[must_use]
pub fn binder_id(binder: &ScopedBinder) -> u64 {
    fnv1a64(format!("{BINDER_SCHEMA}.v{BINDER_VERSION}:{}", binder.canonical()).as_bytes())
}

fn write_binder_term(term: &BinderTerm, stack: &mut Vec<VariableId>, out: &mut String) {
    match term {
        BinderTerm::Leaf(leaf) => write_term(leaf, stack, out),
        BinderTerm::Bind(binder) => write_binder(binder, stack, out),
    }
}

fn write_binder(binder: &ScopedBinder, stack: &mut Vec<VariableId>, out: &mut String) {
    let _ = write!(
        out,
        "bind({},{},",
        escape(binder.kind.canonical_name()),
        binder.family.canonical()
    );
    binder.domain.write_canonical(out);
    out.push(',');
    stack.push(binder.bound.clone());
    write_binder_term(&binder.body, stack, out);
    stack.pop();
    out.push(')');
}

fn write_term(term: &Term, stack: &[VariableId], out: &mut String) {
    match term {
        Term::Variable(variable) => {
            // Innermost binder is index 0.
            if let Some(index) = stack.iter().rev().position(|bound| bound == variable) {
                let _ = write!(out, "bound({index})");
            } else {
                let _ = write!(out, "var({})", escape(&variable.0));
            }
        }
        Term::Constant(symbol) => {
            let _ = write!(out, "const({})", escape(&symbol.0));
        }
        Term::Apply {
            operator,
            arguments,
        } => {
            let _ = write!(out, "apply({}", escape(&operator.0));
            for argument in arguments {
                out.push(',');
                write_term(argument, stack, out);
            }
            out.push(')');
        }
    }
}

fn term_free_variables(term: &Term) -> BTreeSet<VariableId> {
    let mut free = BTreeSet::new();
    collect_term_variables(term, &mut free);
    free
}

fn collect_term_variables(term: &Term, free: &mut BTreeSet<VariableId>) {
    match term {
        Term::Variable(variable) => {
            free.insert(variable.clone());
        }
        Term::Constant(_) => {}
        Term::Apply { arguments, .. } => {
            for argument in arguments {
                collect_term_variables(argument, free);
            }
        }
    }
}

/// Substitute `variable` with `replacement` throughout `term`. Plain
/// capture-introducing substitution: the binder layer's scoped binders
/// handle capture avoidance above this helper.
pub fn substitute_term(term: &Term, variable: &VariableId, replacement: &Term) -> Term {
    match term {
        Term::Variable(candidate) if candidate == variable => replacement.clone(),
        Term::Variable(_) | Term::Constant(_) => term.clone(),
        Term::Apply {
            operator,
            arguments,
        } => Term::Apply {
            operator: operator.clone(),
            arguments: arguments
                .iter()
                .map(|argument| substitute_term(argument, variable, replacement))
                .collect(),
        },
    }
}

fn fresh_name(original: &VariableId, avoid: &BTreeSet<VariableId>) -> VariableId {
    let mut counter = 1_u32;
    loop {
        let candidate = VariableId(format!("{}#{counter}", original.0));
        if !avoid.contains(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            ',' => out.push_str("\\,"),
            _ => out.push(ch),
        }
    }
    out
}
