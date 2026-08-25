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

/// `notation infixl 40 "⋅" => core::math::dot [alias "*"]`.
/// Scoped to the declaring package, imported via `use`; `alias` offers
/// alternative spellings resolving to one canonical path. Typography, not meaning.
#[derive(Clone, Debug, PartialEq)]
pub struct NotationDecl {
    pub fixity: NotationFixity,
    pub precedence: u32,
    pub glyph: String,
    pub target: Vec<String>,
    /// N2 alias clause: `alias "*"` — alternative spelling for the same
    /// operator, resolving to one canonical path.
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
    /// Ordered body statements in source order; `sections()` filters
    /// section statements.
    pub body: Vec<Stmt>,
    /// Fn-like signature (`extern operator` or function head-args), `None`
    /// when the declaration uses section `inputs:` / `outputs:`.
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

/// A named section (`inputs:`) or heading statement (`evaluate <score>:`).
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
    /// `budget iterations = N`).
    Command {
        head: Vec<String>,
        argument: Option<CommandArgument>,
    },
    /// Full-expression equation (`mass * derivative(velocity) = rhs`)
    /// used in `equation:`/`constraint:` sections.
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

/// A generic argument at a use site (`Vector<Float64>`, `Mod<7>`,
/// `GF<2, 3, modulus = ...>`): type or value-level arguments (C10).
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
    /// Path type: `core::math::Real`, `Mod<7>`, `Tensor<Float64, [N, N]>`.
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
/// `m/s^2` = Div(Base("m"), Pow(Base("s"), 2)); `9.81 m` uses `Base("m")`.
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
    /// Flatten to (unit_name, power) pairs.
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

    /// Canonical form: sorted, combined factors rendered as numerator/
    /// denominator powers; equal-factor units converge (`m/(s*s)` and
    /// `m/s^2` → `m^1/s^2`).
    #[must_use]
    pub fn canonical_form(&self) -> String {
        let mut factors = self.flatten();
        factors.sort_by(|a, b| a.0.cmp(&b.0));
        let mut combined: Vec<(String, i32)> = Vec::new();
        for (name, power) in factors {
            if let Some(last) = combined.last_mut() {
                if last.0 == name {
                    last.1 += power;
                    continue;
                }
            }
            combined.push((name, power));
        }
        let num: Vec<&(String, i32)> = combined.iter().filter(|(_, p)| *p > 0).collect();
        let den: Vec<&(String, i32)> = combined.iter().filter(|(_, p)| *p < 0).collect();
        let fmt_part = |parts: &[&(String, i32)]| -> String {
            parts
                .iter()
                .map(|(name, power)| {
                    let abs_power = power.abs();
                    if abs_power == 1 {
                        name.clone()
                    } else {
                        format!("{name}^{abs_power}")
                    }
                })
                .collect::<Vec<_>>()
                .join("*")
        };
        match (num.is_empty(), den.is_empty()) {
            (true, true) => "1".to_string(),
            (false, true) => fmt_part(&num),
            (true, false) => format!("1/{}", fmt_part(&den)),
            (false, false) => format!("{}/{}", fmt_part(&num), fmt_part(&den)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Int(String),
    Float(String),
    Str(String),
    Bool(bool),
    /// `1 ms`, `9.81 [unit m/s^2]`: numeric value with attached unit.
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
        /// B02: optional `if <condition>` guard; the fold includes only
        /// iterations where the guard evaluates true.
        guard: Option<Box<Expr>>,
    },
    /// `derivative(x)`, `∂(T) wrt x` (partial), `total(T) wrt t` (total).
    /// `∂(H) wrt T holding p` — held-fixed set is part of the term.
    Derivative {
        value: Box<Expr>,
        wrt: Option<Vec<Expr>>,
        kind: DerivativeKind,
        /// Held-fixed variables; part of term identity (hash-relevant) —
        /// different holding sets produce different terms.
        holding: Vec<Expr>,
    },
    /// `solve(f) wrt x` — Newton's-method root-finding. The parser
    /// starts with `wrt: None`; the `wrt` postfix clause fills it.
    Solve {
        value: Box<Expr>,
        wrt: Option<Vec<Expr>>,
    },
    /// `minimize(f) wrt x` / `maximize(f) wrt x` — gradient-descent
    /// optimization; `maximize` selects which.
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
    /// `unit of E` or `dimension of E` — compile-time query usable
    /// in `require`, `tests:`, and `expect`.
    UnitQuery {
        kind: UnitQueryKind,
        expr: Box<Expr>,
    },
    /// `limit x -> 0: f(x)` — limit as a claim (B04), not a computation;
    /// one-sided via `0+`/`0-` (FromAbove/FromBelow).
    Limit {
        var: String,
        target: Box<Expr>,
        direction: LimitDirection,
        body: Box<Expr>,
    },
    /// `sample_limit x -> 0: f(x)` — numerical limit approximation (B04):
    /// samples the body approaching the target, returns best estimate.
    SampleLimit {
        var: String,
        target: Box<Expr>,
        direction: LimitDirection,
        body: Box<Expr>,
    },
    /// `cases x: | c1 => e1 | else => e2` (U1), lowers to nested
    /// conditionals; subject optional, arms are full expressions.
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
