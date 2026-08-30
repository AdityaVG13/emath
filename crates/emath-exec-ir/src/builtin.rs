//! Builtin function registry: single source of truth for unary/binary
//! math builtins. Replaces 23 individual EmirOp variants with 2 generic
//! variants (UnaryBuiltin, BinaryBuiltin) that dispatch through BuiltinId.
//!
//! Adding a new unary/binary math builtin = 1 enum variant + 1 arm per method
//! in this file (~10 LOC). Previously: ~30 LOC spread across 4 files.

use crate::DomainObligation;
use std::f64::consts::{LN_2, LN_10};

/// Identifier for a registered builtin math function.
/// Replaces individual EmirOp variants (Exp, Sin, Cos, ...) with a
/// single `UnaryBuiltin(BuiltinId, EmirValue)` op.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltinId {
    // Unary f64 -> f64 (19 builtins)
    Exp,
    Ln,
    Sqrt,
    Sin,
    Cos,
    Tan,
    Tanh,
    Abs,
    Floor,
    Ceil,
    Round,
    Sign,
    Log2,
    Log10,
    Sinh,
    Cosh,
    Atan,
    Cbrt,
    Recip,
    Fract,
    // Binary f64 x f64 -> f64 (5 builtins)
    Hypot,
    Min,
    Max,
    Atan2,
    Mod,
}

impl BuiltinId {
    /// Look up a builtin by its surface name (after namespace stripping).
    /// `"ln"` and `"log"` both map to `Ln`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "exp" => Some(Self::Exp),
            "ln" | "log" => Some(Self::Ln),
            "sqrt" => Some(Self::Sqrt),
            "sin" => Some(Self::Sin),
            "cos" => Some(Self::Cos),
            "tan" => Some(Self::Tan),
            "tanh" => Some(Self::Tanh),
            "abs" => Some(Self::Abs),
            "floor" => Some(Self::Floor),
            "ceil" => Some(Self::Ceil),
            "round" => Some(Self::Round),
            "sign" => Some(Self::Sign),
            "log2" => Some(Self::Log2),
            "log10" => Some(Self::Log10),
            "sinh" => Some(Self::Sinh),
            "cosh" => Some(Self::Cosh),
            "atan" => Some(Self::Atan),
            "cbrt" => Some(Self::Cbrt),
            "recip" => Some(Self::Recip),
            "fract" => Some(Self::Fract),
            "hypot" => Some(Self::Hypot),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            "atan2" => Some(Self::Atan2),
            "mod" => Some(Self::Mod),
            _ => None,
        }
    }

    /// Surface name for diagnostics and codegen.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Exp => "exp",
            Self::Ln => "ln",
            Self::Sqrt => "sqrt",
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Tan => "tan",
            Self::Tanh => "tanh",
            Self::Abs => "abs",
            Self::Floor => "floor",
            Self::Ceil => "ceil",
            Self::Round => "round",
            Self::Sign => "sign",
            Self::Log2 => "log2",
            Self::Log10 => "log10",
            Self::Sinh => "sinh",
            Self::Cosh => "cosh",
            Self::Atan => "atan",
            Self::Cbrt => "cbrt",
            Self::Recip => "recip",
            Self::Fract => "fract",
            Self::Hypot => "hypot",
            Self::Min => "min",
            Self::Max => "max",
            Self::Atan2 => "atan2",
            Self::Mod => "mod",
        }
    }

    /// Number of operands (1 = unary, 2 = binary).
    #[must_use]
    pub fn arity(&self) -> usize {
        match self {
            Self::Exp
            | Self::Ln
            | Self::Sqrt
            | Self::Sin
            | Self::Cos
            | Self::Tan
            | Self::Tanh
            | Self::Abs
            | Self::Floor
            | Self::Ceil
            | Self::Round
            | Self::Sign
            | Self::Log2
            | Self::Log10
            | Self::Sinh
            | Self::Cosh
            | Self::Atan
            | Self::Cbrt
            | Self::Recip
            | Self::Fract => 1,
            Self::Hypot | Self::Min | Self::Max | Self::Atan2 | Self::Mod => 2,
        }
    }

    // ── Interpreter evaluation ──────────────────────────────────────────

    /// Evaluate a unary builtin on an f64.
    #[must_use]
    pub fn eval_unary(&self, x: f64) -> f64 {
        match self {
            Self::Exp => x.exp(),
            Self::Ln => x.ln(),
            Self::Sqrt => x.sqrt(),
            Self::Sin => x.sin(),
            Self::Cos => x.cos(),
            Self::Tan => x.tan(),
            Self::Tanh => x.tanh(),
            Self::Abs => x.abs(),
            Self::Floor => x.floor(),
            Self::Ceil => x.ceil(),
            Self::Round => x.round(),
            // Mathematical sgn: sgn(0) = 0. IEEE `signum` returns ±1 at ±0.
            Self::Sign => {
                if x == 0.0 {
                    0.0
                } else {
                    x.signum()
                }
            }
            Self::Log2 => x.log2(),
            Self::Log10 => x.log10(),
            Self::Sinh => x.sinh(),
            Self::Cosh => x.cosh(),
            Self::Atan => x.atan(),
            Self::Cbrt => x.cbrt(),
            Self::Recip => x.recip(),
            Self::Fract => x.fract(),
            _ => unreachable!("not a unary builtin"),
        }
    }

    /// Evaluate a binary builtin on two f64s.
    #[must_use]
    pub fn eval_binary(&self, a: f64, b: f64) -> f64 {
        match self {
            Self::Hypot => a.hypot(b),
            Self::Min => a.min(b),
            Self::Max => a.max(b),
            Self::Atan2 => a.atan2(b),
            Self::Mod => a % b,
            _ => unreachable!("not a binary builtin"),
        }
    }

    // ── Forward-mode AD (dual numbers) ────────────────────────────────

    /// Forward-mode dual evaluation for a unary builtin.
    /// Returns (primal_out, tangent_out).
    /// For non-differentiable functions (floor, ceil, round, sign),
    /// tangent is 0.
    #[must_use]
    pub fn eval_dual_unary(&self, primal: f64, tangent: f64) -> (f64, f64) {
        match self {
            Self::Exp => {
                let p = primal.exp();
                (p, p * tangent)
            }
            // Domain violation (primal < 0) makes the primal NaN; the
            // tangent must be NaN too — the raw quotient (e.g. 1/(-1))
            // would be a finite value silently masquerading as a
            // derivative. ln(0) = -inf is not NaN, so the singularity at
            // 0 keeps IEEE propagation (tangent +/-inf), matching the
            // no-zeroed-singularities convention in this file.
            Self::Ln => {
                let p = primal.ln();
                let t = tangent / primal;
                (p, if p.is_nan() { f64::NAN } else { t })
            }
            Self::Sqrt => {
                let p = primal.sqrt();
                (p, tangent / (2.0 * p))
            }
            Self::Sin => (primal.sin(), primal.cos() * tangent),
            Self::Cos => (primal.cos(), -primal.sin() * tangent),
            Self::Tan => {
                let c = primal.cos();
                (primal.tan(), tangent / (c * c))
            }
            Self::Tanh => {
                let t = primal.tanh();
                (t, (1.0 - t * t) * tangent)
            }
            // abs'(x) = sgn(x); this crate's sgn(0) = 0, not IEEE signum(±0)=±1.
            Self::Abs => (primal.abs(), Self::Sign.eval_unary(primal) * tangent),
            Self::Floor => (primal.floor(), 0.0),
            Self::Ceil => (primal.ceil(), 0.0),
            Self::Round => (primal.round(), 0.0),
            Self::Sign => (if primal == 0.0 { 0.0 } else { primal.signum() }, 0.0),
            // Same NaN-primal guard as Ln: log2/log10 of a negative have
            // no derivative; the tangent must never be a finite quotient.
            Self::Log2 => {
                let p = primal.log2();
                let t = tangent / (primal * LN_2);
                (p, if p.is_nan() { f64::NAN } else { t })
            }
            Self::Log10 => {
                let p = primal.log10();
                let t = tangent / (primal * LN_10);
                (p, if p.is_nan() { f64::NAN } else { t })
            }
            Self::Sinh => (primal.sinh(), primal.cosh() * tangent),
            Self::Cosh => (primal.cosh(), primal.sinh() * tangent),
            Self::Atan => {
                let d = 1.0 / (1.0 + primal * primal);
                (primal.atan(), d * tangent)
            }
            Self::Cbrt => {
                let p = primal.cbrt();
                (p, tangent / (3.0 * p * p))
            }
            Self::Recip => {
                let p = primal.recip();
                (p, -tangent * p * p)
            }
            Self::Fract => (primal.fract(), tangent),
            _ => unreachable!("not a unary builtin"),
        }
    }

    /// Forward-mode dual evaluation for a binary builtin.
    /// Returns (primal_out, tangent_out).
    #[must_use]
    pub fn eval_dual_binary(&self, pa: f64, ta: f64, pb: f64, tb: f64) -> (f64, f64) {
        match self {
            Self::Hypot => {
                let h = pa.hypot(pb);
                let tangent = if h == 0.0 {
                    0.0
                } else {
                    (pa * ta + pb * tb) / h
                };
                (h, tangent)
            }
            Self::Min => {
                if pa <= pb {
                    (pa, ta)
                } else {
                    (pb, tb)
                }
            }
            Self::Max => {
                if pa >= pb {
                    (pa, ta)
                } else {
                    (pb, tb)
                }
            }
            Self::Atan2 => {
                // ∂/∂a atan2(a,b) = b/(a²+b²), ∂/∂b = -a/(a²+b²).
                // The (1+(a/b)²) form is NaN at b=0 (e.g. atan2(1,0)).
                let p = pa.atan2(pb);
                let denom = pa * pa + pb * pb;
                let tangent = if denom != 0.0 {
                    (ta * pb - tb * pa) / denom
                } else {
                    0.0
                };
                (p, tangent)
            }
            Self::Mod => {
                // d/da [a mod b] = 1 at non-boundary points; b is not differentiable
                (pa % pb, ta)
            }
            _ => unreachable!("not a binary builtin"),
        }
    }

    // ── Reverse-mode AD (adjoint propagation) ─────────────────────────

    /// Backward pass for a unary builtin.
    /// Given the primal input and primal output, and the adjoint of the
    /// output, returns the adjoint to propagate to the input.
    #[must_use]
    pub fn backward_unary(&self, primal_in: f64, primal_out: f64, adj: f64) -> f64 {
        match self {
            Self::Exp => adj * primal_out, // d/dx exp(x) = exp(x)
            // IEEE like dual: do not zero singularities (ln/sqrt/recip at 0).
            // A NaN primal_out means the forward value already violated the
            // domain (e.g. ln of a negative): the adjoint must be NaN too,
            // never the finite adj/primal_in quotient.
            Self::Ln => {
                if primal_out.is_nan() {
                    f64::NAN
                } else {
                    adj / primal_in
                }
            }
            Self::Sqrt => adj / (2.0 * primal_out),
            Self::Sin => adj * primal_in.cos(),
            Self::Cos => -adj * primal_in.sin(),
            Self::Tan => {
                let c = primal_in.cos();
                adj / (c * c)
            }
            Self::Tanh => adj * (1.0 - primal_out * primal_out),
            Self::Abs => adj * Self::Sign.eval_unary(primal_in),
            Self::Floor | Self::Ceil | Self::Round | Self::Sign => 0.0,
            // Same NaN-primal guard as Ln: log2/log10 of a negative have
            // no gradient; the adjoint must never be a finite quotient.
            Self::Log2 => {
                if primal_out.is_nan() {
                    f64::NAN
                } else {
                    adj / (primal_in * LN_2)
                }
            }
            Self::Log10 => {
                if primal_out.is_nan() {
                    f64::NAN
                } else {
                    adj / (primal_in * LN_10)
                }
            }
            Self::Sinh => adj * primal_in.cosh(),
            Self::Cosh => adj * primal_in.sinh(),
            Self::Atan => adj / (1.0 + primal_in * primal_in),
            Self::Cbrt => adj / (3.0 * primal_out * primal_out),
            Self::Recip => -adj / (primal_in * primal_in),
            Self::Fract => adj,
            _ => unreachable!("not a unary builtin"),
        }
    }

    /// Backward pass for a binary builtin.
    /// Given both primal inputs, the primal output, and the adjoint of
    /// the output, returns (adjoint_a, adjoint_b).
    #[must_use]
    pub fn backward_binary(&self, pa: f64, pb: f64, primal_out: f64, adj: f64) -> (f64, f64) {
        match self {
            Self::Hypot => {
                if primal_out != 0.0 {
                    (adj * pa / primal_out, adj * pb / primal_out)
                } else {
                    (0.0, 0.0)
                }
            }
            Self::Min => {
                if pa <= pb {
                    (adj, 0.0)
                } else {
                    (0.0, adj)
                }
            }
            Self::Max => {
                if pa >= pb {
                    (adj, 0.0)
                } else {
                    (0.0, adj)
                }
            }
            Self::Atan2 => {
                let denom = pa * pa + pb * pb;
                if denom != 0.0 {
                    (adj * pb / denom, -adj * pa / denom)
                } else {
                    (0.0, 0.0)
                }
            }
            Self::Mod => (adj, 0.0), // d/da = 1, d/db = 0
            _ => unreachable!("not a binary builtin"),
        }
    }

    // ── Rust codegen ──────────────────────────────────────────────────

    /// Rust codegen for a unary builtin: e.g. `"x.exp()"`.
    #[must_use]
    pub fn rust_unary(&self, arg: &str) -> String {
        match self {
            Self::Exp => format!("{arg}.exp()"),
            Self::Ln => format!("{arg}.ln()"),
            Self::Sqrt => format!("{arg}.sqrt()"),
            Self::Sin => format!("{arg}.sin()"),
            Self::Cos => format!("{arg}.cos()"),
            Self::Tan => format!("{arg}.tan()"),
            Self::Tanh => format!("{arg}.tanh()"),
            Self::Abs => format!("{arg}.abs()"),
            Self::Floor => format!("{arg}.floor()"),
            Self::Ceil => format!("{arg}.ceil()"),
            Self::Round => format!("{arg}.round()"),
            Self::Sign => format!("if {arg} == 0.0 {{ 0.0 }} else {{ {arg}.signum() }}"),
            Self::Log2 => format!("{arg}.log2()"),
            Self::Log10 => format!("{arg}.log10()"),
            Self::Sinh => format!("{arg}.sinh()"),
            Self::Cosh => format!("{arg}.cosh()"),
            Self::Atan => format!("{arg}.atan()"),
            Self::Cbrt => format!("{arg}.cbrt()"),
            Self::Recip => format!("{arg}.recip()"),
            Self::Fract => format!("{arg}.fract()"),
            _ => unreachable!("not a unary builtin"),
        }
    }

    /// Rust codegen for a binary builtin: e.g. `"a.min(b)"`.
    #[must_use]
    pub fn rust_binary(&self, a: &str, b: &str) -> String {
        match self {
            Self::Hypot => format!("{a}.hypot({b})"),
            Self::Min => format!("{a}.min({b})"),
            Self::Max => format!("{a}.max({b})"),
            Self::Atan2 => format!("{a}.atan2({b})"),
            Self::Mod => format!("{a} % {b}"),
            _ => unreachable!("not a binary builtin"),
        }
    }

    // ── Symbolic AD formulas for codegen ─────────────────────────────
    //
    // The interpreter differentiates with numbers (eval_dual_*/backward_*).
    // Generated Rust differentiates with source formulas; these methods
    // return the formula strings so the AD math lives in one place.
    // `e`/`p` map a register to its primal-reference name, `d` maps it to
    // its tangent-reference name, and `idx` is the output register.

    /// Forward-mode tangent formula for a unary builtin.
    #[must_use]
    pub fn rust_tangent_unary(
        &self,
        e: &dyn Fn(u32) -> String,
        d: &dyn Fn(u32) -> String,
        idx: u32,
        input: u32,
    ) -> String {
        let e_in = e(input);
        let d_in = d(input);
        let e_out = e(idx);
        match self {
            Self::Exp => format!("{e_out} * {d_in}"),
            Self::Ln => format!("{d_in} / {e_in}"),
            Self::Sqrt => format!("{d_in} / (2.0 * {e_out})"),
            Self::Sin => format!("{e_in}.cos() * {d_in}"),
            Self::Cos => format!("-{e_in}.sin() * {d_in}"),
            Self::Tan => format!("{d_in} / ({e_in}.cos() * {e_in}.cos())"),
            Self::Tanh => format!("(1.0 - {e_out} * {e_out}) * {d_in}"),
            Self::Abs => {
                format!("(if {e_in} == 0.0 {{ 0.0 }} else {{ {e_in}.signum() }}) * {d_in}")
            }
            Self::Floor | Self::Ceil | Self::Round | Self::Sign => "0.0".to_string(),
            Self::Log2 => format!("{d_in} / ({e_in} * std::f64::consts::LN_2)"),
            Self::Log10 => format!("{d_in} / ({e_in} * std::f64::consts::LN_10)"),
            Self::Sinh => format!("{e_in}.cosh() * {d_in}"),
            Self::Cosh => format!("{e_in}.sinh() * {d_in}"),
            Self::Atan => format!("{d_in} / (1.0 + {e_in} * {e_in})"),
            Self::Cbrt => format!("{d_in} / (3.0 * {e_out} * {e_out})"),
            Self::Recip => format!("-{d_in} / ({e_in} * {e_in})"),
            Self::Fract => d_in,
            _ => unreachable!("not a unary builtin"),
        }
    }

    /// Forward-mode tangent formula for a binary builtin.
    #[must_use]
    pub fn rust_tangent_binary(
        &self,
        e: &dyn Fn(u32) -> String,
        d: &dyn Fn(u32) -> String,
        idx: u32,
        a: u32,
        b: u32,
    ) -> String {
        let (ea, eb, da, db, e_out) = (e(a), e(b), d(a), d(b), e(idx));
        match self {
            Self::Hypot => format!(
                "if {e_out} == 0.0 {{ 0.0 }} else {{ ({ea} * {da} + {eb} * {db}) / {e_out} }}"
            ),
            Self::Min => format!("if {ea} <= {eb} {{ {da} }} else {{ {db} }}"),
            Self::Max => format!("if {ea} >= {eb} {{ {da} }} else {{ {db} }}"),
            Self::Atan2 => format!("({eb} * {da} - {ea} * {db}) / ({ea} * {ea} + {eb} * {eb})"),
            Self::Mod => da,
            _ => unreachable!("not a binary builtin"),
        }
    }

    /// Reverse-mode adjoint update for a unary builtin, as source
    /// statements (`None` when the builtin always has zero gradient).
    #[must_use]
    pub fn rust_adjoint_unary(
        &self,
        adj: &str,
        p: &dyn Fn(u32) -> String,
        idx: u32,
        input: u32,
    ) -> Option<String> {
        let p_in = p(input);
        let p_out = p(idx);
        let acc = |reg: u32| format!("__ra{}", reg);
        Some(match self {
            Self::Exp => format!("{} += {adj} * {p_out};\n", acc(input)),
            Self::Ln => format!("{} += {adj} / {p_in};\n", acc(input)),
            Self::Sqrt => format!("{} += {adj} / (2.0 * {p_out});\n", acc(input)),
            Self::Sin => format!("{} += {adj} * {p_in}.cos();\n", acc(input)),
            Self::Cos => format!("{} -= {adj} * {p_in}.sin();\n", acc(input)),
            Self::Tan => format!("{} += {adj} / ({p_in}.cos() * {p_in}.cos());\n", acc(input)),
            Self::Tanh => format!("{} += {adj} * (1.0 - {p_out} * {p_out});\n", acc(input)),
            Self::Abs => format!(
                "{} += {adj} * (if {p_in} == 0.0 {{ 0.0 }} else {{ {p_in}.signum() }});\n",
                acc(input)
            ),
            Self::Floor | Self::Ceil | Self::Round | Self::Sign => return None,
            Self::Log2 => {
                format!(
                    "{} += {adj} / ({p_in} * std::f64::consts::LN_2);\n",
                    acc(input)
                )
            }
            Self::Log10 => {
                format!(
                    "{} += {adj} / ({p_in} * std::f64::consts::LN_10);\n",
                    acc(input)
                )
            }
            Self::Sinh => format!("{} += {adj} * {p_in}.cosh();\n", acc(input)),
            Self::Cosh => format!("{} += {adj} * {p_in}.sinh();\n", acc(input)),
            Self::Atan => format!("{} += {adj} / (1.0 + {p_in} * {p_in});\n", acc(input)),
            Self::Cbrt => format!("{} += {adj} / (3.0 * {p_out} * {p_out});\n", acc(input)),
            Self::Recip => format!("{} -= {adj} / ({p_in} * {p_in});\n", acc(input)),
            Self::Fract => format!("{} += {adj};\n", acc(input)),
            _ => unreachable!("not a unary builtin"),
        })
    }

    /// Reverse-mode adjoint update for a binary builtin, as source
    /// statements (`None` when the builtin always has zero gradient).
    #[must_use]
    pub fn rust_adjoint_binary(
        &self,
        adj: &str,
        p: &dyn Fn(u32) -> String,
        idx: u32,
        a: u32,
        b: u32,
    ) -> Option<String> {
        let (pa, pb, p_out) = (p(a), p(b), p(idx));
        let acc = |reg: u32| format!("__ra{}", reg);
        Some(match self {
            Self::Hypot => format!(
                "if {p_out} != 0.0 {{ {} += {adj} * {pa} / {p_out}; {} += {adj} * {pb} / {p_out}; }}\n",
                acc(a),
                acc(b)
            ),
            Self::Min => format!(
                "if {pa} <= {pb} {{ {} += {adj}; }} else {{ {} += {adj}; }}\n",
                acc(a),
                acc(b)
            ),
            Self::Max => format!(
                "if {pa} >= {pb} {{ {} += {adj}; }} else {{ {} += {adj}; }}\n",
                acc(a),
                acc(b)
            ),
            Self::Atan2 => {
                let denom = format!("({pa} * {pa} + {pb} * {pb})");
                format!(
                    "if {denom} != 0.0 {{ {} += {adj} * {pb} / {denom}; {} -= {adj} * {pa} / {denom}; }}\n",
                    acc(a),
                    acc(b)
                )
            }
            Self::Mod => format!("{} += {adj};\n", acc(a)),
            _ => unreachable!("not a binary builtin"),
        })
    }

    // ── Metadata ─────────────────────────────────────────────────────

    /// Domain obligations for this builtin (e.g. LogPositive for ln).
    #[must_use]
    pub fn domain_obligations(&self) -> &'static [DomainObligation] {
        match self {
            Self::Ln | Self::Log2 | Self::Log10 => &[DomainObligation::LogPositive],
            Self::Sqrt => &[DomainObligation::SqrtNonNegative],
            _ => &[],
        }
    }
}
