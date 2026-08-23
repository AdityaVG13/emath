use emath_exec_ir::{EmirProgram, EmirValue};
use emath_ir::{ExprId, ExprNode, SemanticPackage, TypeNode};
use emath_rust_ir::ast::{
    escape_ident, BinOp, Expr, FnDef, ImplDef, Item, Param, Stmt, StructDef, Ty, UnOp,
    Visibility, RUST_KEYWORDS,
};
use emath_rust_ir::render::render_expr;
use std::collections::BTreeSet;

use crate::BackendError;
use crate::codegen_render::operand;

pub(crate) fn sanitize_crate_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out = "emath_artifact".to_string();
    }
    if out.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        out.insert_str(0, "emath_");
    }
    // A Rust keyword as a crate name does not compile; escape it with the
    // same `_` suffix the identifier path uses (`type` -> `type_`).
    if RUST_KEYWORDS.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

pub(crate) fn sanitize_version(version: &str) -> String {
    if version.is_empty() {
        return "0.1.0".to_string();
    }
    let mut out = String::new();
    let mut digits = 0;
    for ch in version.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            out.push(ch);
            if ch == '.' {
                digits = 0;
            } else {
                digits += 1;
            }
        } else if ch == '-' {
            out.push('-');
        } else {
            break;
        }
        if digits > 4 {
            break;
        }
    }
    while out.ends_with('.') {
        out.pop();
    }
    if out.is_empty() {
        "0.1.0".to_string()
    } else {
        out
    }
}

pub(crate) fn add_obligations(program: &EmirProgram, out: &mut Vec<String>) {
    for obligation in &program.domain_obligations {
        let text = obligation.as_str();
        if !out.iter().any(|existing| existing == text) {
            out.push(text.to_string());
        }
    }
}

pub(crate) fn collect_var_names(package: &SemanticPackage, id: ExprId, out: &mut BTreeSet<String>) {
    let Some(expr) = package.expr(id) else {
        return;
    };
    match expr {
        ExprNode::Literal(_) => {}
        ExprNode::Variable(name) => {
            out.insert(name.0.clone());
        }
        ExprNode::Call { arguments, .. } => {
            for argument in arguments {
                collect_var_names(package, *argument, out);
            }
        }
        ExprNode::Unary { value, .. } => collect_var_names(package, *value, out),
        ExprNode::Binary { left, right, .. } => {
            collect_var_names(package, *left, out);
            collect_var_names(package, *right, out);
        }
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_var_names(package, *condition, out);
            collect_var_names(package, *then_value, out);
            collect_var_names(package, *else_value, out);
        }
        ExprNode::Record { fields, .. } => {
            for value in fields.values() {
                collect_var_names(package, *value, out);
            }
        }
        ExprNode::Index { value, indices } => {
            collect_var_names(package, *value, out);
            for index in indices {
                collect_var_names(package, *index, out);
            }
        }
        ExprNode::Slice { value, axes } => {
            collect_var_names(package, *value, out);
            for axis in axes {
                match axis {
                    emath_ir::SliceAxis::Point(index) => collect_var_names(package, *index, out),
                    emath_ir::SliceAxis::Range { start, end } => {
                        collect_var_names(package, *start, out);
                        collect_var_names(package, *end, out);
                    }
                }
            }
        }
        ExprNode::Vector(elements) => {
            for element in elements {
                collect_var_names(package, *element, out);
            }
        }
        ExprNode::Matrix(rows) => {
            for row in rows {
                for element in row {
                    collect_var_names(package, *element, out);
                }
            }
        }
        ExprNode::Tensor { elements, .. } => {
            for element in elements {
                collect_var_names(package, *element, out);
            }
        }
        ExprNode::Binder { body, .. } => collect_var_names(package, *body, out),
        ExprNode::Differentiate { body, .. }
        | ExprNode::Solve { body, .. }
        | ExprNode::Optimize { body, .. } => collect_var_names(package, *body, out),
    }
}

pub(crate) fn expand_host_inputs(inputs: &[String], used: &BTreeSet<String>) -> Vec<String> {
    let mut names = Vec::new();
    for input in inputs {
        let prefix = format!("{input}.");
        let mut fields: Vec<String> = used
            .iter()
            .filter(|name| name.starts_with(&prefix))
            .cloned()
            .collect();
        if fields.is_empty() {
            names.push(input.clone());
        } else {
            fields.sort();
            names.extend(fields);
        }
    }
    names
}

pub(crate) fn emit_host_structs(
    items: &mut Vec<Item>,
    declaration: &emath_ir::Declaration,
    package: &SemanticPackage,
    used: &BTreeSet<String>,
    owner: &str,
) -> Result<(), BackendError> {
    let mut emitted = BTreeSet::new();
    for input in &declaration.inputs {
        let Some(TypeNode::Opaque { name, .. }) = package.ty(input.ty) else {
            continue;
        };
        let type_name = name.leaf();
        if type_name.is_empty() || !emitted.insert(type_name.to_string()) {
            continue;
        }
        let prefix = format!("{}.", input.name);
        let fields: Vec<(String, Ty)> = used
            .iter()
            .filter_map(|name| name.strip_prefix(&prefix))
            .filter(|field| !field.is_empty() && !field.contains('.'))
            .map(|field| (field.to_string(), Ty::F64))
            .collect();
        if fields.is_empty() {
            return Err(BackendError::UnsupportedType(format!(
                "host type `{type_name}` on `{owner}` has no accessed fields"
            )));
        }
        items.push(Item::DocComment(format!(
            "Host-deferred `{type_name}`: field types inferred from uses in `{owner}`."
        )));
        let struct_name = escape_ident(type_name);
        items.push(Item::Struct(StructDef {
            name: struct_name.clone(),
            generics: vec![],
            fields: fields.clone(),
            derives: vec!["Clone".to_string(), "Debug".to_string()],
            doc: Vec::new(),
            visibility: Visibility::Public,
        }));
        items.push(Item::Impl(ImplDef {
            target: struct_name.clone(),
            generics: vec![],
            methods: vec![FnDef {
                name: "new".to_string(),
                generics: vec![],
                params: fields
                    .iter()
                    .map(|(field, ty)| Param {
                        name: field.clone(),
                        ty: ty.clone(),
                    })
                    .collect(),
                ret: Ty::Named(struct_name),
                body: Stmt::Expr(Expr::StructLiteral {
                    name: "Self".to_string(),
                    fields: fields
                        .iter()
                        .map(|(field, _)| (field.clone(), Expr::Var(field.clone())))
                        .collect(),
                }),
                doc: vec!["Construct a host-deferred record from accessed fields.".to_string()],
                visibility: Visibility::Public,
                attrs: Vec::new(),
            }],
            doc: Vec::new(),
        }));
    }
    Ok(())
}

pub(crate) fn unary_method(method: &str, value: EmirValue, program: &EmirProgram) -> Expr {
    Expr::Un {
        op: UnOp::Method(method.to_string()),
        value: Box::new(operand(program, value)),
    }
}

pub(crate) fn binary_method(method: &str, left: EmirValue, right: EmirValue, program: &EmirProgram) -> Expr {
    Expr::MethodCall {
        receiver: Box::new(operand(program, left)),
        method: method.to_string(),
        args: vec![operand(program, right)],
    }
}

pub(crate) fn comparison(op: BinOp, left: EmirValue, right: EmirValue, program: &EmirProgram) -> Expr {
    Expr::Bin {
        op,
        left: Box::new(operand(program, left)),
        right: Box::new(operand(program, right)),
    }
}

pub(crate) fn rate_call(state_name: &str, input_args: &[Expr]) -> Expr {
    Expr::MethodCall {
        receiver: Box::new(Expr::SelfValue),
        method: escape_ident(&format!("der_{state_name}")),
        args: input_args.to_vec(),
    }
}

pub(crate) fn rate_lets(
    receiver: &str,
    prefix: &str,
    declaration: &emath_ir::Declaration,
    input_args: &[Expr],
) -> Vec<Stmt> {
    declaration
        .state
        .iter()
        .map(|field| Stmt::Let {
            pattern: format!("{prefix}_{}", field.name),
            value: Box::new(Expr::MethodCall {
                receiver: Box::new(Expr::Var(receiver.to_string())),
                method: escape_ident(&format!("der_{}", field.name)),
                args: input_args.to_vec(),
            }),
        })
        .collect()
}

pub(crate) fn add_scaled_expr(value: Expr, rate: Expr, scale: Expr, node: &TypeNode) -> Expr {
    match node {
        TypeNode::Vector { .. } => Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(a, b)| a + {} * b).collect::<Vec<f64>>()",
            render_expr(&value),
            render_expr(&rate),
            render_expr(&scale),
        )),
        TypeNode::Matrix { .. } => Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(r1, r2)| r1.iter().zip(r2.iter()).map(|(a, b)| a + {} * b).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>()",
            render_expr(&value),
            render_expr(&rate),
            render_expr(&scale),
        )),
        TypeNode::Tensor { .. } => Expr::Raw(format!(
            "{}.iter().zip({}.iter()).map(|(a, b)| a + {} * b).collect::<Vec<f64>>()",
            render_expr(&value),
            render_expr(&rate),
            render_expr(&scale),
        )),
        _ => Expr::Bin {
            op: BinOp::Add,
            left: Box::new(value),
            right: Box::new(Expr::Bin {
                op: BinOp::Mul,
                left: Box::new(scale),
                right: Box::new(rate),
            }),
        },
    }
}

