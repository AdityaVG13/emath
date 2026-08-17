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
    /// World name from `emath custom Name:`.
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
/// `limits` bounds source size and token budget; every violation yields a typed
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

    // `max_tokens` is a token budget, not a line count: scanning stops as
    // soon as the budget is spent, so pathological input cannot blow past it.
    let mut tokens_left = limits.max_tokens;
    let mut lines: Vec<(usize, &str)> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let content = line.trim();
        if content.is_empty() {
            continue;
        }
        let count = content.split_whitespace().count();
        if count > tokens_left {
            break;
        }
        tokens_left -= count;
        lines.push((index + 1, content));
    }

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

    // Declaration header: `emath custom Name:`.
    let header_index = lines
        .iter()
        .position(|(_, content)| content.starts_with("emath custom"));
    if let Some(index) = header_index {
        let (line, content) = lines[index];
        match parse_header(content) {
            Some(name) => world_name = name,
            None => errors.push(GenesisError::new(
                "E-SYN-201",
                format!("line {line}: malformed header, expected `emath custom Name:`"),
            )),
        }
    } else {
        errors.push(GenesisError::new(
            "E-SYN-201",
            "missing `emath custom Name:` declaration header",
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

/// Parses `emath custom Name:` (unified declaration-head spelling).
fn parse_header(content: &str) -> Option<String> {
    let tail = content.strip_prefix("emath custom ")?.strip_suffix(':')?;
    if tail.is_empty() {
        return None;
    }
    Some(tail.to_string())
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

    #[test]
    fn max_tokens_is_a_token_budget_not_a_line_count() {
        let source = "emath custom W:
  body:
  a b c d e f
  answer:
  return r";
        let admitted = parse_genesis(source, &Limits::default());
        assert!(
            admitted.is_ok(),
            "full-budget parse must admit the fixture; errors: {admitted:?}"
        );
        // The same file carries 15 tokens; a budget of 8 must cut the scan
        // before `answer:`, so the missing-answer refusal fires. A line
        // count of 5 would keep everything and admit.
        let limits = Limits {
            max_tokens: 8,
            ..Limits::default()
        };
        let refused = parse_genesis(source, &limits);
        assert!(
            refused.is_err(),
            "token budget must stop the scan before `answer:`, got {refused:?}"
        );
    }
}
