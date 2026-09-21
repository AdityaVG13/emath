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
use emath_rt::code_tree::{CodeTree, TreeBinary, TreeUnary};

fn canonical_ratio(numer: &str, denom: &str) -> Result<CodeValue, String> {
    let numer: i128 = numer
        .parse()
        .map_err(|_| format!("rational numerator `{numer}` exceeds the artifact rational carrier"))?;
    let denom: i128 = denom
        .parse()
        .map_err(|_| format!("rational denominator `{denom}` exceeds the artifact rational carrier"))?;
    if denom == 0 {
        return Err(String::from("rational literal has zero denominator"));
    }
    // Canonical form: reduced, denominator positive (the VM's
    // `CValue::Rat::canon` law - `2/4` is the value `1/2`).
    let (numer, denom) = if denom < 0 {
        (-numer, -denom)
    } else {
        (numer, denom)
    };
    let gcd = gcd(numer.unsigned_abs(), denom.unsigned_abs());
    let (numer, denom) = if gcd == 0 {
        (numer, denom)
    } else {
        (numer / gcd as i128, denom / gcd as i128)
    };
    Ok(CodeValue::Rat((numer, denom)))
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a
}

/// Distill an authored expression into the shared tree. The admitted
/// shapes are exactly the scalar expression family the union lane
/// compiles: literals, paths, scalar calls, binary/unary operators,
/// sequences, and branches. Binders, nested quotes, structured
/// constructors, and ranges refuse by name.
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
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            let mut trees = Vec::with_capacity(items.len());
            for item in items {
                trees.push(distill_tree(item)?);
            }
            Ok(CodeTree::Tuple(trees))
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => Ok(CodeTree::If {
            condition: Box::new(distill_tree(condition)?),
            then_value: Box::new(distill_tree(then_value)?),
            else_value: Box::new(distill_tree(else_value)?),
        }),
        ExprKind::Float(_) => Err(String::from(
            "float templates are not emitted in the artifact tree lane",
        )),
        ExprKind::Str(_) => Err(String::from(
            "text templates are not emitted in the artifact tree lane",
        )),
        other => Err(format!(
            "expression form `{other:?}` is not emitted in artifact trees"
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
