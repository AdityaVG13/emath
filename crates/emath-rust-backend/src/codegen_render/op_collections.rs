//! Universal selection, collection construction, and checked indexing.

use super::{
    BackendError, EmirOp, EmirProgram, EmirValue, Expr, ValueKind, checked_integer_operand,
    exact_int_operand, index_f64, kind_at, kind_is_never_indexable, map_runtime_result, operand,
    owned_operand, render_expr, tensor_slice_call, to_node, typed_operand,
};

fn authored_list_concat(
    values: &[EmirValue],
    program: &EmirProgram,
    kinds: &[ValueKind],
) -> Result<Expr, BackendError> {
    // A concat with any node-family operand stays in the family (the
    // VM's node-sequence law): the head list keeps its family
    // spelling (a tuple of node-carried elements, the Cases arms'
    // pattern-value pairs) and typed-sequence operands fold into the
    // family elementwise through the shared bridge.
    if values.iter().any(|value| kind_at(kinds, *value) == ValueKind::Node) {
        let folded = values
            .iter()
            .map(|value| {
                let kind = kind_at(kinds, *value);
                to_node(operand(program, *value), &kind)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let parts = folded
            .iter()
            .map(|value| render_expr(value))
            .collect::<Vec<_>>()
            .join(", ");
        return Ok(Expr::Raw(format!(
            "emath_rt::code_tree::node_concat_all(vec![{parts}])?"
        )));
    }
    let target = super::numeric_element_kind(values, kinds);
    let ValueKind::Vector(element) = &target else {
        return Err(BackendError::UnsupportedType(
            "list concatenation needs an element carrier".into(),
        ));
    };
    let target_ty = crate::rust_ir::render::render_ty(&target.rust_ty()?);
    let mut parameters = Vec::new();
    let mut arguments = Vec::new();
    let mut lengths = String::new();
    let mut extend = String::new();
    for (index, value) in values.iter().enumerate() {
        let source = kind_at(kinds, *value);
        let ValueKind::Vector(source_element) = &source else {
            return Err(BackendError::UnsupportedType(
                "list concatenation requires sequences".into(),
            ));
        };
        let source_ty = source_element.rust_ty().or_else(|_| element.rust_ty())?;
        let name = format!("__list_part_{index}");
        parameters.push(format!(
            "{name}: &[{}]",
            crate::rust_ir::render::render_ty(&source_ty)
        ));
        arguments.push(format!(
            "<_ as AsRef<[{}]>>::as_ref(&({}))",
            crate::rust_ir::render::render_ty(&source_ty),
            render_expr(&operand(program, *value))
        ));
        lengths.push_str(&format!("count = count.checked_add({name}.len()).ok_or_else(|| String::from(\"concatenated length exceeds usize\"))?; "));
        let item = Expr::Raw(
            if source_element.is_copy() {
                "*__list_element"
            } else {
                "__list_element"
            }
            .into(),
        );
        if let Some(converted) =
            super::numeric_boundary_value(&item, source_element, element, "sequence element")
        {
            extend.push_str(&format!(
                "for __list_element in {name} {{ output.push({}); }} ",
                render_expr(&converted)
            ));
        } else {
            extend.push_str(&format!("output.extend_from_slice({name}); "));
        }
    }
    // Borrow each input once outside the closure's binding scope. Counting
    // lengths must not clone vectors or evaluate an inlined operand twice.
    Ok(map_runtime_result(format!(
        "(|{}| -> Result<{target_ty}, String> {{ let mut count = 0usize; {lengths}let mut output = Vec::new(); output.try_reserve_exact(count).map_err(|_| String::from(\"list allocation exceeds available capacity\"))?; {extend}Ok(output) }})({})",
        parameters.join(", "),
        arguments.join(", ")
    )))
}

pub(super) fn op_collection_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    kinds: &[ValueKind],
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::F64SortTotal(value) => {
            if kind_at(kinds, *value) != ValueKind::Vector(Box::new(ValueKind::F64)) {
                return Err(BackendError::UnsupportedType(
                    "f64-sort-total requires a Float64 vector".into(),
                ));
            }
            let value = render_expr(&owned_operand(program, *value, kinds));
            Ok(Expr::Raw(format!(
                "{{ let mut values = {value}; values.sort_by(f64::total_cmp); values }}"
            )))
        }
        EmirOp::ListConcat(values) => authored_list_concat(values, program, kinds),
        EmirOp::IndexText(value) => Ok(Expr::Raw(format!(
            "({} as usize).to_string()",
            render_expr(&typed_operand(program, *value, ValueKind::F64, kinds))
        ))),
        EmirOp::TextTrim(value) | EmirOp::TextLength(value) | EmirOp::ParseF64(value) => {
            if kind_at(kinds, *value) != ValueKind::Text {
                return Err(BackendError::UnsupportedType(
                    "text operation requires Text".into(),
                ));
            }
            let text = render_expr(&operand(program, *value));
            Ok(Expr::Raw(match op {
                EmirOp::TextTrim(_) => format!("({text}).trim().to_owned()"),
                EmirOp::TextLength(_) => format!(
                    "i64::try_from(({text}).len()).map_err(|_| String::from(\"text length exceeds Int\"))?"
                ),
                _ => format!(
                    "({text}).parse::<f64>().map_err(|_| String::from(\"E-SCALAR-CONVERT: invalid Float64 text\"))?"
                ),
            }))
        }
        EmirOp::TextByte(text, index) => {
            if kind_at(kinds, *text) != ValueKind::Text {
                return Err(BackendError::UnsupportedType(
                    "text_byte requires Text".into(),
                ));
            }
            let text = render_expr(&operand(program, *text));
            let index = render_expr(&checked_integer_operand(program, *index, kinds)?);
            Ok(Expr::Raw(format!(
                "{{ let __byte_index = usize::try_from({index}).map_err(|_| String::from(\"text byte index must be nonnegative\"))?; i64::from(*({text}).as_bytes().get(__byte_index).ok_or_else(|| String::from(\"text byte index out of bounds\"))?) }}"
            )))
        }
        EmirOp::FormatScientific(value, precision) => {
            if kind_at(kinds, *precision) != ValueKind::I64 {
                return Err(BackendError::UnsupportedType(
                    "format precision must be Int".into(),
                ));
            }
            Ok(map_runtime_result(format!(
                "emath_rt::format_scientific({}, {})",
                render_expr(&typed_operand(program, *value, ValueKind::F64, kinds)),
                render_expr(&operand(program, *precision))
            )))
        }
        EmirOp::VectorLength(value) => {
            // A node sequence's length is the family's read-only
            // projection (the VM's `length` arm over CValue
            // sequences); everything else keeps the storage lane.
            if kind_at(kinds, *value) == ValueKind::Node {
                let value_code = render_expr(&operand(program, *value));
                return Ok(Expr::Raw(format!(
                    "match ({value_code}).field(\"length\")? {{ emath_rt::code_tree::NodeValue::Scalar(emath_rt::code::CodeValue::Int(length)) => length, _ => return Err(String::from(\"type: length expects a sequence\")) }}"
                )));
            }
            let value_code = render_expr(&operand(program, *value));
            let storage = match kind_at(kinds, *value) {
                ValueKind::Matrix(_) => format!("({value_code}).as_slice()"),
                ValueKind::Tensor => format!("({value_code}).data"),
                _ => value_code,
            };
            Ok(Expr::Raw(format!("({storage}.len() as i64)")))
        }
        EmirOp::VectorIndex { vector, index } => {
            // Indexing a node sequence (the walk's `node.args[0]`)
            // stays in the family: the checked index law and message
            // mirror the VM's `index_seq`.
            if kind_at(kinds, *vector) == ValueKind::Node {
                let sequence = render_expr(&operand(program, *vector));
                let index = render_expr(&super::coerce_to_ty(
                    operand(program, *index),
                    kind_at(kinds, *index),
                    &super::Ty::I64,
                ));
                return Ok(Expr::Raw(format!("({sequence}).index({index})?")));
            }
            // A concrete non-sequence carrier cannot be indexed: the
            // VM's `vector_of` type confusion (op `vector-index`)
            // faults on this op, so the emitted lane refuses by the
            // same name instead of guessing a `.get` lane the carrier
            // does not carry. The Never kind lets every consumer join
            // absorb the refusal.
            if kind_is_never_indexable(&kind_at(kinds, *vector)) {
                return Ok(Expr::Raw(
                    "return Err(\"vector-index type confusion\".to_string())".into(),
                ));
            }
            let collection = render_expr(&operand(program, *vector));
            let materialize = match kind_at(kinds, *vector) {
                ValueKind::Vector(element) if *element == ValueKind::ExactInt => ".cloned()",
                ValueKind::Vector(element) if !element.is_copy() => "",
                ValueKind::Matrix(_) => "",
                _ => ".cloned()",
            };
            if kind_at(kinds, *index) == ValueKind::I64 {
                let index = render_expr(&operand(program, *index));
                Ok(map_runtime_result(format!(
                    "usize::try_from({index}).ok().and_then(|index| ({collection}).get(index)){materialize}.ok_or(\"vector index out of bounds\")"
                )))
            } else if kind_at(kinds, *index) == ValueKind::ExactInt {
                // The exact-integer index carrier narrows through the
                // checked i64 boundary first (E-INT-002 by name - the
                // same law as call arguments and recursive results),
                // then runs the same bounds-checked usize lane as the
                // i64 carrier; the f64 guard below presumes a float
                // carrier and never fits an ExactInt register.
                let index = render_expr(&exact_int_operand(program, *index, kinds));
                Ok(map_runtime_result(format!(
                    "{{ let __index = {index}.to_i64().ok_or(\"E-INT-002: exact integer result exceeds the i64 lane\"); usize::try_from(__index?).ok().and_then(|index| ({collection}).get(index)){materialize}.ok_or(\"vector index out of bounds\") }}"
                )))
            } else {
                let index = index_f64(program, *index, kinds);
                Ok(map_runtime_result(format!(
                    "{{ let index = {index}; if index.is_finite() && index >= 0.0 && index.fract() == 0.0 {{ ({collection}).get(index as usize){materialize}.ok_or(\"vector index out of bounds\") }} else {{ Err(\"vector index must be a nonnegative integer\") }} }}"
                )))
            }
        }
        EmirOp::TensorSlice { tensor, axes } => {
            Ok(tensor_slice_call(program, *tensor, axes, kinds))
        }
        _ => unreachable!("op_collection_exprs routed a non-universal collection op"),
    }
}

