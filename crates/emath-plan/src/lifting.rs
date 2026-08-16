//! Provider lifting.
//!
//! When no provider serves an operator/subset, the planner emits a Rust
//! provider trait (opaque handle) plus a parametric artifact: the same
//! source compiles once a conforming provider is registered, without
//! language changes.

/// One lifted method (opaque handle).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiftedMethod {
    /// Method name.
    pub name: String,
    /// Arguments as (name, type).
    pub args: Vec<(String, String)>,
    /// Return type.
    pub returns: String,
}

/// A lifted provider trait spec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderTraitSpec {
    /// Trait name.
    pub name: String,
    /// Missing operators this trait covers.
    pub missing_operators: Vec<String>,
    /// Lifted methods.
    pub methods: Vec<LiftedMethod>,
}

/// Lifts missing providers/operators into a deterministic trait spec.
#[must_use]
pub fn lift_missing(goal_target: &str, missing_operators: &[String]) -> ProviderTraitSpec {
    let mut methods: Vec<LiftedMethod> = missing_operators
        .iter()
        .enumerate()
        .map(|(index, operator)| LiftedMethod {
            name: sanitize_ident(operator, index),
            args: vec![("x".to_string(), "f64".to_string())],
            returns: "f64".to_string(),
        })
        .collect();
    methods.push(LiftedMethod {
        name: "__emath_provider_seal".to_string(),
        args: vec![],
        returns: "()".to_string(),
    });
    ProviderTraitSpec {
        name: format!("Op{}Provider", pascal_case(goal_target)),
        missing_operators: missing_operators.to_vec(),
        methods,
    }
}

/// Emits the provider trait source deterministically (byte-comparable).
#[must_use]
pub fn emit_provider_trait(spec: &ProviderTraitSpec) -> String {
    let mut out = String::new();
    out.push_str("#![forbid(unsafe_code)]\n\n");
    out.push_str("//! Parametric artifact: provider trait for missing operators.\n");
    out.push_str("//! Registered providers implement this trait; no language change.\n\n");
    let header = format!("pub trait {} {{\n", spec.name);
    out.push_str(&header);
    for method in &spec.methods {
        let args = method
            .args
            .iter()
            .map(|(name, ty)| format!("{name}: {ty}"))
            .collect::<Vec<_>>()
            .join(", ");
        let line = format!("    fn {}({args}) -> {};\n", method.name, method.returns);
        out.push_str(&line);
    }
    out.push_str("}\n");
    out
}

/// Sanitizes an operator string into a deterministic Rust identifier.
fn sanitize_ident(operator: &str, index: usize) -> String {
    let cleaned: String = operator
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        format!("op_{index}")
    } else {
        format!("op_{trimmed}")
    }
}

/// Pascal-cases a target name for use in a trait name.
fn pascal_case(target: &str) -> String {
    let mut out = String::new();
    let mut upper_next = true;
    for character in target.chars() {
        if character.is_ascii_alphanumeric() {
            if upper_next {
                out.extend(character.to_uppercase());
            } else {
                out.push(character);
            }
            upper_next = false;
        } else {
            upper_next = true;
        }
    }
    if out.is_empty() {
        "O".to_string()
    } else {
        out
    }
}
