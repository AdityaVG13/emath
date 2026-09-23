use super::{CValue, Expr, ExprKind, BTreeSet, Arc, BTreeMap, Code, ConstructorError, Rc, FnDecl};
use super::prelude::*;

pub(super) fn value_to_expr(value: &CValue) -> Expr {
    let kind = match value {
        CValue::Bool(v) => ExprKind::Bool(*v),
        CValue::Str(text) => ExprKind::Str(text.clone()),
        CValue::Int(v) => ExprKind::Int(v.to_string()),
        CValue::Rat { num, den } => ExprKind::Rational {
            numer: num.to_string(),
            denom: den.to_string(),
        },
        CValue::Float64(v) => ExprKind::Float(format!("{v}f64")),
        CValue::Sequence(items) => ExprKind::List(items.iter().map(value_to_expr).collect()),
        CValue::Tuple(items) => ExprKind::Tuple(items.iter().map(value_to_expr).collect()),
        CValue::Code(code) => code.expr.kind.clone(),
        CValue::Record { type_name, fields } => ExprKind::Record {
            type_path: vec![type_name.clone()],
            fields: fields
                .iter()
                .map(|(k, v)| (k.clone(), value_to_expr(v)))
                .collect(),
        },
        other => ExprKind::Path {
            segments: vec![other.to_string()],
            generics: None,
        },
    };
    Expr {
        kind,
        source: emath_core::Span::default(),
    }
}

pub(super) fn substitute_path(expr: &Expr, name: &str, replacement: &Expr, mint: &mut u64) -> Expr {
    // L2 substitution: a binder that REBINDS the reference preserves
    // bound-variable shadowing (no substitution under it); a binder
    // whose param would CAPTURE a free name of the replacement is
    // alpha-renamed to a fresh minted token first. Token spellings
    // depend on mint order; VALUES and de Bruijn identities do not.
    let kind = match &expr.kind {
        ExprKind::Path { segments, .. } if segments.join(".") == name => replacement.kind.clone(),
        ExprKind::FunctionAbs {
            param,
            domain,
            body,
        } => {
            if param == name {
                expr.kind.clone()
            } else {
                let (param, body) = avoid_capture(param, body, replacement, mint);
                ExprKind::FunctionAbs {
                    param,
                    domain: Box::new(substitute_path(domain, name, replacement, mint)),
                    body: Box::new(substitute_path(&body, name, replacement, mint)),
                }
            }
        }
        ExprKind::Recur {
            name: recur_name,
            ty,
            body,
        } => {
            if recur_name == name {
                expr.kind.clone()
            } else {
                let (recur_name, body) = avoid_capture(recur_name, body, replacement, mint);
                ExprKind::Recur {
                    name: recur_name,
                    ty: Box::new(substitute_path(ty, name, replacement, mint)),
                    body: Box::new(substitute_path(&body, name, replacement, mint)),
                }
            }
        }
        ExprKind::QuoteBind {
            param,
            domain,
            body,
        } => {
            if param == name {
                expr.kind.clone()
            } else {
                let (param, body) = avoid_capture(param, body, replacement, mint);
                ExprKind::QuoteBind {
                    param,
                    domain: Box::new(substitute_path(domain, name, replacement, mint)),
                    body: Box::new(substitute_path(&body, name, replacement, mint)),
                }
            }
        }
        ExprKind::CallableBinder {
            callee,
            param,
            domain,
            body,
        } => {
            if param == name {
                expr.kind.clone()
            } else {
                let (param, body) = avoid_capture(param, body, replacement, mint);
                ExprKind::CallableBinder {
                    callee: Box::new(substitute_path(callee, name, replacement, mint)),
                    param,
                    domain: Box::new(substitute_path(domain, name, replacement, mint)),
                    body: Box::new(substitute_path(&body, name, replacement, mint)),
                }
            }
        }
        ExprKind::Binary { op, left, right } => ExprKind::Binary {
            op: *op,
            left: Box::new(substitute_path(left, name, replacement, mint)),
            right: Box::new(substitute_path(right, name, replacement, mint)),
        },
        ExprKind::Unary { op, value } => ExprKind::Unary {
            op: *op,
            value: Box::new(substitute_path(value, name, replacement, mint)),
        },
        ExprKind::Call { function, args } => ExprKind::Call {
            function: Box::new(substitute_path(function, name, replacement, mint)),
            args: args
                .iter()
                .map(|arg| substitute_path(arg, name, replacement, mint))
                .collect(),
        },
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => ExprKind::If {
            condition: Box::new(substitute_path(condition, name, replacement, mint)),
            then_value: Box::new(substitute_path(then_value, name, replacement, mint)),
            else_value: Box::new(substitute_path(else_value, name, replacement, mint)),
        },
        ExprKind::Quote { body } => ExprKind::Quote {
            body: Box::new(substitute_path(body, name, replacement, mint)),
        },
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => ExprKind::Cases {
            subject: subject
                .as_ref()
                .map(|subject| Box::new(substitute_path(subject, name, replacement, mint))),
            arms: arms
                .iter()
                .map(|(cond, value)| {
                    (
                        substitute_path(cond, name, replacement, mint),
                        substitute_path(value, name, replacement, mint),
                    )
                })
                .collect(),
            else_arm: Box::new(substitute_path(else_arm, name, replacement, mint)),
        },
        ExprKind::List(items) => ExprKind::List(
            items
                .iter()
                .map(|item| substitute_path(item, name, replacement, mint))
                .collect(),
        ),
        ExprKind::Tuple(items) => ExprKind::Tuple(
            items
                .iter()
                .map(|item| substitute_path(item, name, replacement, mint))
                .collect(),
        ),
        other => other.clone(),
    };
    Expr {
        kind,
        source: expr.source,
    }
}

/// Alpha-rename a binder whose param would capture a free name of the
/// replacement: a fresh minted token, the body renamed to it. No-op
/// when the param is empty or captures nothing.
pub(super) fn avoid_capture(param: &str, body: &Expr, replacement: &Expr, mint: &mut u64) -> (String, Expr) {
    let mut free = BTreeSet::new();
    free_path_names(replacement, &mut Vec::new(), &mut free);
    if !param.is_empty() && free.contains(param) {
        *mint += 1;
        let token = format!("#{param}.cap.{mint}");
        let renamed = substitute_path(body, param, &path_expr(&token), mint);
        (token, renamed)
    } else {
        (param.to_string(), body.clone())
    }
}

pub(super) fn dummy_expr(kind: ExprKind) -> Expr {
    Expr {
        kind,
        source: emath_core::Span::default(),
    }
}

pub(super) fn path_expr(name: &str) -> Expr {
    dummy_expr(ExprKind::Path {
        segments: vec![name.into()],
        generics: None,
    })
}

pub(super) fn is_refused_recipe(name: &str) -> bool {
    matches!(
        name,
        "derivative"
            | "jacobian"
            | "solve"
            | "minimize"
            | "maximize"
            | "partial"
            | "total"
            | "sum"
            | "product"
            | "forall"
            | "exists"
            | "integral"
            | "series"
            | "limit"
            | "sample_limit"
            | "einsum"
            | "series_from_csv"
            | "rat"
            | "rat_add"
            | "rat_norm"
            | "euler_maruyama"
            | "stratonovich"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "atan2"
            | "sinh"
            | "cosh"
            | "tanh"
            | "exp"
            | "ln"
            | "log"
            | "log2"
            | "log10"
            | "sqrt"
            | "cbrt"
            | "hypot"
            | "pow"
            | "abs"
            | "floor"
            | "ceil"
            | "sign"
            | "recip"
            | "is_finite"
            | "gamma"
            | "lgamma"
            | "beta"
            | "lbeta"
            | "erf"
            | "erfc"
            | "gamma_error_bound"
            | "min"
            | "max"
            | "mod"
            | "generating_function"
            | "coefficient"
            | "dot"
            | "norm"
            | "cross"
            | "transpose"
            | "det"
            | "inv"
            | "matmul"
            | "reachability"
            | "shortest_path"
            | "connected_components"
            | "rk4"
            | "euler"
            | "integrate"
            | "quad"
            | "factorial"
            | "binomial"
            | "choose"
            | "vec_add"
            | "plot"
    )
}

pub(super) fn is_fn_ctor(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::Path { segments, .. } if segments.last().map(String::as_str) == Some("Fn")
    )
}

pub(super) fn is_fn_type(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Call { function, args } => is_fn_ctor(function) && args.len() == 2,
        ExprKind::Record { fields, .. } => {
            !fields.is_empty() && fields.iter().all(|(_, ty)| is_fn_type(ty))
        }
        _ => false,
    }
}

pub(super) fn is_guarded_recur_body(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionAbs { .. } => true,
        ExprKind::Record { fields, .. } => {
            !fields.is_empty() && fields.iter().all(|(_, body)| is_guarded_recur_body(body))
        }
        _ => false,
    }
}

pub(super) fn schema_tag(name: &str) -> CValue {
    CValue::Record {
        type_name: name.into(),
        fields: Arc::new(BTreeMap::new()),
    }
}

pub(super) fn is_schema_tag(name: &str) -> bool {
    matches!(
        name,
        "partial"
            | "satisfied"
            | "unmet"
            | "returned"
            | "suspended"
            | "faulted"
            | "code"
            | "value"
            | "exact_scalar"
            | "method_unavailable"
            | "zero_normalizer"
            | "Literal"
            | "Local"
            | "Global"
            | "Call"
            | "Closure"
            | "Sequence"
            | "Opaque"
            | "Available"
            | "Add"
            | "Sub"
            | "Mul"
            | "Div"
            | "Eq"
            | "Ne"
            | "Lt"
            | "Le"
            | "Gt"
            | "Ge"
            | "Pow"
            | "Imply"
            | "Iff"
            | "Asymp"
            | "In"
            | "transparent"
            | "opaque"
            | "Match"
            | "Cases"
            | "Quotation"
            | "Fragment"
            | "Scope"
            | "Branch"
            | "Index"
            | "Record"
            | "Recur"
            | "Projection"
        )
}

pub(super) fn body_record(kind: &str, identity: Option<&str>, signature: Option<&str>) -> CValue {
    let mut fields = BTreeMap::from([("kind".into(), schema_tag(kind))]);
    if let Some(identity) = identity {
        fields.insert("identity".into(), schema_tag(identity));
    }
    if let Some(signature) = signature {
        fields.insert("signature".into(), schema_tag(signature));
    }
    CValue::Record {
        type_name: kind.into(),
        fields: Arc::new(fields),
    }
}

pub(super) fn available_body(expr: Expr) -> CValue {
    CValue::Record {
        type_name: "Available".into(),
        fields: Arc::new(BTreeMap::from([
            ("kind".into(), schema_tag("Available")),
            ("fragment".into(), CValue::Code(Box::new(Code { expr, deps: BTreeMap::new() }))),
        ])),
    }
}

pub(super) fn code_of(expr: Expr) -> CValue {
    CValue::Code(Box::new(Code { expr, deps: BTreeMap::new() }))
}

pub(super) fn mint_scope(next_ref: &mut u64, scopes: &mut BTreeSet<u64>, binder: Option<&str>) -> CValue {
    *next_ref += 1;
    let id = *next_ref;
    scopes.insert(id);
    CValue::Record {
        type_name: "Scope".into(),
        fields: Arc::new(BTreeMap::from([
            ("id".into(), cint(id)),
            ("binder".into(), schema_tag(binder.unwrap_or("closed"))),
            ("token".into(), schema_tag(&format!("#scope.{id}"))),
        ])),
    }
}

pub(super) fn fragment_package(term: Expr, scope: CValue) -> CValue {
    CValue::Record {
        type_name: "Fragment".into(),
        fields: Arc::new(BTreeMap::from([
            ("term".into(), code_of(term)),
            ("context".into(), scope),
        ])),
    }
}

pub(super) fn fragment_term(value: CValue) -> Result<Expr, String> {
    match value {
        CValue::Code(code) => Ok(code.expr),
        CValue::Record { type_name, fields }
            if type_name == "Fragment" || fields.contains_key("term") =>
        {
            match fields.get("term") {
                Some(CValue::Code(code)) => Ok(code.expr.clone()),
                Some(other) => rebuild_expr(other),
                None => Err("Fragment missing term".into()),
            }
        }
        _ => Err("quote expects Code or Fragment".into()),
    }
}

pub(super) fn open_fragment(package: CValue) -> Result<CValue, ConstructorError> {
    match fragment_term(package) {
        Ok(expr) => Ok(CValue::Code(Box::new(Code { expr, deps: BTreeMap::new() }))),
        Err(message) => Err(fault("type", message)),
    }
}

pub(super) fn view_record(tag: &str, fields: BTreeMap<String, CValue>) -> CValue {
    let mut fields = fields;
    fields.insert("kind".into(), schema_tag(tag));
    CValue::Record {
        type_name: tag.into(),
        fields: Arc::new(fields),
    }
}

pub(super) fn view_of(
    expr: &Expr,
    functions: &BTreeMap<String, Rc<FnDecl>>,
    next_ref: &mut u64,
    scopes: &mut BTreeSet<u64>,
) -> CValue {
    let mut pack = |term: Expr, binder: Option<&str>| {
        fragment_package(term, mint_scope(next_ref, scopes, binder))
    };
    match &expr.kind {
        ExprKind::Int(text) | ExprKind::Float(text) => view_record(
            "Literal",
            BTreeMap::from([("value".into(), parse_int(text).unwrap_or_else(|_| cint(0)))]),
        ),
        ExprKind::Rational { numer, denom } => {
            let value = match (parse_exact(numer), parse_exact(denom)) {
                (Ok(num), Ok(den)) if !den.is_zero() => CValue::Rat { num, den }
                    .canon()
                    .unwrap_or_else(|_| cint(0)),
                _ => cint(0),
            };
            view_record("Literal", BTreeMap::from([("value".into(), value)]))
        }
        ExprKind::Bool(value) => view_record(
            "Literal",
            BTreeMap::from([("value".into(), CValue::Bool(*value))]),
        ),
        ExprKind::Path { segments, .. } => {
            let name = segments.join(".");
            if functions.contains_key(&name) {
                let visibility = if functions[&name].opaque {
                    "opaque"
                } else {
                    "transparent"
                };
                view_record(
                    "Global",
                    BTreeMap::from([
                        ("name".into(), schema_tag(&name)),
                        ("visibility".into(), schema_tag(visibility)),
                    ]),
                )
            } else if segments.len() >= 2 {
                let field = segments[segments.len() - 1].as_str();
                let container = segments[..segments.len() - 1].join(".");
                view_record(
                    "Projection",
                    BTreeMap::from([
                        ("container".into(), code_of(path_expr(&container))),
                        ("field".into(), schema_tag(field)),
                        ("scrutinee".into(), pack(path_expr(&container), None)),
                    ]),
                )
            } else {
                view_record(
                    "Local",
                    BTreeMap::from([
                        ("name".into(), schema_tag(&name)),
                        ("token".into(), schema_tag(&name)),
                    ]),
                )
            }
        }
        ExprKind::Call { function, args } => view_record(
            "Call",
            BTreeMap::from([
                ("callee".into(), code_of(*function.clone())),
                (
                    "args".into(),
                    CValue::Sequence(std::sync::Arc::new(
                        args.iter().cloned().map(code_of).collect(),
                    )),
                ),
                (
                    "children".into(),
                    CValue::Sequence(std::sync::Arc::new(
                        args.iter()
                            .cloned()
                            .map(|arg| pack(arg, None))
                            .collect(),
                    )),
                ),
            ]),
        ),
        ExprKind::FunctionAbs { param, body, .. } | ExprKind::QuoteBind { param, body, .. } => {
            view_record(
                "Closure",
                BTreeMap::from([
                    ("param".into(), schema_tag(param)),
                    ("body".into(), code_of(*body.clone())),
                    ("scope".into(), pack(*body.clone(), Some(param))),
                ]),
            )
        }
        ExprKind::Binary { op, left, right } => {
            let callee = scalar_op_name(*op).map_or_else(|| schema_tag(&format!("{op:?}")), schema_tag);
            view_record(
                "Call",
                BTreeMap::from([
                    ("callee".into(), callee),
                    (
                        "args".into(),
                        CValue::Sequence(std::sync::Arc::new(vec![
                            code_of(*left.clone()),
                            code_of(*right.clone()),
                        ])),
                    ),
                    (
                        "children".into(),
                        CValue::Sequence(std::sync::Arc::new(vec![
                            pack(*left.clone(), None),
                            pack(*right.clone(), None),
                        ])),
                    ),
                ]),
            )
        }
        ExprKind::Unary { op, value } => view_record(
            "Call",
            BTreeMap::from([
                ("callee".into(), schema_tag(&format!("{op:?}"))),
                (
                    "args".into(),
                    CValue::Sequence(std::sync::Arc::new(vec![code_of(*value.clone())])),
                ),
                (
                    "children".into(),
                    CValue::Sequence(std::sync::Arc::new(vec![pack(*value.clone(), None)])),
                ),
            ]),
        ),
        ExprKind::List(items) | ExprKind::Tuple(items) => view_record(
            "Sequence",
            BTreeMap::from([
                (
                    "elements".into(),
                    CValue::Sequence(std::sync::Arc::new(
                        items.iter().cloned().map(code_of).collect(),
                    )),
                ),
                (
                    "children".into(),
                    CValue::Sequence(std::sync::Arc::new(
                        items
                            .iter()
                            .cloned()
                            .map(|item| pack(item, None))
                            .collect(),
                    )),
                ),
            ]),
        ),
        ExprKind::Quote { body } => view_record(
            "Quotation",
            BTreeMap::from([
                ("body".into(), code_of(*body.clone())),
                ("scope".into(), pack(*body.clone(), None)),
            ]),
        ),
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            let mut fields = BTreeMap::from([
                (
                    "arms".into(),
                    CValue::Sequence(std::sync::Arc::new(
                        arms.iter()
                            .map(|(cond, value)| {
                                CValue::Tuple(vec![
                                    code_of(cond.clone()),
                                    code_of(value.clone()),
                                ])
                            })
                            .collect(),
                    )),
                ),
                ("else".into(), code_of(*else_arm.clone())),
                ("otherwise".into(), code_of(*else_arm.clone())),
                (
                    "children".into(),
                    CValue::Sequence(std::sync::Arc::new({
                        let mut children = arms
                            .iter()
                            .map(|(_, value)| pack(value.clone(), None))
                            .collect::<Vec<_>>();
                        children.push(pack(*else_arm.clone(), None));
                        children
                    })),
                ),
            ]);
            if let Some(subject) = subject {
                fields.insert("subject".into(), code_of(*subject.clone()));
                fields.insert(
                    "scrutinee".into(),
                    pack(*subject.clone(), None),
                );
            }
            view_record("Cases", fields)
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            let children = CValue::Sequence(std::sync::Arc::new(vec![
                pack(*condition.clone(), None),
                pack(*then_value.clone(), None),
                pack(*else_value.clone(), None),
            ]));
            view_record(
                "Branch",
                BTreeMap::from([
                    ("condition".into(), code_of(*condition.clone())),
                    ("then".into(), code_of(*then_value.clone())),
                    ("else".into(), code_of(*else_value.clone())),
                    ("otherwise".into(), code_of(*else_value.clone())),
                    ("children".into(), children),
                ]),
            )
        }
        ExprKind::Index { value, indices } => {
            let index_pkgs = indices
                .iter()
                .cloned()
                .map(|index| pack(index, None))
                .collect::<Vec<_>>();
            let mut fields = BTreeMap::from([
                ("container".into(), code_of(*value.clone())),
                (
                    "indices".into(),
                    CValue::Sequence(std::sync::Arc::new(
                        indices.iter().cloned().map(code_of).collect(),
                    )),
                ),
                ("children".into(), CValue::Sequence(std::sync::Arc::new(index_pkgs))),
                ("scrutinee".into(), pack(*value.clone(), None)),
            ]);
            if let Some(index) = indices.first() {
                fields.insert("index".into(), code_of(index.clone()));
            }
            view_record("Index", fields)
        }
        ExprKind::Recur { name, ty, body } => {
            let scope = pack(*body.clone(), Some(name));
            view_record(
                "Recur",
                BTreeMap::from([
                    ("name".into(), schema_tag(name)),
                    ("type".into(), code_of(*ty.clone())),
                    ("body".into(), code_of(*body.clone())),
                    ("scope".into(), scope),
                ]),
            )
        }
        ExprKind::Record { type_path, fields } => {
            let children = fields
                .iter()
                .map(|(name, value)| {
                    CValue::Tuple(vec![schema_tag(name), pack(value.clone(), None)])
                })
                .collect();
            view_record(
                "Record",
                BTreeMap::from([
                    ("schema".into(), schema_tag(&type_path.join("."))),
                    (
                        "fields".into(),
                        CValue::Sequence(std::sync::Arc::new(
                            fields
                                .iter()
                                .map(|(name, value)| {
                                    CValue::Tuple(vec![schema_tag(name), code_of(value.clone())])
                                })
                                .collect(),
                        )),
                    ),
                    ("children".into(), CValue::Sequence(std::sync::Arc::new(children))),
                ]),
            )
        }
        ExprKind::SequenceCons { head, tail } => view_record(
            "Sequence",
            BTreeMap::from([
                ("head".into(), code_of(*head.clone())),
                ("tail".into(), code_of(*tail.clone())),
                (
                    "children".into(),
                    CValue::Sequence(std::sync::Arc::new(vec![
                        pack(*head.clone(), None),
                        pack(*tail.clone(), None),
                    ])),
                ),
            ]),
        ),
        _ => view_record("Opaque", BTreeMap::from([("body".into(), code_of(expr.clone()))])),
    }
}

