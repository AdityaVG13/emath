//! Universal constants, storage, text, set, series, and record lowering.

use super::{
    BackendError, EmirOp, EmirProgram, EmirValue, Expr, InputKinds, ValueKind, borrowed_value,
    escape_ident, input_kind, kind_at, local_state, node_family, operand,
    operand_ref, owned_value, record_layout, render_expr, rt_call, to_node,
};

use super::numeric_boundary_value;

pub(super) fn op_data_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    states: &[String],
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
        EmirOp::SeriesCreate { points, .. } => Ok(Expr::Raw(format!(
            "vec![{}]",
            points
                .iter()
                .map(|(time, value)| format!("({time:?}, {value:?})"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
        EmirOp::SeriesSample { series, time } => Ok(rt_call(
            "series_sample_linear",
            vec![operand_ref(program, *series), operand(program, *time)],
        )),
        EmirOp::SetCreate { elements, guards } => {
            // Carrier-element sets (the packed Sequence of an einsum
            // call) render as a Vec<emath_rt::Tensor>: the scalar
            // flatten form cannot hold Vector/Matrix/Tensor elements.

            let carrier_elements = elements.iter().any(|element| {
                matches!(
                    kind_at(kinds, *element),
                    ValueKind::Vector(_) | ValueKind::Matrix(_) | ValueKind::Tensor
                )
            });
            if carrier_elements {
                let mut entries = Vec::new();
                for (index, element) in elements.iter().enumerate() {
                    let Some(converted) = element_tensor_expr(*element, program, kinds) else {
                        return Err(BackendError::UnsupportedType(
                            "set of mixed carrier elements has no rendering yet".into(),
                        ));
                    };
                    match guards.get(index).copied().flatten() {
                        Some(guard) => entries.push(format!(
                            "if {} {{ Some({converted}) }} else {{ None }}",
                            render_expr(&operand(program, guard))
                        )),
                        None => entries.push(format!("Some({converted})")),
                    }
                }
                return Ok(Expr::Raw(format!(
                    "vec![{}].into_iter().flatten().collect::<Vec<emath_rt::Tensor>>()",
                    entries.join(", ")
                )));
            }
            let mut entries = Vec::new();
            for (index, element) in elements.iter().enumerate() {
                let value = render_expr(&operand(program, *element));
                match guards.get(index).copied().flatten() {
                    Some(guard) => entries.push(format!(
                        "if {} {{ Some({value}) }} else {{ None }}",
                        render_expr(&operand(program, guard))
                    )),
                    None => entries.push(format!("Some({value})")),
                }
            }
            Ok(Expr::Raw(format!(
                "vec![{}].into_iter().flatten().collect::<Vec<_>>()",
                entries.join(", ")
            )))
        }
        EmirOp::SetContains { element, set } => Ok(Expr::Raw(format!(
            "{}.contains(&{})",
            render_expr(&operand(program, *set)),
            render_expr(&operand(program, *element))
        ))),
        EmirOp::RecordCreate { type_name, fields } => {
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
                            render_expr(&to_node(operand(program, *value), &kind))
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
                let expression = operand(program, *value);
                let value = match numeric_boundary_value(
                    &expression,
                    &kind_at(kinds, *value),
                    &kind,
                    name,
                ) {
                    Some(converted) => render_expr(&converted),
                    None => render_expr(&owned_value(expression, &kind)),
                };
                members.push(format!("{}: {value}", escape_ident(name)));
            }
            let fields = members.join(", ");
            Ok(Expr::Raw(format!(
                "EmathRecord_{} {{ {fields} }}",
                escape_ident(type_name)
            )))
        }
        EmirOp::ConstComplex(real, imaginary) => {
            Ok(Expr::Raw(format!("({real:?}, {imaginary:?})")))
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
        EmirOp::LoadState(index) => {
            let name = states
                .get(*index as usize)
                .ok_or_else(|| BackendError::MissingInput(format!("state #{index}")))?;
            let value = if local_state() {
                Expr::Var(escape_ident(name))
            } else {
                Expr::Field {
                    receiver: Box::new(Expr::SelfValue),
                    field: escape_ident(name),
                }
            };
            if input_kind(Some(name), input_kinds).is_copy() {
                Ok(value)
            } else {
                Ok(borrowed_value(value, &input_kind(Some(name), input_kinds)))
            }
        }
        _ => unreachable!("op_data_exprs routed a non-universal data op"),
    }
}

/// One set element converted to an `emath_rt::Tensor` expression.
fn element_tensor_expr(value: EmirValue, program: &EmirProgram, kinds: &[ValueKind]) -> Option<String> {
    let expr = render_expr(&operand(program, value));
    match kind_at(kinds, value) {
        ValueKind::Vector(_) => Some(format!(
            "emath_rt::Tensor {{ shape: vec![({expr}).len()], data: ({expr}).clone() }}"
        )),
        ValueKind::Matrix(_) => Some(format!(
            "emath_rt::Tensor {{ shape: vec![({expr}).rows(), ({expr}).cols()], data: ({expr}).as_slice().to_vec() }}"
        )),
        ValueKind::Tensor => Some(format!("({expr}).clone()")),
        _ => None,
    }
}
