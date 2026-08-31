//! Canonical seeded algebra (schema `emath.csa`, version 1).
//!
//! Deterministic concrete interpretation of any admitted first-order
//! term; a totality baseline only, never author-intended meaning (the
//! [`CSA_MEANING_CLAIM`] label travels with every artifact). Worlds:
//! [`OnePointWorld`] (everything is one carrier point) and
//! [`SeededCsaWorld`] (FNV-1a mixing of seed, symbol, arguments).
//! Bit-exact determinism: pure integer mixing, no floats.

use crate::{EvalError, FirstOrderWorld};
use emath_term::SymbolId;

/// CSA schema id for machine-readable artifacts.
pub const CSA_SCHEMA: &str = "emath.csa";
/// CSA schema version. Bump on changes to the mixing fn or receipt
/// layout; old values would silently stop reproducing.
pub const CSA_SCHEMA_VERSION: u32 = 1;
/// Label every CSA artifact must carry: values witness totality, never
/// assert meaning.
pub const CSA_MEANING_CLAIM: &str = "totality-baseline; never author-intended meaning";

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a(mut state: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }
    state
}

/// The one-point algebra: every symbol means the single carrier point.
#[derive(Debug, Default, Clone, Copy)]
pub struct OnePointWorld;

impl FirstOrderWorld for OnePointWorld {
    type Value = ();
    type Error = EvalError;

    fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        Ok(())
    }

    fn apply(
        &self,
        _operator: &SymbolId,
        _arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        Ok(())
    }

    fn evidence(&self) -> crate::WorldEvidence {
        crate::WorldEvidence::seed("one-point", &[crate::csa::CSA_MEANING_CLAIM])
    }
}

/// Seeded concrete algebra over `u64`: total, deterministic, seed-keyed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeededCsaWorld {
    /// Seed keying the whole interpretation; different seeds are the
    /// built-in negative control.
    pub seed: u64,
}

impl SeededCsaWorld {
    /// The documented default seed for baseline artifacts.
    #[must_use]
    pub const fn baseline() -> Self {
        Self { seed: 0xe4a7_0001 }
    }

    /// Deterministic value for a free variable (CSA is total on open terms).
    #[must_use]
    pub fn variable_value(&self, name: &str) -> u64 {
        fnv1a(fnv1a(self.seed ^ FNV_OFFSET, b"var:"), name.as_bytes())
    }
}

impl FirstOrderWorld for SeededCsaWorld {
    type Value = u64;
    type Error = EvalError;

    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        Ok(fnv1a(
            fnv1a(self.seed ^ FNV_OFFSET, b"const:"),
            symbol.0.as_bytes(),
        ))
    }

    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        let mut state = fnv1a(
            fnv1a(self.seed ^ FNV_OFFSET, b"apply:"),
            operator.0.as_bytes(),
        );
        for argument in arguments {
            state = fnv1a(state, &argument.to_be_bytes());
        }
        Ok(state)
    }

    fn evidence(&self) -> crate::WorldEvidence {
        crate::WorldEvidence::seed("seeded-csa", &[crate::csa::CSA_MEANING_CLAIM])
    }
}
