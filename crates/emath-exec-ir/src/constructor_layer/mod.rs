//! Constructor-layer evaluation: object, function, recur, quote, query.
//!
//! Reuses the shared syntax tree and scalar carrier arithmetic. Mathematical
//! recipes are ordinary module functions, not image `FeatureIDs`.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use emath_core::tree::{
    BinaryOp, Declaration, Expr, ExprKind, GenericArg, Item, StmtKind, SyntaxTree, TypeExpr,
    TypeKind, UnaryOp, UseTree,
};

use crate::exact_int::{
    ExactError, ExactInt, exact_int_hamming, exact_int_poly_eval, exact_int_prod,
    exact_int_prod_from, exact_int_sum, exact_int_sum_from, exact_int_weighted_prod,
};

const DEFAULT_WORK: u64 = 1_000_000;

#[derive(Clone, Debug)]
pub enum CValue {
    Bool(bool),
    Int(ExactInt),
    Rat {
        num: ExactInt,
        den: ExactInt,
    },
    Float64(f64),
    /// Text payload carrier (fbpb6 successor bead e6gvs): a Str literal
    /// evaluates as a first-class value. Structural equality only —
    /// the carrier exists without the string LIBRARY (no ordering,
    /// concatenation, length, or indexing in this cut; the recorded
    /// stance is that the carrier comes first and the library is a
    /// deliberate boundary).
    Str(String),
    /// Dense sequence carrier. Copy-on-write: `Clone` shares the
    /// backing storage (the CPS kont bookkeeping clones argument
    /// values at every engine step, so a deep-copy clone made every
    /// big-sequence call O(len) per step — 17.4s wall for ~13k indexed
    /// reads in the pre-repair carrier); mutation goes through
    /// `Arc::make_mut`, which copies only when shared. `Arc` (not
    /// `Rc`) keeps the carrier `Send` for lanes that move values
    /// across threads. Structural `PartialEq` is unchanged by sharing.
    Sequence(Arc<Vec<CValue>>),
    /// In-place mutable buffer carrier:
    /// `buffer(size, fill)` constructs it, `buffer_set`
    /// writes through shared references, `xs[i]` reads, `.length`
    /// projects. `Arc<Mutex<..>>` (not `Rc<RefCell>`) keeps the carrier
    /// `Send` for lanes that move values across threads. Equality
    /// refuses: mutable state has no total value equality.
    Buffer(Arc<Mutex<Vec<CValue>>>),
    Tuple(Vec<CValue>),
    Record {
        type_name: String,
        /// Copy-on-write record fields (the `Sequence` precedent): the
        /// CPS machine clones argument values at every engine step, so
        /// a deep `BTreeMap` clone made every record-typed call
        /// O(fields) allocations per step. Records are immutable after
        /// construction in the language, so sharing is exact; mutation
        /// sites (none today) would go through `Arc::make_mut`.
        fields: Arc<BTreeMap<String, CValue>>,
    },
    Variant {
        type_name: String,
        tag: String,
        fields: Vec<CValue>,
    },
    Closure(Box<Closure>),
    Code(Box<Code>),
    Receipt(Box<Receipt>),
    Unit,
    Absent,
}

/// Structural equality, unchanged by sequence sharing (bead
/// emath-g9rpo). Buffers compare by cell identity only: mutable state
/// has no total value equality — the language
/// surface refuses `==`/`!=` on buffers; this identity fallback
/// exists so internal comparisons stay total, never as a user-facing
/// value equality.
impl PartialEq for CValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Rat { num: an, den: ad }, Self::Rat { num: bn, den: bd }) => {
                an == bn && ad == bd
            }
            (Self::Float64(a), Self::Float64(b)) => a == b,
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::Sequence(a), Self::Sequence(b)) => a == b,
            (Self::Buffer(a), Self::Buffer(b)) => Arc::ptr_eq(a, b),
            (Self::Tuple(a), Self::Tuple(b)) => a == b,
            (
                Self::Record {
                    type_name: at,
                    fields: af,
                },
                Self::Record {
                    type_name: bt,
                    fields: bf,
                },
            ) => at == bt && af == bf,
            (
                Self::Variant {
                    type_name: at,
                    tag: ag,
                    fields: af,
                },
                Self::Variant {
                    type_name: bt,
                    tag: bg,
                    fields: bf,
                },
            ) => at == bt && ag == bg && af == bf,
            (Self::Closure(a), Self::Closure(b)) => a == b,
            (Self::Code(a), Self::Code(b)) => a == b,
            (Self::Receipt(a), Self::Receipt(b)) => a == b,
            (Self::Unit, Self::Unit) => true,
            (Self::Absent, Self::Absent) => true,
            _ => false,
        }
    }
}

/// Declared domain shape of a closure value, extracted from the
/// `function x in <domain>` spelling (or the recur binder's arrow type)
/// at construction. Carries element information only where the tag
/// spells it; every other form stays [`DomainShape::Unknown`] — nothing
/// is reconstructed that the tag does not carry (the `infer_path`
/// discipline).
#[derive(Clone, Debug, PartialEq)]
pub enum DomainShape {
    /// Scalar carrier spelling: `Int`, `Rat`, `Bool`, `Float64`.
    Scalar(String),
    /// Sequence carrier: the element shape when the spelling carries it
    /// (`sequence(Rat)`), element-blind otherwise.
    Sequence(Option<Box<DomainShape>>),
    /// Domain not reconstructible from the tag: the closure admits with
    /// no claim (the carrier-only discipline this seam replaces for
    /// reconstructible shapes).
    Unknown,
}

impl fmt::Display for DomainShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scalar(name) => write!(f, "{name}"),
            Self::Sequence(None) => write!(f, "sequence"),
            Self::Sequence(Some(element)) => write!(f, "sequence({element})"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Closure {
    pub param: String,
    /// The declared domain of the binder, recorded at construction so
    /// the admission lane (`admit_input_type`) and the runtime lane
    /// (`apply_closure_chain`) check the SAME shape — the two lanes can
    /// never disagree about what the closure demands of its argument.
    pub domain: DomainShape,
    pub body: Expr,
    pub env: BTreeMap<String, CValue>,
    pub recursive: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Code {
    pub expr: Expr,
    /// Capture-time dependency stamps: free callable name -> identity
    /// stamp of the resolved declaration at quote time. Unstamped
    /// (empty) entries are synthetic code values (quote.make /
    /// quote.substitute products built outside a capture site); their
    /// free callables resolve against the current table without a
    /// staleness claim. A stamped entry that resolves differently at
    /// evaluation refuses `stale_dependency` rather than silently
    /// re-resolving.
    pub deps: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub execution: String,
    pub fulfillment: String,
    pub representation: String,
    pub payload: Option<String>,
    pub evidence: Vec<String>,
    pub remaining: Vec<String>,
    pub diagnostic_code: Option<String>,
}

impl fmt::Display for CValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(v) => write!(f, "{v}"),
            Self::Int(v) => write!(f, "{v}"),
            Self::Rat { num, den } => write!(f, "{num}/{den}"),
            Self::Float64(v) => write!(f, "{v}"),
            Self::Str(text) => write!(f, "{text}"),
            // Contents are state, not a value: display the shape only,
            // never a snapshot that invites value-style comparison.
            Self::Buffer(cell) => {
                let len = cell
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .len();
                write!(f, "buffer({len})")
            }
            Self::Sequence(xs) => {
                write!(f, "[")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, "]")
            }
            Self::Tuple(xs) => {
                write!(f, "(")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, ")")
            }
            Self::Record { type_name, fields } => {
                write!(f, "{type_name}(")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}={v}")?;
                }
                write!(f, ")")
            }
            Self::Variant {
                type_name,
                tag,
                fields,
            } => {
                write!(f, "{type_name}.{tag}(")?;
                for (i, v) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, ")")
            }
            Self::Closure(_) => write!(f, "<closure>"),
            Self::Code(_) => write!(f, "<code>"),
            Self::Receipt(r) => write!(
                f,
                "receipt(execution={}, fulfillment={}, representation={})",
                r.execution, r.fulfillment, r.representation
            ),
            Self::Unit => write!(f, "()"),
            Self::Absent => write!(f, "Absent"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructorError {
    pub code: String,
    pub message: String,
}

impl fmt::Display for ConstructorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// The single constructor-fault → language-diagnostic-code mapping.
///
/// Every lane that surfaces a [`ConstructorError`] as a diagnostic (the
/// sema admission pass and the CLI build lanes) goes through this one
/// table; the private per-lane copies it replaced had divergent arms
/// and defaults, so the same fault could surface as different codes
/// depending on the reporting lane.
///
/// The mapping follows the diagnostics contract: unbound names are
/// `E-TYPE-002`; faults where no method, implementation, transform, or
/// dependency resolves are `E-TYPE-003` (unknown function); and the
/// carrier/arity/value family — `type`, `arity`, `division_by_zero`,
/// `overflow`, `non_finite_scalar`, `invalid_index`, `invalid_literal`,
/// `nonexhaustive_match`, `invalid_code_construction`,
/// `object_invariant_failed`, `unguarded_recursive_binding`,
/// `recursion_depth_exceeded`, `incompatible_checkpoint`, and any future
/// fault — is the documented "carrier or condition does not match"
/// `E-TYPE-012`. Faults that already carry a language diagnostic code
/// pass through unchanged (the closed set is enumerated because the
/// return is a `&'static str`).
#[must_use]
pub fn constructor_admit_code(code: &str) -> &'static str {
    match code {
        "E-KIND-GONE" => "E-KIND-GONE",
        "E-KIND-011" => "E-KIND-011",
        "E-SEC-101" => "E-SEC-101",
        "E-NAME-020" => "E-NAME-020",
        "E-NAME-022" => "E-NAME-022",
        "E-PKG-050" => "E-PKG-050",
        "E-USE-ADMISSION" => "E-USE-ADMISSION",
        "E-TYPE-002" => "E-TYPE-002",
        "E-TYPE-003" => "E-TYPE-003",
        "E-TYPE-010" => "E-TYPE-010",
        "unbound" | "unbound_code" => "E-TYPE-002",
        "method_unavailable"
        | "implementation_unavailable"
        | "transformation_rule_unavailable"
        | "unresolved"
        | "stale_dependency" => "E-TYPE-003",
        _ => "E-TYPE-012",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestObservation {
    pub label: String,
    pub passed: bool,
    pub detail: String,
    pub receipt: Option<Receipt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleReport {
    pub bindings: BTreeMap<String, CValue>,
    pub tests: Vec<TestObservation>,
    pub uses: Vec<String>,
}

#[derive(Clone)]
struct FnDecl {
    inputs: Vec<String>,
    input_types: Vec<TypeExpr>,
    outputs: Vec<String>,
    output_types: Vec<TypeExpr>,
    output: Option<String>,
    defs: Vec<(String, Expr)>,
    opaque: bool,
}

#[derive(Clone)]
struct QueryDecl {
    inputs: Vec<String>,
    defs: Vec<(String, Expr)>,
    form: String,
    method: Option<String>,
    accept: Option<String>,
}

/// Remaining-work continuation slot. A budget stop keeps these with the
/// frame; resume applies them instead of replaying completed reductions.
#[derive(Clone, Debug, PartialEq)]
enum EvalTail {
    Value(CValue),
    Call {
        name: String,
        args: Vec<CValue>,
    },
    /// A tail call whose callee resolves to a closure VALUE in the
    /// environment (the recur lane's self-name, or any local
    /// closure): the application chain consumes it with frame reuse.
    Apply {
        callee: CValue,
        args: Vec<CValue>,
    },
}

/// Remaining-work continuation slot. A budget stop keeps these with the
/// frame; resume applies them instead of replaying completed reductions.
#[derive(Clone, Debug, PartialEq)]
pub enum Kont {
    BinLeft {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    BinRight {
        op: BinaryOp,
        left: CValue,
        right: Box<Expr>,
    },
    IfAfterCond {
        condition: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    IfThen {
        then_value: Box<Expr>,
    },
    IfElse {
        else_value: Box<Expr>,
    },
    CallArgs {
        callee: CValue,
        done: Vec<CValue>,
        rest: Vec<Rc<Expr>>,
    },
    FnCall {
        name: String,
        done: Vec<CValue>,
        rest: Vec<Rc<Expr>>,
    },
    SeqItems {
        as_tuple: bool,
        done: Vec<CValue>,
        rest: Vec<Rc<Expr>>,
    },
    RecordFields {
        type_path: Vec<String>,
        done: BTreeMap<String, CValue>,
        current: String,
        current_expr: Box<Expr>,
        rest: Vec<(String, Expr)>,
    },
    IndexAfterSeq {
        seq: CValue,
        index: Box<Expr>,
    },
    UnaryAfter {
        op: UnaryOp,
        value: Box<Expr>,
    },
    ConsLeft {
        head: Box<Expr>,
        tail: Box<Expr>,
    },
    ConsAfterHead {
        head: CValue,
        tail: Box<Expr>,
    },
    MatchWaiting {
        subject: Box<Expr>,
        arms: Vec<(Expr, Expr)>,
        else_arm: Box<Expr>,
    },
    CasesArm {
        cond: Box<Expr>,
        value: Box<Expr>,
        rest: Vec<(Expr, Expr)>,
        else_arm: Box<Expr>,
    },
    OpenAfterPacked {
        type_name: String,
        param: String,
        packed: Box<Expr>,
        body: Box<Expr>,
    },
    EvalExpr {
        expr: Rc<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContinuationFrame<E = BTreeMap<String, CValue>> {
    pub function: String,
    pub pc: u64,
    /// Next unfinished definition or instruction label in this frame.
    pub next: String,
    pub env: E,
    pub kont: Vec<Kont>,
}
impl<E> ContinuationFrame<E> {
    fn map_environment<T>(self, map: impl FnOnce(E) -> T) -> ContinuationFrame<T> {
        ContinuationFrame {
            function: self.function,
            pc: self.pc,
            next: self.next,
            env: map(self.env),
            kont: self.kont,
        }
    }
}

/// Interpreter-local copy-on-write bindings. Snapshot timing is unchanged;
/// only a mutation copies bindings shared with a saved environment or frame.
#[derive(Clone, Debug, Default, PartialEq)]
struct Environment(Rc<BTreeMap<String, CValue>>);

impl Environment {
    fn into_map(self) -> BTreeMap<String, CValue> {
        Rc::unwrap_or_clone(self.0)
    }

    fn clear(&mut self) {
        self.0 = Rc::default();
    }
}

impl From<BTreeMap<String, CValue>> for Environment {
    fn from(values: BTreeMap<String, CValue>) -> Self {
        Self(Rc::new(values))
    }
}

impl std::ops::Deref for Environment {
    type Target = BTreeMap<String, CValue>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Environment {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Rc::make_mut(&mut self.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Checkpoint {
    pub schema: String,
    pub source_id: String,
    pub image: String,
    pub abi: String,
    pub work: u64,
    pub remaining: u64,
    pub accounting: String,
    pub memo: BTreeMap<String, CValue>,
    pub frames: Vec<ContinuationFrame>,
    pub scopes: BTreeSet<u64>,
    pub function: String,
    pub source: String,
    pub inputs: BTreeMap<String, CValue>,
    pub next_ref: u64,
}

pub const CHECKPOINT_SCHEMA: &str = "emath.constructor-checkpoint.v2";
pub const CHECKPOINT_ABI: &str = "constructor-layer/continuation-abi";
pub const ACCOUNTING_VERSION: &str = "constructor-layer/accounting.v1";
pub const IMAGE_IDENTITY: &str = "constructor-layer";
const MAX_CALL_DEPTH: u32 = 256;

impl Default for Checkpoint {
    fn default() -> Self {
        Self {
            schema: CHECKPOINT_SCHEMA.into(),
            source_id: String::new(),
            image: IMAGE_IDENTITY.into(),
            abi: CHECKPOINT_ABI.into(),
            work: 0,
            remaining: 0,
            accounting: ACCOUNTING_VERSION.into(),
            memo: BTreeMap::new(),
            frames: Vec::new(),
            scopes: BTreeSet::new(),
            function: String::new(),
            source: String::new(),
            inputs: BTreeMap::new(),
            next_ref: 0,
        }
    }
}

struct Engine {
    env: Environment,
    /// Sharing a declaration avoids copying its body on every tail call.
    functions: BTreeMap<String, Rc<FnDecl>>,
    queries: BTreeMap<String, QueryDecl>,
    objects: BTreeMap<String, ObjectSchema>,
    /// Empty path for local names, resolved import path otherwise. An import
    /// cannot replace a different origin; same-origin diamond imports coalesce.
    name_sources: BTreeMap<String, PathBuf>,
    work: u64,
    work_limit: u64,
    memo: BTreeMap<String, CValue>,
    visit: usize,
    source_id: String,
    entry: String,
    source_text: String,
    inputs: BTreeMap<String, CValue>,
    call_depth: u32,
    next_ref: u64,
    frames: Vec<ContinuationFrame<Environment>>,
    scopes: BTreeSet<u64>,
    resume_frames: Vec<ContinuationFrame<Environment>>,
    /// Only expect rows may resolve unbound single-segment names as fault atoms.
    expect_atoms: Cell<bool>,
}

#[derive(Clone)]
struct ObjectSchema {
    kind: String,
    invariants: Vec<(String, Expr)>,
}

// Split out of the original single file. `prelude` reexports every
// helper at constructor_layer width so the children can share them;
// the `pub use` lines below are the crate-visible surface and match
// the original single-file API.
mod admit;
mod call;
mod checkpoint;
mod engine_step;
mod eval;
mod expr;
mod identity;
mod ops;
mod prelude;
mod query;
mod residual;
mod scalar;
mod scratch;
mod setup;
mod value;

pub use admit::*;
pub use checkpoint::*;
pub use eval::*;
pub use query::*;
pub use residual::*;
pub use scalar::*;
pub use scratch::*;
pub use setup::*;

/// The engine's schema-tag family (view-layout node kinds, scalar-op
/// tags, and the record family's structural names). Shared with
/// emission so bare tag paths and node-record literals admit the
/// same names the VM's layouts produce (the shared-tree bead's
/// no-claim: no new node kinds beyond this family).
pub fn is_node_tag(name: &str) -> bool {
    expr::is_schema_tag(name)
}

/// The module's callable table for artifact emission (the
/// shared-tree lane): every function declaration in the merged tree
/// with its opacity (view's `Global` layouts) and its declaration
/// stamp - the same FNV-1a digest `decl_stamp` computes for the
/// engine's dependency snapshots, so an artifact's stamped
/// dependencies are byte-identical to the VM's.
pub fn module_callable_table(tree: &SyntaxTree) -> Vec<(String, bool, u64)> {
    let mut table = Vec::new();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        if decl.as_kind != "function" {
            continue;
        }
        let opaque = call::function_is_opaque(decl);
        let decl_fn = FnDecl {
            inputs: Vec::new(),
            input_types: Vec::new(),
            outputs: Vec::new(),
            output_types: Vec::new(),
            output: None,
            defs: call::constructor_defs(decl),
            opaque,
        };
        table.push((decl.name.clone(), opaque, identity::decl_stamp(&decl_fn)));
    }
    table
}

/// The module's definition table for artifact emission (the
/// `quote.body` lane, bead emath-quote-body-defs-trto7): every
/// function declaration with its opacity and - for a transparent
/// callee - the distilled body tree exactly as the VM's
/// `function_body_expr` shapes it (inputs wrap the body in function
/// literals, so only binder-free bodies distill). A transparent row
/// outside the distilled subset carries `None`: the compile-time
/// table cannot invent a body the artifact cannot run, and the
/// runtime unfold refuses by name instead (the no-claim boundary).
/// Opaque rows carry no body at all - the body is never exposed.
pub fn module_definition_table(
    tree: &SyntaxTree,
) -> Vec<(String, bool, Option<emath_rt::code_tree::CodeTree>)> {
    let mut table = Vec::new();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        if decl.as_kind != "function" {
            continue;
        }
        let opaque = call::function_is_opaque(decl);
        let decl_fn = FnDecl {
            inputs: call::section_typed_fields(decl, "inputs")
                .into_iter()
                .map(|(name, _)| name)
                .collect(),
            input_types: Vec::new(),
            outputs: call::section_fields(decl, "outputs"),
            output_types: Vec::new(),
            output: call::section_fields(decl, "outputs").into_iter().next(),
            defs: call::constructor_defs(decl),
            opaque,
        };
        let body = if opaque {
            None
        } else {
            crate::tree_distill::distill_tree(&call::function_body_expr(&decl_fn)).ok()
        };
        table.push((decl.name.clone(), opaque, body));
    }
    table
}
