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
    /// Signed arbitrary-precision integer decimal (constructor ExactInt).
    ConstExactInt(String),
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
    /// Exact integral binary64-to-i64 conversion; rejects rounding and overflow.
    ToInt(EmirValue),
    /// Convert a numeric scalar to binary64 without applying arithmetic.
    ToF64(EmirValue),
    /// Euclidean signed integer quotient; rejects zero divisors.
    IntegerQuotient(EmirValue, EmirValue),
    /// Generic exact-integer machine call (`quot`, `rem`, `root`, `gcd`, ...).
    ExactIntCall { name: String, args: Vec<EmirValue> },
    /// Binary64 representation equality, including signed zero and NaN payloads.
    SameBits(EmirValue, EmirValue),
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
    /// Preserve element carriers without numeric widening.
    ListCreate(Vec<EmirValue>),
    RecordField { record: EmirValue, field: String },
    VectorCreate(Vec<EmirValue>),
    VectorLength(EmirValue),
    /// Pack a homogeneous Float64 sequence without widening exact carriers.
    ToF64Vector(EmirValue),
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
    /// Matrix storage metadata and checked row-major packing.
    MatrixRows(EmirValue),
    TensorShape(EmirValue),
    /// IEEE binary64 intrinsics and UTF-8/decimal representation operations.
    F64Exp2(EmirValue),
    F64PowI(EmirValue, EmirValue),
    TextTrim(EmirValue),
    TextLength(EmirValue),
    /// Native unsigned-index conversion followed by decimal representation.
    IndexText(EmirValue),
    TextByte(EmirValue, EmirValue),
    FormatScientific(EmirValue, EmirValue),
    ParseF64(EmirValue),
    TensorPack { shape: EmirValue, data: EmirValue },
    DenseIndex { dense: EmirValue, index: EmirValue },
    MatrixCols(EmirValue),
    MatrixPack { rows: EmirValue, cols: EmirValue, data: EmirValue },
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
    /// Evaluate only the selected program, with explicit captured arguments.
    Branch {
        condition: EmirValue,
        args: Vec<EmirValue>,
        then_body: EmirProgram,
        else_body: EmirProgram,
    },
    /// Body inputs are iteration index, accumulator, then captured arguments.
    /// An optional stop predicate receives the same inputs before each update.
    Iterate {
        count: EmirValue,
        init: EmirValue,
        args: Vec<EmirValue>,
        stop: Option<EmirProgram>,
        body: EmirProgram,
    },
    /// Build a sequence once; body inputs are index then captured arguments.
    Collect { count: EmirValue, args: Vec<EmirValue>, body: EmirProgram },
    /// A refusal declared by the authored program, never a fabricated value.
    Refuse(String),
    RefuseValue(EmirValue),
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
    /// Evaluate a literal program with explicit typed input and state frames.
    ///
    /// `declared` carries the callee's authored input carrier
    /// signatures (`Int`, `Record<CS>`, `Fn<A,B>`, one entry per
    /// input, empty = unknown). The backend uses them to recover the
    /// kind of degenerate argument expressions (an empty `[]` literal
    /// has no element kind of its own); evaluation ignores them.
    CallFrame {
        body: EmirProgram,
        inputs: Vec<EmirValue>,
        state: Vec<EmirValue>,
        declared: Vec<String>,
    },
    /// Re-enter the active recursive program with a new input frame.
    /// Generic self-application; not a mathematical leaf.
    ///
    /// `result` carries the enclosing function's authored output
    /// carrier signature (empty = unknown, e.g. the recur lane).
    /// Kind inference uses it instead of guessing from the first
    /// argument; a multi-argument recursion's result rarely matches
    /// its first argument's carrier.
    CallSelf {
        inputs: Vec<EmirValue>,
        result: String,
    },
    /// Compare scalar/dense carrier layout, including stored element counts.
    SameDenseShape(EmirValue, EmirValue),
    DenseLayout(EmirValue),
    VectorSlice { vector: EmirValue, offset: EmirValue, count: EmirValue },
    VectorConcat(Vec<EmirValue>),
    /// Stable binary64 representation ordering, including signed zero and NaNs.
    F64SortTotal(EmirValue),
    /// Copy numeric storage into a Float64 vector; scalar Int explicitly widens.
    DenseValues(EmirValue),
    /// Rebuild a floating dense carrier with the template layout.
    DenseRepack { template: EmirValue, data: EmirValue },

    ProgramLiteral {
        body: EmirProgram,
        /// Captures follow explicit arguments in the nested input frame.
        captures: Vec<EmirValue>,
        /// Bind the numeric argument vector as one input, not one input per element.
        vector_input: bool,
        /// The literal's parameter-domain carrier signature
        /// (`Int`, `Rat`, `Record<CS>`, `Fn<A,B>`); empty is the
        /// numeric-lane/VM-converted form with no typed ABI. The
        /// literal's own callable kind composes this with the body's
        /// inferred result.
        signature: String,
    },

    /// Concatenate list carriers, preserving element carriers. The
    /// authored cons spelling `[head, ..tail]` lowers here; unlike
    /// [`Self::VectorConcat`] this is not a Float64 dense-lane op.
    ListConcat(Vec<EmirValue>),

    /// Invoke a closed numeric program with a dynamic Float64 argument vector.
    /// Scalar results are packed as one element; vector results retain their shape.
    /// This instruction does not select a method or impose mathematical guards.
    CallProgram { program: EmirValue, inputs: EmirValue },
    /// Invoke a typed program value with explicit argument values. Curried
    /// programs fold application left, so one op covers the house curried
    /// application form `f(a, b)`. Unlike the `CallProgram` family this is
    /// not the Float64 numeric ABI: arguments and result keep their own
    /// carriers, and captures occupy the callee's trailing input slots.
    CallValue { program: EmirValue, inputs: Vec<EmirValue> },
    /// Require a Float64 callback result without packing or widening it.
    CallScalarProgram { program: EmirValue, inputs: EmirValue },
    /// Convert a Float64 or Int callback result to binary64.
    CallRealProgram { program: EmirValue, inputs: EmirValue },
    /// Recover ordinary callback faults and nonnumeric results; retain budget faults.
    TryCallRealProgram { program: EmirValue, inputs: EmirValue },

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

impl EmirOp {
    /// A closed numeric program with one Float64 input per supplied element.
    pub fn program_literal(body: EmirProgram) -> Self {
        Self::ProgramLiteral { body, captures: Vec::new(), vector_input: false, signature: String::new() }
    }
}
