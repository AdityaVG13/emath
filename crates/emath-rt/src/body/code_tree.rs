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
    Tuple(Vec<CodeTree>),
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
/// and code values compare tree and dependencies. Mixed shapes are
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
        (NodeValue::Code(a), NodeValue::Code(b)) => a.tree == b.tree && a.deps == b.deps,
        _ => false,
    })
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
                .ok_or_else(|| format!("invalid_index: sequence index out of range"))
        }
        NodeValue::Sequence(items) if field == "length" => {
            Ok(NodeValue::Scalar(CodeValue::Int(items.len() as i64)))
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
    pub fn view(&self, module: &ModuleTable) -> Result<NodeValue, String> {
        view_node(self, module)
    }
    pub fn make(&self, module: &ModuleTable) -> Result<ExprCode, String> {
        make_quoted(self, module)
    }
    pub fn equals(&self, other: impl std::borrow::Borrow<NodeValue>) -> Result<bool, String> {
        node_eq(self, other)
    }
}

// ---------------------------------------------------------------------------
// The module table

/// The artifact's static module table: which names are module
/// callables (with their opacity, for view's `Global` layouts) and
/// each callable's declaration stamp (for dependency snapshots and
/// the `stale_dependency` refusal). Const-constructible so the
/// backend emits it as one crate-level static.
pub struct ModuleTable {
    /// (name, opaque) per module callable.
    pub globals: &'static [(&'static str, bool)],
    /// (name, declaration stamp) per module callable - the same
    /// FNV-1a digest the VM's `decl_stamp` computes.
    pub stamps: &'static [(&'static str, u64)],
}

impl ModuleTable {
    /// The empty table (no module callables).
    pub const EMPTY: ModuleTable = ModuleTable {
        globals: &[],
        stamps: &[],
    };

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

// ---------------------------------------------------------------------------
// Tree substitution and the tree evaluator

/// Substitute by name (the VM's `substitute_path` over the emitted
/// subset - which has no binders, so no capture avoidance applies):
/// a matching path becomes the replacement; an absent reference is a
/// no-op.
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
        CodeTree::Tuple(items) => CodeTree::Tuple(
            items
                .iter()
                .map(|item| substitute_tree(item, reference, replacement))
                .collect(),
        ),
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
        CodeTree::Tuple(items) => items.iter().for_each(|item| collect_free_names(item, free)),
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
/// `unbound_code`. Calls and global references refuse by name: the
/// emitted subset resolves callables at compile time, and a made
/// tree's runtime call graph is not an interpreter this lane ships.
pub fn evaluate_tree(
    tree: &CodeTree,
    inputs: &BTreeMap<String, CodeValue>,
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
            let _ = args;
            Err(format!(
                "tree: call `{name}` is not emitted in the scalar tree evaluator"
            ))
        }
        CodeTree::Binary { op, left, right } => match op {
            TreeBinary::And => {
                if code_as_bool(&evaluate_tree(left, inputs)?)? {
                    code_as_bool(&evaluate_tree(right, inputs)?)
                        .map(CodeValue::Bool)
                } else {
                    Ok(CodeValue::Bool(false))
                }
            }
            TreeBinary::Or => {
                if code_as_bool(&evaluate_tree(left, inputs)?)? {
                    Ok(CodeValue::Bool(true))
                } else {
                    code_as_bool(&evaluate_tree(right, inputs)?)
                        .map(CodeValue::Bool)
                }
            }
            _ => {
                let left = evaluate_tree(left, inputs)?;
                let right = evaluate_tree(right, inputs)?;
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
            let value = evaluate_tree(value, inputs)?;
            match op {
                TreeUnary::Neg => code_neg(&value),
                TreeUnary::Not => code_not(&value).map(CodeValue::Bool),
                TreeUnary::Pos => Ok(value),
            }
        }
        CodeTree::Tuple(_) => Err(String::from(
            "tree: structured values are not emitted in the scalar tree evaluator",
        )),
        CodeTree::If {
            condition,
            then_value,
            else_value,
        } => {
            if code_as_bool(&evaluate_tree(condition, inputs)?)? {
                evaluate_tree(then_value, inputs)
            } else {
                evaluate_tree(else_value, inputs)
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
