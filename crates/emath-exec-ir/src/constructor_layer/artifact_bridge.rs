//! The seam a built artifact uses to run one function in the reference
//! engine. `quote.cost`, `quote.world`, and `quote.worlds` have no
//! compiled meaning (compiled code has no engine steps and one carrier),
//! so `emath build` emits a function that uses them as a call into this
//! engine: its declared inputs cross in, its declared outputs cross back.
//! Answers, `work` counts, and refusals are the VM's own.
//!
//! The crossing covers the declared carriers an entry signature can name
//! natively: `Int` (`i64`), `Rat` (`(i128, i128)`), `Bool`, `Float64`,
//! `Str`, sequences (a native tuple is a sequence), and authored objects
//! whose fields cross (the artifact implements the traits for their
//! record structs). A value that does not fit its native
//! carrier refuses by name (`type: ...`), never truncates.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use emath_core::Span;
use emath_core::tree::{BinaryOp, Expr, ExprKind, SyntaxTree, UnaryOp};
use emath_rt::code::ExprCode;
use emath_rt::code_tree::{CodeTree, TreeBinary, TreeDomain, TreeUnary};

use super::{evaluate_function_with_work_at, with_embedded_modules, with_worlds, CValue, CarrierWorld};
use crate::exact_int::ExactInt;

/// A native value the artifact passes into the engine.
pub trait ToCValue {
    fn to_cvalue(&self) -> CValue;
}

/// A native value the artifact reads back from the engine.
pub trait FromCValue: Sized {
    fn from_cvalue(value: &CValue) -> Result<Self, String>;
}

fn mismatch<T>(carrier: &str, value: &CValue) -> Result<T, String> {
    Err(format!("type: the engine answered {value}, which is not a native {carrier}"))
}

impl<T: ToCValue + ?Sized> ToCValue for &T {
    fn to_cvalue(&self) -> CValue {
        (**self).to_cvalue()
    }
}

impl ToCValue for i64 {
    fn to_cvalue(&self) -> CValue {
        CValue::Int(ExactInt::from(*self))
    }
}

impl FromCValue for i64 {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Int(n) => n
                .to_i64()
                .ok_or_else(|| format!("overflow: {n} exceeds the native Int carrier")),
            other => mismatch("Int", other),
        }
    }
}

impl ToCValue for ExactInt {
    fn to_cvalue(&self) -> CValue {
        CValue::Int(self.clone())
    }
}

impl FromCValue for ExactInt {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Int(value) => Ok(value.clone()),
            other => mismatch("Int", other),
        }
    }
}

impl ToCValue for (i128, i128) {
    fn to_cvalue(&self) -> CValue {
        CValue::Rat {
            num: ExactInt::from(self.0),
            den: ExactInt::from(self.1),
        }
    }
}

impl FromCValue for (i128, i128) {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            // A Rat slot admits an Int exactly, as the VM's output check does.
            CValue::Int(n) => n
                .to_i128()
                .map(|n| (n, 1))
                .ok_or_else(|| format!("overflow: {n} exceeds the native Rat carrier")),
            CValue::Rat { num, den } => match (num.to_i128(), den.to_i128()) {
                (Some(num), Some(den)) => Ok((num, den)),
                _ => Err(format!("overflow: {num}/{den} exceeds the native Rat carrier")),
            },
            other => mismatch("Rat", other),
        }
    }
}

impl ToCValue for bool {
    fn to_cvalue(&self) -> CValue {
        CValue::Bool(*self)
    }
}

impl FromCValue for bool {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Bool(b) => Ok(*b),
            other => mismatch("Bool", other),
        }
    }
}

impl ToCValue for f64 {
    fn to_cvalue(&self) -> CValue {
        CValue::Float64(*self)
    }
}

impl FromCValue for f64 {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Float64(x) => Ok(*x),
            other => mismatch("Float64", other),
        }
    }
}

impl ToCValue for str {
    fn to_cvalue(&self) -> CValue {
        CValue::Str(self.to_string())
    }
}

impl ToCValue for String {
    fn to_cvalue(&self) -> CValue {
        CValue::Str(self.clone())
    }
}

impl FromCValue for String {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Str(text) => Ok(text.clone()),
            other => mismatch("Str", other),
        }
    }
}

/// Reconstruct the core expression from the distilled tree, without
/// reparsing source or losing its binder domains and dependency stamps.
fn tree_expr(tree: &CodeTree) -> Expr {
    let wrap = |kind| Expr {
        kind,
        source: Span::default(),
    };
    fn domain(name: &str) -> Expr {
        let kind = match name
            .strip_prefix("sequence(")
            .and_then(|n| n.strip_suffix(')'))
        {
            Some(element) => ExprKind::Call {
                function: Box::new(domain("sequence")),
                args: vec![domain(element)],
            },
            None => ExprKind::Path {
                segments: vec![name.to_string()],
                generics: None,
            },
        };
        Expr {
            kind,
            source: Span::default(),
        }
    }
    wrap(match tree {
        CodeTree::Literal(value) => match value {
            emath_rt::code::CodeValue::Int(n) => ExprKind::Int(n.to_string()),
            emath_rt::code::CodeValue::Rat((num, den)) => ExprKind::Rational {
                numer: num.to_string(),
                denom: den.to_string(),
            },
            emath_rt::code::CodeValue::Bool(b) => ExprKind::Bool(*b),
        },
        CodeTree::Path(segments) => ExprKind::Path {
            segments: segments.clone(),
            generics: None,
        },
        CodeTree::Call { function, args } => ExprKind::Call {
            function: Box::new(tree_expr(function)),
            args: args.iter().map(tree_expr).collect(),
        },
        CodeTree::Binary { op, left, right } => ExprKind::Binary {
            op: match op {
                TreeBinary::Add => BinaryOp::Add,
                TreeBinary::Sub => BinaryOp::Sub,
                TreeBinary::Mul => BinaryOp::Mul,
                TreeBinary::Div => BinaryOp::Div,
                TreeBinary::Eq => BinaryOp::Eq,
                TreeBinary::Ne => BinaryOp::Ne,
                TreeBinary::Lt => BinaryOp::Lt,
                TreeBinary::Le => BinaryOp::Le,
                TreeBinary::Gt => BinaryOp::Gt,
                TreeBinary::Ge => BinaryOp::Ge,
                TreeBinary::And => BinaryOp::And,
                TreeBinary::Or => BinaryOp::Or,
                TreeBinary::Pow => BinaryOp::Pow,
                TreeBinary::Imply => BinaryOp::Imply,
                TreeBinary::Iff => BinaryOp::Iff,
                TreeBinary::Asymp => BinaryOp::Asymp,
                TreeBinary::In => BinaryOp::In,
            },
            left: Box::new(tree_expr(left)),
            right: Box::new(tree_expr(right)),
        },
        CodeTree::Unary { op, value } => ExprKind::Unary {
            op: match op {
                TreeUnary::Neg => UnaryOp::Neg,
                TreeUnary::Pos => UnaryOp::Pos,
                TreeUnary::Not => UnaryOp::Not,
            },
            value: Box::new(tree_expr(value)),
        },
        CodeTree::Closure {
            param,
            domain: ty,
            body,
        } => ExprKind::FunctionAbs {
            param: param.clone(),
            domain: Box::new(domain(match ty {
                TreeDomain::Scalar(name) | TreeDomain::Sequence(name) => name,
                TreeDomain::Unknown => "unknown",
            })),
            body: Box::new(tree_expr(body)),
        },
        CodeTree::Recur { name, ty, body } => ExprKind::Recur {
            name: name.clone(),
            ty: Box::new(tree_expr(ty)),
            body: Box::new(tree_expr(body)),
        },
        CodeTree::Tuple(items) => ExprKind::Tuple(items.iter().map(tree_expr).collect()),
        CodeTree::Index { container, indices } => ExprKind::Index {
            value: Box::new(tree_expr(container)),
            indices: indices.iter().map(tree_expr).collect(),
        },
        CodeTree::If {
            condition,
            then_value,
            else_value,
        } => ExprKind::If {
            condition: Box::new(tree_expr(condition)),
            then_value: Box::new(tree_expr(then_value)),
            else_value: Box::new(tree_expr(else_value)),
        },
    })
}

impl ToCValue for ExprCode {
    fn to_cvalue(&self) -> CValue {
        let (tree, deps) = self.captured_tree();
        CValue::Code(Box::new(super::Code {
            expr: tree_expr(tree),
            deps: deps.clone(),
        }))
    }
}

impl FromCValue for ExprCode {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Code(code) => {
                let tree = crate::tree_distill::distill_tree(&code.expr)
                    .map_err(|error| format!("type: engine Code cannot cross back: {error}"))?;
                let free = emath_rt::code_tree::free_names_tree(&tree)
                    .into_iter()
                    .filter(|name| !code.deps.contains_key(name))
                    .collect();
                Ok(emath_rt::code::open_expr(
                    free,
                    tree,
                    None,
                    code.deps.clone(),
                ))
            }
            other => mismatch("Code", other),
        }
    }
}

impl ToCValue for emath_rt::code_tree::NodeValue {
    fn to_cvalue(&self) -> CValue {
        use emath_rt::{code::CodeValue, code_tree::NodeValue};
        match self {
            NodeValue::Scalar(CodeValue::Int(n)) => n.to_cvalue(),
            NodeValue::Scalar(CodeValue::Rat(q)) => q.to_cvalue(),
            NodeValue::Scalar(CodeValue::Bool(b)) => b.to_cvalue(),
            NodeValue::Text(text) => text.to_cvalue(),
            NodeValue::Code(code) => code.to_cvalue(),
            NodeValue::Sequence(items) => items.to_cvalue(),
            NodeValue::Tuple(items) => {
                CValue::Tuple(items.iter().map(ToCValue::to_cvalue).collect())
            }
            NodeValue::Record(name, fields) => record(
                name,
                fields
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.to_cvalue()))
                    .collect(),
            ),
        }
    }
}

impl FromCValue for emath_rt::code_tree::NodeValue {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        use emath_rt::{code::CodeValue, code_tree::NodeValue};
        Ok(match value {
            CValue::Int(_) => NodeValue::Scalar(CodeValue::Int(i64::from_cvalue(value)?)),
            CValue::Rat { .. } => {
                NodeValue::Scalar(CodeValue::Rat(<(i128, i128)>::from_cvalue(value)?))
            }
            CValue::Bool(b) => NodeValue::Scalar(CodeValue::Bool(*b)),
            CValue::Str(text) => NodeValue::Text(text.clone()),
            CValue::Code(_) => NodeValue::Code(Box::new(ExprCode::from_cvalue(value)?)),
            CValue::Sequence(items) => NodeValue::Sequence(
                items
                    .iter()
                    .map(Self::from_cvalue)
                    .collect::<Result<_, _>>()?,
            ),
            CValue::Tuple(items) => NodeValue::Tuple(
                items
                    .iter()
                    .map(Self::from_cvalue)
                    .collect::<Result<_, _>>()?,
            ),
            CValue::Record { type_name, fields } => NodeValue::Record(
                type_name.to_string(),
                fields
                    .iter()
                    .map(|(name, value)| Ok((name.clone(), Self::from_cvalue(value)?)))
                    .collect::<Result<_, String>>()?,
            ),
            other => return mismatch("node family", other),
        })
    }
}

impl<T: ToCValue> ToCValue for [T] {
    fn to_cvalue(&self) -> CValue {
        CValue::Sequence(Arc::new(self.iter().map(ToCValue::to_cvalue).collect()))
    }
}

impl<T: ToCValue> ToCValue for Vec<T> {
    fn to_cvalue(&self) -> CValue {
        self.as_slice().to_cvalue()
    }
}

impl<T: FromCValue> FromCValue for Vec<T> {
    fn from_cvalue(value: &CValue) -> Result<Self, String> {
        match value {
            CValue::Sequence(items) => items.iter().map(T::from_cvalue).collect(),
            // A native tuple is a sequence of its joined element carrier.
            CValue::Tuple(items) => items.iter().map(T::from_cvalue).collect(),
            other => mismatch("sequence", other),
        }
    }
}

/// An authored object value, built field by field by the artifact's
/// generated `ToCValue` impl for its record struct.
pub fn record(type_name: &str, fields: Vec<(&str, CValue)>) -> CValue {
    CValue::Record {
        type_name: Arc::from(type_name),
        fields: Arc::new(
            fields
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect(),
        ),
    }
}

/// One field of a record answer (a multi-output function packs its
/// outputs as a record named after it).
pub fn field<T: FromCValue>(value: &CValue, name: &str) -> Result<T, String> {
    match value {
        CValue::Record { fields, .. } => match fields.get(name) {
            Some(field) => T::from_cvalue(field),
            None => Err(format!("type: the engine's answer has no field `{name}`")),
        },
        other => mismatch("record", other),
    }
}

/// Read a whole answer as a native value.
pub fn read<T: FromCValue>(value: &CValue) -> Result<T, String> {
    T::from_cvalue(value)
}

thread_local! {
    static PARSED: RefCell<BTreeMap<String, Rc<SyntaxTree>>> = RefCell::new(BTreeMap::new());
}

/// Run `name` from the module text `source` in the reference engine
/// under the world list `worlds` (what `quote.worlds()` returns).
/// `path` is where the module was built from: its `use` imports resolve
/// from there, as `emath run` resolves them. The parse is cached per
/// thread and path.
pub fn run_entry(
    source: &str,
    path: &str,
    module_roots: &[&str],
    modules: &[(&str, &str)],
    name: &str,
    inputs: Vec<(&str, CValue)>,
    worlds: &[&str],
) -> Result<CValue, String> {
    let tree = PARSED.with(|cache| {
        Rc::clone(
            cache
                .borrow_mut()
                .entry(path.to_string())
                .or_insert_with(|| Rc::new(emath_syntax::parse_str(source).0)),
        )
    });
    let worlds = worlds
        .iter()
        .map(|world| {
            CarrierWorld::parse(world).ok_or_else(|| format!("unknown_world: unknown world `{world}`"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let inputs: BTreeMap<String, CValue> = inputs
        .into_iter()
        .map(|(input, value)| (input.to_string(), value))
        .collect();
    with_embedded_modules(module_roots, modules, || {
        // Ordinary emitted entries have no implicit VM-host work cap.
        // Authored quote.cost/world limits remain enforced by their engines.
        with_worlds(&worlds, || evaluate_function_with_work_at(
            &tree, name, &inputs, Some(Path::new(path)), u64::MAX,
        ))
    })
    .map_err(|error| format!("{}: {}", error.code, error.message))
}
