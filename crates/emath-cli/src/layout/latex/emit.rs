//! AST-to-layout-graph emission helpers.

use super::*;

pub(super) fn starts_atom(kind: &TokKind) -> bool {
    matches!(
        kind,
        TokKind::Letter(_)
            | TokKind::Number(_)
            | TokKind::LParen
            | TokKind::LBrace
            | TokKind::Command(_)
    )
}

pub(super) fn token_text(kind: &TokKind) -> String {
    match kind {
        TokKind::Letter(ch) => ch.to_string(),
        TokKind::Number(text) => text.clone(),
        TokKind::Plus => "+".to_string(),
        TokKind::Minus => "-".to_string(),
        TokKind::Star => "*".to_string(),
        TokKind::Slash => "/".to_string(),
        TokKind::Eq => "=".to_string(),
        TokKind::LParen => "(".to_string(),
        TokKind::RParen => ")".to_string(),
        TokKind::LBrace => "{".to_string(),
        TokKind::RBrace => "}".to_string(),
        TokKind::Caret => "^".to_string(),
        TokKind::Underscore => "_".to_string(),
        TokKind::Command(name) => format!("\\{name}"),
    }
}

pub(super) fn emit(builder: &mut GraphBuilder, ast: &Ast) -> NodeId {
    match &ast.kind {
        AstKind::Glyph(text) => builder.add_node(LayoutContent::Glyph(text.clone()), ast.span),
        AstKind::Infix { op, left, right } => {
            let row = builder.add_node(LayoutContent::Row, ast.span);
            let left_id = emit(builder, left);
            builder.add_edge(row, left_id, SpatialRelation::Contains);
            let op_span = (left.span.1, right.span.0);
            let op_id = builder.add_node(LayoutContent::Glyph(op.clone()), op_span);
            builder.add_edge(row, op_id, SpatialRelation::Contains);
            builder.add_edge(left_id, op_id, SpatialRelation::RightOf);
            let right_id = emit(builder, right);
            builder.add_edge(row, right_id, SpatialRelation::Contains);
            builder.add_edge(op_id, right_id, SpatialRelation::RightOf);
            row
        }
        AstKind::Pow { base, exp } => {
            let wrapper = builder.add_node(LayoutContent::Superscript, ast.span);
            let base_id = emit(builder, base);
            builder.add_edge(wrapper, base_id, SpatialRelation::Contains);
            let exp_id = emit(builder, exp);
            builder.add_edge(wrapper, exp_id, SpatialRelation::Contains);
            builder.add_edge(base_id, exp_id, SpatialRelation::SuperscriptOf);
            wrapper
        }
        AstKind::Sub { base, sub } => {
            let wrapper = builder.add_node(LayoutContent::Subscript, ast.span);
            let base_id = emit(builder, base);
            builder.add_edge(wrapper, base_id, SpatialRelation::Contains);
            let sub_id = emit(builder, sub);
            builder.add_edge(wrapper, sub_id, SpatialRelation::Contains);
            builder.add_edge(base_id, sub_id, SpatialRelation::SubscriptOf);
            wrapper
        }
        AstKind::Frac { num, den } => {
            let wrapper = builder.add_node(LayoutContent::Fraction, ast.span);
            let num_id = emit(builder, num);
            builder.add_edge(wrapper, num_id, SpatialRelation::Contains);
            builder.add_edge(wrapper, num_id, SpatialRelation::Above);
            let den_id = emit(builder, den);
            builder.add_edge(wrapper, den_id, SpatialRelation::Contains);
            builder.add_edge(wrapper, den_id, SpatialRelation::Below);
            wrapper
        }
        AstKind::Sqrt(inner) => {
            let wrapper = builder.add_node(LayoutContent::Radical, ast.span);
            let inner_id = emit(builder, inner);
            builder.add_edge(wrapper, inner_id, SpatialRelation::Contains);
            wrapper
        }
        AstKind::BigOp {
            name,
            bound,
            lower,
            upper,
            body,
        } => {
            let kind_name = match name.as_str() {
                "sum" => "sum",
                "prod" => "product",
                "int" => "integral",
                "lim" => "limit",
                other => other,
            };
            let op = builder.add_node(LayoutContent::BigOp(kind_name.to_string()), ast.span);
            if let Some(lower) = lower {
                if name != "lim" {
                    if let Some(bound) = bound {
                        let origin = lower.span.0;
                        let bound_id =
                            builder.add_node(LayoutContent::Glyph(bound.clone()), (origin, origin));
                        let eq_id = builder
                            .add_node(LayoutContent::Glyph("=".to_string()), (origin, origin));
                        let lower_id = emit(builder, lower);
                        for child in [bound_id, eq_id, lower_id] {
                            builder.add_edge(op, child, SpatialRelation::Contains);
                            builder.add_edge(op, child, SpatialRelation::SubscriptOf);
                        }
                        builder.add_edge(bound_id, eq_id, SpatialRelation::RightOf);
                        builder.add_edge(eq_id, lower_id, SpatialRelation::RightOf);
                    } else {
                        let lower_id = emit(builder, lower);
                        builder.add_edge(op, lower_id, SpatialRelation::Contains);
                        builder.add_edge(op, lower_id, SpatialRelation::SubscriptOf);
                    }
                } else {
                    let lower_id = emit(builder, lower);
                    builder.add_edge(op, lower_id, SpatialRelation::Contains);
                    builder.add_edge(op, lower_id, SpatialRelation::SubscriptOf);
                }
            }
            if let Some(upper) = upper {
                let upper_id = emit(builder, upper);
                builder.add_edge(op, upper_id, SpatialRelation::Contains);
                builder.add_edge(op, upper_id, SpatialRelation::SuperscriptOf);
            }
            if name == "lim" {
                if let Some(bound) = bound {
                    let bound_id = builder.add_node(LayoutContent::Glyph(bound.clone()), ast.span);
                    builder.add_edge(op, bound_id, SpatialRelation::Contains);
                    builder.add_edge(op, bound_id, SpatialRelation::SubscriptOf);
                }
            }
            if let Some(body) = body {
                let body_id = emit(builder, body);
                builder.add_edge(op, body_id, SpatialRelation::Contains);
            }
            op
        }
    }
}
