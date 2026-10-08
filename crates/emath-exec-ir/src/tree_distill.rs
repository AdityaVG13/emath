//! The shared-tree distiller: the single `emath_core::Expr` ->
//! `emath_rt::code_tree::CodeTree` conversion. The lowering pass runs
//! it over every emitted expression-template quote, so the compiled
//! factory and the distilled tree come from the SAME authored body in
//! the SAME pass - the dual-representation law the rust-backend
//! contract owns. Anything outside the emitted tree subset refuses by
//! name here (the refusal is a lowering error, so a module using an
//! unemittable quote is not marked runnable, exactly like the
//! body-kind gate).

use emath_core::tree::{BinaryOp, Expr, ExprKind, UnaryOp};
use emath_rt::code::CodeValue;
use emath_rt::code_tree::{CodeTree, TreeBinary, TreeDomain, TreeUnary};

/// The VM's recorded domain shape for a function literal's binder, in
/// the artifact tree's vocabulary.
fn tree_domain(domain: &Expr) -> TreeDomain {
    use crate::constructor_layer::DomainShape;
    match crate::constructor_layer::domain_shape_of_expr(domain) {
        DomainShape::Scalar(name) => TreeDomain::Scalar(name),
        shape @ DomainShape::Sequence(_) => TreeDomain::Sequence(shape.to_string()),
        DomainShape::Unknown => TreeDomain::Unknown,
    }
}

fn canonical_ratio(numer: &str, denom: &str) -> Result<CodeValue, String> {
    let numer = emath_rt::ExactInt::parse(numer).map_err(|error| error.to_string())?;
    let denom = emath_rt::ExactInt::parse(denom).map_err(|error| error.to_string())?;
    emath_rt::ratio_normalize(numer, denom).map(CodeValue::Rat)
}

/// Distill an authored expression into the shared tree. The admitted
/// shapes are exactly the scalar expression family the union lane
/// compiles (literals, paths, scalar calls, binary/unary operators,
/// sequences, branches, function literals, recursion), plus the two
/// structured forms the node lane computes: record literals and
/// nested `quote(expression)` captures. Function quotation, text and
/// float templates, and ranges refuse by name.
pub(crate) fn distill_tree(expr: &Expr) -> Result<CodeTree, String> {
    match &expr.kind {
        ExprKind::Int(text) => {
            let value: i64 = text
                .parse()
                .map_err(|_| format!("literal `{text}` exceeds the artifact Int carrier"))?;
            Ok(CodeTree::Literal(CodeValue::Int(value)))
        }
        ExprKind::Bool(value) => Ok(CodeTree::Literal(CodeValue::Bool(*value))),
        ExprKind::Rational { numer, denom } => Ok(CodeTree::Literal(canonical_ratio(numer, denom)?)),
        ExprKind::Path { segments, generics } => {
            if generics.is_some() {
                return Err(String::from(
                    "generic paths are not emitted in artifact trees",
                ));
            }
            Ok(CodeTree::Path(segments.clone()))
        }
        ExprKind::Binary { op, left, right } => Ok(CodeTree::Binary {
            op: distill_binary(*op)?,
            left: Box::new(distill_tree(left)?),
            right: Box::new(distill_tree(right)?),
        }),
        ExprKind::Unary { op, value } => Ok(CodeTree::Unary {
            op: distill_unary(*op),
            value: Box::new(distill_tree(value)?),
        }),
        ExprKind::Call { function, args } => {
            let mut trees = Vec::with_capacity(args.len());
            for arg in args {
                trees.push(distill_tree(arg)?);
            }
            Ok(CodeTree::Call {
                function: Box::new(distill_tree(function)?),
                args: trees,
            })
        }
        ExprKind::List(items) => {
            let mut trees = Vec::with_capacity(items.len());
            for item in items {
                trees.push(distill_tree(item)?);
            }
            // An authored sequence literal distills as a sequence: the
            // VM evaluates a quoted list as a sequence (its `length`
            // answers), so the tree carries the distinction the node
            // family makes (bead emath-886vm).
            Ok(CodeTree::Sequence(trees))
        }
        ExprKind::Tuple(items) => {
            let mut trees = Vec::with_capacity(items.len());
            for item in items {
                trees.push(distill_tree(item)?);
            }
            Ok(CodeTree::Tuple(trees))
        }
        ExprKind::Index { value, indices } => Ok(CodeTree::Index {
            container: Box::new(distill_tree(value)?),
            indices: indices.iter().map(distill_tree).collect::<Result<_, _>>()?,
        }),
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => Ok(CodeTree::If {
            condition: Box::new(distill_tree(condition)?),
            then_value: Box::new(distill_tree(then_value)?),
            else_value: Box::new(distill_tree(else_value)?),
        }),
        // The definition-table body lane: `function_body_expr` wraps
        // every table body in `FunctionAbs` binders (one per input),
        // so a transparent module body distills exactly when its
        // binder wrapper does. The binder domain is recorded exactly as
        // the VM records it (`domain_shape_of_expr`), so an applied
        // evaluate checks arguments against the same shape.
        ExprKind::FunctionAbs { param, domain, body } => Ok(CodeTree::Closure {
            param: param.clone(),
            domain: tree_domain(domain),
            body: Box::new(distill_tree(body)?),
        }),
        ExprKind::Recur { name, ty, body } => Ok(CodeTree::Recur {
            name: name.clone(),
            ty: Box::new(distill_tree(ty)?),
            body: Box::new(distill_tree(body)?),
        }),
        ExprKind::Record { type_path, fields } => {
            // The record literal: the declared type path plus one tree
            // per field, canonicalized to name-sorted order (the VM
            // stores record fields in a map; the tree's identity and
            // evaluation depend on the canonical order, not source
            // order). Duplicate field names refuse by name.
            let mut built = Vec::with_capacity(fields.len());
            for (name, field) in fields {
                if built.iter().any(|(seen, _): &(String, CodeTree)| seen == name) {
                    return Err(format!("duplicate field `{name}` in record literal"));
                }
                built.push((name.clone(), distill_tree(field)?));
            }
            built.sort_by(|(a, _), (b, _)| a.cmp(b));
            Ok(CodeTree::Record {
                type_path: type_path.clone(),
                fields: built,
            })
        }
        // `quote(expression)`: a closed code capture inside a template.
        // The content distills like any other subtree (the dual
        // representation covers nested quotes); evaluation yields the
        // code value, substitution reaches through it syntactically.
        ExprKind::Quote { body } => Ok(CodeTree::Code(Box::new(distill_tree(body)?))),
        ExprKind::Float(_) => Err(String::from(
            "float templates are not emitted in the artifact tree lane",
        )),
        ExprKind::Str(_) => Err(String::from(
            "text templates are not emitted in the artifact tree lane",
        )),
        other => Err(format!(
            "expression form `{}` at {}..{} is not emitted in artifact trees",
            crate::constructor_emir::kind_form(other),
            expr.source.start,
            expr.source.end
        )),
    }
}

fn distill_binary(op: BinaryOp) -> Result<TreeBinary, String> {
    Ok(match op {
        BinaryOp::Add => TreeBinary::Add,
        BinaryOp::Sub => TreeBinary::Sub,
        BinaryOp::Mul => TreeBinary::Mul,
        BinaryOp::Div => TreeBinary::Div,
        BinaryOp::Eq => TreeBinary::Eq,
        BinaryOp::Ne => TreeBinary::Ne,
        BinaryOp::Lt => TreeBinary::Lt,
        BinaryOp::Le => TreeBinary::Le,
        BinaryOp::Gt => TreeBinary::Gt,
        BinaryOp::Ge => TreeBinary::Ge,
        BinaryOp::And => TreeBinary::And,
        BinaryOp::Or => TreeBinary::Or,
        other => {
            return Err(format!(
                "operator {other:?} is not a scalar carrier operation in artifact trees"
            ))
        }
    })
}

fn distill_unary(op: UnaryOp) -> TreeUnary {
    match op {
        UnaryOp::Neg => TreeUnary::Neg,
        UnaryOp::Pos => TreeUnary::Pos,
        UnaryOp::Not => TreeUnary::Not,
    }
}
