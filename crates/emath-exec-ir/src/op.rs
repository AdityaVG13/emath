//! Universal instructions for the stable executable machine.

use super::{EmirValue, BuiltinId, EmirSliceAxis, EmirProgram, FoldCombine, CellClass, VectorScalarOp, ReduceId};

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
    /// Signed arbitrary-precision integer decimal (constructor `ExactInt`).
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

    /// Generic capability application. `capability` is a `FeatureID` resolved
    /// against the installed Language Image; it is never interpreted by name.
    ApplyCapability {
        capability: String,
        class: CellClass,
        args: Vec<EmirValue>,
    },

    /// Universal program-as-value artifact: a nested program literal that
    /// evaluates to [`crate::interp::Value::Program`]. Domain-neutral
    /// carrier machinery — the literal names no `FeatureID` and dispatches
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

    /// A quoted unary program template as compiled code: the lambda
    /// body lowered once, with the open (free) constant names as
    /// trailing runtime inputs after the parameter. The artifact
    /// value is the compiled closure factory - the function lane
    /// carries no tree and runs no interpreter. Two template shapes
    /// are admitted. A FUNCTION template (`quote(function x in Rat:
    /// ...)`) carries `param: Some(x)`: the declared domain governs
    /// the parameter AND the open constants, so the compiled factory
    /// is monomorphic in the carrier. An EXPRESSION template
    /// (`quote(x + c)`, bead emath-expression-quotes-324y0) carries
    /// `param: None` and carrier `Union`: the body computes over the
    /// dynamic value union, every free name is a runtime input, and
    /// `evaluate` yields a scalar of dynamic carrier, projected at
    /// typed boundaries. The union lane additionally carries the
    /// DISTILLED TREE of the same authored body (the dual
    /// representation, bead emath-shared-tree-view-make-bp8nu):
    /// `quote.view` walks it and `quote.make` rebuilds into it, both
    /// through the shared std-only algorithms in `emath-rt`. The
    /// hygiene law is structural for all lanes: a quote never
    /// captures the ambient frame, so every free name of the body
    /// stays open until substituted.
    CodeLiteral {
        body: EmirProgram,
        /// The template's parameter name (nested input 0); `None` for
        /// an expression template (no parameter, no closure).
        param: Option<String>,
        /// The open names in binding order: a function template's
        /// open constants (nested inputs `1..=free.len()`), or an
        /// expression template's free names (nested inputs
        /// `0..free.len()`, sorted by the free-name collector).
        free: Vec<String>,
        /// The carrier signature: a function template's declared
        /// scalar carrier (`Rat`, `Int`, or `Bool`) that the backend
        /// instantiates the Code factory over, or `Union` for an
        /// expression template (the dynamic value-union lane).
        carrier: String,
        /// The distilled tree of the expression template: `None` on
        /// the function lane, `Some` on the union lane - emitted by
        /// this same lowering pass from the same authored body as the
        /// compiled factory, so the two representations cannot drift.
        tree: Option<emath_rt::code_tree::CodeTree>,
        /// The capture-time dependency snapshot: free names that
        /// resolve against the module's callable table, stamped with
        /// the declaration identity (`decl_stamp`) - the same stamps
        /// the VM's quote capture computes.
        deps: std::collections::BTreeMap<String, u64>,
    },
    /// `quote.substitute(code, name, value)`: bind one open name by
    /// partial application. The reference is a static string resolved
    /// at lowering; the value is a runtime scalar of the template's
    /// carrier (any scalar carrier for an expression template).
    /// Binding a name that is not open is a no-op
    /// (tree-substitution parity: substituting an absent name leaves
    /// the code unchanged).
    CodeSubstitute {
        code: EmirValue,
        reference: String,
        value: EmirValue,
    },
    /// `quote.evaluate(code)`: the guarded executor. Open code
    /// refuses `unbound_code` naming every remaining open name;
    /// closed code yields the specialized unary closure for a
    /// function template (a callable value, so a def bound to it is
    /// closure-valued) or the computed scalar for an expression
    /// template - through the compiled factory when the tree came
    /// from a static template literal, through the shared scalar
    /// tree evaluator when it was made from node records.
    CodeEvaluate {
        code: EmirValue,
    },
    /// `quote.view(code)`: open a Code value (or Fragment package)
    /// into its structural node records - the shared tree walked
    /// through the minted-scope packaging (bead
    /// emath-shared-tree-view-make-bp8nu). The artifact value is the
    /// dynamic node-record family over the same layouts the VM
    /// produces.
    CodeView {
        code: EmirValue,
    },
    /// `quote.make(node)`: rebuild a Code value from node records
    /// (possibly modified). The rebuilt tree's open names and stamped
    /// dependencies follow the VM's `quote_make` laws; node kinds
    /// outside the emitted tree subset refuse by name.
    CodeMake {
        node: EmirValue,
    },
    /// `quote.body(code)`: the definition-table unfold - a Code
    /// naming a module function becomes the Available record with the
    /// body fragment (transparent), the Opaque record (opaque), or
    /// the Opaque-unbound record (a name the table does not carry);
    /// any other Code yields Available of its own tree (bead
    /// emath-quote-body-defs-trto7). The artifact value is the node
    /// family over the embedded definition table.
    CodeBody {
        code: EmirValue,
    },
    /// `quote.open term in package: body` (the binder form): validate
    /// the Fragment package's minted Scope witness (a forged witness
    /// refuses by the VM's name) and unwrap the term as a
    /// dependency-free Code, bound to the binder parameter for the
    /// body's lowering (bead emath-quote-bind-open-consumer-6f86g).
    /// The artifact value is the shared lane's opened `ExprCode`.
    CodeOpen {
        package: EmirValue,
    },
    /// `quote.bind(code)` (the call form): mint nested binder syntax
    /// inside a code value and re-stamp its dependencies - over the
    /// distilled subset (no binder nodes) the mint walk is the
    /// identity and the snapshot re-stamps exactly as the VM's does
    /// (bead emath-quote-bind-open-consumer-6f86g). The binder FORM
    /// of quote.bind (a fresh-tokened function literal) is outside
    /// the distilled subset and refuses at lowering by name.
    CodeBind {
        code: EmirValue,
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
