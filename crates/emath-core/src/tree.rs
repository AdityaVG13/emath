//! Bootstrap syntax tree (lossless spans, provider-free).

use crate::Span;

#[derive(Clone, Debug, PartialEq)]
pub struct SyntaxTree {
    pub source: Span,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// `package examples.square`: the package identity line.
    Package {
        path: Vec<String>,
        source: Span,
    },
    Use {
        path: Vec<String>,
        tree: UseTree,
        source: Span,
    },
    Declaration(Declaration),
    /// `notation infixl 40 "⋅" => core::math::dot [alias "*"]`
    Notation(NotationDecl),
}

#[derive(Clone, Debug, PartialEq)]
pub enum UseTree {
    All,
    Named(Vec<(String, Option<String>)>),
}

/// Fixity for a `notation` declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotationFixity {
    Prefix,
    Postfix,
    InfixLeft,
    InfixRight,
    Infix,
}

/// `notation infixl 40 "⋅" => core::math::dot [alias "*"]`
///
/// N1: Notation declarations are scoped to the package that declares them
/// and are imported via `use`.  N2: The optional `alias` clause provides
/// an alternative spelling (accept-many/canon-one: multiple aliases resolve
/// to one canonical path).  N5: Notation is typography, not meaning —
/// removing a notation import never changes semantic identity.
#[derive(Clone, Debug, PartialEq)]
pub struct NotationDecl {
    pub fixity: NotationFixity,
    pub precedence: u32,
    pub glyph: String,
    pub target: Vec<String>,
    /// N2 alias clause: `alias "*"` — an alternative spelling for the
    /// same operator.  Multiple aliases map to one canonical path.
    pub alias: Option<String>,
    pub source: Span,
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
    /// Ordered body statements. Section-headed blocks (and any other
    /// statement form) appear in source order; `sections()` filters the
    /// section statements.
    pub body: Vec<Stmt>,
    /// Fn-like declaration signature: `extern operator ...(params) -> Ret`
    /// or a stateless `emath function name(args) -> T` head. `None` when
    /// the declaration uses section `inputs:` / `outputs:` (or omits both).
    pub signature: Option<DeclarationSignature>,
    pub source: Span,
    pub head_source: Span,
}

/// Fn-like signature carried on an item (`extern operator` or function head-args).
#[derive(Clone, Debug, PartialEq)]
pub struct DeclarationSignature {
    /// Parameter list.
    pub params: Vec<Param>,
    /// Optional result type after `->`.
    pub ret: Option<TypeExpr>,
}

impl Declaration {
    /// Section statements in source order.
    pub fn sections(&self) -> impl Iterator<Item = &Section> {
        self.body.iter().filter_map(|stmt| match &stmt.kind {
            StmtKind::Section(section) => Some(section),
            _ => None,
        })
    }

    /// Owned sections (consumed by admission and goal elaboration).
    #[must_use]
    pub fn sections_vec(&self) -> Vec<Section> {
        self.sections().cloned().collect()
    }
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
        /// Head word: `fn`, or a fn-like section head such as
        /// `constructor`, `define`, or `method`. Preserved losslessly.
        head: String,
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
        /// B02: optional `if <condition>` guard clause.
        guard: Option<Box<Expr>>,
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
    /// A full-expression equation (`mass * derivative(velocity) = rhs`,
    /// `a * a + b * b = c * c`) used in `equation:`/`constraint:` sections.
    Equation {
        left: Expr,
        right: Expr,
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

/// A generic argument at a use site: `Vector<Float64>`, `Mod<7>`,
/// `Tensor<Float64, [N, N]>`, `GF<2, 3, modulus = x^3 + x + 1>`.
///
/// C10: The grammar previously admitted types only at use sites. This
/// enum allows value-level arguments (literals, expressions, named
/// args, bracket-list extents) alongside type arguments.
#[derive(Clone, Debug, PartialEq)]
pub enum GenericArg {
    /// A type argument: `Float64`, `Real`, `NonNegative`
    Type(TypeExpr),
    /// A value argument: `7`, `[N, N]`, `x^3 + x + 1`
    Value(Expr),
    /// A named argument: `modulus = x^3 + x + 1`, `extent = [N, N]`
    Named { name: String, arg: Box<GenericArg> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    /// `Float64`, `core::math::Real`, `NonNegative<Real>`, `Field<K, Ω * Time>`
    /// `Mod<7>`, `Tensor<Float64, [N, N]>`, `GF<2, 3, modulus = ...>`
    Path {
        segments: Vec<String>,
        generic_args: Vec<GenericArg>,
    },
    List(Vec<TypeExpr>),
    Tuple(Vec<TypeExpr>),
    Ref(Box<TypeExpr>),
    Product(Vec<TypeExpr>),
    /// `Float64 in m` / `Float64 in m/s`: a numeric type with a unit annotation.
    In {
        base: Box<TypeExpr>,
        unit: Box<TypeExpr>,
    },
    /// `Float64 in [0, 1]` / `Float64 in [a, b]`: a numeric type with
    /// a bounded domain (U5). Values outside [lo, hi] are a type error.
    Domain {
        base: Box<TypeExpr>,
        lo: Box<Expr>,
        hi: Box<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub source: Span,
}

/// Compound unit expression for bracket-notation units (F7/U4).
/// `m/s^2` = Div(Base("m"), Pow(Base("s"), 2))
/// `kg*m^2/s^2` = Div(Mul(Base("kg"), Pow(Base("m"), 2)), Pow(Base("s"), 2))
/// Simple units like `9.81 m` use `Base("m")`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitExpr {
    /// Single unit identifier: `m`, `s`, `kg`.
    Base(String),
    /// Multiplication: `a * b`.
    Mul(Box<UnitExpr>, Box<UnitExpr>),
    /// Division: `a / b`.
    Div(Box<UnitExpr>, Box<UnitExpr>),
    /// Power: `a^n` (n is an integer exponent).
    Pow(Box<UnitExpr>, i32),
}

impl UnitExpr {
    /// Flatten to a list of (unit_name, power) pairs.
    /// `m/s^2` → `[("m", 1), ("s", -2)]`
    #[must_use]
    pub fn flatten(&self) -> Vec<(String, i32)> {
        match self {
            Self::Base(name) => vec![(name.clone(), 1)],
            Self::Mul(left, right) => {
                let mut result = left.flatten();
                result.extend(right.flatten());
                result
            }
            Self::Div(left, right) => {
                let mut result = left.flatten();
                for (name, power) in right.flatten() {
                    result.push((name, -power));
                }
                result
            }
            Self::Pow(base, exponent) => {
                base.flatten().into_iter().map(|(name, p)| (name, p * exponent)).collect()
            }
        }
    }

    /// Format as a unit string: `m/s^2`, `kg*m^2/s^2`.
    #[must_use]
    pub fn to_string(&self) -> String {
        match self {
            Self::Base(name) => name.clone(),
            Self::Mul(left, right) => format!("{}*{}", left.to_string(), right.to_string()),
            Self::Div(left, right) => format!("{}/{}", left.to_string(), right.to_string()),
            Self::Pow(base, exp) => format!("{}^{}", base.to_string(), exp),
        }
    }

    /// Whether this is a simple single-unit expression (no compound operators).
    #[must_use]
    pub fn is_simple(&self) -> bool {
        matches!(self, Self::Base(_))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Int(String),
    Float(String),
    Str(String),
    Bool(bool),
    /// `1 ms`, `0 m`: numeric value with attached unit.
    /// `9.81 [unit m/s^2]`: numeric value with compound unit bracket.
    Quantity {
        value: Box<Expr>,
        unit: UnitExpr,
    },
    Path {
        segments: Vec<String>,
        generics: Option<Vec<GenericArg>>,
    },
    Call {
        function: Box<Expr>,
        args: Vec<Expr>,
    },
    Index {
        value: Box<Expr>,
        indices: Vec<Expr>,
    },
    /// Index-axis slice `i:j`, `i:`, `:j`, or `:`. Rank-preserving.
    Slice {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
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
        /// B02: optional `if <condition>` guard clause on the binder.
        /// When present, the fold only includes iterations where the
        /// guard evaluates to true.
        guard: Option<Box<Expr>>,
    },
    /// `derivative(x)`, `∂(T) wrt x` (partial), `total(T) wrt t` (total).
    /// `∂(H) wrt T holding p` — held-fixed set is part of the term.
    Derivative {
        value: Box<Expr>,
        wrt: Option<Vec<Expr>>,
        kind: DerivativeKind,
        /// Held-fixed set: variables held constant during differentiation.
        /// `∂(H) wrt T holding p` — part of the term's identity (hash-relevant).
        /// Different holding sets produce different terms.
        holding: Vec<Expr>,
    },
    /// `solve(f) wrt x` — Newton's-method root-finding.
    /// The parser creates `Solve { value, wrt: None }` and the `wrt`
    /// postfix clause attaches `wrt: Some(...)`.
    Solve {
        value: Box<Expr>,
        wrt: Option<Vec<Expr>>,
    },
    /// `minimize(f) wrt x` or `maximize(f) wrt x` — gradient-descent
    /// optimization.  `maximize` is true for `maximize`, false for
    /// `minimize`.
    Optimize {
        value: Box<Expr>,
        wrt: Option<Vec<Expr>>,
        maximize: bool,
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
    /// `unit of E` or `dimension of E` — compile-time query.
    /// Returns the unit expression or named dimension of E.
    /// Usable in `require`, `tests:`, and `expect`.
    UnitQuery {
        kind: UnitQueryKind,
        expr: Box<Expr>,
    },
    /// `limit x -> 0: f(x)` — limit as a claim (B04).
    /// Not a computation; usable in `require`/`ensure`/`invariant`.
    /// One-sided: `limit x -> 0+: f(x)` (FromAbove), `limit x -> 0-: f(x)` (FromBelow).
    Limit {
        var: String,
        target: Box<Expr>,
        direction: LimitDirection,
        body: Box<Expr>,
    },
    /// `sample_limit x -> 0: f(x)` — numerical limit approximation (B04).
    /// A computation that samples the body at points approaching the target
    /// and returns the best-estimate limit value.
    SampleLimit {
        var: String,
        target: Box<Expr>,
        direction: LimitDirection,
        body: Box<Expr>,
    },
    /// `cases x: | c1 => e1 | c2 => e2 | else => e3` (U1).
    /// Lowers to nested conditional expressions.
    /// The subject is optional (for readability; arm conditions are
    /// full expressions, not pattern matches).
    Cases {
        subject: Option<Box<Expr>>,
        arms: Vec<(Expr, Expr)>,
        else_arm: Box<Expr>,
    },
}

/// Kind of compile-time unit/dimension query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitQueryKind {
    /// `unit of E` — returns the unit expression.
    Unit,
    /// `dimension of E` — returns the named dimension.
    Dimension,
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
    /// `==>` — logical implication (right-associative).
    Imply,
    /// `<==>` — logical biconditional.
    Iff,
    /// `~~` — asymptotic equivalence (B18). Lowers to a limit claim.
    Asymp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinderKind {
    Sum,
    Product,
    Integral,
    ForAll,
    Exists,
    /// `series n in 0..inf: a[n]` — series convergence claim (B06).
    Series,
}

/// Direction for one-sided limits (B04).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitDirection {
    /// Two-sided limit: `limit x -> 0: f(x)`
    TwoSided,
    /// From above: `limit x -> 0+: f(x)`
    FromAbove,
    /// From below: `limit x -> 0-: f(x)`
    FromBelow,
}

/// Kind of derivative operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DerivativeKind {
    /// `derivative(x)` — unqualified (existing behavior).
    Plain,
    /// `∂(T)` / `partial(T)` — partial derivative.
    /// Requires explicit `holding` set or refused as MeaningHole.
    Partial,
    /// `total(T)` / `d(T)` — total/material derivative.
    Total,
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
    /// `define y = expr` / `method score = score`: a trailing `name = value`.
    Assignment {
        name: String,
        value: Expr,
    },
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
