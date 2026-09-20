//! Scalar-kind inference and typed operand/expression helpers.

use super::*;

/// Scalar carrier of an EMIR register. Exact carriers never widen implicitly.
pub(crate) type InputKinds = BTreeMap<String, ValueKind>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ValueKind {
    I64,
    /// Signed constructor integer that may exceed i64.
    ExactInt,
    F64,
    Rational,
    Bool,
    /// Stage-2 (emath-t63iz): exact big field element (emath_rt::UBig).
    BigInt,
    Text,
    Program,
    /// The artifact Code carrier (emath-npky7): a quoted unary
    /// `Rat -> Rat` program compiled once into a closure factory
    /// (`emath_rt::code::Code`). Open until `substitute` closes it;
    /// `evaluate` yields the specialized closure.
    Code,
    /// Typed program value: a closure with explicit parameter and
    /// result kinds (the constructor lane's `Int -> CaseSet -> Rat`
    /// carriers and `CallValue` callees). It renders as a generic call
    /// target - there is no standalone concrete type, so type-annotated
    /// bindings refuse and callers borrow it untyped.
    Closure {
        params: Vec<ValueKind>,
        result: Box<ValueKind>,
    },
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
                "ExactInt" => ValueKind::ExactInt,
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
                    } else if text.starts_with("Fn<") && text.ends_with('>') {
                        // Typed closure signature: every part but the
                        // last is a parameter; the last is the result.
                        let parts = split_signature_parts(&text[3..text.len() - 1]);
                        let Some((result, params)) = parts.split_last() else {
                            return ValueKind::Other;
                        };
                        ValueKind::Closure {
                            params: params.iter().map(|part| parse(part, remaining - 1)).collect(),
                            result: Box::new(parse(result, remaining - 1)),
                        }
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
            Self::ExactInt => Ty::Named("emath_rt::ExactInt".into()),
            Self::F64 => Ty::F64,
            Self::Complex => Ty::Named("(f64, f64)".into()),
            Self::Rational => Ty::Named("emath_rt::ExactRatio".into()),
            Self::Bool => Ty::Bool,
            Self::Text => Ty::Named("String".into()),
            Self::Code => Ty::Named("emath_rt::code::Code".into()),
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
            Self::Closure { params, result } => {
                // Shared `Rc<dyn Fn>` carrier: callable through the
                // handle (call expressions auto-deref), cloneable at
                // every crossing, and `'static` when captured by a
                // program literal. A `&dyn Fn` layer is never emitted
                // because `&Rc<..> -> &dyn Fn` does not coerce.
                Ty::Named(format!("std::rc::Rc<{}>", callable_ty(params, result)?))
            }
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

    pub(crate) fn is_copy(&self) -> bool {
        matches!(
            self,
            Self::I64 | Self::F64 | Self::Rational | Self::Bool | Self::Complex
        )
    }
}

/// A kind that carries no usable carrier information: `Other`, or a
/// vector whose element kind is `Other` (an empty `[]` literal).
pub(super) fn kind_is_degenerate(kind: &ValueKind) -> bool {
    matches!(kind, ValueKind::Other)
        || matches!(kind, ValueKind::Vector(element) if matches!(**element, ValueKind::Other))
}

/// Recover a frame input's kind from the callee's authored
/// declaration when the argument's own kind is degenerate (an empty
/// `[]` literal carries no element kind of its own). The declaration
/// wins only over degenerate kinds, never over a concrete inferred
/// one.
pub(super) fn frame_input_kind(kind: ValueKind, declared: Option<&String>) -> ValueKind {
    let Some(signature) = declared else { return kind };
    if kind_is_degenerate(&kind) {
        let declared_kind = ValueKind::from_signature(signature);
        if !matches!(declared_kind, ValueKind::Other) {
            return declared_kind;
        }
    }
    kind
}

/// Native callable carrier for a closure kind: `dyn Fn(P...) ->
/// Result<R, String>`. The carrier is shared `Rc<dyn Fn>` at every
/// position (parameters, results, captures): handles clone at
/// crossings, calls auto-deref through the handle, and a shared
/// handle is `'static` when a program literal captures it. A `&dyn`
/// layer is never emitted because `&Rc<..> -> &dyn Fn` does not
/// coerce.
pub(super) fn callable_ty(
    params: &[ValueKind],
    result: &ValueKind,
) -> Result<String, BackendError> {
    let param_tys = params
        .iter()
        .map(|param| match param {
            ValueKind::Closure { params, result } => {
                Ok(format!("std::rc::Rc<{}>", callable_ty(params, result)?))
            }
            other => {
                let ty = crate::rust_ir::render::render_ty(&other.rust_ty()?);
                if other.is_copy() {
                    Ok(ty)
                } else {
                    // Non-copy parameters (records, vectors) arrive
                    // borrowed, matching the call-site rendering.
                    Ok(format!("&{ty}"))
                }
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result_ty = match result {
        ValueKind::Closure { params, result } => {
            format!("std::rc::Rc<{}>", callable_ty(params, result)?)
        }
        other => crate::rust_ir::render::render_ty(&other.rust_ty()?),
    };
    Ok(format!(
        "dyn Fn({}) -> Result<{}, String>",
        param_tys.join(", "),
        result_ty
    ))
}

/// Split a `Fn<...>` signature body on top-level commas (nested
/// `<...>` stays intact), so `Fn<Int,Record<CS>,Rat>` yields the
/// parameter parts `["Int", "Record<CS>"]` and result `"Rat"`.
fn split_signature_parts(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, character) in text.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts
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

/// True when a body's result register is a direct self-call (the
/// tail-recursive shape; its kind is the enclosing function's own).
fn result_is_call_self(program: &EmirProgram) -> bool {
    program
        .ops
        .get(program.result.0 as usize)
        .is_some_and(|(op, _)| matches!(op, EmirOp::CallSelf { .. }))
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
        | EmirOp::MatrixRows(_)
        | EmirOp::MatrixCols(_) => ValueKind::I64,
        EmirOp::ConstExactInt(_) => ValueKind::ExactInt,
        EmirOp::ExactIntCall { name, .. } if name == "int_egcd" => {
            ValueKind::Vector(Box::new(ValueKind::ExactInt))
        }
        EmirOp::ExactIntCall { .. } => ValueKind::ExactInt,
        EmirOp::IntegerQuotient(left, right) => {
            if kind_at(kinds, *left) == ValueKind::ExactInt
                || kind_at(kinds, *right) == ValueKind::ExactInt
            {
                ValueKind::ExactInt
            } else {
                ValueKind::I64
            }
        }
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
            record_layout(&name)
                .and_then(|fields| fields.into_iter().find(|(name, _)| name == field))
                .map(|(_, ty)| ValueKind::from_signature(&ty))
                .unwrap_or(ValueKind::Other)
        }
        EmirOp::Refuse(_) | EmirOp::RefuseValue(_) => ValueKind::Never,
        // Typed closure literal: the parameter-domain signature plus
        // the body's inferred result compose the callable kind. An
        // empty signature is the numeric/VM-converted carrier
        // (Program), which renders through the dyn-program ABI.
        EmirOp::ProgramLiteral { body, captures, signature, .. } => {
            if signature.is_empty() {
                return ValueKind::Program;
            }
            let param_kind = ValueKind::from_signature(signature);
            let names = (0..usize::from(body.input_count))
                .map(|index| format!("__program_arg_{index}"))
                .collect::<Vec<_>>();
            let explicit = names.len() - captures.len();
            let frame = names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let kind = if index < explicit {
                        param_kind.clone()
                    } else {
                        kind_at(kinds, captures[index - explicit])
                    };
                    (name.clone(), kind)
                })
                .collect();
            let result = program_kind(body, &names, &[], &frame);
            ValueKind::Closure {
                params: vec![param_kind; explicit],
                result: Box::new(result),
            }
        }
        // The quoted-template carrier: open code (a CodeLiteral or a
        // further partial application) stays a Code value; only the
        // guarded executor yields a callable.
        EmirOp::CodeLiteral { .. } | EmirOp::CodeSubstitute { .. } => ValueKind::Code,
        // quote.evaluate: closed code yields the specialized unary
        // Rat -> Rat closure, so a def bound to it is callable.
        EmirOp::CodeEvaluate { .. } => ValueKind::Closure {
            params: vec![ValueKind::Rational],
            result: Box::new(ValueKind::Rational),
        },
        // A call consumes the callee's declared parameters one stage
        // at a time: a curried callee (`Fn<A, Fn<B, C>>` lowered as
        // one CallValue with all arguments) walks the result chain,
        // applying each stage's arguments and unwrapping with `?`.
        EmirOp::CallValue { program, inputs } => {
            let mut kind = kind_at(kinds, *program);
            let mut remaining = inputs.len();
            while remaining > 0 {
                let ValueKind::Closure { params, result } = kind else {
                    return ValueKind::Other;
                };
                if remaining < params.len() {
                    // Under-applied partial call: no carrier for the
                    // partially applied callable in this cut.
                    return ValueKind::Other;
                }
                remaining -= params.len();
                if remaining == 0 {
                    return *result;
                }
                kind = *result;
            }
            ValueKind::Other
        }
        // A self-recursive call's kind is the enclosing function's
        // authored output; the first-argument fallback only serves
        // the recur lane, whose result carrier is not declared here.
        EmirOp::CallSelf { inputs, result } => {
            if !result.is_empty() {
                let declared = ValueKind::from_signature(result);
                if !matches!(declared, ValueKind::Other) {
                    return declared;
                }
            }
            inputs
                .first()
                .map(|value| kind_at(kinds, *value))
                .unwrap_or(ValueKind::I64)
        }
        EmirOp::CallFrame {
            body,
            inputs,
            state,
            declared,
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
                .enumerate()
                .map(|(index, (name, value))| {
                    (
                        name.clone(),
                        frame_input_kind(kind_at(kinds, *value), declared.get(index)),
                    )
                })
                .chain(states.iter().zip(state).map(|(name, value)| {
                    (name.clone(), frame_input_kind(kind_at(kinds, *value), None))
                }))
                .collect();
            program_kind(body, &names, &states, &frame)
        }
        EmirOp::SameDenseShape(..) => ValueKind::Bool,
        EmirOp::ToF64(_) => ValueKind::F64,
        EmirOp::DenseLayout(value) => ValueKind::DenseLayout(Box::new(kind_at(kinds, *value))),
        EmirOp::VectorSlice { .. } | EmirOp::VectorConcat(_) | EmirOp::F64SortTotal(_) => {
            ValueKind::Vector(Box::new(ValueKind::F64))
        }
        // Authored cons preserves the element carrier (records stay
        // records), unlike the Float64 dense-lane concat above.
        EmirOp::ListConcat(values) => ValueKind::Vector(Box::new(
            values
                .first()
                .map(|value| kind_at(kinds, *value))
                .map(|kind| match kind {
                    ValueKind::Vector(element) => *element,
                    other => other,
                })
                .unwrap_or(ValueKind::Other),
        )),
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
            // A self-recursive tail (`CallSelf` as the body result)
            // returns the enclosing function's own kind - the sibling
            // arm already states it, so the join takes the other side
            // instead of mismatching into `Other`. A degenerate side
            // (an empty `[]` literal carries no element kind) yields
            // to the other side's concrete kind: `if done: [] else:
            // moves(k)` joins to the moves carrier.
            let left_degenerate = kind_is_degenerate(&left);
            let right_degenerate = kind_is_degenerate(&right);
            if left == ValueKind::Never
                || (result_is_call_self(then_body) && right != ValueKind::Other)
                || (left_degenerate && !right_degenerate && right != ValueKind::Other)
            {
                right
            } else if right == ValueKind::Never
                || (result_is_call_self(else_body) && left != ValueKind::Other)
                || (right_degenerate && !left_degenerate && left != ValueKind::Other)
            {
                left
            } else if left == right {
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
            } else if matches!(
                kind_at(kinds, *left),
                ValueKind::Rational | ValueKind::I64 | ValueKind::ExactInt
            ) && matches!(
                kind_at(kinds, *right),
                ValueKind::Rational | ValueKind::I64 | ValueKind::ExactInt
            ) {
                ValueKind::Rational
            } else {
                ValueKind::F64
            }
        }
        EmirOp::LoadInput(index) => input_kind(names.get(*index as usize), input_kinds),
        EmirOp::LoadState(index) => input_kind(states.get(*index as usize), input_kinds),
        EmirOp::F64Add(left, right) | EmirOp::F64Sub(left, right) | EmirOp::F64Mul(left, right) => {
            if (kind_at(kinds, *left) == ValueKind::ExactInt
                || kind_at(kinds, *right) == ValueKind::ExactInt)
                && matches!(
                    kind_at(kinds, *left),
                    ValueKind::I64 | ValueKind::ExactInt
                )
                && matches!(
                    kind_at(kinds, *right),
                    ValueKind::I64 | ValueKind::ExactInt
                )
            {
                ValueKind::ExactInt
            } else if kind_at(kinds, *left) == ValueKind::I64 && kind_at(kinds, *right) == ValueKind::I64 {
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
            } else if matches!(then_kind, ValueKind::I64 | ValueKind::ExactInt)
                && matches!(else_kind, ValueKind::I64 | ValueKind::ExactInt)
            {
                ValueKind::ExactInt
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
        (ValueKind::I64, ValueKind::I64)
        | (ValueKind::ExactInt, ValueKind::ExactInt)
        | (ValueKind::Bool, ValueKind::Bool) => Expr::Bin {
            op,
            left: Box::new(operand(program, left)),
            right: Box::new(operand(program, right)),
        },
        (ValueKind::ExactInt, ValueKind::I64) | (ValueKind::I64, ValueKind::ExactInt) => Expr::Bin {
            op,
            left: Box::new(exact_int_operand(program, left, kinds)),
            right: Box::new(exact_int_operand(program, right, kinds)),
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
    let flat = flat_ssa(program, names, states, input_kinds)?;
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
