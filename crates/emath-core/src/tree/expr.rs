//! Expression syntax: Expr, ExprKind, operators, binders, args.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub source: Span,
}

/// Declared tolerance on an `≈` edge: `within rtol=…, atol=…`. Values are
/// stored as expressions so the formatter round-trips byte-exactly;
/// numeric admission of the tolerance values is lowering's job.
#[derive(Clone, Debug, PartialEq)]
pub struct ApproxTolerance {
    pub rtol: Option<Expr>,
    pub atol: Option<Expr>,
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
            Self::Pow(base, exponent) => base
                .flatten()
                .into_iter()
                .map(|(name, p)| (name, p * exponent))
                .collect(),
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

/// `[(0.0 s, 0.0 V), ...] with interpolation: <mode>` (04 §5.4): how the
/// series produces values between sample points. Declared, hashes into
/// identity; there is no silent default spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeriesInterpolation {
    /// Step function: value of the sample at or before the query time.
    Previous,
    /// Straight line between neighboring samples.
    Linear,
    /// Value of the nearest sample.
    Nearest,
    /// Piecewise-constant on `[t_i, t_{i+1})`.
    Pwc,
    /// Shape-preserving cubic (no overshoot on monotone data).
    MonotoneCubic,
}

impl SeriesInterpolation {
    /// Canonical spelling (04 §5.4).
    #[must_use]
    pub fn spelling(&self) -> &'static str {
        match self {
            Self::Previous => "previous",
            Self::Linear => "linear",
            Self::Nearest => "nearest",
            Self::Pwc => "pwc",
            Self::MonotoneCubic => "monotone_cubic",
        }
    }
}

/// `extrapolation: <mode>` (04 §5.4): what happens outside the sampled
/// support. The default is `refuse` — silent extrapolation is a quiet
/// killer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeriesExtrapolation {
    /// Refuse evaluation outside support (the default).
    Refuse,
    /// Clamp to the nearest endpoint value.
    Clamp,
    /// Extend the first/last segment.
    Extend,
}

impl SeriesExtrapolation {
    /// Canonical spelling (04 §5.4).
    #[must_use]
    pub fn spelling(&self) -> &'static str {
        match self {
            Self::Refuse => "refuse",
            Self::Clamp => "clamp",
            Self::Extend => "extend",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Int(String),
    Float(String),
    /// Exact rational `3//7`. Numerator and denominator keep integer
    /// spellings (underscores preserved). Grammar: `rational_literal`.
    Rational {
        numer: String,
        denom: String,
    },
    Str(String),
    Bool(bool),
    /// `1 ms`, `9.81 [unit m/s^2]`: numeric value with attached unit.
    Quantity {
        value: Box<Expr>,
        unit: UnitExpr,
    },
    /// Measurement literal (spec 04 section 1.5 / ).
    /// Two spellings: explicit `value ± uncertainty` (`uncertainty_digits`
    /// empty) and attached CODATA parenthetical `value(digits)` (digits stay
    /// raw; scaling `d × 10^(exp−frac)` is admission's job). The optional
    /// distribution tag name (`~ normal | uniform | lognormal`) is recorded
    /// raw; provenance defaults to Unstated and is recorded loudly.
    Measured {
        value: String,
        uncertainty: String,
        uncertainty_digits: String,
        distribution: Option<String>,
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
    /// `a ≈ b` (ASCII `a ~= b`) — approximation labeling operator (04 §6.4). A first-class relation that
    /// stamps authority: computing through an `≈` edge is
    /// authority-degraded, never silently exact. The optional
    /// `within rtol=…, atol=…` clause is the DECLARED tolerance; a bare
    /// `≈` (no clause) refuses `E-APPROX-TOL` at admission rather than
    /// masquerading as exactness.
    Approx {
        left: Box<Expr>,
        right: Box<Expr>,
        tolerance: Option<Box<ApproxTolerance>>,
    },
    If {
        condition: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    List(Vec<Expr>),
    /// `|x y| 1, 2 | 3, 4 |` — table literal (U9). Named columns plus
    /// comma-separated rows; ≥2 headers keep the leading `|` unambiguous
    /// with cases arms. Lowers through the Matrix element path (numeric
    /// cells), with headers recorded only in the sema receipt.
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<Expr>>,
    },
    /// `{2, 3, 5}` — set literal (B01). Finite-set carrier; elements are
    /// deduplicated and order-canonicalized at evaluation. Bare `{name:
    /// value}` (record spelling without a path prefix) is ambiguous and
    /// refuses `E-SYN-154` at parse time, never silently a one-element set.
    Set(Vec<Expr>),
    /// `{n in 0..100 if is_prime(n)}` — set comprehension (B01). Desugars
    /// from brace position where the parsed element expression is a
    /// top-level membership binary (`element in domain`) optionally
    /// followed by an `if` guard; the membership reading inside braces is
    /// the comprehension binding, never a Bool element test.
    SetComprehension {
        element: Box<Expr>,
        var: String,
        domain: Box<Expr>,
        guard: Option<Box<Expr>>,
    },
    /// `Point:{x: 1.0, y: 2.0}` — inline record literal (U3). Path-prefixed
    /// braces distinguish records from sets under one ELP ambiguity scan.
    Record {
        type_path: Vec<String>,
        fields: Vec<(String, Expr)>,
    },
    Tuple(Vec<Expr>),
    /// `[(0.0 s, 0.0 V), ...] with interpolation: linear, extrapolation:
    /// refuse` (04 §5.4): a time-series data
    /// literal with its declared interpretation policy. The policy is
    /// part of the value's identity — it changes every downstream
    /// number. `None` means the language default (`refuse`), spelled in
    /// admission. Evaluation semantics are the named next slice.
    WithSeriesPolicy {
        value: Box<Expr>,
        interpolation: Option<SeriesInterpolation>,
        extrapolation: Option<SeriesExtrapolation>,
    },
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
    /// `minimize(f) wrt x` / `maximize(f) wrt x` — Newton on ∇f = 0;
    /// `maximize` requires negative curvature.
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
    /// `v in s` — set membership (B01). ASCII for ∈. In expression
    /// position this is the membership operator; in binder keyword
    /// position (`sum n in 0..10`) `in` is consumed by the binder
    /// parser, so the two uses are provably disjoint (X13 charter).
    In,
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
