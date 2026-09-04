//! Domain-neutral Stage-0 parser contract and structural gate.

/// Forms permanently owned by the Rust bootstrap parser. Mathematical names are
/// absent: capsules and syntax packs supply them as data.
pub const STAGE0_FORMS: &[&str] = &[
    "utf8-source",
    "layout",
    "generic-declaration",
    "generic-section",
    "use",
    "qualified-path",
    "identifier",
    "literal",
    "call",
    "index",
    "field-access",
    "list",
    "record",
    "local-let",
    "registered-operator-slot",
    "generic-binder",
    "hole",
    "unknown-glyph",
];

pub const EXCLUDED_DOMAIN_FORMS: &[&str] = &[
    "cipher",
    "puzzle",
    "protocol",
    "campaign",
    "frontier",
    "criterion",
    "reaction_network",
    "nabla",
    "braket",
    "graph_literal",
    "softmax",
    "hodge",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreservedGlyph {
    pub text: String,
    pub start: u32,
    pub end: u32,
}

/// Preserve non-ASCII mathematical-symbol runs with exact byte spans. This is
/// observation only: no symbol receives meaning here.
#[must_use]
pub fn unknown_glyphs(source: &str) -> Vec<PreservedGlyph> {
    let mut result = Vec::new();
    let mut start = None;
    for (offset, character) in source.char_indices() {
        let is_glyph =
            !character.is_ascii() && !character.is_alphanumeric() && !character.is_whitespace();
        match (start, is_glyph) {
            (None, true) => start = Some(offset),
            (Some(begin), false) => {
                result.push(glyph(source, begin, offset));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(begin) = start {
        result.push(glyph(source, begin, source.len()));
    }
    result
}

fn glyph(source: &str, start: usize, end: usize) -> PreservedGlyph {
    PreservedGlyph {
        text: source[start..end].to_string(),
        start: u32::try_from(start).unwrap_or(u32::MAX),
        end: u32::try_from(end).unwrap_or(u32::MAX),
    }
}

/// Structural lint for accidental feature-name matching in the parser/sema/
/// backend nucleus. Generic registry and schema code should not be passed here.
#[must_use]
pub fn forbidden_domain_matches(source: &str) -> Vec<&'static str> {
    EXCLUDED_DOMAIN_FORMS
        .iter()
        .copied()
        .filter(|name| {
            let snake = format!("\"{name}\"");
            let dashed = format!("\"{}\"", name.replace('_', "-"));
            source.contains(&snake) || source.contains(&dashed)
        })
        .collect()
}
