//! Layout-graph to BinderTerm lowering.

use super::*;

pub(super) fn lower_id(graph: &MathLayoutGraph, id: NodeId) -> Result<BinderTerm, LayoutError> {
    let node = graph.node(id).ok_or_else(|| LayoutError::Unlowered {
        reason: format!("missing node {}", id.0),
    })?;
    match &node.content {
        LayoutContent::FormulaRegion | LayoutContent::Row => {
            lower_sequence(graph, &contained_terms(graph, id)?)
        }
        LayoutContent::Superscript => {
            let kids = contained_terms(graph, id)?;
            let base = kids
                .first()
                .copied()
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "superscript missing base".to_string(),
                })?;
            let exp = graph
                .related(base, SpatialRelation::SuperscriptOf)
                .into_iter()
                .next()
                .or_else(|| kids.get(1).copied())
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "superscript missing exponent".to_string(),
                })?;
            apply2("pow", lower_id(graph, base)?, lower_id(graph, exp)?)
        }
        LayoutContent::Subscript => {
            let kids = contained_terms(graph, id)?;
            let base = kids
                .first()
                .copied()
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "subscript missing base".to_string(),
                })?;
            let sub = graph
                .related(base, SpatialRelation::SubscriptOf)
                .into_iter()
                .next()
                .or_else(|| kids.get(1).copied())
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "subscript missing script".to_string(),
                })?;
            apply2("index", lower_id(graph, base)?, lower_id(graph, sub)?)
        }
        LayoutContent::Fraction => {
            let above = graph
                .related(id, SpatialRelation::Above)
                .into_iter()
                .next()
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "fraction missing numerator".to_string(),
                })?;
            let below = graph
                .related(id, SpatialRelation::Below)
                .into_iter()
                .next()
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "fraction missing denominator".to_string(),
                })?;
            apply2("/", lower_id(graph, above)?, lower_id(graph, below)?)
        }
        LayoutContent::Radical => {
            let inner = contained_terms(graph, id)?
                .into_iter()
                .next()
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "radical missing radicand".to_string(),
                })?;
            match lower_id(graph, inner)? {
                BinderTerm::Leaf(term) => Ok(BinderTerm::Leaf(Term::Apply {
                    operator: SymbolId("sqrt".to_string()),
                    arguments: vec![term],
                })),
                BinderTerm::Bind(_) => Err(LayoutError::Unlowered {
                    reason: "radical over binder".to_string(),
                }),
            }
        }
        LayoutContent::BigOp(name) => lower_bigop(graph, id, name),
        LayoutContent::Glyph(text) => lower_glyph(graph, id, text),
    }
}

pub(super) fn contained_terms(
    graph: &MathLayoutGraph,
    id: NodeId,
) -> Result<Vec<NodeId>, LayoutError> {
    Ok(graph
        .related(id, SpatialRelation::Contains)
        .into_iter()
        .filter(|child| !graph.is_script_target(*child))
        .collect())
}

pub(super) fn lower_glyph(
    graph: &MathLayoutGraph,
    id: NodeId,
    text: &str,
) -> Result<BinderTerm, LayoutError> {
    if is_infix_op(text) {
        return Err(LayoutError::Unlowered {
            reason: format!("operator {text:?} is not a term"),
        });
    }
    let mut term = BinderTerm::Leaf(glyph_term(text));
    if let Some(exp) = graph
        .related(id, SpatialRelation::SuperscriptOf)
        .into_iter()
        .next()
    {
        term = apply2("pow", term, lower_id(graph, exp)?)?;
    }
    if let Some(sub) = graph
        .related(id, SpatialRelation::SubscriptOf)
        .into_iter()
        .next()
    {
        term = apply2("index", term, lower_id(graph, sub)?)?;
    }
    Ok(term)
}

pub(super) fn lower_bigop(
    graph: &MathLayoutGraph,
    id: NodeId,
    name: &str,
) -> Result<BinderTerm, LayoutError> {
    let subs = graph.related(id, SpatialRelation::SubscriptOf);
    let supers = graph.related(id, SpatialRelation::SuperscriptOf);
    let bodies: Vec<NodeId> = graph
        .related(id, SpatialRelation::Contains)
        .into_iter()
        .filter(|child| {
            !graph.is_script_target(*child) && !subs.contains(child) && !supers.contains(child)
        })
        .collect();

    let (kind, family, default_bound) = match name {
        "sum" => (BinderKind::Sum, BinderFamily::Structural, "i"),
        "product" => (BinderKind::Product, BinderFamily::Structural, "i"),
        "integral" => (BinderKind::Integral, BinderFamily::FiniteAnalogue, "x"),
        "limit" => (BinderKind::Limit, BinderFamily::Conventional, "x"),
        other => {
            return Err(LayoutError::Unlowered {
                reason: format!("unknown bigop {other}"),
            });
        }
    };

    let (bound, domain) = if name == "limit" {
        let glyphs = flatten_glyphs(graph, &subs);
        let bound_name = glyphs
            .first()
            .cloned()
            .unwrap_or_else(|| default_bound.to_string());
        let anchor = glyphs
            .iter()
            .rev()
            .find(|glyph| *glyph != "to" && *glyph != "→")
            .cloned()
            .or_else(|| glyphs.last().cloned())
            .unwrap_or_else(|| "0".to_string());
        (VariableId(bound_name), BinderDomain::Symbolic { anchor })
    } else {
        let glyphs = flatten_glyphs(graph, &subs);
        let (bound_name, lower_int) = if let Some(eq) = glyphs.iter().position(|glyph| glyph == "=")
        {
            let name = glyphs
                .first()
                .cloned()
                .filter(|glyph| glyph != "=")
                .unwrap_or_else(|| default_bound.to_string());
            let rhs = glyphs[eq + 1..].join("");
            (name, rhs.parse().ok())
        } else {
            let lower_term = if subs.is_empty() {
                None
            } else {
                Some(lower_related(graph, &subs)?)
            };
            match &lower_term {
                Some(other) => (default_bound.to_string(), as_int_binder(other)),
                None => (default_bound.to_string(), None),
            }
        };
        let upper_term = if supers.is_empty() {
            None
        } else {
            Some(lower_related(graph, &supers)?)
        };
        let upper_int = upper_term.as_ref().and_then(as_int_binder);
        let domain = match (lower_int, upper_int) {
            (Some(lower), Some(upper)) => BinderDomain::FiniteRange { lower, upper },
            _ => BinderDomain::Symbolic {
                anchor: format!(
                    "{}..{}",
                    if glyphs.is_empty() {
                        "_".to_string()
                    } else {
                        glyphs.join("")
                    },
                    upper_term.as_ref().map_or_else(
                        || "_".to_string(),
                        |term| match term {
                            BinderTerm::Leaf(leaf) => leaf.canonical(),
                            BinderTerm::Bind(binder) => binder.canonical(),
                        }
                    )
                ),
            },
        };
        (VariableId(bound_name), domain)
    };

    let body = if let Some(body_id) = bodies.first() {
        lower_id(graph, *body_id)?
    } else {
        BinderTerm::Leaf(Term::Variable(bound.clone()))
    };

    Ok(BinderTerm::Bind(Box::new(ScopedBinder {
        kind,
        family,
        domain,
        bound,
        body,
    })))
}

pub(super) fn lower_related(
    graph: &MathLayoutGraph,
    ids: &[NodeId],
) -> Result<BinderTerm, LayoutError> {
    if ids.len() == 1 {
        lower_id(graph, ids[0])
    } else {
        lower_sequence(graph, ids)
    }
}

pub(super) fn flatten_glyphs(graph: &MathLayoutGraph, ids: &[NodeId]) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        flatten_glyphs_into(graph, *id, &mut out);
    }
    out
}

pub(super) fn flatten_glyphs_into(graph: &MathLayoutGraph, id: NodeId, out: &mut Vec<String>) {
    let Some(node) = graph.node(id) else {
        return;
    };
    match &node.content {
        LayoutContent::Glyph(text) => out.push(text.clone()),
        _ => {
            for child in graph.related(id, SpatialRelation::Contains) {
                flatten_glyphs_into(graph, child, out);
            }
        }
    }
}

pub(super) fn lower_sequence(
    graph: &MathLayoutGraph,
    ids: &[NodeId],
) -> Result<BinderTerm, LayoutError> {
    if ids.is_empty() {
        return Err(LayoutError::Unlowered {
            reason: "empty formula".to_string(),
        });
    }
    let mut items: Vec<SeqItem> = Vec::new();
    for id in ids {
        let node = graph.node(*id).ok_or_else(|| LayoutError::Unlowered {
            reason: format!("missing node {}", id.0),
        })?;
        if let LayoutContent::Glyph(text) = &node.content {
            if is_infix_op(text) && !graph.is_script_target(*id) {
                items.push(SeqItem::Op(text.clone()));
                continue;
            }
        }
        items.push(SeqItem::Term(lower_id(graph, *id)?));
    }
    climb_eq(&items, 0).and_then(|(term, end)| {
        if end == items.len() {
            Ok(term)
        } else {
            Err(LayoutError::Unlowered {
                reason: "trailing tokens in formula sequence".to_string(),
            })
        }
    })
}
