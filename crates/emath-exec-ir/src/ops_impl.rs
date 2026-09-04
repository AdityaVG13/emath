//! Universal instruction metadata.

use super::*;

impl EmirOp {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ConstF64(_) => "const-f64",
            Self::ConstI64(_) => "const-i64",
            Self::ConstBigInt(_) => "const-bigint",
            Self::ConstText(_) => "const-text",
            Self::ConstComplex(_, _) => "const-complex",
            Self::ConstBool(_) => "const-bool",
            Self::LoadInput(_) => "load-input",
            Self::LoadState(_) => "load-state",
            Self::F64Add(_, _) => "scalar-add",
            Self::F64Sub(_, _) => "scalar-sub",
            Self::F64Mul(_, _) => "scalar-mul",
            Self::F64Div(_, _) => "scalar-div",
            Self::F64Pow(_, _) => "scalar-power",
            Self::Neg(_) => "scalar-negate",
            Self::UnaryBuiltin(_, _) => "scalar-unary-kernel",
            Self::BinaryBuiltin(_, _, _) => "scalar-binary-kernel",
            Self::Lt(_, _) => "compare-lt",
            Self::Le(_, _) => "compare-le",
            Self::Gt(_, _) => "compare-gt",
            Self::Ge(_, _) => "compare-ge",
            Self::Eq(_, _) => "compare-eq",
            Self::Ne(_, _) => "compare-ne",
            Self::And(_, _) => "boolean-and",
            Self::Or(_, _) => "boolean-or",
            Self::Imply(_, _) => "boolean-imply",
            Self::Iff(_, _) => "boolean-iff",
            Self::Not(_) => "boolean-not",
            Self::IsFinite(_) => "is-finite",
            Self::Select { .. } => "select",
            Self::FormatText { .. } => "format-text",
            Self::SeriesCreate { .. } => "series-create",
            Self::SeriesSample { .. } => "series-sample",
            Self::SetCreate { .. } => "set-create",
            Self::SetContains { .. } => "set-contains",
            Self::RecordCreate { .. } => "record-create",
            Self::VectorCreate(_) => "vector-create",
            Self::MatrixCreate { .. } => "matrix-create",
            Self::TensorCreate { .. } => "tensor-create",
            Self::VectorIndex { .. } => "vector-index",
            Self::MatrixIndex { .. } => "matrix-index",
            Self::TensorIndex { .. } => "tensor-index",
            Self::TensorSlice { .. } => "tensor-slice",
            Self::OptionSome(_) => "option-some",
            Self::OptionNone => "option-none",
            Self::OptionIsSome(_) => "option-is-some",
            Self::OptionUnwrapOr(_, _) => "option-unwrap-or",
            Self::ResultOk(_) => "result-ok",
            Self::ResultErr(_) => "result-err",
            Self::ResultIsOk(_) => "result-is-ok",
            Self::ResultUnwrapOr(_, _) => "result-unwrap-or",
            Self::ResultErrorOf(_) => "result-error-of",
            Self::Fold { .. } => "fold",
            Self::ApplyCapability { .. } => "apply-capability",
            Self::ProgramLiteral(_) => "program-literal",
            Self::VectorMap { .. } => "vector-map",
            Self::VectorMapScalar { .. } => "vector-map-scalar",
            Self::VectorReduce { .. } => "vector-reduce",
            Self::VectorAllFinite(_) => "vector-all-finite",
        }
    }

    #[must_use]
    pub fn format_ssa(&self) -> String {
        format!("{} {self:?}", self.name())
    }
}
