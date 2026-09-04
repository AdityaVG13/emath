//! Universal instructions for the stable executable machine.

use super::*;

/// Domain-neutral execution instructions.
///
/// Mathematical feature identity never appears as a variant. Feature behavior
/// enters through [`Self::ApplyCapability`], whose reference program or optional
/// native kernel is selected from the installed Language Image.
#[derive(Clone, Debug, PartialEq)]
pub enum EmirOp {
    ConstF64(u64),
    ConstI64(i64),
    ConstBigInt(String),
    ConstText(String),
    ConstComplex(f64, f64),
    ConstBool(bool),
    LoadInput(u16),
    LoadState(u16),

    // Closed scalar reference-bytecode vocabulary. These instructions carry no
    // FeatureID and are not independently admitted language features.
    F64Add(EmirValue, EmirValue),
    F64Sub(EmirValue, EmirValue),
    F64Mul(EmirValue, EmirValue),
    F64Div(EmirValue, EmirValue),
    F64Pow(EmirValue, EmirValue),
    Neg(EmirValue),
    UnaryBuiltin(BuiltinId, EmirValue),
    BinaryBuiltin(BuiltinId, EmirValue, EmirValue),
    Lt(EmirValue, EmirValue),
    Le(EmirValue, EmirValue),
    Gt(EmirValue, EmirValue),
    Ge(EmirValue, EmirValue),
    Eq(EmirValue, EmirValue),
    Ne(EmirValue, EmirValue),
    And(EmirValue, EmirValue),
    Or(EmirValue, EmirValue),
    Imply(EmirValue, EmirValue),
    Iff(EmirValue, EmirValue),
    Not(EmirValue),
    IsFinite(EmirValue),
    Select {
        condition: EmirValue,
        then_value: EmirValue,
        else_value: EmirValue,
    },

    // Universal construction, storage, and indexing.
    FormatText {
        template: String,
        arguments: Vec<EmirValue>,
    },
    SeriesCreate {
        points: Vec<(f64, f64)>,
        interpolation: String,
        extrapolation: String,
    },
    SeriesSample {
        series: EmirValue,
        time: EmirValue,
    },
    SetCreate {
        elements: Vec<EmirValue>,
        guards: Vec<Option<EmirValue>>,
    },
    SetContains {
        element: EmirValue,
        set: EmirValue,
    },
    RecordCreate {
        type_name: String,
        fields: Vec<(String, EmirValue)>,
    },
    VectorCreate(Vec<EmirValue>),
    MatrixCreate {
        rows: usize,
        cols: usize,
        elements: Vec<EmirValue>,
    },
    TensorCreate {
        shape: Vec<usize>,
        elements: Vec<EmirValue>,
    },
    VectorIndex {
        vector: EmirValue,
        index: EmirValue,
    },
    MatrixIndex {
        matrix: EmirValue,
        row: EmirValue,
        col: EmirValue,
    },
    TensorIndex {
        tensor: EmirValue,
        indices: Vec<EmirValue>,
    },
    TensorSlice {
        tensor: EmirValue,
        axes: Vec<EmirSliceAxis>,
    },
    OptionSome(EmirValue),
    OptionNone,
    OptionIsSome(EmirValue),
    OptionUnwrapOr(EmirValue, EmirValue),
    ResultOk(EmirValue),
    ResultErr(EmirValue),
    ResultIsOk(EmirValue),
    ResultUnwrapOr(EmirValue, EmirValue),
    ResultErrorOf(EmirValue),

    // Universal binding/control.
    Fold {
        start: EmirValue,
        end: EmirValue,
        init: EmirValue,
        combine: FoldCombine,
        loop_var_index: u16,
        body: EmirProgram,
    },

    /// Generic capability application. `capability` is a FeatureID resolved
    /// against the installed Language Image; it is never interpreted by name.
    ApplyCapability {
        capability: String,
        class: CellClass,
        args: Vec<EmirValue>,
    },

    /// Universal program-as-value artifact: a nested program literal that
    /// evaluates to [`crate::interp::Value::Program`]. Domain-neutral
    /// carrier machinery — the literal names no FeatureID and dispatches
    /// nothing; like any value it can flow into an `ApplyCapability`
    /// argument register.
    ProgramLiteral(EmirProgram),

    // Closed carrier bytecode used by authored reference programs.
    VectorMap {
        builtin: BuiltinId,
        source: EmirValue,
    },
    VectorMapScalar {
        op: VectorScalarOp,
        vector: EmirValue,
        scalar: EmirValue,
    },
    VectorReduce {
        reduce: ReduceId,
        source: EmirValue,
    },
    VectorAllFinite(EmirValue),
}
