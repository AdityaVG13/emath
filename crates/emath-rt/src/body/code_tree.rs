// The shared tree family (bead emath-shared-tree-view-make-bp8nu):
// the distilled expression tree a quoted program carries, the node
// records `quote.view` produces over it, the minted Scope witnesses
// that package fragments, and the reconstruction `quote.make`
// consumes - all shared std-only algorithms, ported arm for arm from
// the constructor VM's `view_of`/`rebuild_expr`/`mint_scope`
// (normative sources: constructor_layer/expr.rs, call.rs, and the
// forged-scope check in engine_step/apply.rs).
//
// Representation decision (recorded in the rt CONTRACT): the tree is
// a DISTILLED enum, not an include of emath-core's `Expr`. The
// artifact self-containment law embeds this module verbatim into
// generated crates with zero dependencies, and emath-core source is
// not embed-clean; the distillation mirrors exactly the shapes the
// view/make walk consumes (the emitter's distiller is the single
// Expr -> CodeTree conversion, shared by every lowering).
//
// Minting determinism: one per-run `MintState` (a counter plus the
// minted-id set) threaded through the shared algorithms via a
// thread-local cell; entries reset it at their prologue, so the same
// program mints the same `#scope.{id}` tokens on every run. Token
// spellings, mint order, and the refusal names match the VM's
// algorithms (the VM keeps its own CValue-shaped implementation; the
// observable-value parity pins carry the cross-lane proof).
//
// No-claim boundaries (the emitted subset): trees carry the scalar
// expression shapes - literals, paths, scalar calls, binary/unary
// ops, sequences, branches. `Closure`/`Index`/`Quotation`/`Record`/
// `Recur`/`Cases` node kinds refuse by name at reconstruction (the
// definition-tree and binder beads extend this family), and the tree
// evaluator computes scalar values only: a made tree that references
// module globals refuses at evaluation instead of resolving them.

use std::collections::{BTreeMap, BTreeSet};

use super::code::{CodeValue, ExprCode};

// ---------------------------------------------------------------------------
// The distilled tree

/// The binary operators that carry scalar-op names in view layouts.
/// Spellings match the VM's `scalar_op_name` table exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeBinary {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Pow,
    Imply,
    Iff,
    Asymp,
    In,
}

impl TreeBinary {
    /// The view-layout callee tag (the VM's `scalar_op_name`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Sub => "Sub",
            Self::Mul => "Mul",
            Self::Div => "Div",
            Self::Eq => "Eq",
            Self::Ne => "Ne",
            Self::Lt => "Lt",
            Self::Le => "Le",
            Self::Gt => "Gt",
            Self::Ge => "Ge",
            Self::And => "And",
            Self::Or => "Or",
            Self::Pow => "Pow",
            Self::Imply => "Imply",
            Self::Iff => "Iff",
            Self::Asymp => "Asymp",
            Self::In => "In",
        }
    }

    /// The op for a view-layout tag, when the tag names one (the
    /// VM's `as_scalar_op` over record tags and code paths).
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Add" => Self::Add,
            "Sub" => Self::Sub,
            "Mul" => Self::Mul,
            "Div" => Self::Div,
            "Eq" => Self::Eq,
            "Ne" => Self::Ne,
            "Lt" => Self::Lt,
            "Le" => Self::Le,
            "Gt" => Self::Gt,
            "Ge" => Self::Ge,
            "And" => Self::And,
            "Or" => Self::Or,
            "Pow" => Self::Pow,
            "Imply" => Self::Imply,
            "Iff" => Self::Iff,
            "Asymp" => Self::Asymp,
            "In" => Self::In,
            _ => return None,
        })
    }
}

/// The unary operators, spelled as the VM's view arm does
/// (`format!("{op:?}")` over emath-core's `UnaryOp`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeUnary {
    Neg,
    Not,
    Pos,
}

impl TreeUnary {
    pub fn name(self) -> &'static str {
        match self {
            Self::Neg => "Neg",
            Self::Not => "Not",
            Self::Pos => "Pos",
        }
    }
}

/// A function literal's recorded binder domain - the VM's
/// `DomainShape`, narrowed to what an applied union argument can meet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeDomain {
    /// A scalar carrier spelling: `Int`, `Rat`, `Bool`, `Float64`.
    Scalar(String),
    /// A sequence domain, by its VM spelling (`sequence(Rat)`): no
    /// union scalar is a sequence, so it admits none.
    Sequence(String),
    /// Not reconstructible from the authored domain: admits every
    /// argument with no claim (the VM's `Unknown`).
    Unknown,
}

impl TreeDomain {
    /// The VM's `domain_shape_admits_value` over the scalar union:
    /// `Int` admits Int, `Rat` admits Int and Rat, `Bool` admits Bool.
    pub fn admits(&self, value: &CodeValue) -> bool {
        match self {
            TreeDomain::Unknown => true,
            TreeDomain::Sequence(_) => false,
            TreeDomain::Scalar(name) => matches!(
                (name.as_str(), value),
                ("Int", CodeValue::Int(_))
                    | ("Rat", CodeValue::Int(_) | CodeValue::Rat(_))
                    | ("Bool", CodeValue::Bool(_))
            ),
        }
    }
}

impl core::fmt::Display for TreeDomain {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TreeDomain::Scalar(name) | TreeDomain::Sequence(name) => f.write_str(name),
            TreeDomain::Unknown => f.write_str("unknown"),
        }
    }
}

/// The distilled expression tree: exactly the shapes the view/make
/// walk consumes in the emitted subset. `List` and `Tuple` authoring
/// both distill to [`CodeTree::Tuple`] (both view as a `Sequence`
/// node).
#[derive(Clone, Debug, PartialEq)]
pub enum CodeTree {
    Literal(CodeValue),
    Path(Vec<String>),
    Call {
        function: Box<CodeTree>,
        args: Vec<CodeTree>,
    },
    Binary {
        op: TreeBinary,
        left: Box<CodeTree>,
        right: Box<CodeTree>,
    },
    Unary {
        op: TreeUnary,
        value: Box<CodeTree>,
    },
    /// A function literal (the VM's `FunctionAbs`): the binder's
    /// parameter name, its recorded domain, and the body tree. The view
    /// exposes the domain as Code and make rebuilds it; an applied
    /// evaluate checks each argument against it.
    Closure {
        param: String,
        domain: TreeDomain,
        body: Box<CodeTree>,
    },
    /// A recursive function literal (the VM's `Recur`): the name the
    /// body calls itself by, the declared type (`A -> B`, kept as the
    /// code it was written as), and the body (a function literal).
    Recur {
        name: String,
        ty: Box<CodeTree>,
        body: Box<CodeTree>,
    },
    Tuple(Vec<CodeTree>),
    /// Indexing (the VM's `Index`): `container[i, ..]`.
    Index {
        container: Box<CodeTree>,
        indices: Vec<CodeTree>,
    },
    If {
        condition: Box<CodeTree>,
        then_value: Box<CodeTree>,
        else_value: Box<CodeTree>,
    },
}

// ---------------------------------------------------------------------------
// The node-record value family

/// A view-record value: the dynamic family `quote.view` produces and
/// authored structural code consumes. Tags are empty-field records
/// (the VM's `schema_tag`), node records carry named fields, and
/// subterms travel as [`NodeValue::Code`] - the dual-representation
/// Code value (tree, open names, dependencies; a compiled factory
/// only when the tree came from a static template literal).
#[derive(Clone, Debug)]
pub enum NodeValue {
    Scalar(CodeValue),
    Record(String, BTreeMap<String, NodeValue>),
    Sequence(Vec<NodeValue>),
    Tuple(Vec<NodeValue>),
    Code(Box<ExprCode>),
}

/// Structural equality over the node family, mirroring the VM's
/// `eq_values` for the record family: scalar pairs compare VALUES
/// through the union kernels (`2 == 2/1` is true), records compare
/// type name and field set, sequences and tuples compare elementwise,
/// and code values compare canonical trees. Mixed shapes are
/// never equal (the VM's structural default). Total: no node value
/// has mutable state.
pub fn node_eq(
    left: impl std::borrow::Borrow<NodeValue>,
    right: impl std::borrow::Borrow<NodeValue>,
) -> Result<bool, String> {
    let (left, right) = (left.borrow(), right.borrow());
    Ok(match (left, right) {
        (NodeValue::Scalar(a), NodeValue::Scalar(b)) => super::code::code_eq(a, b)?,
        (
            NodeValue::Record(name_a, fields_a),
            NodeValue::Record(name_b, fields_b),
        ) => {
            name_a == name_b
                && fields_a.len() == fields_b.len()
                && fields_a.iter().all(|(name, value)| {
                    fields_b
                        .get(name)
                        .is_some_and(|other| node_eq(value, other).unwrap_or(false))
                })
        }
        (NodeValue::Sequence(a), NodeValue::Sequence(b))
        | (NodeValue::Tuple(a), NodeValue::Tuple(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(x, y)| node_eq(x, y).unwrap_or(false))
        }
        (NodeValue::Code(a), NodeValue::Code(b)) => a == b,
        _ => false,
    })
}

/// `<`, `>`, `<=`, `>=` over the record family: only scalars order
/// (exactly, by cross-multiplication, as the VM's `cmp_numeric`);
/// ordering a record, sequence, or code value refuses.
pub fn node_cmp(
    left: impl std::borrow::Borrow<NodeValue>,
    right: impl std::borrow::Borrow<NodeValue>,
) -> Result<core::cmp::Ordering, String> {
    match (left.borrow(), right.borrow()) {
        (NodeValue::Scalar(a), NodeValue::Scalar(b)) => super::code::code_cmp(a, b),
        _ => Err(String::from("type: ordering requires numeric operands")),
    }
}

/// The scalar a node holds (a viewed literal's `value`), for a
/// declared Int/Rat/Bool parameter; any other node refuses.
pub fn node_scalar(node: impl std::borrow::Borrow<NodeValue>) -> Result<CodeValue, String> {
    match node.borrow() {
        NodeValue::Scalar(value) => Ok(*value),
        _ => Err(String::from(
            "type: a scalar parameter received a non-scalar value",
        )),
    }
}

/// A tag record: the VM's `schema_tag` - a record whose type name IS
/// the tag, with no fields (`Call`, `Add`, `transparent`, ...).
pub fn node_tag(name: &str) -> NodeValue {
    NodeValue::Record(name.to_string(), BTreeMap::new())
}

/// A node record over authored field pairs (fields fold into the
/// sorted map, mirroring the VM's BTreeMap record layout).
pub fn node_record(name: &str, fields: Vec<(String, NodeValue)>) -> NodeValue {
    NodeValue::Record(name.to_string(), fields.into_iter().collect())
}

/// A node sequence over element values.
pub fn node_list(elements: Vec<NodeValue>) -> NodeValue {
    NodeValue::Sequence(elements)
}

/// Sequence concatenation over the node family (the VM's concat law
/// for family values): every operand must be a family sequence and
/// elements keep their family spelling.
pub fn node_concat_all(values: Vec<NodeValue>) -> Result<NodeValue, String> {
    let mut elements = Vec::new();
    for value in values {
        let NodeValue::Sequence(items) = value else {
            return Err(String::from("type: concat requires sequences"));
        };
        elements.extend(items);
    }
    Ok(NodeValue::Sequence(elements))
}

/// Record field projection (the VM's `project_field` over the node
/// family): a missing field is a typed refusal, never a silent
/// default.
pub fn node_field(value: impl std::borrow::Borrow<NodeValue>, field: &str) -> Result<NodeValue, String> {
    let value = value.borrow();
    match value {
        NodeValue::Record(_, fields) => fields
            .get(field)
            .cloned()
            .ok_or_else(|| format!("type: record has no field `{field}`")),
        NodeValue::Tuple(items) => {
            let index = field
                .strip_prefix('_')
                .and_then(|text| text.parse::<usize>().ok())
                .ok_or_else(|| format!("type: tuple has no field `{field}`"))?;
            items
                .get(index)
                .cloned()
                .ok_or_else(|| "invalid_index: sequence index out of range".to_string())
        }
        NodeValue::Sequence(items) if field == "length" => {
            Ok(NodeValue::Scalar(CodeValue::Int(items.len() as i64)))
        }
        // An emitted tuple lowers as a positional sequence (the typed
        // lane's tuple law), so `_N` indexes a family sequence too.
        NodeValue::Sequence(items) if field.starts_with('_') => {
            let index = field[1..]
                .parse::<usize>()
                .map_err(|_| format!("type: value has no field `{field}`"))?;
            items
                .get(index)
                .cloned()
                .ok_or_else(|| "invalid_index: sequence index out of range".to_string())
        }
        _ => Err(format!("type: value has no field `{field}`")),
    }
}

/// Sequence indexing (the VM's `index_seq` law and message).
pub fn node_index(sequence: impl std::borrow::Borrow<NodeValue>, index: i64) -> Result<NodeValue, String> {
    let sequence = sequence.borrow();
    let NodeValue::Sequence(items) = sequence else {
        return Err(String::from("type: index requires a sequence"));
    };
    let Some(index) = usize::try_from(index).ok() else {
        return Err(String::from("invalid_index: sequence index out of range"));
    };
    items
        .get(index)
        .cloned()
        .ok_or_else(|| String::from("invalid_index: sequence index out of range"))
}

/// The `quote.substitute` reference over the node family: a record's
/// type name IS the binder name (the minted-scope packaging spells
/// binder names as tag records), and a Code node wrapping a Path
/// names its joined segments - the VM's dynamic reference forms.
/// Anything else refuses by name, never guessing a binder.
pub fn substitute_reference(
    value: impl std::borrow::Borrow<NodeValue>,
) -> Result<String, String> {
    match value.borrow() {
        NodeValue::Record(name, _) => Ok(name.clone()),
        NodeValue::Code(code) => match &code.tree {
            CodeTree::Path(segments) => Ok(segments.join(".")),
            _ => Err(String::from(
                "type: quote.substitute reference must name a binder",
            )),
        },
        _ => Err(String::from(
            "type: quote.substitute reference must name a binder",
        )),
    }
}

impl NodeValue {
    /// The artifact-side walk surface. Emitted registers hold the
    /// family owned, borrowed, or as inlined temporaries; method
    /// syntax resolves all three through auto-ref/auto-deref, so a
    /// read never moves a multi-use register. Each method is the
    /// same law as its free function (the free functions stay the
    /// test and substrate seam).
    pub fn field(&self, name: &str) -> Result<NodeValue, String> {
        node_field(self, name)
    }
    pub fn index(&self, index: i64) -> Result<NodeValue, String> {
        node_index(self, index)
    }
    /// The substitute reference law as a method (the free function
    /// stays the test seam).
    pub fn reference_name(&self) -> Result<String, String> {
        substitute_reference(self)
    }
    pub fn view(&self, module: &ModuleTable) -> Result<NodeValue, String> {
        view_node(self, module)
    }
    pub fn make(&self, module: &ModuleTable) -> Result<ExprCode, String> {
        make_quoted(self, module)
    }
    /// `quote.open` over a package value (the binder half, bead
    /// emath-quote-bind-open-consumer-6f86g): the witness-validated
    /// unwrap, same law as the free function.
    pub fn open(&self) -> Result<ExprCode, String> {
        open_node(self)
    }
    pub fn equals(&self, other: impl std::borrow::Borrow<NodeValue>) -> Result<bool, String> {
        node_eq(self, other)
    }
    /// Method spellings of `node_as_code` / `node_scalar`: a method
    /// call auto-derefs a binding of any reference depth.
    pub fn to_code(&self) -> Result<ExprCode, String> {
        node_as_code(self)
    }
    pub fn to_scalar(&self) -> Result<CodeValue, String> {
        node_scalar(self)
    }
}

// ---------------------------------------------------------------------------
// The module table

/// The artifact's static module table: which names are module
/// callables (with their opacity, for view's `Global` layouts) and
/// each callable's declaration stamp (for dependency snapshots and
/// the `stale_dependency` refusal). Const-constructible so the
/// backend emits it as one crate-level static.
#[derive(Clone, Copy)]
pub struct ModuleTable {
    /// (name, opaque) per module callable.
    pub globals: &'static [(&'static str, bool)],
    /// (name, declaration stamp) per module callable - the same
    /// FNV-1a digest the VM's `decl_stamp` computes.
    pub stamps: &'static [(&'static str, u64)],
    /// (name, entry adapter) per module function whose compiled entry
    /// crosses as scalars: the tree evaluator's route into module code
    /// (a run-time formula calling a module function).
    pub callables: &'static [(&'static str, ModuleCallable)],
}

/// A module function's compiled entry adapted to the scalar union:
/// arguments project checked onto the declared parameter carriers, the
/// result wraps back into the union.
pub type ModuleCallable = fn(&[CodeValue]) -> Result<CodeValue, String>;

impl ModuleTable {
    /// The empty table (no module callables).
    pub const EMPTY: ModuleTable = ModuleTable {
        globals: &[],
        stamps: &[],
        callables: &[],
    };

    /// The compiled entry adapter of module function `name`, when the
    /// artifact carries one.
    pub fn callable(&self, name: &str) -> Option<ModuleCallable> {
        self.callables
            .iter()
            .find(|(callable, _)| *callable == name)
            .map(|(_, entry)| *entry)
    }

    /// Whether `name` is a module callable, and whether it is opaque.
    pub fn opaque_global(&self, name: &str) -> Option<bool> {
        self.globals
            .iter()
            .find(|(global, _)| *global == name)
            .map(|(_, opaque)| *opaque)
    }

    /// The declaration stamp of `name`, when it is a module callable.
    pub fn stamp_of(&self, name: &str) -> Option<u64> {
        self.stamps
            .iter()
            .find(|(global, _)| *global == name)
            .map(|(_, stamp)| *stamp)
    }
}

// ---------------------------------------------------------------------------
// The definition table

/// One definition-table row: a module function with its opacity and
/// - for a transparent callee - the distilled body tree the
/// `quote.body` unfold exposes as the Available fragment. A
/// transparent row whose body is `None` is outside the distilled
/// subset (binders, quotes, structured bodies): unfolding it refuses
/// by name (the no-claim boundary the VM does not share - the VM
/// shows the raw expression).
pub struct DefinitionRow {
    pub name: String,
    pub opaque: bool,
    pub body: Option<CodeTree>,
}

/// The artifact's definition table (bead emath-quote-body-defs-trto7):
/// the module's functions as DATA, the same bodies the compiled
/// entries come from. The rows are built once at first use (the
/// backend emits this as a `LazyLock` static), so a definition-table
/// unfold and a compiled call cannot disagree.
pub struct DefinitionTable {
    pub rows: Vec<DefinitionRow>,
}

impl DefinitionTable {
    /// The empty table (no definitions).
    pub const EMPTY: DefinitionTable = DefinitionTable {
        rows: Vec::new(),
    };

    fn row(&self, name: &str) -> Option<&DefinitionRow> {
        self.rows.iter().find(|row| row.name == name)
    }
}

/// The definition-table unfold (the VM's `quote_body`, arm for arm):
/// a Code whose tree is a Path naming a table function yields
/// `Opaque { identity, signature: "opaque" }` for an opaque callee or
/// `Available { fragment: Code(body tree) }` for a transparent one;
/// a name the table does not carry yields `Opaque { identity,
/// signature: "unbound" }` (a record, never a fault); any other Code
/// yields `Available` of its own tree. The fragment carries no
/// dependencies (the VM's `available_body` law) and no factory - it
/// is data, walked and made like any other tree.
pub fn quote_body_node(code: &ExprCode, defs: &DefinitionTable) -> Result<NodeValue, String> {
    if let CodeTree::Path(segments) = &code.tree {
        let name = segments.join(".");
        if let Some(row) = defs.row(&name) {
            if row.opaque {
                return Ok(body_record("Opaque", Some(&name), Some("opaque")));
            }
            let Some(body) = &row.body else {
                return Err(format!(
                    "tree: the body of `{name}` is not emitted in artifact trees"
                ));
            };
            return Ok(available_body(body));
        }
        return Ok(body_record("Opaque", Some(&name), Some("unbound")));
    }
    Ok(available_body(&code.tree))
}

/// The VM's `quote_body` over a node value (apply.rs, arm for arm): a
/// Code-carrying node unfolds through the definition table exactly as
/// [`quote_body_node`]; any other node (an operator tag record viewed
/// off a Call node's `callee` field) is `Opaque` with no identity and
/// no signature - a record, never a fault. The authored walk checks
/// `.kind == Available` and takes its named refusal leg on this arm.
pub fn quote_body_over_node(
    node: &NodeValue,
    defs: &DefinitionTable,
) -> Result<NodeValue, String> {
    match node {
        NodeValue::Code(code) => quote_body_node(code, defs),
        _ => Ok(body_record("Opaque", None, None)),
    }
}

/// The `Opaque`/`Available` body records (the VM's `body_record` and
/// `available_body`): tag fields first, identity and signature as
/// tags, the Available fragment as a dependency-free Code value.
fn body_record(kind: &str, identity: Option<&str>, signature: Option<&str>) -> NodeValue {
    let mut fields = vec![("kind".to_string(), node_tag(kind))];
    if let Some(identity) = identity {
        fields.push(("identity".to_string(), node_tag(identity)));
    }
    if let Some(signature) = signature {
        fields.push(("signature".to_string(), node_tag(signature)));
    }
    node_record(kind, fields)
}

fn available_body(tree: &CodeTree) -> NodeValue {
    node_record(
        "Available",
        vec![
            ("kind".to_string(), node_tag("Available")),
            ("fragment".to_string(), code_of(tree.clone())),
        ],
    )
}

// ---------------------------------------------------------------------------
// Minted scope state

/// The per-run minting state: the next witness id and the set of
/// minted ids (the forged-scope check consumes the set). One counter
/// threads every mint in walk order, so token spellings are a
/// deterministic function of the program.
pub struct MintState {
    next_ref: u64,
    scopes: BTreeSet<u64>,
}

impl MintState {
    pub fn new() -> Self {
        MintState {
            next_ref: 0,
            scopes: BTreeSet::new(),
        }
    }
}

impl Default for MintState {
    fn default() -> Self {
        Self::new()
    }
}

thread_local! {
    static MINT: std::cell::RefCell<MintState> = std::cell::RefCell::new(MintState::new());
}

/// Reset the per-run mint state. Emitted entries call this at their
/// prologue when they carry tree operations, so every run of the
/// same entry mints identical tokens.
pub fn reset_mint() {
    MINT.with(|cell| *cell.borrow_mut() = MintState::new());
}

fn with_mint<T>(action: impl FnOnce(&mut MintState) -> T) -> T {
    MINT.with(|cell| action(&mut cell.borrow_mut()))
}

/// Mint one Scope witness (the VM's `mint_scope`): a fresh id, the
/// binder tag (`closed` when the package is closed), and the token
/// string `#scope.{id}`.
fn mint_scope(mint: &mut MintState, binder: Option<&str>) -> NodeValue {
    mint.next_ref += 1;
    let id = mint.next_ref;
    mint.scopes.insert(id);
    node_record(
        "Scope",
        vec![
            ("id".into(), NodeValue::Scalar(CodeValue::Int(id as i64))),
            (
                "binder".into(),
                node_tag(binder.unwrap_or("closed")),
            ),
            ("token".into(), node_tag(&format!("#scope.{id}"))),
        ],
    )
}

/// The forged-scope check (the VM's `check_fragment_scope`): a
/// Fragment package's Scope witness must carry an id this run
/// actually minted. Anything else refuses by the VM's name.
pub fn check_scope(package: impl std::borrow::Borrow<NodeValue>) -> Result<(), String> {
    let package = package.borrow();
    let NodeValue::Record(type_name, fields) = package else {
        return Ok(());
    };
    if type_name != "Fragment" {
        return Ok(());
    }
    if let Some(NodeValue::Record(scope_name, scope)) = fields.get("context") {
        if scope_name == "Scope" {
            if let Some(NodeValue::Scalar(CodeValue::Int(id))) = scope.get("id") {
                let valid = with_mint(|mint| *id >= 0 && mint.scopes.contains(&(*id as u64)));
                if !valid {
                    return Err(String::from(
                        "invalid_code_construction: forged Scope witness",
                    ));
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The view (quote.view)

fn code_of(tree: CodeTree) -> NodeValue {
    let free = free_names_tree(&tree);
    NodeValue::Code(Box::new(super::code::open_expr(
        free,
        tree,
        None,
        BTreeMap::new(),
    )))
}

fn fragment_package(mint: &mut MintState, term: &CodeTree, binder: Option<&str>) -> NodeValue {
    node_record(
        "Fragment",
        vec![
            ("term".into(), code_of(term.clone())),
            ("context".into(), mint_scope(mint, binder)),
        ],
    )
}

fn view_record(tag: &str, fields: Vec<(&str, NodeValue)>) -> NodeValue {
    let mut fields: BTreeMap<String, NodeValue> =
        fields.into_iter().map(|(name, value)| (name.to_string(), value)).collect();
    fields.insert("kind".into(), node_tag(tag));
    NodeValue::Record(tag.to_string(), fields)
}

/// View a tree into node records (the VM's `view_of`, arm for arm).
/// Mint order follows the walk, matching the VM's field layout order.
pub fn view_tree(tree: &CodeTree, module: &ModuleTable) -> NodeValue {
    match tree {
        CodeTree::Literal(value) => {
            view_record("Literal", vec![("value", NodeValue::Scalar(*value))])
        }
        CodeTree::Path(segments) => {
            let name = segments.join(".");
            if let Some(opaque) = module.opaque_global(&name) {
                view_record(
                    "Global",
                    vec![
                        ("name", node_tag(&name)),
                        (
                            "visibility",
                            node_tag(if opaque { "opaque" } else { "transparent" }),
                        ),
                    ],
                )
            } else if segments.len() >= 2 {
                let container = segments[..segments.len() - 1].join(".");
                with_mint(|mint| {
                    view_record(
                        "Projection",
                        vec![
                            ("container", code_of(CodeTree::Path(vec![container.clone()]))),
                            ("field", node_tag(segments.last().expect("nonempty"))),
                            ("scrutinee", fragment_package(mint, &CodeTree::Path(vec![container]), None)),
                        ],
                    )
                })
            } else {
                view_record(
                    "Local",
                    vec![("name", node_tag(&name)), ("token", node_tag(&name))],
                )
            }
        }
        CodeTree::Call { function, args } => with_mint(|mint| {
            view_record(
                "Call",
                vec![
                    ("callee", code_of((**function).clone())),
                    (
                        "args",
                        NodeValue::Sequence(
                            args.iter().map(|arg| code_of(arg.clone())).collect(),
                        ),
                    ),
                    (
                        "children",
                        NodeValue::Sequence(
                            args.iter()
                                .map(|arg| fragment_package(mint, arg, None))
                                .collect(),
                        ),
                    ),
                ],
            )
        }),
        CodeTree::Binary { op, left, right } => with_mint(|mint| {
            view_record(
                "Call",
                vec![
                    ("callee", node_tag(op.name())),
                    (
                        "args",
                        NodeValue::Sequence(vec![
                            code_of((**left).clone()),
                            code_of((**right).clone()),
                        ]),
                    ),
                    (
                        "children",
                        NodeValue::Sequence(vec![
                            fragment_package(mint, left, None),
                            fragment_package(mint, right, None),
                        ]),
                    ),
                ],
            )
        }),
        CodeTree::Unary { op, value } => with_mint(|mint| {
            view_record(
                "Call",
                vec![
                    ("callee", node_tag(op.name())),
                    (
                        "args",
                        NodeValue::Sequence(vec![code_of((**value).clone())]),
                    ),
                    (
                        "children",
                        NodeValue::Sequence(vec![fragment_package(mint, value, None)]),
                    ),
                ],
            )
        }),
        CodeTree::Closure { param, domain, body } => with_mint(|mint| {
            // The VM's `FunctionAbs` view: the binder's parameter as a
            // name tag, its domain and body as Code, and the minted
            // scope package carrying the binder.
            view_record(
                "Closure",
                vec![
                    ("param", node_tag(param)),
                    ("domain", code_of(CodeTree::Path(vec![domain.to_string()]))),
                    ("body", code_of((**body).clone())),
                    ("scope", fragment_package(mint, body, Some(param))),
                ],
            )
        }),
        CodeTree::Recur { name, ty, body } => with_mint(|mint| {
            view_record(
                "Recur",
                vec![
                    ("name", node_tag(name)),
                    ("type", code_of((**ty).clone())),
                    ("body", code_of((**body).clone())),
                    ("scope", fragment_package(mint, body, Some(name))),
                ],
            )
        }),
        CodeTree::Tuple(items) => with_mint(|mint| {
            view_record(
                "Sequence",
                vec![
                    (
                        "elements",
                        NodeValue::Sequence(
                            items.iter().map(|item| code_of(item.clone())).collect(),
                        ),
                    ),
                    (
                        "children",
                        NodeValue::Sequence(
                            items
                                .iter()
                                .map(|item| fragment_package(mint, item, None))
                                .collect(),
                        ),
                    ),
                ],
            )
        }),
        // Mint order is the VM's: the index packages, then the
        // container's scrutinee package.
        CodeTree::Index { container, indices } => with_mint(|mint| {
            let children = indices
                .iter()
                .map(|index| fragment_package(mint, index, None))
                .collect();
            let scrutinee = fragment_package(mint, container, None);
            let mut fields = vec![
                ("container", code_of((**container).clone())),
                (
                    "indices",
                    NodeValue::Sequence(
                        indices.iter().map(|index| code_of(index.clone())).collect(),
                    ),
                ),
                ("children", NodeValue::Sequence(children)),
                ("scrutinee", scrutinee),
            ];
            if let Some(index) = indices.first() {
                fields.push(("index", code_of(index.clone())));
            }
            view_record("Index", fields)
        }),
        CodeTree::If {
            condition,
            then_value,
            else_value,
        } => with_mint(|mint| {
            view_record(
                "Branch",
                vec![
                    ("condition", code_of((**condition).clone())),
                    ("then", code_of((**then_value).clone())),
                    ("else", code_of((**else_value).clone())),
                    ("otherwise", code_of((**else_value).clone())),
                    (
                        "children",
                        NodeValue::Sequence(vec![
                            fragment_package(mint, condition, None),
                            fragment_package(mint, then_value, None),
                            fragment_package(mint, else_value, None),
                        ]),
                    ),
                ],
            )
        }),
    }
}

/// View a Code value (the VM's `quote_view` over `fragment_term`): a
/// Code views its own tree; a Fragment package views its term;
/// anything else refuses by the VM's message.
pub fn view_node(value: impl std::borrow::Borrow<NodeValue>, module: &ModuleTable) -> Result<NodeValue, String> {
    let value = value.borrow();
    match value {
        NodeValue::Code(code) => Ok(view_tree(&code.tree, module)),
        NodeValue::Record(type_name, fields)
            if type_name == "Fragment" || fields.contains_key("term") =>
        {
            match fields.get("term") {
                Some(NodeValue::Code(code)) => Ok(view_tree(&code.tree, module)),
                Some(other) => rebuild_node(other).map(|tree| view_tree(&tree, module)),
                None => Err(String::from("type: Fragment missing term")),
            }
        }
        _ => Err(String::from("type: quote expects Code or Fragment")),
    }
}

// ---------------------------------------------------------------------------
// The reconstruction (quote.make)

fn tag_name(value: &NodeValue) -> Option<&str> {
    match value {
        NodeValue::Record(name, fields) if fields.is_empty() => Some(name.as_str()),
        _ => None,
    }
}

/// The record's node kind: the `kind` field's tag when present, the
/// record's own type name otherwise (the VM's `record_kind_name`).
fn record_kind(type_name: &str, fields: &BTreeMap<String, NodeValue>) -> String {
    match fields.get("kind") {
        Some(kind) => tag_name(kind).unwrap_or(type_name).to_string(),
        None => type_name.to_string(),
    }
}

fn scalar_op_of(value: &NodeValue) -> Option<TreeBinary> {
    match value {
        NodeValue::Record(name, fields) if fields.is_empty() => TreeBinary::from_name(name),
        NodeValue::Code(code) => match &code.tree {
            CodeTree::Path(segments) => TreeBinary::from_name(&segments.join(".")),
            _ => None,
        },
        _ => None,
    }
}

fn rebuild_call(fields: &BTreeMap<String, NodeValue>) -> Result<CodeTree, String> {
    let args = match fields.get("args") {
        Some(NodeValue::Sequence(items)) => items
            .iter()
            .map(rebuild_node)
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(String::from("Call missing args")),
    };
    let callee = fields
        .get("callee")
        .ok_or_else(|| String::from("Call missing callee"))?;
    if let Some(op) = scalar_op_of(callee) {
        if args.len() != 2 {
            return Err(String::from("scalar call expects two arguments"));
        }
        return Ok(CodeTree::Binary {
            op,
            left: Box::new(args[0].clone()),
            right: Box::new(args[1].clone()),
        });
    }
    Ok(CodeTree::Call {
        function: Box::new(rebuild_node(callee)?),
        args,
    })
}

/// The opened term of a package (the VM's `check_fragment_scope` +
/// `open_fragment`, arm for arm): a Code opens to its own tree with
/// the compiled factory and the capture snapshot dropped; a Fragment
/// validates its minted Scope witness FIRST (a forged witness
/// refuses by the VM's name) and unwraps its term (a Code term
/// opens; any other node value rebuilds; a missing term refuses);
/// anything else refuses by the VM's message.
pub fn open_node(package: impl std::borrow::Borrow<NodeValue>) -> Result<ExprCode, String> {
    let package = package.borrow();
    match package {
        NodeValue::Code(code) => Ok(super::code::open_code(code.as_ref())),
        NodeValue::Record(type_name, fields) if type_name == "Fragment" => {
            check_scope(package)?;
            match fields.get("term") {
                Some(NodeValue::Code(code)) => Ok(super::code::open_code(code.as_ref())),
                Some(other) => {
                    let tree = rebuild_node(other)?;
                    Ok(super::code::open_expr(
                        free_names_tree(&tree),
                        tree,
                        None,
                        BTreeMap::new(),
                    ))
                }
                None => Err(String::from("Fragment missing term")),
            }
        }
        _ => Err(String::from("quote expects Code or Fragment")),
    }
}

/// The items of a family sequence or tuple - the call-marshal bridge
/// for a declared sequence parameter (`sequence(Code)`,
/// `sequence((Code, Code))`), whose items then cross one by one.
pub fn node_items(node: impl std::borrow::Borrow<NodeValue>) -> Result<Vec<NodeValue>, String> {
    match node.borrow() {
        NodeValue::Sequence(items) | NodeValue::Tuple(items) => Ok(items.clone()),
        _ => Err(String::from("type: expected a sequence value")),
    }
}

/// `quote.identity`: the VM's canonical code identity (normative
/// source: constructor_layer/identity.rs `code_identity`) - FNV-1a over
/// de Bruijn-style bytes, so code equal up to renaming of bound names
/// has one identity, cast to Int exactly as the VM casts it. Every
/// tree arm writes the bytes the VM writes for the expression it was
/// distilled from. A binder domain the tree does not record (the VM's
/// `unknown` shape) cannot be spelled back and refuses by name.
pub fn tree_identity(tree: &CodeTree) -> Result<i64, String> {
    let mut out = Vec::new();
    canonical_tree_bytes(tree, &mut Vec::new(), &mut out)?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in out {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(hash as i64)
}

/// The VM's code equality (`constructor_layer/identity.rs` `same_code`):
/// the same canonical bytes, so equal up to renaming of bound names. A
/// tree with an unrecorded binder domain has no canonical bytes here;
/// such trees compare structurally.
pub fn same_tree(left: &CodeTree, right: &CodeTree) -> bool {
    let (mut a, mut b) = (Vec::new(), Vec::new());
    match (
        canonical_tree_bytes(left, &mut Vec::new(), &mut a),
        canonical_tree_bytes(right, &mut Vec::new(), &mut b),
    ) {
        (Ok(()), Ok(())) => a == b,
        _ => left == right,
    }
}

fn count_bytes(n: usize, out: &mut Vec<u8>) {
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

fn text_bytes(text: &str, out: &mut Vec<u8>) {
    count_bytes(text.len(), out);
    out.extend_from_slice(text.as_bytes());
}

/// The VM's bytes for a free path (no generics).
fn free_path_bytes(segments: &[impl AsRef<str>], out: &mut Vec<u8>) {
    out.push(b'P');
    count_bytes(segments.len(), out);
    for segment in segments {
        text_bytes(segment.as_ref(), out);
    }
    out.push(0);
}

/// The VM's prefix-free canonical bytes (constructor_layer/identity.rs
/// `canonical_expr_bytes`): text carries its length, subterm lists their
/// count, a path its generics presence byte.
fn canonical_tree_bytes(
    tree: &CodeTree,
    binders: &mut Vec<String>,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    match tree {
        CodeTree::Literal(CodeValue::Int(value)) => {
            out.push(b'I');
            text_bytes(&value.to_string(), out);
        }
        CodeTree::Literal(CodeValue::Rat((numer, denom))) => {
            out.push(b'R');
            text_bytes(&numer.to_string(), out);
            text_bytes(&denom.to_string(), out);
        }
        CodeTree::Literal(CodeValue::Bool(value)) => {
            out.push(b'B');
            out.push(u8::from(*value));
        }
        CodeTree::Path(segments) => {
            if let Some(depth) = binders.iter().rposition(|b| b == &segments[0]) {
                out.push(b'#');
                count_bytes(depth, out);
                count_bytes(segments.len() - 1, out);
                for segment in &segments[1..] {
                    text_bytes(segment, out);
                }
                out.push(0);
            } else if segments.len() == 1 && segments[0].contains('(') {
                // `quote.view` exposes a sequence domain as a one-segment
                // path spelled `sequence(T)`; the VM's view holds the
                // domain expression itself, a call.
                domain_spelling_bytes(&segments[0], out)?;
            } else {
                free_path_bytes(segments, out);
            }
        }
        CodeTree::Call { function, args } => {
            out.push(b'C');
            canonical_tree_bytes(function, binders, out)?;
            count_bytes(args.len(), out);
            for arg in args {
                canonical_tree_bytes(arg, binders, out)?;
            }
        }
        CodeTree::Binary { op, left, right } => {
            out.push(b'Y');
            text_bytes(op.name(), out);
            canonical_tree_bytes(left, binders, out)?;
            canonical_tree_bytes(right, binders, out)?;
        }
        CodeTree::Unary { op, value } => {
            out.push(b'U');
            text_bytes(op.name(), out);
            canonical_tree_bytes(value, binders, out)?;
        }
        CodeTree::Closure { param, domain, body } => {
            out.push(b'L');
            match domain {
                TreeDomain::Scalar(name) => free_path_bytes(&[name.as_str()], out),
                TreeDomain::Sequence(spelling) => domain_spelling_bytes(spelling, out)?,
                TreeDomain::Unknown => return Err(unrecorded_domain()),
            }
            binders.push(param.clone());
            canonical_tree_bytes(body, binders, out)?;
            binders.pop();
        }
        CodeTree::Recur { name, ty, body } => {
            out.push(b'V');
            canonical_tree_bytes(ty, binders, out)?;
            binders.push(name.clone());
            canonical_tree_bytes(body, binders, out)?;
            binders.pop();
        }
        CodeTree::Tuple(items) => {
            out.push(b'[');
            count_bytes(items.len(), out);
            for item in items {
                canonical_tree_bytes(item, binders, out)?;
            }
        }
        CodeTree::Index { container, indices } => {
            out.push(b'N');
            canonical_tree_bytes(container, binders, out)?;
            count_bytes(indices.len(), out);
            for index in indices {
                canonical_tree_bytes(index, binders, out)?;
            }
        }
        CodeTree::If {
            condition,
            then_value,
            else_value,
        } => {
            out.push(b'?');
            canonical_tree_bytes(condition, binders, out)?;
            canonical_tree_bytes(then_value, binders, out)?;
            canonical_tree_bytes(else_value, binders, out)?;
        }
    }
    Ok(())
}

/// The VM's domain expression bytes for a recorded domain spelling:
/// a scalar name is a path, `sequence(T)` is a call of `sequence`.
fn domain_spelling_bytes(spelling: &str, out: &mut Vec<u8>) -> Result<(), String> {
    if let Some(element) = spelling
        .strip_prefix("sequence(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        out.push(b'C');
        free_path_bytes(&["sequence"], out);
        count_bytes(1, out);
        return domain_spelling_bytes(element, out);
    }
    if matches!(spelling, "Int" | "Rat" | "Bool" | "Float64") {
        free_path_bytes(&[spelling], out);
        return Ok(());
    }
    Err(unrecorded_domain())
}

fn unrecorded_domain() -> String {
    String::from(
        "unsupported: quote.identity over a binder domain the artifact tree does not record (only Int, Rat, Bool, Float64 and sequences of them)",
    )
}

/// A node value in a Code position (the call-marshal bridge): a Code
/// node unwraps to its own ExprCode unchanged (the VM's call arguments
/// ARE codes); any other node value rebuilds into a factory-free,
/// dependency-free code over the distilled subset (refusing by name
/// outside it) - the VM's `rebuild_expr` law for a value in a code
/// slot.
pub fn node_as_code(node: impl std::borrow::Borrow<NodeValue>) -> Result<ExprCode, String> {
    let node = node.borrow();
    match node {
        NodeValue::Code(code) => Ok((**code).clone()),
        other => {
            let tree = rebuild_node(other)?;
            Ok(super::code::open_expr(
                free_names_tree(&tree),
                tree,
                None,
                BTreeMap::new(),
            ))
        }
    }
}

/// Rebuild a tree from node records (the VM's `rebuild_expr`, arm for
/// arm over the emitted subset). Node kinds outside the subset
/// refuse by name; the refusal spellings match the VM's.
pub fn rebuild_node(value: &NodeValue) -> Result<CodeTree, String> {
    match value {
        NodeValue::Code(code) => Ok(code.tree.clone()),
        NodeValue::Scalar(value) => Ok(CodeTree::Literal(*value)),
        NodeValue::Sequence(items) => {
            let trees = items
                .iter()
                .map(rebuild_node)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CodeTree::Tuple(trees))
        }
        NodeValue::Tuple(items) => {
            let trees = items
                .iter()
                .map(rebuild_node)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CodeTree::Tuple(trees))
        }
        NodeValue::Record(type_name, fields) => {
            match record_kind(type_name, fields).as_str() {
                "Literal" => match fields.get("value") {
                    Some(NodeValue::Scalar(value)) => Ok(CodeTree::Literal(*value)),
                    Some(NodeValue::Code(code)) => Ok(code.tree.clone()),
                    Some(NodeValue::Sequence(items)) => Ok(CodeTree::Tuple(
                        items
                            .iter()
                            .map(rebuild_node)
                            .collect::<Result<Vec<_>, _>>()?,
                    )),
                    Some(NodeValue::Tuple(items)) => Ok(CodeTree::Tuple(
                        items
                            .iter()
                            .map(rebuild_node)
                            .collect::<Result<Vec<_>, _>>()?,
                    )),
                    _ => Err(String::from("Literal node missing value")),
                },
                "Local" | "Global" => {
                    let name = match fields.get("name") {
                        Some(name) => tag_name(name)
                            .map(str::to_string)
                            .or_else(|| match name {
                                NodeValue::Code(code) => match &code.tree {
                                    CodeTree::Path(segments) => Some(segments.join(".")),
                                    _ => None,
                                },
                                _ => None,
                            })
                            .unwrap_or_else(|| type_name.clone()),
                        None => type_name.clone(),
                    };
                    Ok(CodeTree::Path(vec![name]))
                }
                "Call" => rebuild_call(fields),
                "Sequence" => {
                    let elements = match fields.get("elements") {
                        Some(NodeValue::Sequence(items)) => items,
                        _ => return Err(String::from("Sequence missing elements")),
                    };
                    let trees = elements
                        .iter()
                        .map(rebuild_node)
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(CodeTree::Tuple(trees))
                }
                "Closure" => {
                    // The VM's `FunctionAbs` rebuild: the parameter
                    // from its name tag, the body from the Code field;
                    // the minted scope witness is not part of the term.
                    let param = match fields.get("param") {
                        Some(name) => tag_name(name)
                            .map(str::to_string)
                            .ok_or_else(|| String::from("Closure param is not a name tag"))?,
                        None => return Err(String::from("Closure missing param")),
                    };
                    let domain = match fields.get("domain") {
                        Some(NodeValue::Code(code)) => match &code.tree {
                            CodeTree::Path(segments) if segments.len() == 1 => match segments[0].as_str() {
                                "unknown" => TreeDomain::Unknown,
                                name if name.starts_with("sequence(") => TreeDomain::Sequence(name.to_string()),
                                name => TreeDomain::Scalar(name.to_string()),
                            },
                            _ => return Err(String::from("Closure domain is not a domain name")),
                        },
                        _ => return Err(String::from("Closure missing domain")),
                    };
                    let body = match fields.get("body") {
                        Some(NodeValue::Code(code)) => code.tree.clone(),
                        _ => return Err(String::from("Closure body is not code")),
                    };
                    Ok(CodeTree::Closure {
                        param,
                        domain,
                        body: Box::new(body),
                    })
                }
                "Recur" => {
                    let name = match fields.get("name") {
                        Some(name) => tag_name(name)
                            .map(str::to_string)
                            .ok_or_else(|| String::from("Recur missing name"))?,
                        None => return Err(String::from("Recur missing name")),
                    };
                    let ty = rebuild_node(
                        fields
                            .get("type")
                            .ok_or_else(|| String::from("Recur missing type"))?,
                    )?;
                    let body = rebuild_node(
                        fields
                            .get("body")
                            .ok_or_else(|| String::from("Recur missing body"))?,
                    )?;
                    Ok(CodeTree::Recur {
                        name,
                        ty: Box::new(ty),
                        body: Box::new(body),
                    })
                }
                "Index" => {
                    let container = rebuild_node(
                        fields
                            .get("container")
                            .ok_or_else(|| String::from("Index missing container"))?,
                    )?;
                    let indices = match fields.get("indices") {
                        Some(NodeValue::Sequence(items)) => items
                            .iter()
                            .map(rebuild_node)
                            .collect::<Result<Vec<_>, _>>()?,
                        _ => match fields.get("index") {
                            Some(index) => vec![rebuild_node(index)?],
                            None => return Err(String::from("Index missing index")),
                        },
                    };
                    Ok(CodeTree::Index {
                        container: Box::new(container),
                        indices,
                    })
                }
                "Fragment" => fields
                    .get("term")
                    .ok_or_else(|| String::from("Fragment missing term"))
                    .and_then(rebuild_node),
                "Branch" | "If" => {
                    let condition = rebuild_node(
                        fields
                            .get("condition")
                            .ok_or_else(|| String::from("Branch missing condition"))?,
                    )?;
                    let then_value = rebuild_node(
                        fields
                            .get("then")
                            .ok_or_else(|| String::from("Branch missing then"))?,
                    )?;
                    let else_value = fields
                        .get("otherwise")
                        .or_else(|| fields.get("else"))
                        .ok_or_else(|| String::from("Branch missing else"))?;
                    Ok(CodeTree::If {
                        condition: Box::new(condition),
                        then_value: Box::new(then_value),
                        else_value: Box::new(rebuild_node(else_value)?),
                    })
                }
                "Projection" => {
                    let container = rebuild_node(
                        fields
                            .get("container")
                            .ok_or_else(|| String::from("Projection missing container"))?,
                    )?;
                    let field = match fields.get("field") {
                        Some(field) => tag_name(field)
                            .map(str::to_string)
                            .ok_or_else(|| String::from("Projection missing field"))?,
                        None => return Err(String::from("Projection missing field")),
                    };
                    match container {
                        CodeTree::Path(mut segments) => {
                            segments.push(field);
                            Ok(CodeTree::Path(segments))
                        }
                        _ => Err(String::from("Projection container is not a path")),
                    }
                }
                other => Err(format!(
                    "node `{other}` is not yet emitted in artifact trees"
                )),
            }
        }
    }
}

/// Reconstruct a Code value from node records (the VM's `quote_make`):
/// a Code passes through, a Fragment yields its term, any other node
/// record rebuilds; the rebuilt tree's open names (minus its stamped
/// dependencies) become the open set, and free names that resolve
/// against the module table stamp exactly as the VM's
/// `dependency_snapshot` does.
pub fn make_quoted(node: impl std::borrow::Borrow<NodeValue>, module: &ModuleTable) -> Result<ExprCode, String> {
    let node = node.borrow();
    let tree = match node {
        NodeValue::Code(code) => code.tree.clone(),
        other => rebuild_node(other)?,
    };
    let deps = dependency_snapshot_tree(&tree, module);
    let free = free_names_tree(&tree)
        .into_iter()
        .filter(|name| !deps.contains_key(name))
        .collect();
    Ok(super::code::open_expr(free, tree, None, deps))
}

/// `quote.join` (the VM's arm in engine_step/call.rs): the code naming
/// `left_right`. Each side is a quoted single identifier or a name tag;
/// the right side may instead be a nonnegative Int (`formula_3`).
pub fn join_quoted(
    left: &NodeValue,
    right: &NodeValue,
    module: &ModuleTable,
) -> Result<ExprCode, String> {
    let suffix = match right {
        NodeValue::Scalar(CodeValue::Int(n)) if *n >= 0 => n.to_string(),
        other => quoted_identifier(other)?,
    };
    let name = format!("{}_{}", quoted_identifier(left)?, suffix);
    make_quoted(
        NodeValue::Code(Box::new(super::code::open_expr(
            Vec::new(),
            CodeTree::Path(vec![name]),
            None,
            BTreeMap::new(),
        ))),
        module,
    )
}

fn quoted_identifier(value: &NodeValue) -> Result<String, String> {
    let name = match value {
        NodeValue::Code(code) => match &code.tree {
            CodeTree::Path(segments) if segments.len() == 1 => segments[0].clone(),
            _ => String::new(),
        },
        NodeValue::Record(name, fields) if fields.is_empty() => name.clone(),
        _ => String::new(),
    };
    let mut chars = name.chars();
    let identifier = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if identifier {
        Ok(name)
    } else {
        Err(String::from("type: quote.join expects quoted identifiers"))
    }
}

// ---------------------------------------------------------------------------
// Tree substitution and the tree evaluator

/// Substitute by name (the VM's `substitute_path` over the emitted
/// subset): a matching path becomes the replacement; an absent
/// reference is a no-op. A closure binder shadows the reference inside
/// its body - the body is substituted only when the reference is a
/// different name.
pub fn substitute_tree(tree: &CodeTree, reference: &str, replacement: &CodeTree) -> CodeTree {
    match tree {
        CodeTree::Path(segments) if segments.join(".") == reference => replacement.clone(),
        CodeTree::Path(segments) => CodeTree::Path(segments.clone()),
        CodeTree::Literal(value) => CodeTree::Literal(*value),
        CodeTree::Call { function, args } => CodeTree::Call {
            function: Box::new(substitute_tree(function, reference, replacement)),
            args: args
                .iter()
                .map(|arg| substitute_tree(arg, reference, replacement))
                .collect(),
        },
        CodeTree::Binary { op, left, right } => CodeTree::Binary {
            op: *op,
            left: Box::new(substitute_tree(left, reference, replacement)),
            right: Box::new(substitute_tree(right, reference, replacement)),
        },
        CodeTree::Unary { op, value } => CodeTree::Unary {
            op: *op,
            value: Box::new(substitute_tree(value, reference, replacement)),
        },
        CodeTree::Closure {
            param,
            domain,
            body,
        } => {
            if param == reference {
                tree.clone()
            } else {
                CodeTree::Closure {
                    param: param.clone(),
                    domain: domain.clone(),
                    body: Box::new(substitute_tree(body, reference, replacement)),
                }
            }
        }
        CodeTree::Recur { name, .. } if name == reference => tree.clone(),
        CodeTree::Recur { name, ty, body } => CodeTree::Recur {
            name: name.clone(),
            ty: Box::new(substitute_tree(ty, reference, replacement)),
            body: Box::new(substitute_tree(body, reference, replacement)),
        },
        CodeTree::Tuple(items) => CodeTree::Tuple(
            items
                .iter()
                .map(|item| substitute_tree(item, reference, replacement))
                .collect(),
        ),
        CodeTree::Index { container, indices } => CodeTree::Index {
            container: Box::new(substitute_tree(container, reference, replacement)),
            indices: indices
                .iter()
                .map(|index| substitute_tree(index, reference, replacement))
                .collect(),
        },
        CodeTree::If {
            condition,
            then_value,
            else_value,
        } => CodeTree::If {
            condition: Box::new(substitute_tree(condition, reference, replacement)),
            then_value: Box::new(substitute_tree(then_value, reference, replacement)),
            else_value: Box::new(substitute_tree(else_value, reference, replacement)),
        },
    }
}

/// The free path names of the tree (the VM's `free_path_names` over
/// the subset), sorted.
pub fn free_names_tree(tree: &CodeTree) -> Vec<String> {
    let mut free = BTreeSet::new();
    collect_free_names(tree, &mut free);
    free.into_iter().collect()
}

fn collect_free_names(tree: &CodeTree, free: &mut BTreeSet<String>) {
    match tree {
        CodeTree::Path(segments) => {
            free.insert(segments.join("."));
        }
        CodeTree::Literal(_) => {}
        CodeTree::Call { function, args } => {
            collect_free_names(function, free);
            args.iter().for_each(|arg| collect_free_names(arg, free));
        }
        CodeTree::Binary { left, right, .. } => {
            collect_free_names(left, free);
            collect_free_names(right, free);
        }
        CodeTree::Unary { value, .. } => collect_free_names(value, free),
        CodeTree::Closure { param, body, .. } => {
            // The binder binds its parameter inside the body: the
            // body's free names minus the parameter.
            let mut inner = BTreeSet::new();
            collect_free_names(body, &mut inner);
            inner.remove(param);
            free.extend(inner);
        }
        // The type is a type expression, never a value reference.
        CodeTree::Recur { name, body, .. } => {
            let mut inner = BTreeSet::new();
            collect_free_names(body, &mut inner);
            inner.remove(name);
            free.extend(inner);
        }
        CodeTree::Tuple(items) => items.iter().for_each(|item| collect_free_names(item, free)),
        CodeTree::Index { container, indices } => {
            collect_free_names(container, free);
            indices
                .iter()
                .for_each(|index| collect_free_names(index, free));
        }
        CodeTree::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_free_names(condition, free);
            collect_free_names(then_value, free);
            collect_free_names(else_value, free);
        }
    }
}

/// Capture-time dependency snapshot (the VM's `dependency_snapshot`):
/// free names that resolve against the module table, stamped with
/// the declaration identity they resolve to.
pub fn dependency_snapshot_tree(
    tree: &CodeTree,
    module: &ModuleTable,
) -> BTreeMap<String, u64> {
    free_names_tree(tree)
        .into_iter()
        .filter_map(|name| module.stamp_of(&name).map(|stamp| (name, stamp)))
        .collect()
}

/// A stamped dependency that no longer resolves, or resolves to a
/// different declaration, refuses (the VM's `verify_dependencies`).
pub fn verify_deps(
    deps: &BTreeMap<String, u64>,
    module: &ModuleTable,
) -> Result<(), String> {
    for (name, stamp) in deps {
        if module.stamp_of(name) != Some(*stamp) {
            return Err(format!(
                "stale_dependency: quoted dependency `{name}` changed since capture"
            ));
        }
    }
    Ok(())
}

/// The scalar tree evaluator: computes a made tree over the same
/// dynamic kernels the compiled template lane uses (the VM's exact
/// carrier rules), with short-circuit boolean combinators and the
/// VM's `if` admission. Free names resolve from the input
/// environment; a name the environment does not supply refuses
/// `unbound_code`. A call to a module function runs that function's
/// compiled entry through the module table (`ModuleTable::callables`,
/// arguments crossing as scalars); a call the table does not carry and
/// a global value reference refuse by name.
pub fn evaluate_tree(
    tree: &CodeTree,
    inputs: &BTreeMap<String, CodeValue>,
    module: &ModuleTable,
) -> Result<CodeValue, String> {
    use super::code::{code_add, code_as_bool, code_cmp, code_div, code_eq, code_mul, code_neg, code_not, code_sub};
    match tree {
        CodeTree::Literal(value) => Ok(*value),
        CodeTree::Path(segments) => {
            let name = segments.join(".");
            inputs.get(&name).cloned().ok_or_else(|| {
                format!(
                    "tree: global reference `{name}` is not emitted in the scalar tree evaluator"
                )
            })
        }
        CodeTree::Call { function, args } => {
            let name = match &**function {
                CodeTree::Path(segments) => segments.join("."),
                _ => String::from("expression"),
            };
            // A bound name is a scalar, never a function (the VM's
            // words); a module function calls its compiled entry
            // through the table, arguments evaluated left to right.
            if let Some(value) = inputs.get(&name) {
                return Err(format!("type: value is not callable: {value}"));
            }
            let Some(callable) = module.callable(&name) else {
                return Err(format!(
                    "tree: call `{name}` is not emitted in the scalar tree evaluator"
                ));
            };
            let values = args
                .iter()
                .map(|arg| evaluate_tree(arg, inputs, module))
                .collect::<Result<Vec<_>, _>>()?;
            callable(&values)
        }
        CodeTree::Closure { param, .. } => Err(format!(
            "tree: closure `{param} -> _` is not a scalar value in the tree evaluator"
        )),
        CodeTree::Recur { name, .. } => Err(format!(
            "tree: recursive function `{name}` is not a scalar value in the tree evaluator"
        )),
        CodeTree::Binary { op, left, right } => match op {
            TreeBinary::And => {
                if code_as_bool(&evaluate_tree(left, inputs, module)?)? {
                    code_as_bool(&evaluate_tree(right, inputs, module)?)
                        .map(CodeValue::Bool)
                } else {
                    Ok(CodeValue::Bool(false))
                }
            }
            TreeBinary::Or => {
                if code_as_bool(&evaluate_tree(left, inputs, module)?)? {
                    Ok(CodeValue::Bool(true))
                } else {
                    code_as_bool(&evaluate_tree(right, inputs, module)?)
                        .map(CodeValue::Bool)
                }
            }
            _ => {
                let left = evaluate_tree(left, inputs, module)?;
                let right = evaluate_tree(right, inputs, module)?;
                match op {
                    TreeBinary::Add => code_add(&left, &right),
                    TreeBinary::Sub => code_sub(&left, &right),
                    TreeBinary::Mul => code_mul(&left, &right),
                    TreeBinary::Div => code_div(&left, &right),
                    TreeBinary::Eq => code_eq(&left, &right).map(CodeValue::Bool),
                    TreeBinary::Ne => code_eq(&left, &right).map(|equal| CodeValue::Bool(!equal)),
                    TreeBinary::Lt => code_cmp(&left, &right).map(|order| CodeValue::Bool(order.is_lt())),
                    TreeBinary::Le => code_cmp(&left, &right).map(|order| CodeValue::Bool(order.is_le())),
                    TreeBinary::Gt => code_cmp(&left, &right).map(|order| CodeValue::Bool(order.is_gt())),
                    TreeBinary::Ge => code_cmp(&left, &right).map(|order| CodeValue::Bool(order.is_ge())),
                    other => Err(format!(
                        "tree: operator {} is not emitted in the scalar tree evaluator",
                        other.name()
                    )),
                }
            }
        },
        CodeTree::Unary { op, value } => {
            let value = evaluate_tree(value, inputs, module)?;
            match op {
                TreeUnary::Neg => code_neg(&value),
                TreeUnary::Not => code_not(&value).map(CodeValue::Bool),
                TreeUnary::Pos => Ok(value),
            }
        }
        CodeTree::Tuple(_) | CodeTree::Index { .. } => Err(String::from(
            "tree: structured values are not emitted in the scalar tree evaluator",
        )),
        CodeTree::If {
            condition,
            then_value,
            else_value,
        } => {
            if code_as_bool(&evaluate_tree(condition, inputs, module)?)? {
                evaluate_tree(then_value, inputs, module)
            } else {
                evaluate_tree(else_value, inputs, module)
            }
        }
    }
}

/// View a Code value's tree (the Code-kind entry point the backend
/// renders for `quote.view`). The code parameter admits owned,
/// borrowed, and doubly-borrowed shapes (`impl Borrow`): emitted
/// registers hold either, and the walk must not care which.
pub fn view_quoted(code: impl std::borrow::Borrow<ExprCode>, module: &ModuleTable) -> NodeValue {
    let code = code.borrow();
    view_tree(&code.tree, module)
}
