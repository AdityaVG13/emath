//! Universal constants, storage, text, set, series, and record lowering.

use super::{
    BackendError, EmirOp, EmirProgram, Expr, InputKinds, ValueKind, borrowed_value,
    escape_ident, input_kind, kind_at, node_family, operand,
    infer_instantiation, owned_value, record_ident, record_layout, record_template,
    render_expr, to_node,
};

use super::numeric_boundary_value;

pub(super) fn op_data_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    input_kinds: &InputKinds,
    kinds: &[ValueKind],
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::FormatText {
            template,
            arguments,
        } => {
            // VM formatting replaces only the supplied positional holes.
            // Literal braces remain text; operands are expressions, not Rust captures.
            let escape = |text: &str| text.replace('{', "{{").replace('}', "}}");
            let mut parts = template.splitn(arguments.len() + 1, "{}");
            let mut rendered = escape(parts.next().unwrap_or_default());
            let mut values = String::new();
            for (part, argument) in parts.zip(arguments) {
                rendered.push_str("{}");
                rendered.push_str(&escape(part));
                values.push_str(", ");
                values.push_str(&render_expr(&operand(program, *argument)));
            }
            Ok(Expr::Raw(format!("format!({rendered:?}{values})")))
        }
        EmirOp::RecordCreate { type_name, fields } => {
            // A generic object's literal builds the instantiation its
            // field carriers bind.
            let instantiation;
            let type_name = if record_template(type_name).is_some() {
                let field_kinds = fields
                    .iter()
                    .map(|(name, value)| (name.clone(), kind_at(kinds, *value)))
                    .collect::<Vec<_>>();
                instantiation = infer_instantiation(type_name, &field_kinds).ok_or_else(|| {
                    BackendError::UnsupportedType(format!(
                        "generic record {type_name} literal leaves a type parameter unbound"
                    ))
                })?;
                &instantiation
            } else {
                type_name
            };
            if record_layout(type_name).is_none() {
                // A node-family record (the structural walk's
                // `Call: {callee: .., args: ..}` construction): the
                // dynamic node value over the shared tree family.
                // Field values fold into the family - scalars and
                // Code values wrap, node values pass through.
                if !emath_exec_ir::constructor_layer::is_node_tag(type_name) {
                    return Err(BackendError::UnsupportedType(format!(
                        "record {type_name} has no authored layout"
                    )));
                }

                let members = fields
                    .iter()
                    .map(|(name, value)| {
                        let kind = kind_at(kinds, *value);
                        if kind != ValueKind::Node && !node_family(&kind) {
                            return Err(BackendError::UnsupportedType(format!(
                                "node record {type_name} field {name} carries {kind:?} - outside the node family"
                            )));
                        }
                        Ok(format!(
                            "(String::from({name:?}), {})",
                            render_expr(&to_node(operand(program, *value), &kind)?)
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ");
                return Ok(Expr::Raw(format!(
                    "emath_rt::code_tree::node_record({type_name:?}, vec![{members}])"
                )));
            }
            let layout = record_layout(type_name)
                .ok_or_else(|| BackendError::UnsupportedType(type_name.clone()))?;
            for (field, _) in &layout {
                if !fields.iter().any(|(name, _)| name == field) {
                    return Err(BackendError::UnsupportedType(format!(
                        "record {type_name} literal must list every field: missing {field}"
                    )));
                }
            }
            let mut members = Vec::with_capacity(fields.len());
            for (name, value) in fields {
                let ty = layout
                    .iter()
                    .find(|(field, _)| field == name)
                    .ok_or_else(|| {
                        BackendError::UnsupportedType(format!("unknown record field {name}"))
                    })?;
                let kind = ValueKind::from_signature(&ty.1);
                // The artifact's tree evaluator yields scalars only: an
                // evaluated run-time code cannot become a function value.
                if kind_at(kinds, *value) == ValueKind::CodeValue
                    && matches!(kind, ValueKind::Closure { .. })
                {
                    return Err(BackendError::UnsupportedType(format!(
                        "record {type_name} field {name} needs a function; evaluated run-time code is a scalar in emitted code (the VM runs it)"
                    )));
                }
                let expression = operand(program, *value);
                let value = match numeric_boundary_value(
                    &expression,
                    &kind_at(kinds, *value),
                    &kind,
                    name,
                ) {
                    Some(converted) => render_expr(&converted),
                    // Method-call clone: the receiver fixes the
                    // literal's concrete closure type, which then
                    // unsizes into the field's `Rc<dyn Fn>`
                    // (`Rc::clone(&x)` takes its type from the field
                    // and refuses the concrete closure).
                    None if matches!(kind, ValueKind::Closure { .. }) => {
                        format!("{}.clone()", render_expr(&expression))
                    }
                    None => render_expr(&owned_value(expression, &kind)),
                };
                members.push(format!("{}: {value}", escape_ident(name)));
            }
            let fields = members.join(", ");
            Ok(Expr::Raw(format!(
                "EmathRecord_{} {{ {fields} }}",
                record_ident(type_name)
            )))
        }
        EmirOp::ConstBool(value) => Ok(Expr::Bool(*value)),
        EmirOp::LoadInput(index) => {
            let name = names
                .get(*index as usize)
                .ok_or_else(|| BackendError::MissingInput(format!("input #{index}")))?;
            let value = Expr::Var(escape_ident(name));
            if input_kind(Some(name), input_kinds).is_copy() {
                Ok(value)
            } else {
                Ok(borrowed_value(value, &input_kind(Some(name), input_kinds)))
            }
        }
        _ => unreachable!("op_data_exprs routed a non-universal data op"),
    }
}

