//! LaTeX document/ math-environment scanning.

use super::*;

pub(super) fn has_formula_delimiters(source: &str) -> bool {
    source.contains('$') || source.contains("\\[")
}

pub(super) fn parse_bare_math(source: &str) -> Result<MathLayoutGraph, LayoutError> {
    let ast = parse_math_str(source, 0)?;
    let mut builder = GraphBuilder::new(source.to_string());
    let region = builder.add_node(LayoutContent::FormulaRegion, (0, source.len()));
    let root = emit(&mut builder, &ast);
    builder.add_edge(region, root, SpatialRelation::Contains);
    Ok(builder.finish())
}

pub(super) fn parse_document(source: &str) -> Result<MathLayoutGraph, LayoutError> {
    let mut builder = GraphBuilder::new(source.to_string());
    let bytes = source.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes.get(pos) == Some(&b'$') {
            let open = pos;
            let inner_start = open + 1;
            let Some(close) = find_unescaped_dollar(bytes, inner_start) else {
                return Err(LayoutError::UnterminatedDollar { offset: open });
            };
            let Some(inner) = source.get(inner_start..close) else {
                return Err(LayoutError::UnterminatedDollar { offset: open });
            };
            let ast = parse_math_str(inner, inner_start)?;
            let region = builder.add_node(LayoutContent::FormulaRegion, (open, close + 1));
            let root = emit(&mut builder, &ast);
            builder.add_edge(region, root, SpatialRelation::Contains);
            pos = close + 1;
        } else if source
            .get(pos..)
            .is_some_and(|rest| rest.starts_with("\\["))
        {
            let open = pos;
            let inner_start = open + 2;
            let Some(rel) = source.get(inner_start..).and_then(|rest| rest.find("\\]")) else {
                return Err(LayoutError::UnterminatedDisplay { offset: open });
            };
            let close = inner_start + rel;
            let Some(inner) = source.get(inner_start..close) else {
                return Err(LayoutError::UnterminatedDisplay { offset: open });
            };
            let ast = parse_math_str(inner, inner_start)?;
            let region = builder.add_node(LayoutContent::FormulaRegion, (open, close + 2));
            let root = emit(&mut builder, &ast);
            builder.add_edge(region, root, SpatialRelation::Contains);
            pos = close + 2;
        } else {
            pos += source
                .get(pos..)
                .and_then(|rest| rest.chars().next())
                .map_or(1, char::len_utf8);
        }
    }
    Ok(builder.finish())
}

pub(super) fn find_unescaped_dollar(bytes: &[u8], start: usize) -> Option<usize> {
    bytes.get(start..).and_then(|rest| {
        rest.iter()
            .position(|byte| *byte == b'$')
            .map(|rel| start + rel)
    })
}
