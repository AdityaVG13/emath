//! Universal selection, collection construction, and checked indexing.

use super::*;

pub(super) fn op_collection_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    kinds: &[ValueKind],
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::ToF64(value) => {
            if !matches!(kind_at(kinds, *value), ValueKind::F64 | ValueKind::I64) {
                return Err(BackendError::UnsupportedType("to-f64 requires a numeric scalar".into()));
            }
            Ok(Expr::Raw(format!("({}) as f64", render_expr(&operand(program, *value)))))
        }
        EmirOp::DenseLayout(value) => {
            let code = render_expr(&operand(program, *value));
            Ok(Expr::Raw(match kind_at(kinds, *value) {
                ValueKind::F64 | ValueKind::I64 => "emath_rt::DenseLayout::Scalar".into(),
                ValueKind::Vector(element) if *element == ValueKind::F64 => format!("emath_rt::DenseLayout::Vector(({code}).len())"),
                ValueKind::Matrix(element) if *element == ValueKind::F64 => format!("emath_rt::DenseLayout::Matrix {{ rows: ({code}).rows(), cols: ({code}).cols(), len: ({code}).as_slice().len() }}"),
                ValueKind::Tensor => format!("emath_rt::DenseLayout::Tensor {{ shape: ({code}).shape.clone(), len: ({code}).data.len() }}"),
                _ => return Err(BackendError::UnsupportedType("dense-layout requires a numeric carrier".into())),
            }))
        }
        EmirOp::VectorSlice { vector, offset, count } => {
            if kind_at(kinds, *vector) != ValueKind::Vector(Box::new(ValueKind::F64))
                || kind_at(kinds, *offset) != ValueKind::I64 || kind_at(kinds, *count) != ValueKind::I64 {
                return Err(BackendError::UnsupportedType("vector-slice requires Float64 storage and Int bounds".into()));
            }
            let vector = render_expr(&operand(program, *vector));
            let offset = render_expr(&operand(program, *offset));
            let count = render_expr(&operand(program, *count));
            Ok(map_runtime_result(format!("(|| -> Result<Vec<f64>, String> {{ let offset = usize::try_from({offset}).map_err(|_| String::from(\"slice index must be nonnegative and fit usize\"))?; let count = usize::try_from({count}).map_err(|_| String::from(\"slice index must be nonnegative and fit usize\"))?; offset.checked_add(count).and_then(|end| ({vector}).get(offset..end)).map(<[f64]>::to_vec).ok_or_else(|| String::from(\"slice is outside vector storage\")) }})()")))
        }
        EmirOp::F64SortTotal(value) => {
            if kind_at(kinds, *value) != ValueKind::Vector(Box::new(ValueKind::F64)) {
                return Err(BackendError::UnsupportedType("f64-sort-total requires a Float64 vector".into()));
            }
            let value = render_expr(&owned_operand(program, *value, kinds));
            Ok(Expr::Raw(format!("{{ let mut values = {value}; values.sort_by(f64::total_cmp); values }}")))
        }
        EmirOp::VectorConcat(values) => {
            if values.iter().any(|value| kind_at(kinds, *value) != ValueKind::Vector(Box::new(ValueKind::F64))) {
                return Err(BackendError::UnsupportedType("vector-concat requires Float64 vectors".into()));
            }
            let values = values.iter().map(|value| render_expr(&operand(program, *value))).collect::<Vec<_>>();
            let mut code = String::from("(|| -> Result<Vec<f64>, String> { let mut count = 0usize; ");
            for value in &values { code.push_str(&format!("count = count.checked_add(({value}).len()).ok_or_else(|| String::from(\"concatenated length exceeds usize\"))?; ")); }
            code.push_str("let mut output = Vec::new(); output.try_reserve_exact(count).map_err(|_| String::from(\"vector allocation exceeds available capacity\"))?; ");
            for value in &values { code.push_str(&format!("output.extend_from_slice(&{value}); ")); }
            code.push_str("Ok(output) })()");
            Ok(map_runtime_result(code))
        }
        EmirOp::ListConcat(values) => {
            // Authored cons: element carriers are preserved (records stay
            // records), unlike the Float64 dense-lane VectorConcat. Each
            // part renders owned so extend consumes it.
            let parts = values
                .iter()
                .map(|value| render_expr(&owned_operand(program, *value, kinds)))
                .collect::<Vec<_>>();
            let mut code = String::from("(|| -> Result<Vec<_>, String> { let mut count = 0usize; ");
            for part in &parts {
                code.push_str(&format!("count = count.checked_add(({part}).len()).ok_or_else(|| String::from(\"concatenated length exceeds usize\"))?; "));
            }
            code.push_str("let mut output = Vec::new(); output.try_reserve_exact(count).map_err(|_| String::from(\"list allocation exceeds available capacity\"))?; ");
            for part in &parts {
                code.push_str(&format!("output.extend({part}); "));
            }
            code.push_str("Ok(output) })()");
            Ok(map_runtime_result(code))
        }
        EmirOp::SameDenseShape(left, right) => {
            let a = render_expr(&operand(program, *left));
            let b = render_expr(&operand(program, *right));
            let expression = match (kind_at(kinds, *left), kind_at(kinds, *right)) {
                (ValueKind::DenseLayout(_), ValueKind::DenseLayout(_)) => format!("({a}) == ({b})"),
                (ValueKind::DenseLayout(_), kind) => layout_match_code(&a, &b, &kind),
                (kind, ValueKind::DenseLayout(_)) => layout_match_code(&b, &a, &kind),
                (ValueKind::F64 | ValueKind::I64, ValueKind::F64 | ValueKind::I64) => "true".into(),
                (ValueKind::Vector(a_kind), ValueKind::Vector(b_kind)) if *a_kind == ValueKind::F64 && *b_kind == ValueKind::F64 => format!("({a}).len() == ({b}).len()"),
                (ValueKind::Matrix(a_kind), ValueKind::Matrix(b_kind)) if *a_kind == ValueKind::F64 && *b_kind == ValueKind::F64 => format!("({a}).rows() == ({b}).rows() && ({a}).cols() == ({b}).cols()"),
                (ValueKind::Tensor, ValueKind::Tensor) => format!("({a}).shape == ({b}).shape && ({a}).data.len() == ({b}).data.len()"),
                _ => "false".into(),
            };
            Ok(Expr::Raw(expression))
        }
        EmirOp::DenseValues(value) => {
            let code = render_expr(&operand(program, *value));
            Ok(match kind_at(kinds, *value) {
                ValueKind::F64 | ValueKind::I64 => Expr::Raw(format!("vec![({code}) as f64]")),
                ValueKind::Vector(element) if *element == ValueKind::F64 => owned_operand(program, *value, kinds),
                ValueKind::Matrix(element) if *element == ValueKind::F64 => Expr::Raw(format!("({code}).as_slice().to_vec()")),
                ValueKind::Tensor => Expr::Raw(format!("({code}).data.clone()")),
                _ => return Err(BackendError::UnsupportedType("dense-values requires a numeric carrier".into())),
            })
        }
        EmirOp::DenseRepack { template, data } => {
            if kind_at(kinds, *data) != ValueKind::Vector(Box::new(ValueKind::F64)) {
                return Err(BackendError::UnsupportedType("dense-repack requires Float64 data".into()));
            }
            let template_code = render_expr(&operand(program, *template));
            let data_code = render_expr(&owned_operand(program, *data, kinds));
            let ValueKind::DenseLayout(kind) = kind_at(kinds, *template) else {
                return Err(BackendError::UnsupportedType("dense-repack requires a layout".into()));
            };
            let (pattern, result) = match *kind {
                ValueKind::F64 | ValueKind::I64 => ("emath_rt::DenseLayout::Scalar", "data[0]"),
                ValueKind::Vector(element) if *element == ValueKind::F64 => ("emath_rt::DenseLayout::Vector(_)", "data"),
                ValueKind::Matrix(element) if *element == ValueKind::F64 => ("emath_rt::DenseLayout::Matrix { rows, cols, .. }", "emath_rt::Matrix::new(*rows, *cols, data).map_err(str::to_owned)?"),
                ValueKind::Tensor => ("emath_rt::DenseLayout::Tensor { shape, .. }", "emath_rt::Tensor { shape: shape.clone(), data }"),
                _ => return Err(BackendError::UnsupportedType("dense-repack layout has a nonnumeric carrier".into())),
            };
            Ok(map_runtime_result(format!("(|| -> Result<_, String> {{ let template = &{template_code}; let data = {data_code}; if data.len() != template.len() {{ return Err(String::from(\"numeric storage does not match template\")); }} match template {{ {pattern} => Ok({result}), _ => Err(String::from(\"numeric layout has the wrong carrier\")) }} }})()")))
        }
        EmirOp::IndexText(value) => Ok(Expr::Raw(format!("({} as usize).to_string()", render_expr(&typed_operand(program, *value, ValueKind::F64, kinds))))),
        EmirOp::F64Exp2(value) => Ok(Expr::Raw(format!("({}).exp2()", render_expr(&typed_operand(program, *value, ValueKind::F64, kinds))))),
        EmirOp::F64PowI(base, exponent) => {
            if kind_at(kinds, *exponent) != ValueKind::I64 { return Err(BackendError::UnsupportedType("powi exponent must be Int".into())); }
            Ok(Expr::Raw(format!("({}).powi(i32::try_from({}).map_err(|_| String::from(\"E-SCALAR-CONVERT: exponent exceeds i32\"))?)", render_expr(&typed_operand(program, *base, ValueKind::F64, kinds)), render_expr(&operand(program, *exponent)))))
        }
        EmirOp::TextTrim(value) | EmirOp::TextLength(value) | EmirOp::ParseF64(value) => {
            if kind_at(kinds, *value) != ValueKind::Text { return Err(BackendError::UnsupportedType("text operation requires Text".into())); }
            let text = render_expr(&operand(program, *value));
            Ok(Expr::Raw(match op {
                EmirOp::TextTrim(_) => format!("({text}).trim().to_owned()"),
                EmirOp::TextLength(_) => format!("i64::try_from(({text}).len()).map_err(|_| String::from(\"text length exceeds Int\"))?"),
                _ => format!("({text}).parse::<f64>().map_err(|_| String::from(\"E-SCALAR-CONVERT: invalid Float64 text\"))?"),
            }))
        }
        EmirOp::TextByte(text, index) => {
            if kind_at(kinds, *text) != ValueKind::Text { return Err(BackendError::UnsupportedType("text_byte requires Text".into())); }
            let text = render_expr(&operand(program, *text));
            let index = render_expr(&checked_integer_operand(program, *index, kinds)?);
            Ok(Expr::Raw(format!("{{ let __byte_index = usize::try_from({index}).map_err(|_| String::from(\"text byte index must be nonnegative\"))?; i64::from(*({text}).as_bytes().get(__byte_index).ok_or_else(|| String::from(\"text byte index out of bounds\"))?) }}")))
        }
        EmirOp::FormatScientific(value, precision) => {
            if kind_at(kinds, *precision) != ValueKind::I64 { return Err(BackendError::UnsupportedType("format precision must be Int".into())); }
            Ok(map_runtime_result(format!("emath_rt::format_scientific({}, {})", render_expr(&typed_operand(program, *value, ValueKind::F64, kinds)), render_expr(&operand(program, *precision)))))
        }
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => Ok(Expr::IfElse {
            condition: Box::new(operand(program, *condition)),
            then: Box::new(Stmt::Expr(operand(program, *then_value))),
            else_value: Box::new(Stmt::Expr(operand(program, *else_value))),
        }),
        EmirOp::VectorLength(value) => {
            let value_code = render_expr(&operand(program, *value));
            let storage = match kind_at(kinds, *value) {
                ValueKind::Matrix(_) => format!("({value_code}).as_slice()"),
                ValueKind::Tensor => format!("({value_code}).data"),
                _ => value_code,
            };
            Ok(Expr::Raw(format!("({storage}.len() as i64)")))
        }
        EmirOp::VectorCreate(elements) => Ok(Expr::Macro {
            name: "vec".to_string(),
            args: elements
                .iter()
                .map(|value| typed_operand(program, *value, if elements.iter().any(|value| kind_at(kinds, *value) == ValueKind::Rational) { ValueKind::Rational } else { ValueKind::F64 }, kinds))
                .collect(),
        }),
        EmirOp::MatrixCreate { rows, cols, elements } => {
            let values = elements.iter().map(|value| render_expr(&typed_operand(program, *value, ValueKind::F64, kinds))).collect::<Vec<_>>().join(", ");
            Ok(map_runtime_result(format!("emath_rt::Matrix::new({rows}, {cols}, vec![{values}])")))
        }
        EmirOp::MatrixRows(value) | EmirOp::MatrixCols(value) => {
            let matrix = render_expr(&operand(program, *value));
            let dimension = if matches!(op, EmirOp::MatrixRows(_)) { "rows" } else { "cols" };
            Ok(map_runtime_result(format!(r#"i64::try_from(({matrix}).{dimension}()).map_err(|_| "matrix dimension exceeds Int")"#)))
        }
        EmirOp::MatrixPack { rows, cols, data } => {
            let rows = render_expr(&operand(program, *rows));
            let cols = render_expr(&operand(program, *cols));
            let data = render_expr(&owned_operand(program, *data, kinds));
            Ok(map_runtime_result(format!(r#"(|| -> Result<emath_rt::Matrix, String> {{
                let fault = || "E-MATRIX-SHAPE: invalid dimensions or data length".to_string();
                let rows = usize::try_from({rows}).map_err(|_| fault())?;
                let cols = usize::try_from({cols}).map_err(|_| fault())?;
                emath_rt::Matrix::new(rows, cols, {data}).map_err(str::to_string)
            }})()"#)))
        }
        EmirOp::TensorShape(value) => {
            let value = render_expr(&operand(program, *value));
            Ok(map_runtime_result(format!(r#"{{ let tensor = &({value}); if tensor.shape.iter().try_fold(1usize, |size, axis| size.checked_mul(*axis)) != Some(tensor.data.len()) {{ Err("tensor storage does not match shape") }} else {{ tensor.shape.iter().map(|axis| i64::try_from(*axis).map_err(|_| "tensor dimension exceeds Int")).collect::<Result<Vec<i64>, _>>() }} }}"#)))
        }
        EmirOp::TensorPack { shape, data } => {
            if kind_at(kinds, *shape) != ValueKind::Vector(Box::new(ValueKind::I64)) || kind_at(kinds, *data) != ValueKind::Vector(Box::new(ValueKind::F64)) {
                return Err(BackendError::UnsupportedType("tensor packing requires Int dimensions and Float64 data".into()));
            }
            let shape = render_expr(&operand(program, *shape));
            let borrowed = render_expr(&operand(program, *data));
            let data = render_expr(&owned_operand(program, *data, kinds));
            Ok(map_runtime_result(format!(r#"(|| -> Result<emath_rt::Tensor, String> {{
                let fault = || "E-TENSOR-SHAPE: invalid dimensions or data length".to_string();
                let shape = ({shape}).iter().map(|axis| usize::try_from(*axis).map_err(|_| fault())).collect::<Result<Vec<_>, _>>()?;
                if shape.iter().try_fold(1usize, |size, axis| size.checked_mul(*axis)) != Some(({borrowed}).len()) {{ return Err(fault()); }}
                Ok(emath_rt::Tensor {{ shape, data: {data} }})
            }})()"#)))
        }
        EmirOp::DenseIndex { dense, index } => {
            let value = render_expr(&operand(program, *dense));
            let (storage, validation) = match kind_at(kinds, *dense) {
                ValueKind::Vector(element) if *element == ValueKind::F64 => (format!("({value}).as_slice()"), String::new()),
                ValueKind::Matrix(element) if *element == ValueKind::F64 => (format!("({value}).as_slice()"), String::new()),
                ValueKind::Tensor => (format!("({value}).data.as_slice()"), format!(r#"if ({value}).shape.iter().try_fold(1usize, |size, axis| size.checked_mul(*axis)) != Some(({value}).data.len()) {{ return Err("tensor storage does not match shape"); }}"#)),
                _ => return Err(BackendError::UnsupportedType("dense indexing requires a Float64 dense carrier".into())),
            };
            let index = render_expr(&operand(program, *index));
            Ok(map_runtime_result(format!(r#"(|| -> Result<f64, &str> {{ {validation} usize::try_from({index}).ok().and_then(|index| {storage}.get(index)).copied().ok_or("dense index out of bounds") }})()"#)))
        }
        EmirOp::TensorCreate { shape, elements } => {
            let shape = shape
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let data = elements
                .iter()
                .map(|value| render_expr(&typed_operand(program, *value, ValueKind::F64, kinds)))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(Expr::Raw(format!(
                "emath_rt::Tensor {{ shape: vec![{shape}], data: vec![{data}] }}"
            )))
        }
        EmirOp::VectorIndex { vector, index } => {
            let collection = render_expr(&operand(program, *vector));
            let materialize = match kind_at(kinds, *vector) {
                ValueKind::Vector(element) if *element == ValueKind::ExactInt => ".cloned()",
                ValueKind::Vector(element) if !element.is_copy() => "",
                ValueKind::Matrix(_) => "",
                _ => ".cloned()",
            };
            if kind_at(kinds, *index) == ValueKind::I64 {
                let index = render_expr(&operand(program, *index));
                Ok(map_runtime_result(format!("usize::try_from({index}).ok().and_then(|index| ({collection}).get(index)){materialize}.ok_or(\"vector index out of bounds\")")))
            } else {
                let index = index_f64(program, *index, kinds);
                Ok(map_runtime_result(format!("{{ let index = {index}; if index.is_finite() && index >= 0.0 && index.fract() == 0.0 {{ ({collection}).get(index as usize){materialize}.ok_or(\"vector index out of bounds\") }} else {{ Err(\"vector index must be a nonnegative integer\") }} }}")))
            }
        }
        EmirOp::MatrixIndex { matrix, row, col } => {
            let checked = |value| {
                let index = render_expr(&operand(program, value));
                if kind_at(kinds, value) == ValueKind::I64 { format!("usize::try_from({index}).ok()") }
                else { format!("{{ let index = {index}; if index.is_finite() && index >= 0.0 && index.fract() == 0.0 {{ Some(index as usize) }} else {{ None }} }}") }
            };
            let matrix = render_expr(&operand(program, *matrix));
            let row = checked(*row);
            let col = checked(*col);
            Ok(map_runtime_result(format!(r#"({row}).zip({col}).and_then(|(row, col)| ({matrix}).get(row, col)).copied().ok_or("matrix index out of bounds")"#)))
        }
        EmirOp::TensorIndex { tensor, indices } => {
            Ok(tensor_index_call(program, *tensor, indices, kinds))
        }
        EmirOp::TensorSlice { tensor, axes } => {
            Ok(tensor_slice_call(program, *tensor, axes, kinds))
        }
        _ => unreachable!("op_collection_exprs routed a non-universal collection op"),
    }
}

/// Preserve coordinate rank and integer admission at the dense storage boundary.
pub(super) fn dense_point_index_expr(args: &[EmirValue], program: &EmirProgram, kinds: &[ValueKind]) -> Option<Expr> {
    let [dense, coordinates] = args else { return None; };
    let (shape, data) = match kind_at(kinds, *dense) {
        ValueKind::Vector(element) if *element == ValueKind::F64 => ("&[__dense.len()][..]", "__dense.as_slice()"),
        ValueKind::Matrix(element) if *element == ValueKind::F64 => ("&[__dense.rows(), __dense.cols()][..]", "__dense.as_slice()"),
        ValueKind::Tensor => ("__dense.shape.as_slice()", "__dense.data.as_slice()"),
        _ => return None,
    };
    let conversion = match kind_at(kinds, *coordinates) {
        ValueKind::Vector(element) if *element == ValueKind::I64 => "*coordinate",
        ValueKind::Vector(element) if *element == ValueKind::F64 => r#"{
            if !coordinate.is_finite() || coordinate.fract() != 0.0 || *coordinate < 0.0 || *coordinate > i64::MAX as f64 {
                return Err(format!("E-SHAPE-006: index axis {axis} must be a whole non-negative integer"));
            }
            *coordinate as i64
        }"#,
        _ => return None,
    };
    let dense = render_expr(&operand(program, *dense));
    let coordinates = render_expr(&operand(program, *coordinates));
    Some(map_runtime_result(format!(r#"(|| -> Result<f64, String> {{
        let __dense = &{dense};
        let shape: &[usize] = {shape};
        let data: &[f64] = {data};
        let coordinates = &{coordinates};
        let expected = shape.iter().try_fold(1usize, |size, axis| size.checked_mul(*axis))
            .ok_or_else(|| String::from("E-LINALG-004: dense carrier extent overflow"))?;
        if expected != data.len() {{ return Err(String::from("E-LINALG-004: dense carrier data length does not match its shape")); }}
        if coordinates.len() != shape.len() {{
            return Err(format!("E-SHAPE-006: index requires {{}} subscript(s), found {{}}", shape.len(), coordinates.len()));
        }}
        let mut offset = 0usize;
        for (axis, (extent, coordinate)) in shape.iter().zip(coordinates.iter()).enumerate() {{
            let index: i64 = {conversion};
            if index < 0 || index as usize >= *extent {{
                return Err(format!("E-SHAPE-006: index {{index}} is outside axis {{axis}} of length {{extent}}"));
            }}
            offset = offset.checked_mul(*extent).and_then(|value| value.checked_add(index as usize))
                .ok_or_else(|| String::from("E-SHAPE-006: dense index offset overflow"))?;
        }}
        data.get(offset).copied().ok_or_else(|| format!("E-SHAPE-006: index {{offset}} is outside the dense carrier of length {{}}", data.len()))
    }})()"#)))
}

fn layout_match_code(layout: &str, value: &str, kind: &ValueKind) -> String {
    match kind {
        ValueKind::F64 | ValueKind::I64 => format!("matches!(&({layout}), emath_rt::DenseLayout::Scalar)"),
        ValueKind::Vector(element) if **element == ValueKind::F64 => format!("matches!(&({layout}), emath_rt::DenseLayout::Vector(len) if *len == ({value}).len())"),
        ValueKind::Matrix(element) if **element == ValueKind::F64 => format!("matches!(&({layout}), emath_rt::DenseLayout::Matrix {{ rows, cols, len }} if *rows == ({value}).rows() && *cols == ({value}).cols() && *len == ({value}).as_slice().len())"),
        ValueKind::Tensor => format!("matches!(&({layout}), emath_rt::DenseLayout::Tensor {{ shape, len }} if shape == &({value}).shape && *len == ({value}).data.len())"),
        _ => "false".into(),
    }
}
