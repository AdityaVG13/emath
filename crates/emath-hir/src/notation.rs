//! Scoped notation.
//!
//! Imports operator declarations into a scoped notation table with
//! precedence/fixity, alias resolution, arity checks and ambiguity
//! detection.

use emath_ir::operator::{DeclaredOperator, Fixity};

/// How a notation symbol is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UseKind {
    Infix,
    Prefix,
    Postfix,
}

impl UseKind {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Infix => "infix",
            Self::Prefix => "prefix",
            Self::Postfix => "postfix",
        }
    }
}

/// One mounted notation entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotationEntry {
    pub symbol: String,
    pub binds: String,
    pub fixity: Fixity,
    pub precedence: u8,
    /// Declared arity; `None` when the bound function's arity is unknown.
    /// Unknown arity must not be enforced as a guessed 2.
    pub arity: Option<u8>,
    pub scope: String,
}

impl NotationEntry {
    /// Canonical rendering of the notation over the data operand(s).
    #[must_use]
    pub fn render(&self, left: &str, right: &str) -> String {
        match self.fixity {
            Fixity::Infix => format!("{left} {} {right}", self.symbol),
            Fixity::Prefix => format!("{} {right}", self.symbol),
            Fixity::Postfix => format!("{left} {}", self.symbol),
        }
    }
}

/// Registered symbol table plus alias map.
pub struct NotationContext<'a> {
    /// Qualified names already registered (declaration surface).
    pub symbols: &'a [&'a str],
    /// Alias pairs `(surface, canonical)`.
    pub aliases: &'a [(&'a str, &'a str)],
}

/// One notation refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotationIssue {
    pub code: &'static str,
    pub symbol: String,
    pub detail: String,
}

/// Result of mounting a notation set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotationMount {
    pub entries: Vec<NotationEntry>,
    pub issues: Vec<NotationIssue>,
}

/// Canonical arity of a symbol by its builtin group.
#[must_use]
pub fn arity_of(symbol: &str) -> Option<u8> {
    let base = symbol.rsplit("::").next().unwrap_or(symbol);
    let arity = match base {
        "add" | "sub" | "mul" | "div" | "pow" | "min" | "max" | "atan2" => 2,
        "is_finite" | "exp" | "ln" | "log" | "sqrt" | "sin" | "cos" | "tan" | "tanh" | "abs"
        | "floor" | "ceil" | "neg" | "not" => 1,
        _ => return None,
    };
    Some(arity)
}

/// Default precedence for a fixity class.
#[must_use]
pub fn default_precedence(fixity: Fixity) -> u8 {
    match fixity {
        Fixity::Infix => 10,
        Fixity::Prefix | Fixity::Postfix => 16,
    }
}

/// Mount declared operators into the scoped notation table. Refusals:
/// empty symbol (`E-NAME-021`), duplicate (`E-NAME-020`), fixity/arity
/// mismatch (`E-NAME-021`), ambiguity (`E-NAME-022`); aliases resolve.
#[must_use]
pub fn mount_notation(
    context: &NotationContext<'_>,
    declared: &[DeclaredOperator],
) -> NotationMount {
    let mut mount = NotationMount::default();
    let mut by_symbol: Vec<(String, Fixity, u8, String)> = Vec::new();

    for operator in declared {
        if operator.symbol.is_empty() {
            mount.issues.push(NotationIssue {
                code: "E-NAME-021",
                symbol: "<empty>".into(),
                detail: "operator symbol cannot be empty".into(),
            });
            continue;
        }
        let canonical_binds = resolve_alias(context.aliases, &operator.binds.0)
            .unwrap_or_else(|| operator.binds.0.clone());
        let arity = arity_of(&canonical_binds);
        let fixity = operator.fixity;
        if let Some(arity) = arity {
            match fixity {
                Fixity::Infix if arity < 2 => {
                    mount.issues.push(NotationIssue {
                        code: "E-NAME-021",
                        symbol: operator.symbol.clone(),
                        detail: format!("`{}` is unary and cannot be infix", operator.binds.0),
                    });
                    continue;
                }
                Fixity::Prefix | Fixity::Postfix if arity != 1 => {
                    mount.issues.push(NotationIssue {
                        code: "E-NAME-021",
                        symbol: operator.symbol.clone(),
                        detail: format!(
                            "`{}` is not unary and cannot be {}",
                            operator.binds.0,
                            fixity.as_str()
                        ),
                    });
                    continue;
                }
                _ => {}
            }
        }
        let precedence = operator
            .precedence
            .unwrap_or_else(|| default_precedence(fixity));
        if let Some((_, previous_fixity, previous_precedence, previous_binds)) = by_symbol
            .iter()
            .find(|(symbol, _, _, _)| symbol == &operator.symbol)
        {
            mount.issues.push(NotationIssue {
                code: "E-NAME-020",
                symbol: operator.symbol.clone(),
                detail: format!(
                    "duplicate notation symbol `{}` (previously bound to `{previous_binds}`)",
                    operator.symbol
                ),
            });
            if previous_fixity == &fixity
                && previous_precedence == &precedence
                && previous_binds != &canonical_binds
            {
                mount.issues.push(NotationIssue {
                    code: "E-NAME-022",
                    symbol: operator.symbol.clone(),
                    detail: format!(
                        "ambiguous notation: `{}` at precedence {precedence} {} binds `{previous_binds}` and `{canonical_binds}`",
                        operator.symbol,
                        fixity.as_str()
                    ),
                });
            }
            continue;
        }
        by_symbol.push((
            operator.symbol.clone(),
            fixity,
            precedence,
            canonical_binds.clone(),
        ));
        mount.entries.push(NotationEntry {
            symbol: operator.symbol.clone(),
            binds: canonical_binds,
            fixity,
            precedence,
            arity,
            scope: "prelude".into(),
        });
    }
    mount
}

/// Check notation use against the bound arity; known mismatches refuse
/// (`E-NAME-021`), unknown arities pass (deferred to sema).
#[must_use]
pub fn check_use_arity(entry: &NotationEntry, used: usize) -> Option<NotationIssue> {
    let Some(arity) = entry.arity else {
        return None; // unknown arity: no enforcement
    };
    (usize::from(arity) != used).then(|| NotationIssue {
        code: "E-NAME-021",
        symbol: entry.symbol.clone(),
        detail: format!(
            "`{}` expects {arity} operand(s), found {used}",
            entry.symbol
        ),
    })
}

/// Aliasing surface names to canonical names when declared.
#[must_use]
pub fn resolve_alias(aliases: &[(&str, &str)], surface: &str) -> Option<String> {
    aliases
        .iter()
        .find(|(alias, _)| *alias == surface)
        .map(|(_, canonical)| (*canonical).to_string())
}
