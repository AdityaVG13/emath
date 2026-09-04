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
    /// One `reactions:` line (`r1: 2H2 + O2 -> 2H2O`, 04 section 3.1):
    /// a labeled stoichiometric multiset transformation. Reaction lines
    /// are T3 SECTION grammar — coefficients attach to species here even
    /// though expression juxtaposition stays refused (C15).
    Reaction {
        name: String,
        lhs: Vec<ReactionTerm>,
        arrow: ReactionArrow,
        rhs: Vec<ReactionTerm>,
    },
}

/// One term of a [`StmtKind::Reaction`] line: (coefficient, species).
#[derive(Clone, Debug, PartialEq)]
pub struct ReactionTerm {
    pub coefficient: u64,
    pub species: String,
}

/// Arrow kinds of a `reactions:` line (04 section 3.1). `->` and `=>`
/// share one lexer token, so both spellings denote the irreversible arrow;
/// the lambda/notation reading of `=>` has no production inside a
/// `reactions:` suite (no lambda position in the grammar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactionArrow {
    /// `->` (or the token-equivalent `=>`): irreversible kinetic.
    Irreversible,
    /// `<->`: reversible pair, both rates required.
    Reversible,
    /// `<=>`: equilibrium, thermodynamic constraint.
    Equilibrium,
}

impl ReactionArrow {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Irreversible => "->",
            Self::Reversible => "<->",
            Self::Equilibrium => "<=>",
        }
    }
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
    /// Left-associative `*` / `/` in a type (`m/s`, `m*m`).
    /// Operators are recorded so `m*m` is area, not a quotient, and
    /// `m/s*s` is length (C2), not acceleration.
    Product {
        left: Box<TypeExpr>,
        op: TypeProductOp,
        right: Box<TypeExpr>,
    },
    /// Unit power in a type (`m^2`).
    Pow {
        base: Box<TypeExpr>,
        exponent: i32,
    },
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

/// `*` or `/` between type factors in a unit annotation (`m/s`, `m*m`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeProductOp {
    /// `a * b`.
    Mul,
    /// `a / b`.
    Div,
}

impl TypeProductOp {
    /// Surface spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mul => "*",
            Self::Div => "/",
        }
    }
}

mod expr;

pub use expr::*;
