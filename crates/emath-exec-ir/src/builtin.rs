//! Closed scalar kernel codes used only by reference bytecode.
//!
//! These are machine instructions, not surface names: there is deliberately no
//! name lookup table, alias registry, code generator, or feature admission API.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltinId {
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
    Hypot,
    Min,
    Max,
    Atan2,
    Mod,
}

impl BuiltinId {
    #[must_use]
    pub fn eval_unary(self, value: f64) -> Option<f64> {
        Some(match self {
            Self::Exp => value.exp(),
            Self::Ln => value.ln(),
            Self::Sqrt => value.sqrt(),
            Self::Sin => value.sin(),
            Self::Cos => value.cos(),
            Self::Tan => value.tan(),
            Self::Tanh => value.tanh(),
            Self::Abs => value.abs(),
            Self::Floor => value.floor(),
            Self::Ceil => value.ceil(),
            Self::Round => value.round(),
            Self::Sign => {
                if value == 0.0 {
                    0.0
                } else {
                    value.signum()
                }
            }
            Self::Log2 => value.log2(),
            Self::Log10 => value.log10(),
            Self::Sinh => value.sinh(),
            Self::Cosh => value.cosh(),
            Self::Atan => value.atan(),
            Self::Cbrt => value.cbrt(),
            Self::Recip => value.recip(),
            Self::Fract => value.fract(),
            Self::Hypot | Self::Min | Self::Max | Self::Atan2 | Self::Mod => return None,
        })
    }

    #[must_use]
    pub fn eval_binary(self, left: f64, right: f64) -> Option<f64> {
        Some(match self {
            Self::Hypot => left.hypot(right),
            Self::Min => left.min(right),
            Self::Max => left.max(right),
            Self::Atan2 => left.atan2(right),
            Self::Mod => left % right,
            _ => return None,
        })
    }
}
