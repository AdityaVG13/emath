//! Constructor-layer evaluation: object, function, recur, quote, query.
//!
//! Reuses the shared syntax tree and scalar carrier arithmetic. Mathematical
//! recipes are ordinary module functions, not image FeatureIDs.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use emath_core::tree::{
    BinaryOp, Declaration, Expr, ExprKind, Item, StmtKind, SyntaxTree, TypeExpr, TypeKind, UnaryOp,
    UseTree,
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
    Rat { num: ExactInt, den: ExactInt },
    Float64(f64),
    /// Dense sequence carrier. Copy-on-write: `Clone` shares the
    /// backing storage (the CPS kont bookkeeping clones argument
    /// values at every engine step, so a deep-copy clone made every
    /// big-sequence call O(len) per step — bead emath-g9rpo, PE P8:
    /// 17.4s wall for ~13k indexed reads); mutation goes through
    /// `Arc::make_mut`, which copies only when shared. `Arc` (not
    /// `Rc`) keeps the carrier `Send` for lanes that move values
    /// across threads. Structural `PartialEq` is unchanged by sharing.
    Sequence(Arc<Vec<CValue>>),
    /// In-place mutable buffer carrier (bead emath-84sfr, design note
    /// 12 Option B1): `buffer(size, fill)` constructs it, `buffer_set`
    /// writes through shared references, `xs[i]` reads, `.length`
    /// projects. `Arc<Mutex<..>>` (not `Rc<RefCell>`) keeps the carrier
    /// `Send` for lanes that move values across threads. Equality
    /// refuses: mutable state has no total value equality.
    Buffer(Arc<Mutex<Vec<CValue>>>),
    Tuple(Vec<CValue>),
    Record {
        type_name: String,
        fields: BTreeMap<String, CValue>,
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
/// has no total value equality (bead emath-84sfr) — the language
/// surface refuses `==`/`!=` on buffers; this identity fallback
/// exists so internal comparisons stay total, never as a user-facing
/// value equality.
impl PartialEq for CValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (
                Self::Rat {
                    num: an,
                    den: ad,
                },
                Self::Rat {
                    num: bn,
                    den: bd,
                },
            ) => an == bn && ad == bd,
            (Self::Float64(a), Self::Float64(b)) => a == b,
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

#[derive(Clone, Debug, PartialEq)]
pub struct Closure {
    pub param: String,
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
            // Contents are state, not a value: display the shape only,
            // never a snapshot that invites value-style comparison.
            Self::Buffer(cell) => {
                let len = cell.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).len();
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
        "E-PKG-050" => "E-PKG-050",
        "E-USE-ADMISSION" => "E-USE-ADMISSION",
        "E-TYPE-002" => "E-TYPE-002",
        "E-TYPE-003" => "E-TYPE-003",
        "E-TYPE-010" => "E-TYPE-010",
        "unbound" | "unbound_code" => "E-TYPE-002",
        "method_unavailable" | "implementation_unavailable"
        | "transformation_rule_unavailable" | "unresolved" | "stale_dependency" => "E-TYPE-003",
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
    Call { name: String, args: Vec<CValue> },
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
        rest: Vec<Expr>,
    },
    FnCall {
        name: String,
        done: Vec<CValue>,
        rest: Vec<Expr>,
    },
    SeqItems {
        as_tuple: bool,
        done: Vec<CValue>,
        rest: Vec<Expr>,
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
        expr: Box<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContinuationFrame {
    pub function: String,
    pub pc: u64,
    /// Next unfinished definition or instruction label in this frame.
    pub next: String,
    pub env: BTreeMap<String, CValue>,
    pub kont: Vec<Box<Kont>>,
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

pub const CHECKPOINT_SCHEMA: &str = "emath.constructor-checkpoint.v1";
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
    env: BTreeMap<String, CValue>,
    functions: BTreeMap<String, FnDecl>,
    queries: BTreeMap<String, QueryDecl>,
    objects: BTreeMap<String, ObjectSchema>,
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
    frames: Vec<ContinuationFrame>,
    scopes: BTreeSet<u64>,
    resume_frames: Vec<ContinuationFrame>,
    /// Expect-row fault-name atoms. While set, a single-segment path that
    /// resolves nowhere evaluates (and infers) as a nominal atom instead of
    /// faulting `unbound`, so `expect diagnostic.code == <fault-name>` can
    /// demand any named refusal — including user-quoted refusal reasons —
    /// and a wrong name simply fails the example. Definitions and every
    /// other expression context keep the hard unbound fault.
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
mod prelude;
mod admit;
mod call;
mod checkpoint;
mod engine_step;
mod eval;
mod expr;
mod identity;
mod ops;
mod query;
mod residual;
mod scalar;
mod setup;
mod value;

pub use admit::*;
pub use checkpoint::*;
pub use eval::*;
pub use query::*;
pub use residual::*;
pub use scalar::*;
pub use setup::*;

