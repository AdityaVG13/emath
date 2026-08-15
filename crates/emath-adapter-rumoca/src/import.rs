//!: Modelica subset import.
//!
//! `import_modelica` produces retained foreign-model declarations with
//! adapter identity. The source text is preserved verbatim — there is no
//! silent source rewrite; every recognized construct is classified through
//! the semantic mapping table, and unsupported constructs are typed
//! refusals.

use emath_core::fnv1a64_bytes;

use crate::map::{classify, MappingClass};

/// A retained foreign-model declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignModelDeclaration {
    /// Model name.
    pub name: String,
    /// Verbatim source text (never rewritten).
    pub source: String,
    /// Adapter identity.
    pub adapter: String,
    /// Declared parameters (in source order).
    pub parameters: Vec<String>,
    /// Counted equations.
    pub equations: usize,
    /// Recognized constructs with their mapping classification.
    pub constructs: Vec<(String, MappingClass)>,
    identity: u64,
}

impl ForeignModelDeclaration {
    /// Deterministic canonical rendering.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "foreign:name:{},adapter:{},parameters:{},equations:{},constructs:{}",
            self.name,
            self.adapter,
            self.parameters.join(","),
            self.equations,
            self.constructs
                .iter()
                .map(|(name, class)| format!("{name}:{class:?}"))
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    /// FNV-1a64 identity over the canonical rendering.
    #[must_use]
    pub fn content_identity(&self) -> u64 {
        self.identity
    }
}

/// Import failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportError {
    /// Stable code (`E-PROV-240`/`E-PROV-241`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Imports a Modelica subset source into retained declarations.
pub fn import_modelica(source: &str) -> Result<Vec<ForeignModelDeclaration>, ImportError> {
    let mut declarations = Vec::new();
    let mut remaining = source;
    while let Some(start) = remaining.find("model ") {
        let header = &remaining[start..];
        let name_end = header[6..]
            .find(|c: char| c.is_whitespace() || c == '(')
            .map_or(header.len(), |offset| offset + 6);
        let name = header[6..name_end].trim();
        if name.is_empty() {
            return Err(ImportError {
                code: "E-PROV-240",
                message: "malformed `model` header".into(),
            });
        }
        let body = match header.find("end ") {
            Some(end) => &header[..end],
            None => {
                return Err(ImportError {
                    code: "E-PROV-240",
                    message: format!("model `{name}` has no `end` terminator"),
                });
            }
        };
        declarations.push(parse_declaration(name, body)?);
        remaining = &header[body.len()..];
    }
    if declarations.is_empty() {
        return Err(ImportError {
            code: "E-PROV-240",
            message: "no `model` declaration found".into(),
        });
    }
    Ok(declarations)
}

fn parse_declaration(name: &str, body: &str) -> Result<ForeignModelDeclaration, ImportError> {
    let mut parameters = Vec::new();
    let mut equations = 0usize;
    let mut constructs: Vec<(String, MappingClass)> = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.starts_with("parameter") {
            if let Some(equals) = line.find('=') {
                let declaration = &line[..equals];
                let ident = declaration
                    .split_whitespace()
                    .last()
                    .unwrap_or_default()
                    .trim();
                if !ident.is_empty() {
                    parameters.push(ident.to_string());
                }
            }
            constructs.push(("parameter".to_string(), MappingClass::Exact));
        } else if line.contains('=') || line.contains("der(") {
            equations += usize::from(line.contains('='));
            constructs.push(("equation".to_string(), MappingClass::Exact));
        }
        for keyword in [
            "der", "connect", "when", "record", "sample", "outer", "inner",
        ] {
            if line.contains(keyword) && !constructs.iter().any(|(known, _)| known == keyword) {
                let known = classify(keyword).ok_or_else(|| ImportError {
                    code: "E-PROV-241",
                    message: format!("construct `{keyword}` is not in the mapping table"),
                })?;
                constructs.push((keyword.to_string(), known.class));
            }
        }
    }
    if let Some((construct, _)) = constructs
        .iter()
        .find(|(_, class)| *class == MappingClass::Unsupported)
    {
        return Err(ImportError {
            code: "E-PROV-241",
            message: format!("unsupported construct `{construct}` in model `{name}`"),
        });
    }
    let mut declaration = ForeignModelDeclaration {
        name: name.to_string(),
        source: body.to_string(),
        adapter: "rumoca".to_string(),
        parameters,
        equations,
        constructs,
        identity: 0,
    };
    declaration.identity = fnv1a64_bytes(declaration.canonical().as_bytes());
    Ok(declaration)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASS_SPRING: &str = "\
model MassSpring
  parameter Real m = 1;
  parameter Real c = 1;
  parameter Real k = 1;
  Real x;
  Real v;
equation
  der(x) = v;
  m * der(v) = -c * v - k * x;
end MassSpring;
";

    #[test]
    fn mass_spring_imports_with_retention() {
        let declarations = import_modelica(MASS_SPRING).unwrap();
        assert_eq!(declarations.len(), 1);
        let decl = &declarations[0];
        assert_eq!(decl.name, "MassSpring");
        assert_eq!(decl.adapter, "rumoca");
        assert_eq!(decl.parameters, ["m", "c", "k"]);
        assert_eq!(decl.equations, 2);
        assert!(decl.source.contains("der(x) = v;"));
        // parameter x3, equation x2, der x1 = 6 exact mappings.
        assert_eq!(
            decl.constructs
                .iter()
                .filter(|(_, class)| *class == MappingClass::Exact)
                .count(),
            6
        );
        let again = import_modelica(MASS_SPRING).unwrap();
        assert_eq!(decl.canonical(), again[0].canonical());
    }

    #[test]
    fn unsupported_construct_is_typed_refusal() {
        let errors = import_modelica(
            "model Sampled\n  Real x;\nequation\n  x = sample(0, 1);\nend Sampled;\n",
        )
        .unwrap_err();
        assert_eq!(errors.code, "E-PROV-241");
    }

    #[test]
    fn missing_terminator_is_typed_error() {
        let errors = import_modelica("model Broken\n  Real x;\n").unwrap_err();
        assert_eq!(errors.code, "E-PROV-240");
    }
}
