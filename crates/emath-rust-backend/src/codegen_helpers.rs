use crate::rust_ir::ast::{
    BinOp, Expr, FnDef, ImplDef, Item, Param, RUST_KEYWORDS, Stmt, StructDef, Ty, Visibility,
    escape_ident,
};
use emath_exec_ir::{EmirProgram, EmirValue};
use emath_ir::{ExprId, ExprNode, SemanticPackage, TypeNode};
use std::collections::BTreeSet;

use crate::BackendError;
use crate::codegen_render::{InputKinds, ValueKind, operand};

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
        ExprNode::Set { elements, guards } => {
            for element in elements {
                collect_var_names(package, *element, out);
            }
            for guard in guards.iter().flatten() {
                collect_var_names(package, *guard, out);
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
        ExprNode::Binder {
            variables, body, ..
        } => {
            // Names used only inside a binder domain (e.g. `n` in `2..n`)
            // are live inputs of the emitted code; skipping them would let
            // codegen drop the corresponding function input.
            for variable in variables {
                collect_var_names(package, variable.domain, out);
            }
            collect_var_names(package, *body, out);
        }
        ExprNode::Apply { arguments, .. } => {
            for argument in arguments {
                collect_var_names(package, *argument, out);
            }
        }
        ExprNode::Program { body, inputs } => {
            let mut free = BTreeSet::new();
            collect_var_names(package, *body, &mut free);
            for input in inputs {
                free.remove(input);
            }
            out.extend(free);
        }
        // A series data constant carries no free variables: the pairs
        // are literals and the policy is declared (04 §5.4 slice 1).
        ExprNode::Series { .. } => {}
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
            field_visibility: Visibility::Private,
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

pub(crate) fn type_is_i64(package: &SemanticPackage, ty: emath_ir::TypeId) -> bool {
    node_is_i64(package.ty(ty))
}

fn node_is_i64(node: Option<&TypeNode>) -> bool {
    match node {
        Some(TypeNode::Int | TypeNode::Nat) => true,
        Some(TypeNode::Refinement { base, .. }) => node_is_i64(Some(base)),
        _ => false,
    }
}

pub(crate) fn field_value_kinds(
    package: &SemanticPackage,
    declaration: &emath_ir::Declaration,
) -> InputKinds {
    fn carrier(node: Option<&TypeNode>) -> ValueKind {
        match node {
            Some(TypeNode::Int | TypeNode::Nat) => ValueKind::I64,
            Some(TypeNode::Rational) => ValueKind::Rational,
            Some(TypeNode::Bool) => ValueKind::Bool,
            Some(TypeNode::Other(name)) if name.0 == "Text" => ValueKind::Text,
            Some(TypeNode::Float64) => ValueKind::F64,
            Some(TypeNode::Record(name)) => ValueKind::Record(name.0.clone()),
            Some(TypeNode::Vector { element, .. }) => {
                ValueKind::Vector(Box::new(carrier(Some(element))))
            }
            Some(TypeNode::Matrix { element, .. }) => {
                ValueKind::Matrix(Box::new(carrier(Some(element))))
            }
            Some(TypeNode::Tensor { .. }) => ValueKind::Tensor,
            Some(TypeNode::Refinement { base, .. }) => carrier(Some(base)),
            _ => ValueKind::Other,
        }
    }
    let mut names = InputKinds::new();
    let mut consider = |field: &emath_ir::Field| {
        names.insert(field.name.clone(), carrier(package.ty(field.ty)));
    };
    for field in declaration
        .inputs
        .iter()
        .chain(declaration.state.iter())
        .chain(declaration.algebraic.iter())
        .chain(declaration.outputs.iter())
    {
        consider(field);
    }
    for constructor in &declaration.constructors {
        for parameter in &constructor.parameters {
            consider(parameter);
        }
    }
    names
}

pub(crate) fn comparison(
    op: BinOp,
    left: EmirValue,
    right: EmirValue,
    program: &EmirProgram,
) -> Expr {
    Expr::Bin {
        op,
        left: Box::new(operand(program, left)),
        right: Box::new(operand(program, right)),
    }
}
