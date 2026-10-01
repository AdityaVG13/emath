//! Scalar-kind inference and typed operand/expression helpers.

use super::{
    BTreeMap, BackendError, BinOp, Block, EmirOp, EmirProgram, EmirValue, Expr, FoldCombine, Stmt,
    Ty, UnOp, checked_integer_result, escape_ident, exact_int_ref, flat_ssa, op_expr, operand,
    infer_instantiation, owned_value, record_ident, record_layout, record_template, render_expr,
    rt_call, to_code_value, to_node,
};

/// Scalar carrier of an EMIR register. Exact carriers never widen implicitly.
pub(crate) type InputKinds = BTreeMap<String, ValueKind>;

thread_local! {
    /// The result carrier of the recursive wrapper currently being
    /// rendered (`__self` at entries, `__frame_self` in inlined
    /// frames). The wrapper's emitted return type is decided once,
    /// over the enclosing body's full join; a `CallSelf` site nested
    /// in a branch cannot re-derive it (the local inference reads
    /// only the branch), so the render reads the scope first and
    /// crosses from the carrier the wrapper actually returns.
    /// Beside it, the wrapper's parameter carriers in position order:
    /// a `CallSelf` inside a closure body renders among the closure's
    /// own inputs, so it reads the target's parameters here.
    static SELF_WRAPPER_KIND: std::cell::RefCell<Vec<(ValueKind, Vec<ValueKind>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// The emitted entry's result carrier per sibling function
    /// (pre-lowered over the crate's own functions before any
    /// rendering). A `CallSibling` register carries the callee's
    /// EMITTED carrier, not the authored declaration: the two
    /// disagree when the callee's body joins mixed-kind arms and
    /// emits the ratio spelling against an Int-declared tail.
    static SIBLING_ENTRY_KINDS: std::cell::RefCell<Vec<BTreeMap<String, ValueKind>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// The emitted entry ABI per module function, registered as each
    /// entry emits (callees before callers), beside the crate's
    /// tree-lane functions (their internal calls target the
    /// non-resetting body fn). A named `CallFrame` whose frame carriers
    /// equal its callee's registered ABI renders as a call to that
    /// entry instead of re-inlining the body at the site.
    static ENTRY_ABIS: std::cell::RefCell<Vec<(BTreeMap<String, EntryAbi>, std::collections::BTreeSet<String>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// One emitted entry's carrier ABI: the parameter carriers in input
/// order and the carrier the entry actually returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EntryAbi {
    pub(crate) inputs: Vec<ValueKind>,
    pub(crate) result: ValueKind,
}

/// Guarded entry-ABI table (see `ENTRY_ABIS`): one crate emission's
/// registry over its tree-lane function names, popped on drop. Outside
/// a scope no entry is registered and every named frame inlines.
pub(crate) struct EntryAbiScope;
impl EntryAbiScope {
    pub(crate) fn enter(tree_lane: std::collections::BTreeSet<String>) -> Self {
        ENTRY_ABIS.with(|stack| stack.borrow_mut().push((BTreeMap::new(), tree_lane)));
        Self
    }
}
impl Drop for EntryAbiScope {
    fn drop(&mut self) {
        ENTRY_ABIS.with(|stack| stack.borrow_mut().pop());
    }
}

/// Register an emitted entry's ABI in the innermost scope.
pub(crate) fn register_entry_abi(name: &str, abi: EntryAbi) {
    ENTRY_ABIS.with(|stack| {
        if let Some((table, _)) = stack.borrow_mut().last_mut() {
            table.insert(name.to_string(), abi);
        }
    });
}

/// A module function's emitted entry ABI, when it has one.
pub(crate) fn entry_abi(name: &str) -> Option<EntryAbi> {
    ENTRY_ABIS.with(|stack| {
        stack
            .borrow()
            .last()
            .and_then(|(table, _)| table.get(name).cloned())
    })
}

/// The body fn of a tree-lane entry. The public entry resets the
/// shared-tree mint (one deterministic token sequence per run) and
/// delegates here; calls from inside a run - sibling cycle edges and
/// named frames - target this fn, so they continue the caller's mint
/// instead of restarting it (the VM mints once per query). The name
/// carries no `__` run: control-scope rendering renames `__e*`
/// registers textually.
pub(crate) fn tree_entry_body_name(name: &str) -> String {
    format!("emath_body_{name}")
}

/// The fn an in-crate call to `name` targets: the tree-lane body fn
/// when the crate emits `name` on the tree lane, else the entry itself.
pub(crate) fn internal_entry_name(name: &str) -> String {
    let tree_lane = ENTRY_ABIS.with(|stack| {
        stack
            .borrow()
            .last()
            .is_some_and(|(_, tree_lane)| tree_lane.contains(name))
    });
    if tree_lane {
        tree_entry_body_name(name)
    } else {
        escape_ident(name)
    }
}

/// Guarded entry onto the wrapper-carrier stack (see
/// `SELF_WRAPPER_KIND`); drops pop on scope exit, so nested frames
/// shadow their enclosing wrapper exactly while they render.
pub(crate) struct SelfWrapperScope;
impl SelfWrapperScope {
    pub(crate) fn enter(kind: ValueKind, params: Vec<ValueKind>) -> Self {
        SELF_WRAPPER_KIND.with(|stack| stack.borrow_mut().push((kind, params)));
        Self
    }
}
impl Drop for SelfWrapperScope {
    fn drop(&mut self) {
        SELF_WRAPPER_KIND.with(|stack| stack.borrow_mut().pop());
    }
}

/// The innermost rendering wrapper's result carrier, when one is
/// being rendered.
pub(crate) fn self_wrapper_kind() -> Option<ValueKind> {
    SELF_WRAPPER_KIND.with(|stack| stack.borrow().last().map(|(kind, _)| kind.clone()))
}

/// The innermost rendering wrapper's parameter carrier at `position`.
pub(crate) fn self_wrapper_param(position: usize) -> Option<ValueKind> {
    SELF_WRAPPER_KIND.with(|stack| {
        stack
            .borrow()
            .last()
            .and_then(|(_, params)| params.get(position).cloned())
    })
}

thread_local! {
    /// The recursive wrapper currently rendering as a loop: its
    /// parameter names in position order, and every `CallSelf` op in
    /// tail position of its body (keyed by op address while the
    /// borrowed tree renders), each with the argument positions that
    /// pass their parameter through unchanged.
    static TAIL_LOOP: std::cell::RefCell<Vec<(Vec<String>, std::collections::HashMap<usize, Vec<bool>>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Guarded entry onto the tail-loop stack: while a recursive
/// wrapper's body renders, its tail self calls render as parameter
/// rebinding plus `continue` (the wrapper body runs inside `loop`),
/// so a walk of any length runs in constant stack and passes an
/// unchanged sequence along without copying it. Only calls reached
/// through the body's result and branch arms qualify: those render
/// as plain blocks of the wrapper, never inside a closure or an
/// inner `for` loop that would capture the `continue`.
pub(crate) struct TailLoopScope;
impl TailLoopScope {
    pub(crate) fn enter(body: &EmirProgram, params: Vec<String>, states: usize) -> Self {
        let inputs = params.len() - states;
        let roots: Vec<Option<usize>> = (0..inputs).map(Some).collect();
        let state_roots: Vec<Option<usize>> = (inputs..params.len()).map(Some).collect();
        let mut tails = std::collections::HashMap::new();
        collect_tail_calls(body, &roots, &state_roots, &mut tails);
        TAIL_LOOP.with(|stack| stack.borrow_mut().push((params, tails)));
        Self
    }
}
impl Drop for TailLoopScope {
    fn drop(&mut self) {
        TAIL_LOOP.with(|stack| stack.borrow_mut().pop());
    }
}

/// The wrapper parameter a register of `program` is, when it is one:
/// a load of an input or state that the enclosing chain maps to a
/// parameter position.
fn param_of(
    program: &EmirProgram,
    value: EmirValue,
    inputs: &[Option<usize>],
    states: &[Option<usize>],
) -> Option<usize> {
    match program.ops.get(value.0 as usize).map(|(op, _)| op) {
        Some(EmirOp::LoadInput(index)) => inputs.get(*index as usize).copied().flatten(),
        Some(EmirOp::LoadState(index)) => states.get(*index as usize).copied().flatten(),
        _ => None,
    }
}

fn collect_tail_calls(
    program: &EmirProgram,
    inputs: &[Option<usize>],
    states: &[Option<usize>],
    tails: &mut std::collections::HashMap<usize, Vec<bool>>,
) {
    let Some((op, _)) = program.ops.get(program.result.0 as usize) else {
        return;
    };
    match op {
        EmirOp::CallSelf { inputs: args, .. } => {
            let unchanged = args
                .iter()
                .enumerate()
                .map(|(position, value)| {
                    param_of(program, *value, inputs, states) == Some(position)
                })
                .collect();
            tails.insert(std::ptr::from_ref(op) as usize, unchanged);
        }
        EmirOp::Branch {
            args,
            then_body,
            else_body,
            ..
        } => {
            // Arm input k is the branch's argument k in this program.
            let arm_inputs: Vec<Option<usize>> = args
                .iter()
                .map(|value| param_of(program, *value, inputs, states))
                .collect();
            collect_tail_calls(then_body, &arm_inputs, &[], tails);
            collect_tail_calls(else_body, &arm_inputs, &[], tails);
        }
        _ => {}
    }
}

/// For a `CallSelf` op in tail position of the innermost loop-rendered
/// wrapper: the wrapper's parameter names and which arguments pass
/// their parameter through unchanged.
pub(crate) fn tail_call_slot(op: &EmirOp) -> Option<(Vec<String>, Vec<bool>)> {
    TAIL_LOOP.with(|stack| {
        let stack = stack.borrow();
        let (params, tails) = stack.last()?;
        tails
            .get(&(std::ptr::from_ref(op) as usize))
            .map(|unchanged| (params.clone(), unchanged.clone()))
    })
}

/// Whether every path of `program` ends in a loop-rendered tail call,
/// so its rendered block has type `!` and takes no carrier widening.
pub(crate) fn ends_in_tail_loop(program: &EmirProgram) -> bool {
    match program.ops.get(program.result.0 as usize).map(|(op, _)| op) {
        Some(op @ EmirOp::CallSelf { .. }) => tail_call_slot(op).is_some(),
        Some(EmirOp::Branch {
            then_body,
            else_body,
            ..
        }) => ends_in_tail_loop(then_body) && ends_in_tail_loop(else_body),
        _ => false,
    }
}

/// Guarded entry onto the sibling-entry-carrier table stack (see
/// `SIBLING_ENTRY_KINDS`).
pub(crate) struct SiblingEntryScope;
impl SiblingEntryScope {
    pub(crate) fn enter(kinds: BTreeMap<String, ValueKind>) -> Self {
        SIBLING_ENTRY_KINDS.with(|stack| stack.borrow_mut().push(kinds));
        Self
    }
}
impl Drop for SiblingEntryScope {
    fn drop(&mut self) {
        SIBLING_ENTRY_KINDS.with(|stack| stack.borrow_mut().pop());
    }
}

/// A sibling function's emitted entry carrier, when the emission
/// pre-lowered it.
pub(crate) fn sibling_entry_kind(name: &str) -> Option<ValueKind> {
    SIBLING_ENTRY_KINDS.with(|stack| {
        stack
            .borrow()
            .iter()
            .rev()
            .find_map(|table| table.get(name).cloned())
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ValueKind {
    I64,
    /// Signed constructor integer that may exceed i64.
    ExactInt,
    F64,
    Rational,
    Bool,
    /// Stage-2 (emath-t63iz): exact big field element (`emath_rt::UBig`).
    BigInt,
    Text,
    Program,
    /// The artifact Code carrier (emath-npky7): a quoted unary
    /// program compiled once into a closure factory
    /// (`emath_rt::code::Code<V>`), instantiated over the template's
    /// declared scalar carrier (the boxed kind). Open until
    /// `substitute` closes it; `evaluate` yields the specialized
    /// closure.
    Code(Box<ValueKind>),
    /// The open expression template (emath-expression-quotes-324y0):
    /// a quoted EXPRESSION with free names, compiled over the value
    /// union - no parameter, no closure. Open until `substitute`
    /// closes it; `evaluate` yields the computed
    /// [`ValueKind::CodeValue`].
    ExprCode,
    /// The dynamic scalar union an expression template computes over
    /// and `evaluate` yields: the Int/Rat/Bool carriers as one
    /// value. Typed boundaries project it checked onto the declared
    /// carrier.
    CodeValue,
    /// The node-record family (bead emath-shared-tree-view-make-bp8nu):
    /// the dynamic values `quote.view` produces and authored
    /// structural code consumes - node records (tag records are
    /// empty-field records), sequences, tuples, scalars, and Code
    /// values folded into one family (`emath_rt::code_tree::NodeValue`).
    /// Kind rules coerce scalars and Code values into it wherever a
    /// node value flows.
    Node,
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
                "Code" => ValueKind::ExprCode,
                // The dynamic node family: a Fragment package record
                // and the unparameterized (heterogeneous) sequence.
                "Fragment" | "Sequence" => ValueKind::Node,
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
                    } else if let Some(inner) = text
                        .strip_prefix("Code<")
                        .and_then(|text| text.strip_suffix('>'))
                    {
                        // The program template's scalar carrier (the
                        // authored `Code[T]` spelling): a quoted unary
                        // program over T - substitutes bind T constants
                        // and evaluate yields the specialized T -> T
                        // closure. Bare `Code` (matched above) is the
                        // expression template, whose evaluate yields
                        // the union scalar.
                        ValueKind::Code(Box::new(parse(inner, remaining - 1)))
                    } else if text.starts_with("Fn<") && text.ends_with('>') {
                        // Typed closure signature: every part but the
                        // last is a parameter; the last is the result.
                        let parts = split_signature_parts(&text[3..text.len() - 1]);
                        let Some((result, params)) = parts.split_last() else {
                            return ValueKind::Other;
                        };
                        ValueKind::Closure {
                            params: params
                                .iter()
                                .map(|part| parse(part, remaining - 1))
                                .collect(),
                            result: Box::new(parse(result, remaining - 1)),
                        }
                    } else if text.starts_with("Tuple<") && text.ends_with('>') {
                        // A tuple kinds as the positional sequence
                        // its literals lower to (ListCreate): the
                        // element carrier joins the declared element
                        // domains - the vector-element law, so a
                        // heterogeneous tuple refuses exactly where
                        // its literal does.
                        let parts = split_signature_parts(&text[6..text.len() - 1]);
                        let joined = parts
                            .iter()
                            .map(|part| parse(part, remaining - 1))
                            .fold(ValueKind::Other, join_tuple_element);
                        ValueKind::Vector(Box::new(joined))
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
            Self::Code(carrier) => Ty::Named(format!(
                "emath_rt::code::Code<{}>",
                render_ty(&carrier.rust_ty()?)
            )),
            Self::ExprCode => Ty::Named("emath_rt::code::ExprCode".into()),
            Self::CodeValue => Ty::Named("emath_rt::code::CodeValue".into()),
            Self::Node => Ty::Named("emath_rt::code_tree::NodeValue".into()),
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
            Self::Record(name) => Ty::Named(format!("EmathRecord_{}", record_ident(name))),
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
            Self::I64 | Self::F64 | Self::Rational | Self::Bool | Self::Complex | Self::CodeValue
        )
    }
}

/// An operand that can join the value-union lane: the union itself or
/// a scalar the render wraps into it (Int, Rat, Bool). Float64 and
/// structured carriers beside the union fall back to their own lanes
/// and the expression-template body check refuses them named.
pub(super) fn union_promotable(kind: &ValueKind) -> bool {
    matches!(
        kind,
        ValueKind::CodeValue | ValueKind::I64 | ValueKind::Rational | ValueKind::Bool
    )
}

/// True when one side is the union and the other can join it.
pub(super) fn union_pair(kinds: &[ValueKind], left: EmirValue, right: EmirValue) -> bool {
    let left_kind = kind_at(kinds, left);
    let right_kind = kind_at(kinds, right);
    (left_kind == ValueKind::CodeValue && union_promotable(&right_kind))
        || (right_kind == ValueKind::CodeValue && union_promotable(&left_kind))
}

/// A receiver kind that can never be indexed: every concrete
/// non-sequence carrier faults the VM's `vector_of` type confusion
/// (op `vector-index`). `Other` is excluded - an uninferred kind may
/// still be a sequence at runtime, so the float-index lane keeps its
/// dynamic guard.
pub(super) fn kind_is_never_indexable(kind: &ValueKind) -> bool {
    !matches!(
        kind,
        ValueKind::Vector(_)
            | ValueKind::Matrix(_)
            | ValueKind::Tensor
            | ValueKind::Node
            | ValueKind::Other
    )
}

/// A kind that carries no usable carrier information: `Other`, or a
/// vector whose element kind is itself degenerate (an empty `[]`
/// literal carries no element kind of its own; a list OF empty lists
/// carries neither its own nor its elements').
pub(super) fn kind_is_degenerate(kind: &ValueKind) -> bool {
    match kind {
        ValueKind::Other => true,
        ValueKind::Vector(element) => kind_is_degenerate(element),
        _ => false,
    }
}

/// The carrier one branch renders when its arms carry these kinds -
/// one law, one spelling, shared by the kind walk's `Branch` rule and
/// the render's arm-widening (the two computed different joins when
/// the arms disagreed: the kind rule yields a degenerate side to the
/// other arm's concrete carrier while the render joined bare, so an
/// `if done: [] else: <family tail>` kinded as the family but
/// rendered `vec!()` against it).
pub(super) fn branch_carrier(
    left: ValueKind,
    right: ValueKind,
    left_is_call_self: bool,
    right_is_call_self: bool,
) -> ValueKind {
    let left_degenerate = kind_is_degenerate(&left);
    let right_degenerate = kind_is_degenerate(&right);
    if left == ValueKind::Never
        || (left_is_call_self && right != ValueKind::Other)
        || (left_degenerate && !right_degenerate && right != ValueKind::Other)
    {
        right
    } else if right == ValueKind::Never
        || (right_is_call_self && left != ValueKind::Other)
        || (right_degenerate && !left_degenerate && left != ValueKind::Other)
    {
        left
    } else {
        // One representation: the arms fold under the scalar join
        // law, element-wise through vectors (an Int-element arm
        // widens exactly into the Rat-element vector at the arm
        // boundary); a declared Int context narrows back at the
        // checked i64 boundary. A union arm (an evaluated template or
        // applied quoted function) beside a typed scalar arm takes the
        // typed carrier - the render projects the union arm checked
        // (`if can: claim(x) else: false` is Bool). Pairs the law
        // cannot fold stay unrepresentable.
        match (&left, &right) {
            (ValueKind::CodeValue, typed) | (typed, ValueKind::CodeValue)
                if matches!(
                    typed,
                    ValueKind::I64 | ValueKind::Rational | ValueKind::ExactInt | ValueKind::Bool
                ) =>
            {
                typed.clone()
            }
            _ => join_arm_kinds(left, right),
        }
    }
}

/// Recover a frame input's kind from the callee's authored
/// declaration when the argument's own kind is degenerate (an empty
/// `[]` literal carries no element kind of its own). The declaration
/// wins only over degenerate kinds, never over a concrete inferred
/// one - except the node-lane code wrapper: the view's args/children
/// fields wrap codes as Code nodes, so a node value crossing into a
/// declared Code parameter converts through the shared bridge (the
/// declared carrier wins there, exactly as it does for a degenerate
/// argument).
pub(super) fn frame_input_kind(kind: ValueKind, declared: Option<&String>) -> ValueKind {
    let Some(signature) = declared else {
        return kind;
    };
    if kind_is_degenerate(&kind) {
        let declared_kind = ValueKind::from_signature(signature);
        if !matches!(declared_kind, ValueKind::Other) {
            return declared_kind;
        }
    }
    if kind == ValueKind::Node {
        let declared_kind = ValueKind::from_signature(signature);
        if matches!(
            declared_kind,
            ValueKind::ExprCode | ValueKind::I64 | ValueKind::Rational | ValueKind::Bool
        ) {
            return declared_kind;
        }
    }
    // An Int actual into a Rat-declared parameter widens to the
    // declared ratio carrier (the VM's Int-to-Rat admission,
    // `rat_pow(4, m)` with `p: Rat`): the body's arithmetic
    // and its recursive calls carry Rat registers, so the frame's
    // carrier is the declared one and the caller's actual crosses at
    // the binding.
    if matches!(kind, ValueKind::I64 | ValueKind::ExactInt)
        && ValueKind::from_signature(signature) == ValueKind::Rational
    {
        return ValueKind::Rational;
    }
    kind
}

/// A generic callee's declared input signatures with each `Var<name>`
/// replaced by the kind the call site's arguments bind it to, so an
/// argument with no carrier of its own (an `[]` seed for `acc: A`)
/// recovers it from a sibling argument (`f: A -> T -> A`). Variables no
/// argument binds stay `Var<name>`, which kinds as `Other`.
pub(super) fn instantiate_declared(arg_kinds: &[ValueKind], declared: &[String]) -> Vec<String> {
    if !declared.iter().any(|signature| signature.contains("Var<")) {
        return declared.to_vec();
    }
    let mut bound = BTreeMap::new();
    for (kind, signature) in arg_kinds.iter().zip(declared) {
        bind_signature_vars(signature, kind, &mut bound);
    }
    declared
        .iter()
        .map(|signature| substitute_signature_vars(signature, &bound))
        .collect()
}

thread_local! {
    static TYPE_BINDINGS: std::cell::RefCell<Vec<BTreeMap<String, String>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The type parameters an inlined generic frame's call site binds,
/// in scope while that frame's body is kind-walked and rendered, so a
/// binder domain lowered as `Var<T>` resolves to the site's carrier.
pub(crate) struct TypeBindingScope;
impl TypeBindingScope {
    pub(crate) fn enter(arg_kinds: &[ValueKind], declared: &[String]) -> Self {
        let mut bound = BTreeMap::new();
        for (kind, signature) in arg_kinds.iter().zip(declared) {
            bind_signature_vars(signature, kind, &mut bound);
        }
        TYPE_BINDINGS.with(|stack| stack.borrow_mut().push(bound));
        Self
    }
}
impl Drop for TypeBindingScope {
    fn drop(&mut self) {
        TYPE_BINDINGS.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

/// `signature` with each `Var<T>` the innermost binding frame knows
/// replaced by its carrier; unbound variables stay (and kind as Other).
pub(crate) fn bound_signature(signature: &str) -> String {
    if !signature.contains("Var<") {
        return signature.to_string();
    }
    TYPE_BINDINGS.with(|stack| {
        let mut text = signature.to_string();
        for bound in stack.borrow().iter().rev() {
            text = substitute_signature_vars(&text, bound);
        }
        text
    })
}

pub(crate) fn bind_signature_vars(signature: &str, kind: &ValueKind, bound: &mut BTreeMap<String, String>) {
    let signature = signature.trim();
    if let Some(name) = signature.strip_prefix("Var<").and_then(|rest| rest.strip_suffix('>')) {
        if !kind_is_degenerate(kind)
            && let Some(text) = kind_signature(kind)
        {
            bound.entry(name.to_string()).or_insert(text);
        }
    } else if let Some(inner) = signature.strip_prefix("Vector<").and_then(|rest| rest.strip_suffix('>')) {
        if let ValueKind::Vector(element) = kind {
            bind_signature_vars(inner, element, bound);
        }
    } else if let Some(inner) = signature.strip_prefix("Record<").and_then(|rest| rest.strip_suffix('>')) {
        // `Record<Pair<Var<A>,Int>>` against the instantiation
        // `Pair<Bool,Int>`: the arguments bind pairwise.
        if let ValueKind::Record(name) = kind
            && let Some((base, args)) = inner.strip_suffix('>').and_then(|text| text.split_once('<'))
            && let Some((kind_base, kind_args)) =
                name.strip_suffix('>').and_then(|text| text.split_once('<'))
            && base == kind_base
        {
            let args = split_signature_parts(args);
            let kind_args = split_signature_parts(kind_args);
            if args.len() == kind_args.len() {
                for (arg, kind_arg) in args.iter().zip(kind_args) {
                    bind_signature_vars(arg, &ValueKind::from_signature(kind_arg), bound);
                }
            }
        }
    } else if signature.starts_with("Fn<") && signature.ends_with('>') {
        let parts = split_signature_parts(&signature[3..signature.len() - 1]);
        if let (Some((result, params)), ValueKind::Closure { params: kinds, result: out }) =
            (parts.split_last(), kind)
            && params.len() == kinds.len()
        {
            for (part, kind) in params.iter().zip(kinds) {
                bind_signature_vars(part, kind, bound);
            }
            bind_signature_vars(result, out, bound);
        }
    }
}

pub(crate) fn substitute_signature_vars(signature: &str, bound: &BTreeMap<String, String>) -> String {
    let mut text = signature.to_string();
    for (name, kind) in bound {
        text = text.replace(&format!("Var<{name}>"), kind);
    }
    text
}

/// The signature spelling `from_signature` parses back to `kind`, for
/// the carriers a type parameter can stand for.
fn kind_signature(kind: &ValueKind) -> Option<String> {
    Some(match kind {
        ValueKind::I64 => "Int".into(),
        ValueKind::ExactInt => "ExactInt".into(),
        ValueKind::F64 => "Float64".into(),
        ValueKind::Rational => "Rat".into(),
        ValueKind::Bool => "Bool".into(),
        ValueKind::Text => "Text".into(),
        ValueKind::BigInt => "BigInt".into(),
        ValueKind::Complex => "Complex".into(),
        ValueKind::Vector(element) => format!("Vector<{}>", kind_signature(element)?),
        ValueKind::Matrix(element) => format!("Matrix<{}>", kind_signature(element)?),
        ValueKind::Record(name) => format!("Record<{name}>"),
        ValueKind::Closure { params, result } => {
            let mut parts = params.iter().map(kind_signature).collect::<Option<Vec<_>>>()?;
            parts.push(kind_signature(result)?);
            format!("Fn<{}>", parts.join(","))
        }
        _ => return None,
    })
}

thread_local! {
    /// Argument kinds seen at `CallSelf` sites during the kind walk of
    /// the recursive frame being refined (innermost frame on top). Self
    /// calls sit inside branch bodies with their own registers, so the
    /// walk records them where their kinds are known.
    static SELF_CALL_ARGS: std::cell::RefCell<Vec<Vec<ValueKind>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn record_self_call_args(passed: impl Iterator<Item = ValueKind>) {
    SELF_CALL_ARGS.with(|stack| {
        if let Some(top) = stack.borrow_mut().last_mut() {
            for (index, kind) in passed.enumerate() {
                if top.len() <= index {
                    top.resize(index + 1, ValueKind::Other);
                }
                if kind_is_degenerate(&top[index]) {
                    top[index] = kind;
                }
            }
        }
    });
}

/// Run a kind walk whose `CallSelf` sites belong to a frame of its own,
/// returning what it saw.
fn with_self_call_scope<R>(walk: impl FnOnce() -> R) -> (R, Vec<ValueKind>) {
    SELF_CALL_ARGS.with(|stack| stack.borrow_mut().push(Vec::new()));
    let result = walk();
    let seen = SELF_CALL_ARGS.with(|stack| stack.borrow_mut().pop()).unwrap_or_default();
    (result, seen)
}

/// A recursive frame input whose seed carries no element kind (an `[]`
/// accumulator, or a type-parameter declaration) takes the kind its own
/// self-calls pass for it: `tabulate_down(f, i, k - 1, [f(k), ..acc])`
/// fixes `acc` from `f`'s result at this frame's instantiation.
pub(super) fn refine_recursive_frame_kinds(
    body: &EmirProgram,
    names: &[String],
    states: &[String],
    frame: &mut InputKinds,
) {
    if !crate::contains_call_self(body) {
        return;
    }
    // Each pass can only replace a degenerate kind, so the number of
    // inputs bounds the passes.
    for _ in 0..names.len() {
        let (_, seen) = with_self_call_scope(|| value_kinds(body, names, states, frame));
        let mut changed = false;
        for (name, passed) in names.iter().zip(seen) {
            if frame.get(name).is_none_or(kind_is_degenerate) && !kind_is_degenerate(&passed) {
                frame.insert(name.clone(), passed);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// A declared arrow signature an evaluated expression template can be
/// adapted to: one stage of scalar (Int, Rat, Bool) parameters with a
/// scalar result - the carriers the value union crosses both ways.
pub(crate) fn declared_scalar_arrow(signature: &str) -> Option<ValueKind> {
    let scalar = |kind: &ValueKind| {
        matches!(kind, ValueKind::I64 | ValueKind::Rational | ValueKind::Bool)
    };
    match ValueKind::from_signature(signature) {
        kind @ ValueKind::Closure { .. } => match &kind {
            ValueKind::Closure { params, result } if params.iter().all(scalar) && scalar(result) => {
                Some(kind)
            }
            _ => None,
        },
        _ => None,
    }
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
/// The tuple element join: the vector-element law over the declared
/// tuple domains (Other absorbs, the integers widen, anything else
/// keeps the left spelling - the same join a tuple literal takes).
fn join_tuple_element(left: ValueKind, right: ValueKind) -> ValueKind {
    match (left, right) {
        (ValueKind::Other, right) => right,
        (ValueKind::I64, ValueKind::ExactInt) | (ValueKind::ExactInt, ValueKind::I64) => {
            ValueKind::ExactInt
        }
        (ValueKind::I64 | ValueKind::ExactInt, ValueKind::Rational)
        | (ValueKind::Rational, ValueKind::I64 | ValueKind::ExactInt) => ValueKind::Rational,
        (left, _) => left,
    }
}

pub(crate) fn split_signature_parts(text: &str) -> Vec<&str> {
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
    // A body containing a self-recursive call resolves those sites
    // from the body's own instantiated kind: the authored output is a
    // template over generic inputs (tabulate_at's `sequence(Rat)` at
    // an `Int -> sequence(Rat)` closure instantiation), wrong at
    // every non-declared instantiation. The provisional walk treats
    // the self-call as `Other` (join-neutral), so the body's kind
    // comes from everything else; a degenerate provisional (an
    // empty-literal tail) keeps the declared-carrier policy.
    let has_call_self = program
        .ops
        .iter()
        .any(|(op, _)| matches!(op, EmirOp::CallSelf { .. }));
    let mut resolved: Option<ValueKind> = None;
    if has_call_self {
        let mut provisional = vec![ValueKind::Other; n];
        for (i, (op, _)) in program.ops.iter().enumerate() {
            provisional[i] = kind_of_op(
                op,
                &provisional,
                names,
                states,
                input_kinds,
                Some(&ValueKind::Other),
            );
        }
        let body = provisional
            .get(program.result.0 as usize)
            .cloned()
            .unwrap_or(ValueKind::Other);
        if !kind_is_degenerate(&body) {
            resolved = Some(body);
        }
    }
    for (i, (op, _)) in program.ops.iter().enumerate() {
        kinds[i] = kind_of_op(op, &kinds, names, states, input_kinds, resolved.as_ref());
    }
    kinds
}

pub(super) fn kind_at(kinds: &[ValueKind], value: EmirValue) -> ValueKind {
    kinds
        .get(value.0 as usize)
        .cloned()
        .unwrap_or(ValueKind::Other)
}

/// True when a body's result register is a self-call (the
/// tail-recursive shape; its kind is the enclosing function's own),
/// or a branch whose arms both are: an arm sub-program's walk cannot
/// see the enclosing body's kind, so `if p: self(..) else: self(..)`
/// must yield to its sibling exactly as a bare self-call does.
pub(super) fn result_is_call_self(program: &EmirProgram) -> bool {
    register_is_call_self(program, program.result)
}

/// A self-call register, a branch whose arms both are, or a concat of
/// self-call registers (`[..self(a), ..self(b)]` carries the
/// function's own sequence carrier).
fn register_is_call_self(program: &EmirProgram, value: EmirValue) -> bool {
    program
        .ops
        .get(value.0 as usize)
        .is_some_and(|(op, _)| match op {
            EmirOp::CallSelf { .. } => true,
            EmirOp::Branch {
                then_body,
                else_body,
                ..
            } => result_is_call_self(then_body) && result_is_call_self(else_body),
            EmirOp::ListConcat(parts) => {
                !parts.is_empty() && parts.iter().all(|part| register_is_call_self(program, *part))
            }
            _ => false,
        })
}

/// The carrier one branch renders when its arms carry these kinds:
/// the scalar join law (ExactInt absorbs I64, Rational absorbs the
/// integer kinds - `if k == 0: 0 else: a / b` joins to Rat exactly as
/// the VM's Int-to-Rat widening does) applied element-wise through
/// vectors. A Rat-typed inlined carrier (replace_at's declared Rat
/// signatures instantiated over Int vectors) joins a function's
/// Int-recursive tail as the ratio vector; the render side widens
/// the non-dominant arm through `numeric_boundary_value` onto the
/// same join. Every pair the law cannot fold is unrepresentable.
pub(super) fn join_arm_kinds(left: ValueKind, right: ValueKind) -> ValueKind {
    match (left, right) {
        (ValueKind::ExactInt, ValueKind::I64) | (ValueKind::I64, ValueKind::ExactInt) => {
            ValueKind::ExactInt
        }
        (ValueKind::Rational, ValueKind::I64 | ValueKind::ExactInt)
        | (ValueKind::I64 | ValueKind::ExactInt, ValueKind::Rational) => ValueKind::Rational,
        // A Code arm, or a list of family values (`[program]`), beside
        // a node arm (`[v.body]` is a node sequence) joins in the node
        // family, which holds code as a Code node.
        (ValueKind::ExprCode, ValueKind::Node) | (ValueKind::Node, ValueKind::ExprCode) => {
            ValueKind::Node
        }
        (ValueKind::Vector(element), ValueKind::Node)
        | (ValueKind::Node, ValueKind::Vector(element))
            if super::node_family(&element) =>
        {
            ValueKind::Node
        }
        (ValueKind::Vector(left_element), ValueKind::Vector(right_element)) => {
            let joined = join_arm_kinds(*left_element, *right_element);
            if matches!(joined, ValueKind::Other) {
                ValueKind::Other
            } else {
                ValueKind::Vector(Box::new(joined))
            }
        }
        (joined, other) if joined == other => joined,
        _ => ValueKind::Other,
    }
}

pub(super) fn numeric_element_kind(values: &[EmirValue], kinds: &[ValueKind]) -> ValueKind {
    fn join(left: ValueKind, right: ValueKind) -> ValueKind {
        match (left, right) {
            (ValueKind::Other, right) => right,
            (ValueKind::I64, ValueKind::ExactInt) | (ValueKind::ExactInt, ValueKind::I64) => {
                ValueKind::ExactInt
            }
            (ValueKind::I64 | ValueKind::ExactInt, ValueKind::Rational)
            | (ValueKind::Rational, ValueKind::I64 | ValueKind::ExactInt) => ValueKind::Rational,
            (ValueKind::Vector(left), ValueKind::Vector(right)) => {
                ValueKind::Vector(Box::new(join(*left, *right)))
            }
            (left, _) => left,
        }
    }
    values
        .iter()
        .map(|value| kind_at(kinds, *value))
        .fold(ValueKind::Other, join)
}

pub(super) fn kind_of_op(
    op: &EmirOp,
    kinds: &[ValueKind],
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
    call_self_kind: Option<&ValueKind>,
) -> ValueKind {
    match op {
        EmirOp::ConstI64(_)
        | EmirOp::VectorLength(_)
        | EmirOp::ToInt(_)
        | EmirOp::MatrixRows(_)
        | EmirOp::MatrixCols(_) => ValueKind::I64,
        EmirOp::ConstExactInt(_) => ValueKind::ExactInt,
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
        EmirOp::RecordCreate { type_name, fields } => {
            if record_template(type_name).is_some() {
                let fields = fields
                    .iter()
                    .map(|(name, value)| (name.clone(), kind_at(kinds, *value)))
                    .collect::<Vec<_>>();
                return infer_instantiation(type_name, &fields)
                    .map_or(ValueKind::Other, ValueKind::Record);
            }
            // A node-family tag or node record (not an authored
            // object layout) constructs a dynamic node value; the
            // emitter's admission gate guarantees only authored
            // objects and node tags reach here.
            if record_layout(type_name).is_none()
                && emath_exec_ir::constructor_layer::is_node_tag(type_name)
            {
                ValueKind::Node
            } else {
                ValueKind::Record(type_name.clone())
            }
        }
        EmirOp::RecordField { record, field } => {
            // A field off a node value stays in the node family
            // (dynamic records: the field's own kind is a runtime
            // fact); a field off a typed record keeps the declared
            // layout carrier.
            if kind_at(kinds, *record) == ValueKind::Node {
                return ValueKind::Node;
            }
            // The authored `p.length` sugar: a `length` projection
            // off a sequence-shaped receiver (the path lowering
            // emits RecordField for it) is the storage length - the
            // same Int the `length(x)` builtin computes.
            if field == "length"
                && matches!(
                    kind_at(kinds, *record),
                    ValueKind::Vector(_) | ValueKind::Matrix(_) | ValueKind::Tensor
                )
            {
                return ValueKind::I64;
            }
            // A tuple-position projection (`_0`, `_1`, ...) off a
            // sequence-kinded receiver: the positional element
            // carrier - a tuple lowers as a sequence and `_N` indexes
            // it (the VM's tuple field law). A node-family receiver
            // keeps the dynamic family arm above.
            if let Some(index) = field
                .strip_prefix('_')
                .and_then(|text| text.parse::<usize>().ok())
            {
                if let ValueKind::Vector(element) = kind_at(kinds, *record) {
                    let _ = index;
                    return *element.clone();
                }
            }
            // A part projection off the Rational carrier carries the
            // exact-integer parts (`CValue::Rat { num, den }`); the
            // emitter's Rational carrier is the `(i128, i128)` tuple.
            if kind_at(kinds, *record) == ValueKind::Rational {
                return match field.as_str() {
                    "numer" | "denom" => ValueKind::ExactInt,
                    _ => ValueKind::Other,
                };
            }
            let ValueKind::Record(name) = kind_at(kinds, *record) else {
                return ValueKind::Other;
            };
            record_layout(&name)
                .and_then(|fields| fields.into_iter().find(|(name, _)| name == field))
                .map_or(ValueKind::Other, |(_, ty)| ValueKind::from_signature(&ty))
        }
        EmirOp::Refuse(_) | EmirOp::RefuseValue(_) => ValueKind::Never,
        // Typed closure literal: the parameter-domain signature plus
        // the body's inferred result compose the callable kind. An
        // empty signature is the numeric/VM-converted carrier
        // (Program), which renders through the dyn-program ABI.
        EmirOp::ProgramLiteral {
            body,
            captures,
            signature,
            ..
        } => {
            if signature.is_empty() {
                return ValueKind::Program;
            }
            let param_kind = ValueKind::from_signature(&bound_signature(signature));
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
        // The quoted-template carrier. A FUNCTION template is open
        // code over its declared scalar carrier (a CodeLiteral or a
        // further partial application stays a Code value; only the
        // guarded executor yields a callable). An EXPRESSION
        // template (param None) is the union lane: an ExprCode value
        // whose executor yields the computed CodeValue scalar.
        EmirOp::CodeLiteral { param, carrier, .. } => {
            if param.is_none() {
                ValueKind::ExprCode
            } else {
                ValueKind::Code(Box::new(ValueKind::from_signature(carrier)))
            }
        }
        EmirOp::CodeSubstitute { code, .. } => match kind_at(kinds, *code) {
            kind @ ValueKind::Code(_) => kind,
            ValueKind::ExprCode | ValueKind::Node => ValueKind::ExprCode,
            _ => ValueKind::Other,
        },
        // quote.view: a Code (or a Fragment node value) opens into
        // the node-record family over the embedded tree.
        EmirOp::CodeView { .. } => ValueKind::Node,
        // quote.make: a node-record tree (or a Code passthrough)
        // rebuilds into the dual-representation Code value.
        EmirOp::CodeMake { .. } => ValueKind::ExprCode,
        // quote.body: the definition-table unfold yields the
        // Available/Opaque body record (the node family).
        EmirOp::CodeBody { .. } => ValueKind::Node,
        // quote.open: the witness-validated unwrap yields the opened
        // term as the dual-representation Code value.
        EmirOp::CodeOpen { .. } => ValueKind::ExprCode,
        // quote.bind (call form): the mint walk (identity over the
        // distilled subset) with the snapshot re-stamped - still the
        // dual-representation Code value.
        EmirOp::CodeBind { .. } => ValueKind::ExprCode,
        EmirOp::CodeIdentity { .. } => ValueKind::I64,
        // quote.evaluate: closed code yields the specialized unary
        // closure over the template's carrier (a def bound to it is
        // callable), or - for an expression template - the computed
        // union scalar, projected at the typed boundary.
        // An expression template whose value is APPLIED is a quoted
        // function literal: a callable over the value union, one union
        // parameter per applied argument, yielding the union.
        // Passed into a declared arrow, it carries that arrow's
        // signature when the render can adapt it (scalar parameters and
        // result); otherwise the carrier is unknown and the render
        // refuses by name.
        EmirOp::CodeEvaluate { code, applied } => match kind_at(kinds, *code) {
            ValueKind::Code(carrier) => ValueKind::Closure {
                params: vec![carrier.as_ref().clone()],
                result: carrier,
            },
            ValueKind::ExprCode => match applied {
                Some(emath_exec_ir::CodeApplication::Union(arity)) => ValueKind::Closure {
                    params: vec![ValueKind::CodeValue; usize::from(*arity)],
                    result: Box::new(ValueKind::CodeValue),
                },
                Some(emath_exec_ir::CodeApplication::Declared(signature)) => {
                    declared_scalar_arrow(signature).unwrap_or(ValueKind::Other)
                }
                None => ValueKind::CodeValue,
            },
            _ => ValueKind::Other,
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
        // A self-recursive call returns the enclosing function's OWN
        // result, and the register's kind must mirror the render's
        // crossing decision exactly: when the declared output and
        // the body's instantiated carrier differ and a numeric
        // boundary exists between them, the render crosses the call
        // to the declared lane (`ExactInt::from` / E-INT-002), so the
        // register carries the declared kind; when no boundary exists
        // (a generic instantiation like tabulate_at's
        // `sequence(Rat)` at an `Int -> sequence(Rat)` closure), the
        // render emits the body's carrier raw and the register
        // carries the body kind. Degenerate bodies keep the declared
        // carrier; the first-argument fallback only serves the recur
        // lane, whose result carrier is not declared here.
        EmirOp::CallSelf { inputs, result } => {
            record_self_call_args(inputs.iter().map(|value| kind_at(kinds, *value)));
            let declared = if result.is_empty() {
                ValueKind::Other
            } else {
                ValueKind::from_signature(result)
            };
            if let Some(body_kind) = call_self_kind {
                if !matches!(body_kind, ValueKind::Other) {
                    if !matches!(declared, ValueKind::Other)
                        && numeric_boundary_value(
                            &Expr::Raw(String::new()),
                            body_kind,
                            &declared,
                            "",
                        )
                        .is_some()
                    {
                        return declared;
                    }
                    return body_kind.clone();
                }
            }
            if !matches!(declared, ValueKind::Other) {
                return declared;
            }
            // An unresolved self call (inside a branch arm, whose own
            // walk cannot see the enclosing body): the wrapper being
            // rendered has decided its carrier. The provisional walk
            // (`call_self_kind` set) never reads it, so a frame's own
            // carrier is never decided from an enclosing wrapper.
            if call_self_kind.is_none()
                && let Some(wrapper) = self_wrapper_kind()
                && !kind_is_degenerate(&wrapper)
            {
                return wrapper;
            }
            if call_self_kind == Some(&ValueKind::Other) {
                // Provisional: the first-argument guess would leak an
                // argument's carrier (a closure `f`) into the body's
                // resolved kind; stay join-neutral.
                return ValueKind::Other;
            }
            inputs
                .first()
                .map_or(ValueKind::I64, |value| kind_at(kinds, *value))
        }
        // A sibling entry call (a recursion-cycle edge) renders as a
        // call to the callee's emitted entry fn, whose result type is
        // the callee's inferred body carrier - so the register
        // carries THAT, not the authored declaration: the two
        // disagree when the callee's body joins mixed-kind arms (a
        // Rat-typed inlined carrier against an Int-declared recursive
        // tail emits the ratio carrier). Without a pre-lowered table
        // entry the declared signature stands.
        EmirOp::CallSibling { name, result, .. } => {
            if let Some(kind) = sibling_entry_kind(name) {
                return kind;
            }
            if !result.is_empty() {
                let declared = ValueKind::from_signature(result);
                if !matches!(declared, ValueKind::Other) {
                    return declared;
                }
            }
            ValueKind::Other
        }
        EmirOp::CallFrame {
            body,
            inputs,
            state,
            declared,
            ..
        } => {
            let names = (0..inputs.len())
                .map(|index| format!("__frame_input_{index}"))
                .collect::<Vec<_>>();
            let states = (0..state.len())
                .map(|index| format!("__frame_state_{index}"))
                .collect::<Vec<_>>();
            let arg_kinds = inputs.iter().map(|value| kind_at(kinds, *value)).collect::<Vec<_>>();
            let _types = TypeBindingScope::enter(&arg_kinds, declared);
            let declared = instantiate_declared(&arg_kinds, declared);
            let mut frame = names
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
            refine_recursive_frame_kinds(body, &names, &states, &mut frame);
            with_self_call_scope(|| program_kind(body, &names, &states, &frame)).0
        }
        EmirOp::SameDenseShape(..) => ValueKind::Bool,
        EmirOp::ToF64(_) => ValueKind::F64,
        EmirOp::DenseLayout(value) => ValueKind::DenseLayout(Box::new(kind_at(kinds, *value))),
        EmirOp::VectorSlice { .. } | EmirOp::VectorConcat(_) | EmirOp::F64SortTotal(_) => {
            ValueKind::Vector(Box::new(ValueKind::F64))
        }
        // Authored cons preserves the element carrier (records stay
        // records), unlike the Float64 dense-lane concat above.
        EmirOp::ListConcat(values) => {
            // A concat with any node-family operand stays in the
            // family (the VM's node-sequence law): typed-sequence
            // operands fold into the family at the render.
            if values.iter().any(|value| kind_at(kinds, *value) == ValueKind::Node) {
                return ValueKind::Node;
            }
            match numeric_element_kind(values, kinds) {
                ValueKind::Vector(element) => ValueKind::Vector(element),
                other => ValueKind::Vector(Box::new(other)),
            }
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
            // A self-recursive tail (`CallSelf` as the body result)
            // returns the enclosing function's own kind - the sibling
            // arm already states it, so the join takes the other side
            // instead of mismatching into `Other`. A degenerate side
            // (an empty `[]` literal carries no element kind) yields
            // to the other side's concrete kind: `if done: [] else:
            // moves(k)` joins to the moves carrier.
            branch_carrier(
                left,
                right,
                result_is_call_self(then_body),
                result_is_call_self(else_body),
            )
        }
        EmirOp::ListCreate(values)
        | EmirOp::SetCreate {
            elements: values, ..
        } => {
            // A list with any node-family element joins the node
            // family (its dynamic sequence): the walk's `[code, arg]`
            // argument lists mix Code and node values, exactly as
            // the VM's heterogeneous CValue sequences do.
            if values
                .iter()
                .any(|value| kind_at(kinds, *value) == ValueKind::Node)
            {
                return ValueKind::Node;
            }
            ValueKind::Vector(Box::new(if matches!(op, EmirOp::ListCreate(_)) {
                numeric_element_kind(values, kinds)
            } else {
                values
                    .first()
                    .map_or(ValueKind::Other, |value| kind_at(kinds, *value))
            }))
        }
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
            // Indexing a node sequence (the walk's `node.args[0]`)
            // stays in the node family.
            ValueKind::Node => ValueKind::Node,
            // A concrete non-sequence carrier cannot be indexed: the
            // VM's `vector_of` type confusion (op `vector-index`)
            // faults, so the op never yields and the Never kind lets
            // every consumer join absorb the refusal. `Other` keeps
            // the float-index lane - an uninferred kind may still be
            // a sequence at runtime.
            receiver if kind_is_never_indexable(&receiver) => ValueKind::Never,
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
        EmirOp::BinaryBuiltin(..) | EmirOp::MatrixIndex { .. } | EmirOp::TensorIndex { .. } => {
            ValueKind::F64
        }
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
                    .map_or(ValueKind::Other, |(_, output)| {
                        ValueKind::from_signature(output)
                    });
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
            // The union lane: division over the union stays the
            // union (the kernel's result is ALWAYS Rational - the
            // VM's never-collapse rule for division).
            if union_pair(kinds, *left, *right) {
                ValueKind::CodeValue
            } else if kind_at(kinds, *left) == ValueKind::Complex
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
            // A Never operand (an op that always refuses - the
            // `vector-index` type confusion) never yields a value, so
            // the surviving operand's carrier wins; the render
            // propagates the refusal expression through its own lane.
            if kind_at(kinds, *left) == ValueKind::Never {
                kind_at(kinds, *right)
            } else if kind_at(kinds, *right) == ValueKind::Never {
                kind_at(kinds, *left)
            } else if union_pair(kinds, *left, *right) {
                // The expression-template union lane: one union
                // operand with a joinable other routes the op onto
                // the dynamic kernels; the result stays the union
                // until a typed boundary projects it.
                ValueKind::CodeValue
            } else if (kind_at(kinds, *left) == ValueKind::ExactInt
                || kind_at(kinds, *right) == ValueKind::ExactInt)
                && matches!(kind_at(kinds, *left), ValueKind::I64 | ValueKind::ExactInt)
                && matches!(kind_at(kinds, *right), ValueKind::I64 | ValueKind::ExactInt)
            {
                ValueKind::ExactInt
            } else if kind_at(kinds, *left) == ValueKind::I64
                && kind_at(kinds, *right) == ValueKind::I64
            {
                ValueKind::I64
            } else if matches!(kind_at(kinds, *left), ValueKind::Rational)
                && matches!(
                    kind_at(kinds, *right),
                    ValueKind::Rational | ValueKind::I64 | ValueKind::ExactInt
                )
                || matches!(kind_at(kinds, *right), ValueKind::Rational)
                    && matches!(
                        kind_at(kinds, *left),
                        ValueKind::Rational | ValueKind::I64 | ValueKind::ExactInt
                    )
            {
                // Mixed Int/Rational arithmetic: the VM's `as_rat`
                // law - the Int operand widens exactly (`n` becomes
                // `n/1`) and a Rational operand locks the Rational
                // carrier (`x + 1/2` with Int x computes Rat, never
                // a float join).
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
        // The union lane: comparisons and boolean combinators over a
        // joinable union pair compute the union (a Bool-valued
        // expression template yields CodeValue::Bool; the typed
        // boundary projects it). A non-joinable pair (Float64 beside
        // the union) keeps the Bool lane, and the expression-template
        // body check refuses it named.
        EmirOp::Eq(..) | EmirOp::Ne(..) | EmirOp::Lt(..) | EmirOp::Le(..) | EmirOp::Gt(..)
        | EmirOp::Ge(..) => {
            // Comparisons yield Bool in every lane - the union lane's
            // `matches!(code_cmp(..)?, ..)` / `code_eq(..)?` render is
            // a checked bool, exactly like the typed lanes; only the
            // boolean COMBINATORS carry the union result (their
            // `CodeValue::Bool(..)` render keeps the dynamic value).
            ValueKind::Bool
        }
        EmirOp::And(left, right) | EmirOp::Or(left, right) => {
            if union_pair(kinds, *left, *right) {
                ValueKind::CodeValue
            } else {
                ValueKind::Bool
            }
        }
        EmirOp::Not(value) => {
            if matches!(kind_at(kinds, *value), ValueKind::CodeValue) {
                ValueKind::CodeValue
            } else {
                ValueKind::Bool
            }
        }
        EmirOp::IsFinite(_)
        | EmirOp::SameBits(_, _)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..)
        | EmirOp::SetContains { .. }
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

pub(super) fn operand_kind(kinds: &[ValueKind], value: EmirValue) -> ValueKind {
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
        // The VM's mixed Int/Float64 widening (`CValue::Int::to_f64`).
        (ValueKind::ExactInt, ValueKind::F64) => {
            Expr::Raw(format!("({}).to_f64()", render_expr(&expr)))
        }
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

fn exact_comparison(
    op: BinOp,
    program: &EmirProgram,
    left: EmirValue,
    right: EmirValue,
    kinds: &[ValueKind],
) -> Expr {
    // UFCS fixes both argument types to &ExactInt; UFCS arguments do not
    // auto-deref, so each side must be exactly one reference layer.
    let ordering = Expr::Raw(format!(
        "<emath_rt::ExactInt as core::cmp::Ord>::cmp({}, {})",
        exact_int_ref(program, left, kinds),
        exact_int_ref(program, right, kinds)
    ));
    Expr::Bin {
        op,
        left: Box::new(ordering),
        right: Box::new(Expr::Raw("core::cmp::Ordering::Equal".into())),
    }
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
        // The node family (bead emath-shared-tree-view-make-bp8nu):
        // structural comparison over the dynamic records - tags by
        // name, records by type and field set, scalars by VALUE
        // through the union kernels (`node.kind == Call` is the
        // walk's kind test). A scalar beside a node folds into the
        // family (mixed shapes are never equal).
        (ValueKind::Node, _) | (_, ValueKind::Node) => {
            let left = render_expr(&to_node(operand(program, left), &lk));
            let right = render_expr(&to_node(operand(program, right), &rk));
            let ordered = |orderings: &str| {
                format!(
                    "matches!(emath_rt::code_tree::node_cmp(&{left}, &{right})?, {orderings})"
                )
            };
            let raw = match op {
                BinOp::Eq => format!("({left}).equals(&{right})?"),
                BinOp::Ne => format!("!({left}).equals(&{right})?"),
                BinOp::Lt => ordered("core::cmp::Ordering::Less"),
                BinOp::Gt => ordered("core::cmp::Ordering::Greater"),
                BinOp::Le => ordered("core::cmp::Ordering::Less | core::cmp::Ordering::Equal"),
                BinOp::Ge => ordered("core::cmp::Ordering::Greater | core::cmp::Ordering::Equal"),
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::Rem
                | BinOp::Pow
                | BinOp::And
                | BinOp::Or => unreachable!("cmp_expr only emits comparison BinOps"),
            };
            Expr::Raw(raw)
        }
        // The union lane: comparisons route through the dynamic
        // kernels - exact cross-multiplication ordering, VALUE
        // equality (`2 == 2/1` is true), mixed kinds never equal.
        // The other side is Int/Rat/Bool/union, or an exact Int that
        // narrows checked onto the union's i64 (`to_code_value`).
        (ValueKind::CodeValue, _) | (_, ValueKind::CodeValue) => {
            let left = to_code_value(operand(program, left), &lk);
            let right = to_code_value(operand(program, right), &rk);
            let (left, right) = (render_expr(&left), render_expr(&right));
            let raw = match op {
                BinOp::Eq => format!("emath_rt::code::code_eq(&{left}, &{right})?"),
                BinOp::Ne => format!("!emath_rt::code::code_eq(&{left}, &{right})?"),
                BinOp::Lt => format!(
                    "matches!(emath_rt::code::code_cmp(&{left}, &{right})?, core::cmp::Ordering::Less)"
                ),
                BinOp::Gt => format!(
                    "matches!(emath_rt::code::code_cmp(&{left}, &{right})?, core::cmp::Ordering::Greater)"
                ),
                BinOp::Le => format!(
                    "matches!(emath_rt::code::code_cmp(&{left}, &{right})?, core::cmp::Ordering::Less | core::cmp::Ordering::Equal)"
                ),
                BinOp::Ge => format!(
                    "matches!(emath_rt::code::code_cmp(&{left}, &{right})?, core::cmp::Ordering::Greater | core::cmp::Ordering::Equal)"
                ),
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::Rem
                | BinOp::Pow
                | BinOp::And
                | BinOp::Or => unreachable!("cmp_expr only emits comparison BinOps"),
            };
            Expr::Raw(raw)
        }
        (ValueKind::I64, ValueKind::I64) | (ValueKind::Bool, ValueKind::Bool) => Expr::Bin {
            op,
            left: Box::new(operand(program, left)),
            right: Box::new(operand(program, right)),
        },
        // ExactInt operands can render in mixed borrow states (an
        // owned machine-call result beside a borrowed capture
        // passthrough); binary `==`/`<` need both sides in one state,
        // so both materialize owned at this boundary (the
        // `owned_operand` clone idiom).
        (ValueKind::ExactInt, ValueKind::ExactInt) => {
            exact_comparison(op, program, left, right, kinds)
        }
        (ValueKind::ExactInt, ValueKind::I64) | (ValueKind::I64, ValueKind::ExactInt) => {
            exact_comparison(op, program, left, right, kinds)
        }
        // One representation for mixed Rat/Int comparisons: the Int
        // side widens to canonical ratio parts — equality compares
        // the tuples elementwise, order cross-multiplies through
        // `ratio_lt` (denominators are canonically positive). Never
        // an f64 roundtrip: the blanket Int arms below would cast a
        // Rational tuple through `as f64` (E0308 against (i128,i128),
        // and a 2^53 lie even where it compiled).
        (ValueKind::Rational, ValueKind::I64) | (ValueKind::I64, ValueKind::Rational) => {
            let (rat, int) = if lk == ValueKind::Rational {
                (left, right)
            } else {
                (right, left)
            };
            let rat_e = render_expr(&operand(program, rat));
            let int_e = render_expr(&operand(program, int));
            let int_ratio = format!("((({int_e}) as i128), 1i128)");
            let raw = match op {
                BinOp::Eq => format!("{rat_e} == {int_ratio}"),
                BinOp::Ne => format!("{rat_e} != {int_ratio}"),
                BinOp::Lt => format!("emath_rt::ratio_lt({rat_e}, {int_ratio})?"),
                BinOp::Gt => format!("emath_rt::ratio_lt({int_ratio}, {rat_e})?"),
                BinOp::Le => format!("!emath_rt::ratio_lt({int_ratio}, {rat_e})?"),
                BinOp::Ge => format!("!emath_rt::ratio_lt({rat_e}, {int_ratio})?"),
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::Rem
                | BinOp::Pow
                | BinOp::And
                | BinOp::Or => unreachable!("cmp_expr only emits comparison BinOps"),
            };
            Expr::Raw(raw)
        }
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
            if matches!(
                &lk,
                ValueKind::I64
                    | ValueKind::Bool
                    | ValueKind::F64
                    | ValueKind::Complex
                    | ValueKind::Rational
            ) && matches!(
                &rk,
                ValueKind::I64
                    | ValueKind::Bool
                    | ValueKind::F64
                    | ValueKind::Complex
                    | ValueKind::Rational
            ) {
                Expr::Bin {
                    op,
                    left: Box::new(operand(program, left)),
                    right: Box::new(operand(program, right)),
                }
            } else {
                // Non-copy carriers compare through a common view: the
                // observed binding renders as a borrow (`&Vec<f64>`)
                // and the expected literal as an owned value
                // (`Vec<f64>`), and `&T == T` has no `PartialEq` impl.
                // Both sides project to the same slice/str view, which
                // compares elementwise exactly like the interp's carrier
                // comparison. Every remaining non-copy carrier (records,
                // closures, payloads) compares through exactly ONE
                // reference layer: a borrowed register (the non-copy load
                // lane) already renders as one, so the `&` is added only
                // for owned operand expressions.
                let view = |kind: &ValueKind, value: EmirValue| {
                    let rendered = render_expr(&operand(program, value));
                    match kind {
                        ValueKind::Text => format!("({rendered}).as_str()"),
                        ValueKind::Tensor => format!("({rendered}).data.as_slice()"),
                        ValueKind::Vector(_) | ValueKind::Matrix(_) => {
                            format!("({rendered}).as_slice()")
                        }
                        _ if super::rtcalls::borrowed_register(program, value, kinds) => rendered,
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

/// Reconcile different representations of admitted numeric values at a fixed
/// native slot. None means this boundary needs no numeric representation change.
pub(super) fn numeric_boundary_value(
    expression: &Expr,
    from: &ValueKind,
    to: &ValueKind,
    slot: &str,
) -> Option<Expr> {
    match (from, to) {
        (ValueKind::ExactInt, ValueKind::I64) => Some(coerce_to_ty(
            expression.clone(),
            ValueKind::ExactInt,
            &Ty::I64,
        )),
        (ValueKind::I64, ValueKind::ExactInt) => Some(Expr::Raw(format!(
            "emath_rt::ExactInt::from({})",
            render_expr(expression)
        ))),
        (ValueKind::I64, ValueKind::Rational) => Some(Expr::Raw(format!(
            "(i128::from({}), 1i128)",
            render_expr(expression)
        ))),
        (ValueKind::ExactInt, ValueKind::Rational) => Some(Expr::Raw(
            super::op_arith::exact_int_ratio_parts(&render_expr(expression)),
        )),
        // A ratio narrowing onto the i64 lane carries the emission
        // lane's checked law (E-INT-002 by name): the division must
        // be exact and the quotient in range. A Rat-computed actual
        // crossing an Int-declared parameter (the replace_at
        // instantiation's ratio arms folding back onto a function's
        // Int-recursive tail) admits exactly the integral values the
        // VM's arbitrary-precision Int lane would carry.
        (ValueKind::Rational, ValueKind::I64) => Some(Expr::Raw(format!(
            "{{ let (__narrow_num, __narrow_den) = {}; if __narrow_den == 0 || __narrow_num % __narrow_den != 0 || !({} <= __narrow_num / __narrow_den && __narrow_num / __narrow_den <= {}) {{ return Err(String::from(\"E-INT-002: ratio result does not narrow exactly onto the i64 lane\")); }} (__narrow_num / __narrow_den) as i64 }}",
            render_expr(expression),
            i64::MIN as i128,
            i64::MAX as i128,
        ))),
        // The union collapses onto a declared carrier through the
        // checked projections - the same law the declared output
        // boundary applies, at record fields, sequence elements, and
        // branch arms. The refusal names the slot and the found
        // value; an Int widens into Rat/ExactInt exactly.
        (ValueKind::CodeValue, ValueKind::I64) => Some(Expr::Raw(format!(
            "emath_rt::code::project_i64(&{}, {slot:?})?",
            render_expr(expression)
        ))),
        (ValueKind::CodeValue, ValueKind::Rational) => Some(Expr::Raw(format!(
            "emath_rt::code::project_ratio(&{}, {slot:?})?",
            render_expr(expression)
        ))),
        (ValueKind::CodeValue, ValueKind::ExactInt) => Some(Expr::Raw(format!(
            "emath_rt::code::project_exact_int(&{}, {slot:?})?",
            render_expr(expression)
        ))),
        (ValueKind::CodeValue, ValueKind::Bool) => Some(Expr::Raw(format!(
            "emath_rt::code::project_bool(&{}, {slot:?})?",
            render_expr(expression)
        ))),
        (ValueKind::Vector(source), ValueKind::Vector(target)) => {
            let item = Expr::Raw(
                if source.is_copy() {
                    "*__numeric"
                } else {
                    "__numeric"
                }
                .into(),
            );
            let converted = numeric_boundary_value(&item, source, target, slot)?;
            Some(Expr::Raw(format!(
                "({}).iter().map(|__numeric| -> Result<_, String> {{ Ok({}) }}).collect::<Result<Vec<_>, String>>()?",
                render_expr(expression),
                render_expr(&converted)
            )))
        }
        _ => None,
    }
}

pub(crate) fn coerce_to_ty(expr: Expr, from: ValueKind, to: &Ty) -> Expr {
    match (from, to) {
        (ValueKind::I64, Ty::F64) => as_f64(expr),
        (ValueKind::F64, Ty::I64) => as_i64(expr),
        // A fixed native i64 slot is narrower than the VM's arbitrary-precision
        // Int. Refuse an out-of-lane value rather than truncating it.
        (ValueKind::ExactInt, Ty::I64) => Expr::Raw(format!(
            "({}).to_i64().ok_or_else(|| String::from(\"E-INT-002: exact integer result exceeds the i64 lane\"))?",
            render_expr(&expr)
        )),
        _ => expr,
    }
}

pub(crate) fn value_expr(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
) -> Result<Expr, BackendError> {
    // Infer the register table once for this body and input context, not once
    // per rendered operation. Nested bodies still infer their own contexts.
    let kinds = value_kinds(program, names, states, input_kinds);
    let result_kind = kind_at(&kinds, program.result);
    if program.ops.len() == 1 {
        let expression = op_expr(
            &program.ops[0].0,
            program,
            names,
            states,
            input_kinds,
            &kinds,
        )?;
        return owned_result(program, expression, &result_kind);
    }
    let flat = flat_ssa(program, names, states, input_kinds, &kinds)?;
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
        &result_kind,
    )?));
    Ok(Expr::Block(Box::new(Stmt::Block(Block { statements }))))
}

fn owned_result(
    program: &EmirProgram,
    expression: Expr,
    kind: &ValueKind,
) -> Result<Expr, BackendError> {
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
        Ok(owned_value(expression, kind))
    } else {
        Ok(expression)
    }
}
