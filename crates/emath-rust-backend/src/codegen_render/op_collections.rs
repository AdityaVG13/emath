//! Universal selection, collection construction, and checked indexing.

use super::*;

pub(super) fn op_collection_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    kinds: &[ScalarKind],
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => Ok(Expr::IfElse {
            condition: Box::new(operand(program, *condition)),
            then: Box::new(Stmt::Expr(operand(program, *then_value))),
            else_value: Box::new(Stmt::Expr(operand(program, *else_value))),
        }),
        EmirOp::VectorCreate(elements) => Ok(Expr::Macro {
            name: "vec".to_string(),
            args: elements
                .iter()
                .map(|value| typed_operand(program, *value, ScalarKind::F64, kinds))
                .collect(),
        }),
        EmirOp::MatrixCreate {
            rows,
            cols,
            elements,
        } => {
            let mut rendered_rows = Vec::with_capacity(*rows);
            for row in 0..*rows {
                let values = (0..*cols)
                    .map(|col| {
                        typed_operand(program, elements[row * *cols + col], ScalarKind::F64, kinds)
                    })
                    .collect();
                rendered_rows.push(Expr::Macro {
                    name: "vec".to_string(),
                    args: values,
                });
            }
            Ok(Expr::Macro {
                name: "vec".to_string(),
                args: rendered_rows,
            })
        }
        EmirOp::TensorCreate { shape, elements } => {
            let shape = shape
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let data = elements
                .iter()
                .map(|value| render_expr(&typed_operand(program, *value, ScalarKind::F64, kinds)))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(Expr::Raw(format!(
                "emath_rt::Tensor {{ shape: vec![{shape}], data: vec![{data}] }}"
            )))
        }
        EmirOp::VectorIndex { vector, index } => Ok(map_index_result(format!(
            "emath_rt::vec_index_checked(&{}, {})",
            render_expr(&operand(program, *vector)),
            index_f64(program, *index, kinds)
        ))),
        EmirOp::MatrixIndex { matrix, row, col } => Ok(map_index_result(format!(
            "emath_rt::matrix_index_checked(&{}, {}, {})",
            render_expr(&operand(program, *matrix)),
            index_f64(program, *row, kinds),
            index_f64(program, *col, kinds)
        ))),
        EmirOp::TensorIndex { tensor, indices } => {
            Ok(tensor_index_call(program, *tensor, indices, kinds))
        }
        EmirOp::TensorSlice { tensor, axes } => {
            Ok(tensor_slice_call(program, *tensor, axes, kinds))
        }
        _ => unreachable!("op_collection_exprs routed a non-universal collection op"),
    }
}
