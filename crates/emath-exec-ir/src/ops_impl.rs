//! Universal instruction metadata.

use super::EmirOp;

impl EmirOp {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ConstF64(_) => "const-f64",
            Self::ConstI64(_) => "const-i64",
            Self::ConstExactInt(_) => "const-exact-int",
            Self::ConstText(_) => "const-text",
            Self::ConstBool(_) => "const-bool",
            Self::LoadInput(_) => "load-input",
            Self::F64Add(_, _) => "scalar-add",
            Self::F64Sub(_, _) => "scalar-sub",
            Self::F64Mul(_, _) => "scalar-mul",
            Self::F64Div(_, _) => "scalar-div",
            Self::Neg(_) => "scalar-negate",
            Self::ExactIntCall { .. } => "exact-int-call",
            Self::SameBits(_, _) => "same-bits",
            Self::Lt(_, _) => "compare-lt",
            Self::Le(_, _) => "compare-le",
            Self::Gt(_, _) => "compare-gt",
            Self::Ge(_, _) => "compare-ge",
            Self::Eq(_, _) => "compare-eq",
            Self::Ne(_, _) => "compare-ne",
            Self::And(_, _) => "boolean-and",
            Self::Or(_, _) => "boolean-or",
            Self::Not(_) => "boolean-not",
            Self::IsFinite(_) => "is-finite",
            Self::FormatText { .. } => "format-text",
            Self::RecordCreate { .. } => "record-create",
            Self::ListCreate(_) => "list-create",
            Self::BufferCreate { .. } => "buffer-create",
            Self::BufferWrite { .. } => "buffer-write",
            Self::RecordField { .. } => "record-field",
            Self::Branch { .. } => "branch",
            Self::Iterate { .. } => "iterate",
            Self::Refuse(_) => "refuse",
            Self::RefuseValue(_) => "refuse-value",
            Self::VectorLength(_) => "vector-length",
            Self::ToF64Vector(_) => "to-f64-vector",
            Self::VectorIndex { .. } => "vector-index",
            Self::TextTrim(_) => "text-trim",
            Self::TextLength(_) => "text-length",
            Self::IndexText(_) => "index-text",
            Self::TextByte(..) => "text-byte",
            Self::FormatScientific(..) => "format-scientific",
            Self::ParseF64(_) => "parse-f64",
            Self::TensorSlice { .. } => "tensor-slice",
            Self::ResultIsOk(_) => "result-is-ok",
            Self::ResultUnwrapOr(_, _) => "result-unwrap-or",
            Self::Collect { .. } => "collect",
            Self::Fold { .. } => "fold",
            Self::ApplyCapability { .. } => "apply-capability",
            Self::CallFrame { .. } => "call-frame",
            Self::CallSelf { .. } => "call-self",
            Self::CallSibling { .. } => "call-sibling",
            Self::F64SortTotal(_) => "f64-sort-total",
            Self::ProgramLiteral { .. } => "program-literal",
            Self::CodeLiteral { .. } => "code-literal",
            Self::CodeSubstitute { .. } => "code-substitute",
            Self::CodeEvaluate { .. } => "code-evaluate",
            Self::CodeView { .. } => "code-view",
            Self::CodeMake { .. } => "code-make",
            Self::CodeBody { .. } => "code-body",
            Self::CodeOpen { .. } => "code-open",
            Self::CodeBind { .. } => "code-bind",
            Self::CodeIdentity { .. } => "code-identity",
            Self::CodeJoin { .. } => "code-join",
            Self::EngineCall { .. } => "engine-call",
            Self::CallProgram { .. } => "call-program",
            Self::CallScalarProgram { .. } => "call-scalar-program",
            Self::CallRealProgram { .. } => "call-real-program",
            Self::TryCallRealProgram { .. } => "try-call-real-program",
            Self::CallValue { .. } => "call-value",
            Self::ListConcat(_) => "list-concat",
        }
    }

    #[must_use]
    pub fn format_ssa(&self) -> String {
        if let Self::ApplyCapability { capability, class, args } = self {
            format!("apply-capability name={capability} class={class:?} args={args:?}")
        } else {
            format!("{} {self:?}", self.name())
        }
    }
}
