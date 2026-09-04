//! Universal constants, storage, text, set, series, and record lowering.

use super::*;

pub(super) fn op_data_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    states: &[String],
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::FormatText {
            template,
            arguments,
        } => {
            let args = arguments
                .iter()
                .map(|value| render_expr(&operand(program, *value)))
                .collect::<Vec<_>>();
            let mut rendered = template.clone();
            for argument in args {
                rendered = rendered.replacen("{}", &format!("{{{argument}}}"), 1);
            }
            Ok(Expr::Raw(format!(
                "format!(\"{}\")",
                rendered.replace('"', "\\\"")
            )))
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
        EmirOp::RecordCreate { fields, .. } => {
            let fields = fields
                .iter()
                .map(|(name, value)| {
                    format!("({name:?}, {})", render_expr(&operand(program, *value)))
                })
                .collect::<Vec<_>>()
                .join(", ");
            Ok(Expr::Raw(format!(
                "std::collections::BTreeMap::from([{fields}])"
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
            Ok(Expr::Var(escape_ident(name)))
        }
        EmirOp::LoadState(index) => {
            let name = states
                .get(*index as usize)
                .ok_or_else(|| BackendError::MissingInput(format!("state #{index}")))?;
            Ok(Expr::Field {
                receiver: Box::new(Expr::SelfValue),
                field: escape_ident(name),
            })
        }
        _ => unreachable!("op_data_exprs routed a non-universal data op"),
    }
}
