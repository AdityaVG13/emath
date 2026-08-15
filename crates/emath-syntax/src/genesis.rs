//! G0: custom-world genesis grammar.
//!
//! Parses `emath custom` declarations (see
//! `language/grammar/GENESIS_GRAMMAR_ADDENDUM.ebnf` and
//! `language/examples/01_arbitrary_glyphs.emath`). The parser is UTF-8
//! byte-exact: glyphs (including non-ASCII identifier bytes) are preserved
//! verbatim into [`GenesisFile::body_text`] for the forest stage. Malformed
//! sections are recovered: parsing continues, every problem is reported as a
//! typed [`GenesisError`], and the function never panics on user input.

use emath_core::limits::Limits;

/// Parsed `emath custom` genesis file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisFile {
    /// World name from `emath custom <Name>:`.
    pub world_name: String,
    /// Trimmed body expression, glyph-preserved for the forest stage.
    pub body_text: String,
    /// `explore` meaning-clause items, in file order.
    pub explore: Vec<String>,
    /// `protect` meaning-clause items, in file order.
    pub protect: Vec<String>,
    /// `keep: pareto N` budget, if declared.
    pub keep_pareto: Option<u32>,
    /// Name returned by the `answer:` section (after `return`).
    pub answer: String,
}

/// Typed genesis grammar error with a stable diagnostic code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisError {
    /// Stable code (`E-SYN-2xx`, never repurposed).
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
}

impl GenesisError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Top,
    Body,
    Construct,
    Explore,
    Protect,
    Keep,
    Answer,
    Skip,
}

/// Parses a genesis source into a [`GenesisFile`].
///
/// `limits` bounds source size and line count; every violation yields a typed
/// error instead of a panic. Returns `Err` with all recovered errors when any
/// section is malformed.
pub fn parse_genesis(text: &str, limits: &Limits) -> Result<GenesisFile, Vec<GenesisError>> {
    let mut errors = Vec::new();
    if let Err(max) = limits.check_source(text.len()) {
        errors.push(GenesisError::new(
            "E-SYN-207",
            format!("source is {} bytes; limit is {max} bytes", text.len()),
        ));
    }

    let lines: Vec<(usize, &str)> = text
        .lines()
        .take(limits.max_tokens)
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, content)| !content.is_empty())
        .collect();

    let mut world_name = String::new();
    let mut mode = Mode::Top;
    let mut in_construct = false;
    let mut body_parts: Vec<String> = Vec::new();
    let mut body_seen = false;
    let mut answer_seen = false;
    let mut explore: Vec<String> = Vec::new();
    let mut protect: Vec<String> = Vec::new();
    let mut keep_pareto: Option<u32> = None;
    let mut answer = String::new();

    // Declaration header: `emath custom <Name>:`.
    let header_index = lines
        .iter()
        .position(|(_, content)| content.starts_with("emath custom"));
    if let Some(index) = header_index {
        let (line, content) = lines[index];
        match parse_header(content) {
            Some(name) => world_name = name,
            None => errors.push(GenesisError::new(
                "E-SYN-201",
                format!("line {line}: malformed header, expected `emath custom <Name>:`"),
            )),
        }
    } else {
        errors.push(GenesisError::new(
            "E-SYN-201",
            "missing `emath custom <Name>:` declaration header",
        ));
    }

    let rest = lines.iter().skip(header_index.map_or(0, |index| index + 1));
    for &(line, content) in rest {
        if content.ends_with(':') {
            match content {
                "body:" => {
                    if body_seen {
                        errors.push(GenesisError::new(
                            "E-SYN-209",
                            format!("line {line}: duplicate `body:` section"),
                        ));
                    }
                    body_seen = true;
                    mode = Mode::Body;
                }
                "construct meaning:" => {
                    in_construct = true;
                    mode = Mode::Construct;
                }
                "answer:" => {
                    if answer_seen {
                        errors.push(GenesisError::new(
                            "E-SYN-209",
                            format!("line {line}: duplicate `answer:` section"),
                        ));
                    }
                    answer_seen = true;
                    mode = Mode::Answer;
                }
                "explore:" | "protect:" | "keep:" if in_construct => {
                    mode = match content {
                        "explore:" => Mode::Explore,
                        "protect:" => Mode::Protect,
                        _ => Mode::Keep,
                    };
                }
                "explore:" | "protect:" | "keep:" => {
                    errors.push(GenesisError::new(
                        "E-SYN-202",
                        format!("line {line}: `{content}` clause outside `construct meaning:`"),
                    ));
                }
                other if mode == Mode::Construct => {
                    errors.push(GenesisError::new(
                        "E-SYN-202",
                        format!("line {line}: unsupported `construct meaning:` clause `{other}`"),
                    ));
                    mode = Mode::Skip;
                }
                other => {
                    errors.push(GenesisError::new(
                        "E-SYN-202",
                        format!("line {line}: unknown section `{other}`"),
                    ));
                    mode = Mode::Skip;
                }
            }
        } else {
            match mode {
                Mode::Body => body_parts.push(content.to_string()),
                Mode::Explore => explore.push(content.to_string()),
                Mode::Protect => protect.push(content.to_string()),
                Mode::Keep => match parse_pareto(content) {
                    Some(value) => keep_pareto = Some(value),
                    None => errors.push(GenesisError::new(
                        "E-SYN-203",
                        format!("line {line}: malformed `keep:` clause, expected `pareto <u32>`"),
                    )),
                },
                Mode::Answer => match parse_answer_clause(content) {
                    Some(value) => answer = value,
                    None => errors.push(GenesisError::new(
                        "E-SYN-204",
                        format!(
                            "line {line}: malformed `answer:` clause, expected `return <name>`"
                        ),
                    )),
                },
                Mode::Top | Mode::Construct => errors.push(GenesisError::new(
                    "E-SYN-208",
                    format!("line {line}: unexpected content `{content}`"),
                )),
                Mode::Skip => {}
            }
        }
    }

    if !body_seen {
        errors.push(GenesisError::new("E-SYN-205", "missing `body:` section"));
    }
    if !answer_seen {
        errors.push(GenesisError::new("E-SYN-206", "missing `answer:` section"));
    }

    if errors.is_empty() {
        Ok(GenesisFile {
            world_name,
            body_text: body_parts.join(" "),
            explore,
            protect,
            keep_pareto,
            answer,
        })
    } else {
        Err(errors)
    }
}

/// Parses `emath custom <Name>:`.
fn parse_header(content: &str) -> Option<String> {
    let tail = content.strip_prefix("emath custom ")?.strip_suffix(':')?;
    let name = tail.strip_prefix('<')?.strip_suffix('>')?;
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

/// Parses `pareto <u32>`.
fn parse_pareto(content: &str) -> Option<u32> {
    let mut parts = content.split_whitespace();
    let keyword = parts.next()?;
    if keyword != "pareto" {
        return None;
    }
    parts
        .next()?
        .parse::<u32>()
        .ok()
        .filter(|_| parts.next().is_none())
}

/// Parses `return <name>` or `return [id, ...]` (first identifier wins).
fn parse_answer_clause(content: &str) -> Option<String> {
    let rest = content.strip_prefix("return")?.trim();
    if rest.is_empty() {
        return None;
    }
    let name = rest
        .strip_prefix('[')
        .and_then(|list| list.strip_suffix(']'))
        .and_then(|list| list.split(',').next())
        .map_or(rest, str::trim);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFERENCE: &str = include_str!("../../../language/examples/01_arbitrary_glyphs.emath");
    const REFERENCE_BODY: &str = "⧖(a ⋈ b) ⊛ ζ";

    #[test]
    fn reference_file_parses_and_preserves_glyphs() {
        let file = parse_genesis(REFERENCE, &Limits::default()).unwrap();
        assert_eq!(file.world_name, "AlienGlyphs");
        assert_eq!(file.body_text, REFERENCE_BODY);
        // Byte-exact glyph survival (UTF-8 identity, no transcoding).
        assert_eq!(file.body_text.as_bytes(), REFERENCE_BODY.as_bytes());
        assert_eq!(
            file.explore,
            [
                "free_symbolic",
                "Boolean_algebra",
                "modular_numeric",
                "matrix",
                "graph"
            ]
        );
        assert_eq!(file.protect, ["total", "deterministic"]);
        assert_eq!(file.keep_pareto, Some(8));
        assert_eq!(file.answer, "interpretation_portfolio");
    }

    #[test]
    fn malformed_header_is_typed_error() {
        let errors = parse_genesis(
            "emath custom AlienGlyphs:\n    body:\n        x",
            &Limits::default(),
        )
        .unwrap_err();
        assert!(errors.iter().any(|e| e.code == "E-SYN-201"));
    }

    #[test]
    fn missing_body_and_answer_are_typed_errors() {
        let errors = parse_genesis(
            "emath custom <AlienGlyphs>:\n    answer:\n        return portfolio",
            &Limits::default(),
        )
        .unwrap_err();
        assert!(errors.iter().any(|e| e.code == "E-SYN-205"));
        assert!(!errors.iter().any(|e| e.code == "E-SYN-206"));
    }

    #[test]
    fn malformed_keep_recovers_and_reports() {
        let errors = parse_genesis(
            "emath custom <AlienGlyphs>:\n    body:\n        x\n    construct meaning:\n        keep:\n            pareto many\n",
            &Limits::default(),
        )
        .unwrap_err();
        assert!(errors.iter().any(|e| e.code == "E-SYN-203"));
    }

    #[test]
    fn unknown_section_recovers() {
        let errors = parse_genesis(
            "emath custom <AlienGlyphs>:\n    body:\n        x\n    bogus:\n        y\n    answer:\n        return ok\n",
            &Limits::default(),
        )
        .unwrap_err();
        assert!(errors.iter().any(|e| e.code == "E-SYN-202"));
    }

    #[test]
    fn oversized_source_is_typed_error() {
        let limits = Limits {
            max_source_bytes: 32,
            ..Limits::default()
        };
        let errors = parse_genesis(REFERENCE, &limits).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "E-SYN-207"));
    }

    #[test]
    fn answer_list_form_takes_first_identifier() {
        let file = parse_genesis(
            "emath custom <AlienGlyphs>:\n    body:\n        x\n    answer:\n        return [first, second]\n",
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(file.answer, "first");
    }
}
