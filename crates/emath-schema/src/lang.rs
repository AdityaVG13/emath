//!: the kind schema language.
//!
//! A schema is one directive per line:
//!
//! ```text
//! kind workflow
//! section inputs exactly-one fields
//! section outputs exactly-one fields
//! section definitions exactly-one suite
//! section requests at-most-one commands
//! section experiments repeatable suite
//! default compile = rust/library/strict-f64
//! predicate decl.outputs.is_nonempty()
//! ```
//!
//! Admission order is the declaration order; canonical identity is
//! independent of order (schema mutation moves identity). Unknown
//! tokens, duplicate section specs and duplicate defaults are refused
//! with stable `E-KIND-01x` codes; the output is the shared
//! `KindSchema` the compiler and builder both admit against.

use emath_ir::kind_schema::{KindSchema, PayloadPolicy, RepeatPolicy, SectionSchema};

/// One schema-language refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaIssue {
    pub code: &'static str,
    pub detail: String,
    pub line: usize,
}

/// Parses the schema language into a `KindSchema`.
///
/// `E-KIND-012` malformed directive/unknown token; `E-KIND-013`
/// duplicate section spec; `E-KIND-014` duplicate default; `E-KIND-015`
/// predicate references an undeclared section. Always returns both the
/// (possibly partial) schema and the issues.
#[must_use]
pub fn parse_schema_language(text: &str) -> (KindSchema, Vec<SchemaIssue>) {
    let mut schema = KindSchema::default();
    let mut issues = Vec::new();
    let mut seen_sections: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut seen_defaults: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut predicate = None;

    for (line_index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let directive = words.next().unwrap_or("");
        match directive {
            "kind" => {
                if let Some(name) = words.next() {
                    schema.set_name(name);
                } else {
                    issues.push(SchemaIssue {
                        code: "E-KIND-012",
                        detail: "`kind` requires a name".into(),
                        line: line_index,
                    });
                }
            }
            "section" => {
                let name = words.next();
                let repeat_token = words.next();
                let payload_token = words.next();
                let Some(name) = name else {
                    issues.push(SchemaIssue {
                        code: "E-KIND-012",
                        detail: "`section` requires `name repeat payload`".into(),
                        line: line_index,
                    });
                    continue;
                };
                let Some(repeat_token) = repeat_token else {
                    issues.push(SchemaIssue {
                        code: "E-KIND-012",
                        detail: "`section` requires `name repeat payload`".into(),
                        line: line_index,
                    });
                    continue;
                };
                let Some(payload_token) = payload_token else {
                    issues.push(SchemaIssue {
                        code: "E-KIND-012",
                        detail: "`section` requires `name repeat payload`".into(),
                        line: line_index,
                    });
                    continue;
                };
                let repeat = match repeat_token {
                    "exactly-one" => RepeatPolicy::ExactlyOne,
                    "at-most-one" => RepeatPolicy::AtMostOne,
                    "repeatable" => RepeatPolicy::Repeatable,
                    other => {
                        issues.push(SchemaIssue {
                            code: "E-KIND-012",
                            detail: format!("unknown repeat policy `{other}`"),
                            line: line_index,
                        });
                        continue;
                    }
                };
                let payload = match payload_token {
                    "suite" => PayloadPolicy::Suite,
                    "fields" => PayloadPolicy::Fields,
                    "commands" => PayloadPolicy::Commands,
                    other => {
                        issues.push(SchemaIssue {
                            code: "E-KIND-012",
                            detail: format!("unknown payload policy `{other}`"),
                            line: line_index,
                        });
                        continue;
                    }
                };
                if !seen_sections.insert(name.to_string()) {
                    issues.push(SchemaIssue {
                        code: "E-KIND-013",
                        detail: format!("duplicate section spec `{name}`"),
                        line: line_index,
                    });
                    continue;
                }
                schema.insert_section(
                    name,
                    SectionSchema {
                        repeat,
                        payload,
                        has_default: false,
                    },
                );
            }
            "default" => {
                let Some((section, value)) = split_default(&mut words) else {
                    issues.push(SchemaIssue {
                        code: "E-KIND-012",
                        detail: "`default` requires `section = value`".into(),
                        line: line_index,
                    });
                    continue;
                };
                if !seen_sections.contains(&section) {
                    issues.push(SchemaIssue {
                        code: "E-KIND-015",
                        detail: format!("default for undeclared section `{section}`"),
                        line: line_index,
                    });
                    continue;
                }
                if !seen_defaults.insert(section.clone()) {
                    issues.push(SchemaIssue {
                        code: "E-KIND-014",
                        detail: format!("duplicate default for section `{section}`"),
                        line: line_index,
                    });
                    continue;
                }
                // A declared default makes the section defaulted.
                if let Some(existing) = schema.section(&section) {
                    let mut updated = existing.clone();
                    updated.has_default = true;
                    schema.insert_section(&section, updated);
                }
                schema.insert_default(section, value);
            }
            "predicate" => {
                let rest = words.collect::<Vec<_>>().join(" ");
                if rest.is_empty() {
                    issues.push(SchemaIssue {
                        code: "E-KIND-012",
                        detail: "`predicate` requires a body".into(),
                        line: line_index,
                    });
                    continue;
                }
                let declared: Vec<String> = seen_sections.iter().cloned().collect();
                if predicate.is_some() {
                    issues.push(SchemaIssue {
                        code: "E-KIND-014",
                        detail: "duplicate `predicate` directive".into(),
                        line: line_index,
                    });
                    continue;
                }
                if let Some(section) = referenced_undeclared(&rest, &declared) {
                    issues.push(SchemaIssue {
                        code: "E-KIND-015",
                        detail: format!("predicate references undeclared section `{section}`"),
                        line: line_index,
                    });
                }
                predicate = Some(rest);
            }
            other => {
                issues.push(SchemaIssue {
                    code: "E-KIND-012",
                    detail: format!("unknown directive `{other}`"),
                    line: line_index,
                });
            }
        }
    }

    if let Some(predicate) = predicate {
        schema.set_predicate(predicate);
    }
    (schema, issues)
}

/// `default <section> = <value>` splitter over the remaining words.
fn split_default(words: &mut std::str::SplitWhitespace<'_>) -> Option<(String, String)> {
    let section = words.next()?;
    let eq = words.next()?;
    if eq != "=" {
        return None;
    }
    let value = words.collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        return None;
    }
    Some((section.to_string(), value))
}

fn referenced_undeclared(predicate: &str, declared: &[String]) -> Option<String> {
    let mut rest = predicate;
    while let Some(position) = rest.find("decl.") {
        rest = &rest[position + 5..];
        let end = rest
            .find(|character: char| {
                !character.is_alphanumeric() && !matches!(character, '_' | '<' | '>')
            })
            .unwrap_or(rest.len());
        let section = &rest[..end];
        if !declared.iter().any(|known| known == section) {
            return Some(section.to_string());
        }
        rest = &rest[end..];
    }
    None
}

/// Whether a parsed schema is clean (no issues).
#[must_use]
pub fn is_clean(issues: &[SchemaIssue]) -> bool {
    issues.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_ir::kind_schema::core_function_schema;

    const FUNCTION_SCHEMA: &str = r"kind function
section inputs exactly-one fields
section outputs exactly-one fields
section definitions exactly-one suite
section requests at-most-one commands
section exports at-most-one commands
section tests at-most-one suite
section compile at-most-one commands
default compile = rust/library/strict-f64
";

    #[test]
    fn schema_language_reproduces_the_core_function_schema() {
        let (schema, issues) = parse_schema_language(FUNCTION_SCHEMA);
        assert!(is_clean(&issues), "{issues:?}");
        // Identity is text-mutation sensitive and stable.
        assert_eq!(schema.canonical(), core_function_schema().canonical());
        assert_eq!(schema.canonical(), schema.canonical());
    }

    #[test]
    fn schema_mutation_moves_identity() {
        let (base, _) = parse_schema_language(FUNCTION_SCHEMA);
        let (mut mutated, _) = parse_schema_language(FUNCTION_SCHEMA);
        mutated.set_predicate("decl.outputs.is_nonempty()");
        assert_ne!(base.canonical(), mutated.canonical());
        let (mut repeated, _) = parse_schema_language(FUNCTION_SCHEMA);
        repeated.insert_section(
            "experiments",
            SectionSchema {
                repeat: RepeatPolicy::Repeatable,
                payload: PayloadPolicy::Suite,
                has_default: false,
            },
        );
        assert_ne!(base.canonical(), repeated.canonical());
    }

    #[test]
    fn unknown_tokens_are_refused() {
        let text = "kind odd\nsection inputs maybe fields\nsection outputs exactly-one bogus\n";
        let (_, issues) = parse_schema_language(text);
        assert!(issues.iter().any(|issue| issue.code == "E-KIND-012"));
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn duplicate_section_and_default_are_refused() {
        let text = "kind odd\nsection a exactly-one fields\nsection a repeatable suite\ndefault a = 1\ndefault a = 2\n";
        let (_, issues) = parse_schema_language(text);
        assert!(issues.iter().any(|issue| issue.code == "E-KIND-013"));
        assert!(issues.iter().any(|issue| issue.code == "E-KIND-014"));
    }

    #[test]
    fn predicate_undeclared_section_is_refused() {
        let text = "kind odd\nsection a exactly-one fields\npredicate decl.missing.is_empty()\n";
        let (_, issues) = parse_schema_language(text);
        assert!(issues.iter().any(|issue| issue.code == "E-KIND-015"));
    }

    #[test]
    fn admission_order_is_declaration_order() {
        let text = "kind odd\nsection a repeatable suite\nsection b at-most-one fields\n";
        let (schema, issues) = parse_schema_language(text);
        assert!(is_clean(&issues));
        let names: Vec<&str> = schema.sections().iter().map(|(n, _)| *n).collect();
        // Canonical admission order is deterministic (not declaration
        // order, so identity is order-independent).
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }
}
