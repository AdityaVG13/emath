//! core: scoped notation.
//!
//! Imports an operator declaration (`` surface) into a
//! scoped notation table with precedence/fixity, alias resolution,
//! arity checks and ambiguity detection. Rendering follows canonical
//! rules (`infix`/`prefix`/`postfix` place the data operand).

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
    pub arity: u8,
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

/// Mounts declared operators into the scoped notation table:
/// - empty symbols are refused (`E-NAME-021`);
/// - duplicate symbols are refused (`E-NAME-020`);
/// - alias resolution maps surface names to canonical names;
/// - fixity/arity incompatibility is refused (`E-NAME-021`);
/// - same precedence + same fixity with different bindings is an
///   ambiguity (`E-NAME-022`).
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
            arity: arity.unwrap_or(2),
            scope: "prelude".into(),
        });
    }
    mount
}

/// Checks a use of the notation against the bound arity; known-arity
/// mismatches are refused (`E-NAME-021`), unknown symbols are allowed
/// (deferred to sema).
#[must_use]
pub fn check_use_arity(entry: &NotationEntry, used: usize) -> Option<NotationIssue> {
    (usize::from(entry.arity) != used).then(|| NotationIssue {
        code: "E-NAME-021",
        symbol: entry.symbol.clone(),
        detail: format!(
            "`{}` expects {} operand(s), found {used}",
            entry.symbol, entry.arity
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

#[cfg(test)]
mod tests {
    use super::*;
    use emath_core::QualifiedName;

    fn op(symbol: &str, binds: &str, fixity: Fixity, precedence: Option<u8>) -> DeclaredOperator {
        DeclaredOperator {
            symbol: symbol.into(),
            binds: QualifiedName(binds.into()),
            fixity,
            precedence,
            provenance: "operators".into(),
        }
    }

    fn no_aliases() -> NotationContext<'static> {
        NotationContext {
            symbols: &["core::math::pow", "core::math::neg", "core::math::abs"],
            aliases: &[],
        }
    }

    #[test]
    fn mounts_infix_prefix_and_postfix_with_defaults() {
        let declared = [
            op("**", "core::math::pow", Fixity::Infix, Some(12)),
            op("~", "core::math::neg", Fixity::Prefix, None),
            op("!", "core::math::abs", Fixity::Postfix, None),
        ];
        let mount = mount_notation(&no_aliases(), &declared);
        assert!(mount.issues.is_empty(), "{:?}", mount.issues);
        assert_eq!(mount.entries.len(), 3);
        assert_eq!(mount.entries[0].precedence, 12);
        assert_eq!(mount.entries[1].precedence, 16);
        assert_eq!(mount.entries[2].arity, 1);
    }

    #[test]
    fn duplicate_symbol_is_refused() {
        let declared = [
            op("**", "core::math::pow", Fixity::Infix, Some(12)),
            op("**", "core::math::mul", Fixity::Infix, Some(10)),
        ];
        let mount = mount_notation(&no_aliases(), &declared);
        assert!(mount.issues.iter().any(|i| i.code == "E-NAME-020"));
    }

    #[test]
    fn unary_symbol_cannot_be_infix() {
        let declared = [op("**", "core::math::neg", Fixity::Infix, None)];
        let mount = mount_notation(&no_aliases(), &declared);
        assert!(mount.issues.iter().any(|i| i.code == "E-NAME-021"));
    }

    #[test]
    fn alias_resolution_maps_surface_to_canonical() {
        let context = NotationContext {
            symbols: &["core::math::pow"],
            aliases: &[("math::power", "core::math::pow")],
        };
        let declared = [op("**", "math::power", Fixity::Infix, Some(12))];
        let mount = mount_notation(&context, &declared);
        assert!(mount.issues.is_empty());
        assert_eq!(mount.entries[0].binds, "core::math::pow");
    }

    #[test]
    fn arity_mismatch_on_use_is_refused() {
        let declared = [op("**", "core::math::pow", Fixity::Infix, Some(12))];
        let mount = mount_notation(&no_aliases(), &declared);
        let entry = &mount.entries[0];
        assert!(check_use_arity(entry, 2).is_none());
        assert_eq!(check_use_arity(entry, 1).unwrap().code, "E-NAME-021");
    }

    #[test]
    fn same_precedence_same_fixity_is_ambiguous() {
        let declared = [
            op("⊕", "core::math::add", Fixity::Infix, Some(10)),
            op("⊕", "core::math::max", Fixity::Infix, Some(10)),
        ];
        let mount = mount_notation(&no_aliases(), &declared);
        assert!(mount.issues.iter().any(|i| i.code == "E-NAME-022"));
    }

    #[test]
    fn canonical_rendering_is_stable() {
        let mount = mount_notation(
            &no_aliases(),
            &[
                op("**", "core::math::pow", Fixity::Infix, Some(12)),
                op("~", "core::math::neg", Fixity::Prefix, None),
                op("!", "core::math::abs", Fixity::Postfix, None),
            ],
        );
        let infix = mount
            .entries
            .iter()
            .find(|e| e.fixity == Fixity::Infix)
            .unwrap();
        let prefix = mount
            .entries
            .iter()
            .find(|e| e.fixity == Fixity::Prefix)
            .unwrap();
        let postfix = mount
            .entries
            .iter()
            .find(|e| e.fixity == Fixity::Postfix)
            .unwrap();
        assert_eq!(infix.render("x", "2"), "x ** 2");
        assert_eq!(prefix.render("", "x"), "~ x");
        assert_eq!(postfix.render("x", ""), "x !");
    }
}
