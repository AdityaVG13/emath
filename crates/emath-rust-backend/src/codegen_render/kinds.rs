//! Scalar-kind inference and typed operand/expression helpers.

use super::*;

/// Scalar carrier of an EMIR register. Exact carriers never widen implicitly.
pub(crate) type InputKinds = BTreeMap<String, ValueKind>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ValueKind {
    I64,
    F64,
    Rational,
    Bool,
    /// Stage-2 (emath-t63iz): exact big field element (emath_rt::UBig).
    BigInt,
    Text,
    Program,
    /// Complex scalar `(f64, f64)` — the VM's complex carrier.
    Complex,
    DenseLayout(Box<ValueKind>),
    Result(Box<ValueKind>, Box<ValueKind>),
    Vector(Box<ValueKind>),
    Matrix(Box<ValueKind>),
    Tensor,
    Record(String),
    Never,
    Other,
}

impl ValueKind {
    pub(crate) fn from_signature(text: &str) -> Self {
        fn parse(text: &str, remaining: usize) -> ValueKind {
            if remaining == 0 {
                return ValueKind::Other;
            }
            match text.trim() {
                "Int" | "I64" | "Nat" => ValueKind::I64,
                "Float64" => ValueKind::F64,
                "Rat" => ValueKind::Rational,
                "Bool" => ValueKind::Bool,
                "Text" => ValueKind::Text,
                "Program" => ValueKind::Program,
                "BigInt" => ValueKind::BigInt,
                "Complex" | "Complex<Float64>" => ValueKind::Complex,
                "emath_rt::Tensor" => ValueKind::Tensor,
                text if text.starts_with("Tensor<") || text.starts_with("SameTensor<") => {
                    ValueKind::Tensor
                }
                text => {
                    if let Some(inner) = text
                        .strip_prefix("Vector<")
                        .or_else(|| text.strip_prefix("SameVector<"))
                        .and_then(|text| text.strip_suffix('>'))
                    {
                        ValueKind::Vector(Box::new(parse(inner, remaining - 1)))
                    } else if let Some(inner) = text
                        .strip_prefix("Matrix<")
                        .or_else(|| text.strip_prefix("SameMatrix<"))
                        .and_then(|text| text.strip_suffix('>'))
                    {
                        ValueKind::Matrix(Box::new(parse(inner, remaining - 1)))
                    } else if let Some(name) = text
                        .strip_prefix("Record<")
                        .and_then(|text| text.strip_suffix('>'))
                    {
                        ValueKind::Record(name.to_string())
                    } else {
                        ValueKind::Other
                    }
                }
            }
        }
        parse(text, 32)
    }

    pub(crate) fn rust_ty(&self) -> Result<Ty, BackendError> {
        use crate::rust_ir::render::render_ty;
        Ok(match self {
            Self::I64 => Ty::I64,
            Self::F64 => Ty::F64,
            Self::Complex => Ty::Named("(f64, f64)".into()),
            Self::Rational => Ty::Named("emath_rt::ExactRatio".into()),
            Self::Bool => Ty::Bool,
            Self::Text => Ty::Named("String".into()),
            Self::DenseLayout(_) => Ty::Named("emath_rt::DenseLayout".into()),
            Self::Program => Ty::Named(
                "std::sync::Arc<dyn Fn(&[f64]) -> Result<emath_rt::NumericProgramResult, String>>"
                    .into(),
            ),
            Self::Result(ok, error) => Ty::Named(format!(
                "Result<{}, {}>",
                render_ty(&ok.rust_ty()?),
                render_ty(&error.rust_ty()?)
            )),
            Self::BigInt => Ty::Named("emath_rt::UBig".into()),
            Self::Vector(element) => Ty::Named(format!("Vec<{}>", render_ty(&element.rust_ty()?))),
            Self::Matrix(element) => Ty::Named(format!(
                "emath_rt::Matrix<{}>",
                render_ty(&element.rust_ty()?)
            )),
            Self::Tensor => Ty::Named("emath_rt::Tensor".into()),
            Self::Record(name) => Ty::Named(format!("EmathRecord_{}", escape_ident(name))),
            Self::Never => Ty::Named("!".into()),
            Self::Other => {
                return Err(BackendError::UnsupportedType(
                    "unknown value carrier".into(),
                ));
            }
        })
    }

    pub(super) fn borrowed_rust_ty(&self) -> Result<Ty, BackendError> {
        if *self == Self::Text {
            Ok(Ty::Named("str".into()))
        } else {
            self.rust_ty()
        }
    }

    pub(super) fn is_copy(&self) -> bool {
        matches!(
            self,
            Self::I64 | Self::F64 | Self::Rational | Self::Bool | Self::Complex
        )
    }
}

pub(super) fn checked_integer_operand(
    program: &EmirProgram,
    value: EmirValue,
    kinds: &[ValueKind],
) -> Result<Expr, BackendError> {
    match kind_at(kinds, value) {
        ValueKind::I64 => Ok(operand(program, value)),
        ValueKind::F64 => Ok(Expr::Raw(format!(
            "{{ let __convert = {}; if !(__convert >= i64::MIN as f64 && __convert < -(i64::MIN as f64) && __convert.fract() == 0.0) {{ return Err(String::from(\"E-SCALAR-CONVERT: value must be an exact integer in i64 range\")); }} __convert as i64 }}",
            render_expr(&operand(program, value))
        ))),
        _ => Err(BackendError::UnsupportedType(
            "integer conversion requires Int or Float64".into(),
        )),
    }
}

pub(crate) fn program_kind(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
) -> ValueKind {
    if program.ops.is_empty() {
        return ValueKind::Other;
    }
    let kinds = value_kinds(program, names, states, input_kinds);
    kinds
        .get(program.result.0 as usize)
        .cloned()
        .unwrap_or(ValueKind::Other)
}

pub(super) fn value_kinds(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
) -> Vec<ValueKind> {
    let n = program.ops.len();
    let mut kinds = vec![ValueKind::Other; n];
    for (i, (op, _)) in program.ops.iter().enumerate() {
        kinds[i] = kind_of_op(op, &kinds, names, states, input_kinds);
    }
    kinds
}

pub(super) fn kind_at(kinds: &[ValueKind], value: EmirValue) -> ValueKind {
    kinds
        .get(value.0 as usize)
        .cloned()
        .unwrap_or(ValueKind::Other)
}

pub(super) fn kind_of_op(
    op: &EmirOp,
    kinds: &[ValueKind],
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
) -> ValueKind {
    match op {
        EmirOp::ConstI64(_)
        | EmirOp::VectorLength(_)
        | EmirOp::ToInt(_)
        | EmirOp::IntegerQuotient(_, _)
        | EmirOp::MatrixRows(_)
        | EmirOp::MatrixCols(_) => ValueKind::I64,
        EmirOp::MatrixPack { .. } | EmirOp::MatrixCreate { .. } => {
            ValueKind::Matrix(Box::new(ValueKind::F64))
        }
        EmirOp::TensorPack { .. } | EmirOp::TensorCreate { .. } => ValueKind::Tensor,
        EmirOp::TensorShape(_) => ValueKind::Vector(Box::new(ValueKind::I64)),
        EmirOp::F64Exp2(_) | EmirOp::F64PowI(..) | EmirOp::ParseF64(_) => ValueKind::F64,
        EmirOp::TextTrim(_) | EmirOp::FormatScientific(..) | EmirOp::IndexText(_) => {
            ValueKind::Text
        }
        EmirOp::TextLength(_) | EmirOp::TextByte(..) => ValueKind::I64,
        EmirOp::DenseIndex { .. } => ValueKind::F64,
        EmirOp::ConstBigInt(_) => ValueKind::BigInt,
        EmirOp::ConstF64(_) => ValueKind::F64,
        EmirOp::ConstBool(_) => ValueKind::Bool,
        EmirOp::ConstText(_) | EmirOp::FormatText { .. } => ValueKind::Text,
        EmirOp::RecordCreate { type_name, .. } => ValueKind::Record(type_name.clone()),
        EmirOp::RecordField { record, field } => {
            let ValueKind::Record(name) = kind_at(kinds, *record) else {
                return ValueKind::Other;
            };
            emath_exec_ir::native_kernel::installed_record_layout(&name)
                .and_then(|layout| layout.fields.into_iter().find(|(name, _)| name == field))
                .map(|(_, ty)| ValueKind::from_signature(&ty))
                .unwrap_or(ValueKind::Other)
        }
        EmirOp::Refuse(_) | EmirOp::RefuseValue(_) => ValueKind::Never,
        EmirOp::ProgramLiteral { .. } => ValueKind::Program,
        EmirOp::CallSelf { inputs } => inputs
            .first()
            .map(|value| kind_at(kinds, *value))
            .unwrap_or(ValueKind::I64),
        EmirOp::CallFrame {
            body,
            inputs,
            state,
        } => {
            let names = (0..inputs.len())
                .map(|index| format!("__frame_input_{index}"))
                .collect::<Vec<_>>();
            let states = (0..state.len())
                .map(|index| format!("__frame_state_{index}"))
                .collect::<Vec<_>>();
            let frame = names
                .iter()
                .zip(inputs)
                .chain(states.iter().zip(state))
                .map(|(name, value)| (name.clone(), kind_at(kinds, *value)))
                .collect();
            program_kind(body, &names, &states, &frame)
        }
        EmirOp::SameDenseShape(..) => ValueKind::Bool,
        EmirOp::ToF64(_) => ValueKind::F64,
        EmirOp::DenseLayout(value) => ValueKind::DenseLayout(Box::new(kind_at(kinds, *value))),
        EmirOp::VectorSlice { .. } | EmirOp::VectorConcat(_) | EmirOp::F64SortTotal(_) => {
            ValueKind::Vector(Box::new(ValueKind::F64))
        }
        EmirOp::DenseValues(_) => ValueKind::Vector(Box::new(ValueKind::F64)),
        EmirOp::DenseRepack { template, .. } => match kind_at(kinds, *template) {
            ValueKind::DenseLayout(kind) => {
                if *kind == ValueKind::I64 {
                    ValueKind::F64
                } else {
                    *kind
                }
            }
            _ => ValueKind::Other,
        },
        EmirOp::CallProgram { .. } | EmirOp::ToF64Vector(_) => {
            ValueKind::Vector(Box::new(ValueKind::F64))
        }
        EmirOp::CallScalarProgram { .. } | EmirOp::CallRealProgram { .. } => ValueKind::F64,
        EmirOp::TryCallRealProgram { .. } => {
            ValueKind::Result(Box::new(ValueKind::F64), Box::new(ValueKind::Text))
        }
        EmirOp::Iterate { init, .. } => kind_at(kinds, *init),
        EmirOp::Collect { args, body, .. } => {
            let names = (0..=args.len())
                .map(|index| format!("capture_{index}"))
                .collect::<Vec<_>>();
            let inputs = names
                .iter()
                .cloned()
                .zip(
                    std::iter::once(ValueKind::I64)
                        .chain(args.iter().map(|value| kind_at(kinds, *value))),
                )
                .collect();
            ValueKind::Vector(Box::new(program_kind(body, &names, &[], &inputs)))
        }
        EmirOp::Branch {
            args,
            then_body,
            else_body,
            ..
        } => {
            let names = (0..args.len())
                .map(|index| format!("capture_{index}"))
                .collect::<Vec<_>>();
            let inputs = names
                .iter()
                .cloned()
                .zip(args.iter().map(|value| kind_at(kinds, *value)))
                .collect();
            let left = program_kind(then_body, &names, &[], &inputs);
            let right = program_kind(else_body, &names, &[], &inputs);
            if left == ValueKind::Never {
                right
            } else if right == ValueKind::Never || left == right {
                left
            } else {
                ValueKind::Other
            }
        }
        EmirOp::ListCreate(values)
        | EmirOp::SetCreate {
            elements: values, ..
        } => ValueKind::Vector(Box::new(
            values
                .first()
                .map(|value| kind_at(kinds, *value))
                .unwrap_or(ValueKind::Other),
        )),
        EmirOp::VectorCreate(values) => ValueKind::Vector(Box::new(
            if values
                .iter()
                .any(|value| kind_at(kinds, *value) == ValueKind::Rational)
            {
                ValueKind::Rational
            } else {
                ValueKind::F64
            },
        )),
        EmirOp::VectorIndex { vector, .. } => match kind_at(kinds, *vector) {
            ValueKind::Vector(element) => *element,
            ValueKind::Matrix(element) => ValueKind::Vector(element),
            _ => ValueKind::F64,
        },
        EmirOp::ConstComplex(..) => ValueKind::Complex,
        EmirOp::SeriesCreate { .. }
        | EmirOp::TensorSlice { .. }
        | EmirOp::OptionSome(_)
        | EmirOp::OptionNone
        | EmirOp::ResultOk(_)
        | EmirOp::ResultErr(_)
        | EmirOp::ResultErrorOf(_)
        | EmirOp::VectorMap { .. }
        | EmirOp::VectorMapScalar { .. }
        | EmirOp::VectorReduce { .. } => ValueKind::Other,
        EmirOp::SeriesSample { .. } | EmirOp::F64Pow(..) => ValueKind::F64,
        EmirOp::UnaryBuiltin(_, value) => {
            // Transcendentals preserve the complex carrier when applied to
            // a complex operand (`sqrt(-1) = i`); the op_arith renderer
            // picks the complex builtin for those, others refuse typed.
            if kind_at(kinds, *value) == ValueKind::Complex {
                ValueKind::Complex
            } else {
                ValueKind::F64
            }
        }
        EmirOp::BinaryBuiltin(..)
        | EmirOp::MatrixIndex { .. }
        | EmirOp::TensorIndex { .. } => ValueKind::F64,
        EmirOp::ApplyCapability {
            capability, args, ..
        } => {
            if let Some(signature) = emath_exec_ir::native_kernel::installed_signature(capability) {
                return ValueKind::from_signature(&signature.output);
            }
            if let Ok(binding) = emath_exec_ir::native_kernel::verified_kernel_binding(capability) {
                return binding
                    .signature
                    .split_once(")->")
                    .map(|(_, output)| ValueKind::from_signature(output))
                    .unwrap_or(ValueKind::Other);
            }
            if let Some(cell) = emath_exec_ir::native_kernel::installed_reference_cell(capability) {
                let names: Vec<_> = cell.params.iter().map(|(name, _)| name.clone()).collect();
                let inputs = names
                    .iter()
                    .zip(args)
                    .map(|(name, arg)| (name.clone(), kind_at(kinds, *arg)))
                    .collect();
                return program_kind(&cell.program, &names, &[], &inputs);
            }
            ValueKind::Other
        }
        EmirOp::F64Div(left, right) => {
            if kind_at(kinds, *left) == ValueKind::Complex
                || kind_at(kinds, *right) == ValueKind::Complex
            {
                ValueKind::Complex
            } else if (kind_at(kinds, *left) == ValueKind::Rational
                && kind_at(kinds, *right) == ValueKind::Rational)
                || (kind_at(kinds, *left) == ValueKind::I64
                    && kind_at(kinds, *right) == ValueKind::I64)
            {
                ValueKind::Rational
            } else {
                ValueKind::F64
            }
        }
        EmirOp::LoadInput(index) => input_kind(names.get(*index as usize), input_kinds),
        EmirOp::LoadState(index) => input_kind(states.get(*index as usize), input_kinds),
        EmirOp::F64Add(left, right) | EmirOp::F64Sub(left, right) | EmirOp::F64Mul(left, right) => {
            if kind_at(kinds, *left) == ValueKind::I64 && kind_at(kinds, *right) == ValueKind::I64 {
                ValueKind::I64
            } else if kind_at(kinds, *left) == ValueKind::Rational
                && kind_at(kinds, *right) == ValueKind::Rational
            {
                ValueKind::Rational
            } else if kind_at(kinds, *left) == ValueKind::Complex
                || kind_at(kinds, *right) == ValueKind::Complex
            {
                ValueKind::Complex
            } else {
                ValueKind::F64
            }
        }
        EmirOp::Neg(value) => kind_at(kinds, *value),
        EmirOp::IsFinite(_)
        | EmirOp::SameBits(_, _)
        | EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..)
        | EmirOp::SetContains { .. }
        | EmirOp::Not(_)
        | EmirOp::OptionIsSome(_)
        | EmirOp::ResultIsOk(_)
        | EmirOp::VectorAllFinite(_) => ValueKind::Bool,
        EmirOp::Select {
            then_value,
            else_value,
            ..
        } => {
            let then_kind = kind_at(kinds, *then_value);
            let else_kind = kind_at(kinds, *else_value);
            if then_kind == else_kind {
                then_kind
            } else {
                ValueKind::F64
            }
        }
        EmirOp::Fold {
            combine,
            init,
            loop_var_index,
            body,
            ..
        } => match combine {
            FoldCombine::And | FoldCombine::Or => ValueKind::Bool,
            FoldCombine::Add | FoldCombine::Mul => {
                if fold_is_i64(
                    kinds,
                    *init,
                    *loop_var_index,
                    body,
                    names,
                    states,
                    input_kinds,
                ) {
                    ValueKind::I64
                } else {
                    ValueKind::F64
                }
            }
        },
        EmirOp::OptionUnwrapOr(_, default) | EmirOp::ResultUnwrapOr(_, default) => {
            kind_at(kinds, *default)
        }
    }
}

pub(super) fn input_kind(name: Option<&String>, input_kinds: &InputKinds) -> ValueKind {
    name.and_then(|name| input_kinds.get(name))
        .cloned()
        .unwrap_or(ValueKind::F64)
}

pub(super) fn fold_is_i64(
    kinds: &[ValueKind],
    init: EmirValue,
    loop_var_index: u16,
    body: &EmirProgram,
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
) -> bool {
    if kind_at(kinds, init) != ValueKind::I64 {
        return false;
    }
    let mut body_names = names.to_vec();
    let lv = loop_var_index as usize;
    while body_names.len() <= lv {
        body_names.push(String::new());
    }
    // Name the loop variable by its slot index: nested binders each own a
    // distinct index, so per-index names cannot shadow an outer binder's
    // variable (a shared name made inner bodies read the inner loop var).
    let loop_name = format!("__loop{lv}");
    body_names[lv] = loop_name.clone();
    let mut body_kinds = input_kinds.clone();
    body_kinds.insert(loop_name, ValueKind::I64);
    program_kind(body, &body_names, states, &body_kinds) == ValueKind::I64
}

pub(super) fn as_f64(expr: Expr) -> Expr {
    Expr::Raw(format!("({}) as f64", render_expr(&expr)))
}

pub(super) fn as_i64(expr: Expr) -> Expr {
    Expr::Raw(format!("({}) as i64", render_expr(&expr)))
}

pub(super) fn operand_kind<'a>(kinds: &'a [ValueKind], value: EmirValue) -> ValueKind {
    kind_at(kinds, value)
}

pub(super) fn typed_operand(
    program: &EmirProgram,
    value: EmirValue,
    want: ValueKind,
    kinds: &[ValueKind],
) -> Expr {
    let expr = operand(program, value);
    match (operand_kind(kinds, value), want) {
        (ValueKind::I64, ValueKind::F64) => as_f64(expr),
        (ValueKind::F64, ValueKind::I64) => as_i64(expr),
        _ => expr,
    }
}

pub(super) fn i64_checked_bin(method: &str, left: Expr, right: Expr) -> Expr {
    checked_integer_result(Expr::MethodCall {
        receiver: Box::new(left),
        method: method.to_string(),
        args: vec![right],
    })
}

pub(super) fn cmp_expr(
    op: BinOp,
    program: &EmirProgram,
    left: EmirValue,
    right: EmirValue,
    kinds: &[ValueKind],
) -> Expr {
    let lk = operand_kind(kinds, left);
    let rk = operand_kind(kinds, right);
    match (&lk, &rk) {
        (ValueKind::I64, ValueKind::I64) | (ValueKind::Bool, ValueKind::Bool) => Expr::Bin {
            op,
            left: Box::new(operand(program, left)),
            right: Box::new(operand(program, right)),
        },
        (ValueKind::I64, ValueKind::F64) => mixed_i64_f64_cmp(
            op,
            operand(program, left),
            typed_operand(program, right, ValueKind::F64, kinds),
            true,
        ),
        (ValueKind::F64, ValueKind::I64) => mixed_i64_f64_cmp(
            op,
            operand(program, right),
            typed_operand(program, left, ValueKind::F64, kinds),
            false,
        ),
        (ValueKind::I64, _) => Expr::Bin {
            op,
            left: Box::new(as_f64(operand(program, left))),
            right: Box::new(typed_operand(program, right, ValueKind::F64, kinds)),
        },
        (_, ValueKind::I64) => Expr::Bin {
            op,
            left: Box::new(typed_operand(program, left, ValueKind::F64, kinds)),
            right: Box::new(as_f64(operand(program, right))),
        },
        _ => {
            let left = operand(program, left);
            let right = operand(program, right);
            if matches!(
                &lk,
                ValueKind::I64 | ValueKind::Bool | ValueKind::F64 | ValueKind::Complex | ValueKind::Rational
            ) && matches!(
                &rk,
                ValueKind::I64 | ValueKind::Bool | ValueKind::F64 | ValueKind::Complex | ValueKind::Rational
            ) {
                Expr::Bin {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                }
            } else {
                // Non-copy carriers compare through a common view: the
                // observed binding renders as a borrow (`&Vec<f64>`)
                // and the expected literal as an owned value
                // (`Vec<f64>`), and `&T == T` has no `PartialEq` impl.
                // Both sides project to the same slice/str view, which
                // compares elementwise exactly like the interp's carrier
                // comparison.
                let view = |kind: &ValueKind, expr: Expr| {
                    let rendered = render_expr(&expr);
                    match kind {
                        ValueKind::Text => format!("({rendered}).as_str()"),
                        ValueKind::Tensor => format!("({rendered}).data.as_slice()"),
                        ValueKind::Vector(_) | ValueKind::Matrix(_) => {
                            format!("({rendered}).as_slice()")
                        }
                        _ => format!("&({rendered})"),
                    }
                };
                Expr::Bin {
                    op,
                    left: Box::new(Expr::Raw(view(&lk, left))),
                    right: Box::new(Expr::Raw(view(&rk, right))),
                }
            }
        }
    }
}

/// Mixed Int/Float64 compare must not widen through `as f64` (2^53 lie).
pub(super) fn mixed_i64_f64_cmp(
    op: BinOp,
    int_expr: Expr,
    float_expr: Expr,
    int_on_left: bool,
) -> Expr {
    let eq = rt_call("eq_i64_f64", vec![int_expr.clone(), float_expr.clone()]);
    let cmp = render_expr(&rt_call("cmp_i64_f64", vec![int_expr, float_expr]));
    let (lt, gt) = if int_on_left {
        ("Less", "Greater")
    } else {
        ("Greater", "Less")
    };
    match op {
        BinOp::Eq => eq,
        BinOp::Ne => Expr::Un {
            op: UnOp::Not,
            value: Box::new(eq),
        },
        BinOp::Lt => Expr::Raw(format!("{cmp} == Some(core::cmp::Ordering::{lt})")),
        BinOp::Gt => Expr::Raw(format!("{cmp} == Some(core::cmp::Ordering::{gt})")),
        BinOp::Le => Expr::Raw(format!(
            "matches!({cmp}, Some(core::cmp::Ordering::{lt} | core::cmp::Ordering::Equal))"
        )),
        BinOp::Ge => Expr::Raw(format!(
            "matches!({cmp}, Some(core::cmp::Ordering::{gt} | core::cmp::Ordering::Equal))"
        )),
        BinOp::Add
        | BinOp::Sub
        | BinOp::Mul
        | BinOp::Div
        | BinOp::Rem
        | BinOp::Pow
        | BinOp::And
        | BinOp::Or => unreachable!("cmp_expr only emits comparison BinOps"),
    }
}

pub(super) fn i64_or_f64_bin(
    f64_op: BinOp,
    i64_method: &str,
    program: &EmirProgram,
    left: EmirValue,
    right: EmirValue,
    kinds: &[ValueKind],
) -> Expr {
    if operand_kind(kinds, left) == ValueKind::I64 && operand_kind(kinds, right) == ValueKind::I64 {
        i64_checked_bin(i64_method, operand(program, left), operand(program, right))
    } else {
        Expr::Bin {
            op: f64_op,
            left: Box::new(typed_operand(program, left, ValueKind::F64, kinds)),
            right: Box::new(typed_operand(program, right, ValueKind::F64, kinds)),
        }
    }
}

pub(crate) fn coerce_to_ty(expr: Expr, from: ValueKind, to: &Ty) -> Expr {
    match (from, to) {
        (ValueKind::I64, Ty::F64) => as_f64(expr),
        (ValueKind::F64, Ty::I64) => as_i64(expr),
        _ => expr,
    }
}

/// Render the program as an expression. Multi-op programs become a block
/// `{ let __e0 = ...; ...; __eN }`; single-op programs inline directly.
pub(crate) fn value_expr(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
) -> Result<Expr, BackendError> {
    if program.ops.len() == 1 {
        let expression = op_expr(&program.ops[0].0, program, names, states, input_kinds)?;
        return owned_result(program, expression, names, states, input_kinds);
    }
    let flat = flat_ssa(program, names, states, input_kinds, None)?;
    let mut statements: Vec<Stmt> = Vec::with_capacity(flat.e_lets.len() + 1);
    for (pattern, src) in flat.e_lets {
        statements.push(Stmt::Let {
            pattern,
            value: Box::new(Expr::Raw(src)),
        });
    }
    statements.push(Stmt::Expr(owned_result(
        program,
        Expr::Raw(flat.e_tail),
        names,
        states,
        input_kinds,
    )?));
    Ok(Expr::Block(Box::new(Stmt::Block(Block { statements }))))
}

fn owned_result(
    program: &EmirProgram,
    expression: Expr,
    names: &[String],
    states: &[String],
    inputs: &InputKinds,
) -> Result<Expr, BackendError> {
    let kind = program_kind(program, names, states, inputs);
    let borrowed = program
        .ops
        .get(program.result.0 as usize)
        .is_some_and(|(op, _)| {
            matches!(
                op,
                EmirOp::LoadInput(_)
                    | EmirOp::LoadState(_)
                    | EmirOp::RecordField { .. }
                    | EmirOp::VectorIndex { .. }
                    | EmirOp::ConstText(_)
            )
        });
    if borrowed && !kind.is_copy() {
        Ok(owned_value(expression, &kind))
    } else {
        Ok(expression)
    }
}
