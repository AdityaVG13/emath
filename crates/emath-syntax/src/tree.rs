//! Bootstrap syntax tree (lossless spans, provider-free).

use emath_core::Span;

#[derive(Clone, Debug, PartialEq)]
pub struct SyntaxTree {
    pub source: Span,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Use {
        path: Vec<String>,
        tree: UseTree,
        source: Span,
    },
    Declaration(Declaration),
}

#[derive(Clone, Debug, PartialEq)]
pub enum UseTree {
    All,
    Named(Vec<(String, Option<String>)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub args: Vec<String>,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Declaration {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub item_kind: String,
    pub as_kind: String,
    pub attributes: Vec<Attribute>,
    pub sections: Vec<Section>,
    pub source: Span,
    pub head_source: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenericParam {
    pub name: String,
    pub bound: Option<TypeExpr>,
    pub source: Span,
}

/// A named section such as `inputs:` or a heading statement such as
/// `evaluate <score>:`.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    pub generic: Option<String>,
    pub args: Option<Vec<Argument>>,
    pub suite: Suite,
    pub source: Span,
    pub head_source: Span,
}

/// A suite: the indented body of a section or block.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Suite {
    pub statements: Vec<Stmt>,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Section(Section),
    FieldDecl {
        visibility: Option<Visibility>,
        name: String,
        ty: TypeExpr,
        default: Option<Expr>,
    },
    FnDecl {
        visibility: Option<Visibility>,
        name: String,
        params: Vec<Param>,
        ret: Option<TypeExpr>,
        suite: Option<Suite>,
        source: Span,
    },
    OperatorDecl {
        name: String,
        params: Vec<Param>,
        ret: Option<TypeExpr>,
        source: Span,
    },
    Let {
        name: String,
        ty: Option<TypeExpr>,
        value: Expr,
    },
    Assign {
        target: Place,
        value: Expr,
    },
    Require(Expr),
    Ensure(Expr),
    Invariant(Expr),
    Given {
        name: String,
        value: Expr,
    },
    Expect(Expr),
    Expr(Expr),
    If {
        condition: Expr,
        then: Suite,
        else_branches: Vec<(Expr, Suite)>,
        else_tail: Option<Suite>,
    },
    BinderStmt {
        kind: BinderKind,
        binders: Vec<Binder>,
        suite: Suite,
    },
    SelfBlock {
        assignments: Vec<(String, Expr)>,
    },
    /// Generic word-headed command (`produce rust.library`,
    /// `budget iterations = N`, `compare Self against X`).
    Command {
        head: Vec<String>,
        argument: Option<CommandArgument>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Package,
    Private,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub by_ref: bool,
    pub default: Option<Expr>,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeExpr {
    pub kind: TypeKind,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    /// `Float64`, `core::math::Real`, `NonNegative<Real>`, `Field<K, Ω * Time>`
    Path {
        segments: Vec<String>,
        generic_args: Vec<TypeExpr>,
    },
    List(Vec<TypeExpr>),
    Tuple(Vec<TypeExpr>),
    Ref(Box<TypeExpr>),
    Product(Vec<TypeExpr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Int(String),
    Float(String),
    Str(String),
    Bool(bool),
    /// `1 ms`, `0 m`: numeric value with attached unit path.
    Quantity {
        value: Box<Expr>,
        unit: Vec<String>,
    },
    Path {
        segments: Vec<String>,
        generics: Option<Vec<TypeExpr>>,
    },
    Call {
        function: Box<Expr>,
        args: Vec<Expr>,
    },
    Index {
        value: Box<Expr>,
        indices: Vec<Expr>,
    },
    Unary {
        op: UnaryOp,
        value: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
    },
    Binder {
        kind: BinderKind,
        binders: Vec<Binder>,
        body: Box<Expr>,
    },
    /// `derivative(x)` or `derivative temperature wrt time`.
    Derivative {
        value: Box<Expr>,
        wrt: Option<Vec<Expr>>,
    },
    /// `temperature at time.start`
    At {
        value: Box<Expr>,
        location: Box<Expr>,
    },
    /// `temperature on boundary(Ω)`
    On {
        value: Box<Expr>,
        location: Box<Expr>,
    },
    /// `provider if condition` (strategy lists; parse-level only).
    Conditioned {
        value: Box<Expr>,
        condition: Box<Expr>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Pos,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinderKind {
    Sum,
    Product,
    Integral,
    ForAll,
    Exists,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Binder {
    pub name: String,
    pub domain: Option<Expr>,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub segments: Vec<String>,
    pub indices: Vec<Expr>,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandArgument {
    Expr(Expr),
    List(Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Argument {
    pub name: Option<String>,
    pub value: ArgumentValue,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArgumentValue {
    Expr(Expr),
    /// Type-expr arguments such as `w: Witness`.
    Type(TypeExpr),
}
