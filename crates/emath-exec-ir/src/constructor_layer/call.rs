use super::*;
use super::prelude::*;

pub(super) fn scalar_op_name(op: BinaryOp) -> Option<&'static str> {
    match op {
        BinaryOp::Add => Some("Add"),
        BinaryOp::Sub => Some("Sub"),
        BinaryOp::Mul => Some("Mul"),
        BinaryOp::Div => Some("Div"),
        BinaryOp::Eq => Some("Eq"),
        BinaryOp::Ne => Some("Ne"),
        BinaryOp::Lt => Some("Lt"),
        BinaryOp::Le => Some("Le"),
        BinaryOp::Gt => Some("Gt"),
        BinaryOp::Ge => Some("Ge"),
        BinaryOp::And => Some("And"),
        BinaryOp::Or => Some("Or"),
        BinaryOp::Pow => Some("Pow"),
        BinaryOp::Imply => Some("Imply"),
        BinaryOp::Iff => Some("Iff"),
        BinaryOp::Asymp => Some("Asymp"),
        BinaryOp::In => Some("In"),
    }
}

pub(super) fn function_body_expr(decl: &FnDecl) -> Expr {
    let mut body = if decl.outputs.len() > 1 {
        let items = decl
            .outputs
            .iter()
            .filter_map(|out| {
                decl.defs
                    .iter()
                    .find(|(name, _)| name == out)
                    .map(|(_, expr)| expr.clone())
            })
            .collect::<Vec<_>>();
        dummy_expr(ExprKind::Tuple(items))
    } else {
        decl.output
            .as_ref()
            .and_then(|out| {
                decl.defs
                    .iter()
                    .find(|(name, _)| name == out)
                    .map(|(_, expr)| expr.clone())
            })
            .or_else(|| decl.defs.last().map(|(_, expr)| expr.clone()))
            .unwrap_or_else(|| dummy_expr(ExprKind::Tuple(Vec::new())))
    };
    for param in decl.inputs.iter().rev() {
        body = dummy_expr(ExprKind::FunctionAbs {
            param: param.clone(),
            domain: Box::new(path_expr("Rat")),
            body: Box::new(body),
        });
    }
    body
}

pub(super) fn record_kind_name(type_name: &str, fields: &BTreeMap<String, CValue>) -> String {
    match fields.get("kind") {
        Some(CValue::Record { type_name, .. }) => type_name.clone(),
        _ => type_name.to_string(),
    }
}

pub(super) fn as_scalar_op(value: &CValue) -> Option<BinaryOp> {
    let name = match value {
        CValue::Record { type_name, fields } => record_kind_name(type_name, fields),
        CValue::Code(code) => match &code.expr.kind {
            ExprKind::Path { segments, .. } => segments.join("."),
            _ => return None,
        },
        _ => return None,
    };
    match name.as_str() {
        "Add" => Some(BinaryOp::Add),
        "Sub" => Some(BinaryOp::Sub),
        "Mul" => Some(BinaryOp::Mul),
        "Div" => Some(BinaryOp::Div),
        "Eq" => Some(BinaryOp::Eq),
        "Ne" => Some(BinaryOp::Ne),
        "Lt" => Some(BinaryOp::Lt),
        "Le" => Some(BinaryOp::Le),
        "Gt" => Some(BinaryOp::Gt),
        "Ge" => Some(BinaryOp::Ge),
        "And" => Some(BinaryOp::And),
        "Or" => Some(BinaryOp::Or),
        "Pow" => Some(BinaryOp::Pow),
        "Imply" => Some(BinaryOp::Imply),
        "Iff" => Some(BinaryOp::Iff),
        "Asymp" => Some(BinaryOp::Asymp),
        "In" => Some(BinaryOp::In),
        _ => None,
    }
}

pub(super) fn rebuild_call(fields: &BTreeMap<String, CValue>) -> Result<Expr, String> {
    let args = match fields.get("args") {
        Some(CValue::Sequence(items)) => items
            .iter()
            .map(rebuild_expr)
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err("Call missing args".into()),
    };
    let callee = fields.get("callee").ok_or("Call missing callee")?;
    if let Some(op) = as_scalar_op(callee) {
        if args.len() != 2 {
            return Err("scalar call expects two arguments".into());
        }
        return Ok(dummy_expr(ExprKind::Binary {
            op,
            left: Box::new(args[0].clone()),
            right: Box::new(args[1].clone()),
        }));
    }
    Ok(dummy_expr(ExprKind::Call {
        function: Box::new(rebuild_expr(callee)?),
        args,
    }))
}

pub(super) fn rebuild_expr(value: &CValue) -> Result<Expr, String> {
    match value {
        CValue::Code(code) => Ok(code.expr.clone()),
        CValue::Int(_) | CValue::Rat { .. } | CValue::Bool(_) | CValue::Float64(_) => {
            Ok(value_to_expr(value))
        }
        CValue::Sequence(items) => {
            let exprs = items
                .iter()
                .map(rebuild_expr)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(dummy_expr(ExprKind::Tuple(exprs)))
        }
        CValue::Tuple(items) => {
            let exprs = items
                .iter()
                .map(rebuild_expr)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(dummy_expr(ExprKind::Tuple(exprs)))
        }
        CValue::Record { type_name, fields } => {
            let kind = record_kind_name(type_name, fields);
            match kind.as_str() {
                "Literal" => fields
                    .get("value")
                    .map(value_to_expr)
                    .ok_or_else(|| "Literal node missing value".into()),
                "Local" | "Global" => {
                    let name = match fields.get("name") {
                        Some(CValue::Record { type_name, .. }) => type_name.clone(),
                        Some(CValue::Code(code)) => match &code.expr.kind {
                            ExprKind::Path { segments, .. } => segments.join("."),
                            _ => return Err("reference name is not a path".into()),
                        },
                        _ => type_name.clone(),
                    };
                    Ok(path_expr(&name))
                }
                "Call" => rebuild_call(fields),
                "Closure" => {
                    let param = match fields.get("param") {
                        Some(CValue::Record { type_name, .. }) => type_name.clone(),
                        _ => return Err("Closure missing param".into()),
                    };
                    let body = rebuild_expr(fields.get("body").ok_or("Closure missing body")?)?;
                    Ok(dummy_expr(ExprKind::FunctionAbs {
                        param,
                        domain: Box::new(path_expr("Rat")),
                        body: Box::new(body),
                    }))
                }
                "Sequence" => {
                    let elements = match fields.get("elements") {
                        Some(CValue::Sequence(items)) => items,
                        _ => return Err("Sequence missing elements".into()),
                    };
                    let exprs = elements
                        .iter()
                        .map(rebuild_expr)
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(dummy_expr(ExprKind::Tuple(exprs)))
                }
                "Fragment" => fields
                    .get("term")
                    .ok_or_else(|| String::from("Fragment missing term"))
                    .and_then(rebuild_expr),
                "Branch" | "If" => {
                    let condition =
                        rebuild_expr(fields.get("condition").ok_or("Branch missing condition")?)?;
                    let then_value =
                        rebuild_expr(fields.get("then").ok_or("Branch missing then")?)?;
                    let else_value = fields
                        .get("otherwise")
                        .or_else(|| fields.get("else"))
                        .ok_or("Branch missing else")?;
                    let else_value = rebuild_expr(else_value)?;
                    Ok(dummy_expr(ExprKind::If {
                        condition: Box::new(condition),
                        then_value: Box::new(then_value),
                        else_value: Box::new(else_value),
                    }))
                }
                "Projection" => {
                    let container =
                        rebuild_expr(fields.get("container").ok_or("Projection missing container")?)?;
                    let field = match fields.get("field") {
                        Some(CValue::Record { type_name, .. }) => type_name.clone(),
                        _ => return Err("Projection missing field".into()),
                    };
                    match container.kind {
                        ExprKind::Path { mut segments, generics } => {
                            segments.push(field);
                            Ok(dummy_expr(ExprKind::Path { segments, generics }))
                        }
                        _ => Err("Projection container is not a path".into()),
                    }
                }
                "Index" => {
                    let container =
                        rebuild_expr(fields.get("container").ok_or("Index missing container")?)?;
                    let indices = match fields.get("indices") {
                        Some(CValue::Sequence(items)) => items
                            .iter()
                            .map(rebuild_expr)
                            .collect::<Result<Vec<_>, _>>()?,
                        _ => match fields.get("index") {
                            Some(index) => vec![rebuild_expr(index)?],
                            None => return Err("Index missing index".into()),
                        },
                    };
                    Ok(dummy_expr(ExprKind::Index {
                        value: Box::new(container),
                        indices,
                    }))
                }
                "Quotation" => {
                    let body = rebuild_expr(fields.get("body").ok_or("Quotation missing body")?)?;
                    Ok(dummy_expr(ExprKind::Quote {
                        body: Box::new(body),
                    }))
                }
                "Record" => {
                    let schema = match fields.get("schema") {
                        Some(CValue::Record { type_name, .. }) => type_name.clone(),
                        _ => return Err("Record missing schema".into()),
                    };
                    let record_fields = match fields.get("fields") {
                        Some(CValue::Sequence(items)) => items
                            .iter()
                            .map(|item| match item {
                                CValue::Tuple(pair) if pair.len() == 2 => {
                                    let name = match &pair[0] {
                                        CValue::Record { type_name, .. } => type_name.clone(),
                                        _ => return Err("Record field name is not a tag".into()),
                                    };
                                    Ok((name, rebuild_expr(&pair[1])?))
                                }
                                _ => Err(String::from("Record field is not a pair")),
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                        _ => return Err("Record missing fields".into()),
                    };
                    Ok(dummy_expr(ExprKind::Record {
                        type_path: schema.split('.').map(str::to_string).collect(),
                        fields: record_fields,
                    }))
                }
                "Recur" => {
                    let name = match fields.get("name") {
                        Some(CValue::Record { type_name, .. }) => type_name.clone(),
                        _ => return Err("Recur missing name".into()),
                    };
                    let ty = rebuild_expr(fields.get("type").ok_or("Recur missing type")?)?;
                    let body = rebuild_expr(fields.get("body").ok_or("Recur missing body")?)?;
                    Ok(dummy_expr(ExprKind::Recur {
                        name,
                        ty: Box::new(ty),
                        body: Box::new(body),
                    }))
                }
                "Match" | "Cases" => {
                    let arms = match fields.get("arms") {
                        Some(CValue::Sequence(items)) => items
                            .iter()
                            .map(|item| match item {
                                CValue::Tuple(pair) if pair.len() == 2 => {
                                    Ok((rebuild_expr(&pair[0])?, rebuild_expr(&pair[1])?))
                                }
                                _ => Err(String::from("Match arm is not a pair")),
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                        _ => return Err("Match missing arms".into()),
                    };
                    let else_arm = fields
                        .get("otherwise")
                        .or_else(|| fields.get("else"))
                        .ok_or("Match missing else")?;
                    let else_arm = rebuild_expr(else_arm)?;
                    let subject = fields
                        .get("subject")
                        .map(rebuild_expr)
                        .transpose()?
                        .map(Box::new);
                    Ok(dummy_expr(ExprKind::Cases {
                        subject,
                        arms,
                        else_arm: Box::new(else_arm),
                    }))
                }
                other => Err(format!("unknown node `{other}`")),
            }
        }
        other => Err(format!("{other}")),
    }
}

pub(super) fn project_field(value: &CValue, field: &str) -> Option<CValue> {
    match value {
        CValue::Record { fields, .. } => fields.get(field).cloned(),
        CValue::Tuple(items) => tuple_index(field).and_then(|index| items.get(index).cloned()),
        CValue::Sequence(items) if field == "length" => Some(cint(items.len())),
        // A read-only projection: the slot count is fixed at
        // construction, so poison recovery cannot lie about it.
        CValue::Buffer(cell) if field == "length" => Some(cint(
            cell.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).len(),
        )),
        CValue::Rat { num, den } if field == "numer" => Some(CValue::Int(num.clone())),
        CValue::Rat { den, .. } if field == "denom" => Some(CValue::Int(den.clone())),
        CValue::Int(n) if field == "numer" => Some(CValue::Int(n.clone())),
        CValue::Int(_) if field == "denom" => Some(cint(1)),
        _ => None,
    }
}

pub(super) fn function_is_opaque(decl: &Declaration) -> bool {
    for section in decl.sections().filter(|section| section.name == "exports") {
        for stmt in &section.suite.statements {
            match &stmt.kind {
                StmtKind::FieldDecl { ty, .. } => {
                    if format!("{ty:?}").contains("opaque") {
                        return true;
                    }
                }
                StmtKind::Assign { value, .. } => {
                    if let ExprKind::Path { segments, .. } = &value.kind {
                        if segments.iter().any(|segment| segment == "opaque") {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    false
}

pub(super) fn object_representation_kind(decl: &Declaration) -> String {
    if let Some(kind) = section_text(decl, "representation", "kind")
        .or_else(|| section_text(decl, "representation", "shape"))
    {
        return kind;
    }
    decl.sections()
        .find(|section| section.name == "representation")
        .and_then(|section| {
            section.suite.statements.first().and_then(|stmt| match &stmt.kind {
                StmtKind::FieldDecl { name, ty, .. } => {
                    if name == "abstract" || name == "package" {
                        Some(name.clone())
                    } else if format!("{ty:?}").contains("abstract") {
                        Some("abstract".into())
                    } else if format!("{ty:?}").contains("package") {
                        Some("package".into())
                    } else {
                        None
                    }
                }
                StmtKind::Assign { value, .. } => match &value.kind {
                    ExprKind::Path { segments, .. } => segments.last().cloned(),
                    ExprKind::Call { function, .. } => match &function.kind {
                        ExprKind::Path { segments, .. } => segments.last().cloned(),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            })
        })
        .unwrap_or_else(|| "record".into())
}

pub(super) fn section_assigns(decl: &Declaration, name: &str) -> Vec<(String, Expr)> {
    decl.sections()
        .filter(|section| section.name == name)
        .flat_map(|section| {
            section.suite.statements.iter().filter_map(|stmt| {
                if let StmtKind::Assign { target, value } = &stmt.kind {
                    target.segments.first().cloned().map(|name| (name, value.clone()))
                } else {
                    None
                }
            })
        })
        .collect()
}

pub(super) fn section_fields(decl: &Declaration, name: &str) -> Vec<String> {
    section_typed_fields(decl, name)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// Output definitions with annotation-directed exactness: under a `Rat`
/// output annotation, bare decimals in the def's numeric spine denote
/// their exact decimal rationals (reference: types chapter). Admission
/// inference and the engine read definitions through this one seam, so
/// both agree on the carrier.
pub(super) fn constructor_defs(decl: &Declaration) -> Vec<(String, Expr)> {
    let rat_outputs: BTreeSet<String> = section_typed_fields(decl, "outputs")
        .into_iter()
        .filter(|(_, ty)| ctype_from_type(ty) == CType::Rat)
        .map(|(name, _)| name)
        .collect();
    section_assigns(decl, "definitions")
        .into_iter()
        .map(|(name, expr)| {
            if rat_outputs.contains(&name) {
                (name, exact_decimal_spine(&expr))
            } else {
                (name, expr)
            }
        })
        .collect()
}

pub(super) fn section_typed_fields(decl: &Declaration, name: &str) -> Vec<(String, TypeExpr)> {
    decl.sections()
        .filter(|section| section.name == name)
        .flat_map(|section| {
            section.suite.statements.iter().filter_map(|stmt| {
                if let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind {
                    Some((name.clone(), ty.clone()))
                } else {
                    None
                }
            })
        })
        .collect()
}

/// Two fields with the same name in one `inputs:`/`outputs:` section
/// would silently collapse (last-wins) when collected into a map; the
/// identity fork is refused instead (E-NAME-020, the same class the
/// sema admitter's duplicate-field check pins).
pub(super) fn refuse_duplicate_fields(decl: &Declaration, section: &str) -> Result<(), ConstructorError> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for (name, _) in section_typed_fields(decl, section) {
        if !seen.insert(name.clone()) {
            return Err(fault(
                "E-NAME-020",
                format!(
                    "duplicate field `{name}` in `{section}:` — the second declaration would silently overwrite the first"
                ),
            ));
        }
    }
    Ok(())
}

pub(super) fn admit_input_type(ty: &TypeExpr, value: &CValue) -> Result<(), ConstructorError> {
    if type_admits(ty, value) {
        Ok(())
    } else if let (
        TypeExpr {
            kind: TypeKind::Fn { domain, .. },
            ..
        },
        CValue::Closure(clos),
    ) = (ty, value) {
        // Name the mismatch: the closure's recorded domain vs the
        // declared input domain. A generic "wrong type" message here
        // buried the element semantics the closure actually demands.
        Err(fault(
            "type",
            format!(
                "closure domain `{}` does not match the declared input domain `{}`",
                clos.domain,
                domain_shape_of_type(domain)
            ),
        ))
    } else {
        Err(fault(
            "type",
            format!("value `{value}` does not have the declared input type"),
        ))
    }
}

/// Carrier shape spelled by a closure-literal domain expression
/// (`function x in <domain>`): `Rat`, `sequence(Rat)`, or
/// [`DomainShape::Unknown`] when the tag does not carry a
/// reconstructible shape.
pub(in crate::constructor_layer) fn domain_shape_of_expr(expr: &Expr) -> DomainShape {
    match &expr.kind {
        ExprKind::Path { segments, .. } if segments.len() == 1 => match segments[0].as_str() {
            "Int" | "Rat" | "Bool" | "Float64" => DomainShape::Scalar(segments[0].clone()),
            _ => DomainShape::Unknown,
        },
        ExprKind::Call { function, args } => {
            let ExprKind::Path { segments, .. } = &function.kind else {
                return DomainShape::Unknown;
            };
            // `sequence(T)` in expression position parses as a call
            // of the carrier name with the element argument.
            if segments.len() == 1 && segments[0] == "sequence" && args.len() == 1 {
                DomainShape::Sequence(Some(Box::new(domain_shape_of_expr(&args[0]))))
            } else {
                DomainShape::Unknown
            }
        }
        _ => DomainShape::Unknown,
    }
}

/// Domain half of a fn-type EXPRESSION (`A -> B` desugars to
/// `Fn(A, B)`): what a recur binder's closure demands of its argument.
/// Record-of-functions shapes stay Unknown.
pub(in crate::constructor_layer) fn arrow_domain_of_expr(ty: &Expr) -> DomainShape {
    match &ty.kind {
        ExprKind::Call { function, args } => {
            let ExprKind::Path { segments, .. } = &function.kind else {
                return DomainShape::Unknown;
            };
            if segments.as_slice() == ["Fn"] && args.len() == 2 {
                domain_shape_of_expr(&args[0])
            } else {
                DomainShape::Unknown
            }
        }
        _ => DomainShape::Unknown,
    }
}

/// Carrier shape spelled by a DECLARED type (`A -> B` input
/// annotations): the same [`DomainShape`] vocabulary the closure
/// records, so both check sites compare like against like.
pub(in crate::constructor_layer) fn domain_shape_of_type(ty: &TypeExpr) -> DomainShape {
    match &ty.kind {
        TypeKind::Path {
            segments,
            generic_args,
        } => {
            if segments.len() == 1
                && generic_args.is_empty()
                && matches!(segments[0].as_str(), "Int" | "Rat" | "Bool" | "Float64")
            {
                DomainShape::Scalar(segments[0].clone())
            } else if segments.len() == 1 && segments[0] == "sequence" {
                match generic_args.first() {
                    Some(GenericArg::Type(element)) => {
                        DomainShape::Sequence(Some(Box::new(domain_shape_of_type(element))))
                    }
                    _ => DomainShape::Sequence(None),
                }
            } else {
                DomainShape::Unknown
            }
        }
        // The bracket sequence spelling (`T[n]`).
        TypeKind::List(items) if items.len() == 1 => {
            DomainShape::Sequence(Some(Box::new(domain_shape_of_type(&items[0]))))
        }
        _ => DomainShape::Unknown,
    }
}

/// The one conformance relation for recorded vs declared domain shapes,
/// shared by the admission lane and the runtime lane so the two can
/// never disagree. `Unknown` on either side admits with no claim;
/// `Int` widens to `Rat` exactly as the scalar carriers do; every
/// carrier/element mismatch refuses.
pub(in crate::constructor_layer) fn domain_shape_conforms(
    recorded: &DomainShape,
    declared: &DomainShape,
) -> bool {
    match (recorded, declared) {
        (DomainShape::Unknown, _) | (_, DomainShape::Unknown) => true,
        (DomainShape::Scalar(rec), DomainShape::Scalar(dec)) => {
            rec == dec || (rec == "Int" && dec == "Rat")
        }
        (DomainShape::Sequence(rec), DomainShape::Sequence(dec)) => match (rec, dec) {
            (Some(rec_element), Some(dec_element)) => {
                domain_shape_conforms(rec_element, dec_element)
            }
            _ => true,
        },
        (DomainShape::Scalar(_), DomainShape::Sequence(_))
        | (DomainShape::Sequence(_), DomainShape::Scalar(_)) => false,
    }
}

/// The scalar widening rules of `type_admits`'s Path arm, named so the
/// closure domain check reuses the SAME carrier discipline.
pub(in crate::constructor_layer) fn scalar_carrier_admits(name: &str, value: &CValue) -> bool {
    match name {
        "Int" => matches!(value, CValue::Int(_)),
        "Bool" => matches!(value, CValue::Bool(_)),
        "Rat" => matches!(value, CValue::Rat { .. } | CValue::Int(_)),
        "Float64" => matches!(value, CValue::Float64(_)),
        _ => true,
    }
}

/// Runtime-lane check: does the argument satisfy the closure's
/// recorded domain shape? Sequence elements are checked item by item
/// with the same widening rules as the admission lane.
pub(in crate::constructor_layer) fn domain_shape_admits_value(
    shape: &DomainShape,
    value: &CValue,
) -> bool {
    match shape {
        DomainShape::Unknown => true,
        DomainShape::Scalar(name) => scalar_carrier_admits(name, value),
        DomainShape::Sequence(element) => match (value, element) {
            (CValue::Sequence(items), Some(element_shape)) => items
                .iter()
                .all(|item| domain_shape_admits_value(element_shape, item)),
            (CValue::Sequence(_), None) => true,
            _ => false,
        },
    }
}

pub(super) fn type_admits(ty: &TypeExpr, value: &CValue) -> bool {
    match &ty.kind {
        // A closure value satisfies an arrow type only when its
        // RECORDED domain conforms to the declared domain, element
        // semantics included. Unknown recorded shapes admit with no
        // claim — the carrier-only discipline this arm replaced.
        TypeKind::Fn { domain, .. } => match value {
            CValue::Closure(clos) => {
                domain_shape_conforms(&clos.domain, &domain_shape_of_type(domain))
            }
            _ => false,
        },
        TypeKind::List(_) => matches!(value, CValue::Sequence(_)),
        TypeKind::Tuple(_) => matches!(value, CValue::Tuple(_) | CValue::Record { .. }),
        TypeKind::Path { segments, .. } => match segments.last().map(String::as_str) {
            Some("Int") => matches!(value, CValue::Int(_)),
            Some("Bool") => matches!(value, CValue::Bool(_)),
            Some("Rat") => matches!(value, CValue::Rat { .. } | CValue::Int(_)),
            Some("Str") => matches!(value, CValue::Str(_)),
            Some("Float64") => matches!(value, CValue::Float64(_)),
            Some("Code") => matches!(value, CValue::Code(_)),
            Some("sequence") => matches!(value, CValue::Sequence(_)),
            _ => true,
        },
        _ => true,
    }
}

pub(super) fn section_text(decl: &Declaration, section: &str, field: &str) -> Option<String> {
    for s in decl.sections().filter(|s| s.name == section) {
        for stmt in &s.suite.statements {
            if let StmtKind::Assign { target, value } = &stmt.kind {
                if target.segments.first().map(String::as_str) == Some(field) {
                    if let ExprKind::Path { segments, .. } = &value.kind {
                        return Some(segments.join("."));
                    }
                    if let ExprKind::Str(text) = &value.kind {
                        return Some(text.clone());
                    }
                }
            }
            if let StmtKind::FieldDecl { name, default, .. } = &stmt.kind {
                if name == field {
                    if let Some(ExprKind::Path { segments, .. }) = default.as_ref().map(|e| &e.kind)
                    {
                        return Some(segments.join("."));
                    }
                }
            }
        }
    }
    None
}

