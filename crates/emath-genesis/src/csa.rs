//! Canonical seeded algebra (schema `emath.csa`, version 1).
//!
//! CSA is the totality baseline (ADR-003): a deterministic concrete
//! interpretation of *any* admitted first-order term. Its only purpose is
//! exercising codegen, receipts and replay -- it is **never** presented as
//! the author-intended meaning of a term, and the [`CSA_MEANING_CLAIM`]
//! label travels with every CSA artifact so no consumer can mistake it
//! for one.
//!
//! Two worlds are provided:
//!
//! - [`OnePointWorld`]: the degenerate one-point algebra. Every constant
//!   and every operator application is the single carrier point. Total by
//!   construction; collapses all structure (that collapse is the point --
//!   it is the terminal object of the world category, useful as the
//!   cheapest possible totality witness).
//! - [`SeededCsaWorld`]: a seeded concrete algebra over `u64`. Constants
//!   and operator applications mix the seed, the symbol identity and the
//!   argument values through FNV-1a, so every finite term has exactly one
//!   reproducible value per seed, distinct seeds are a built-in negative
//!   control, and no symbol or operator is ever unknown.
//!
//! Determinism class: bit-exact across runs, hosts and tool versions
//! (pure integer mixing, no floats, no iteration-order dependence).

use crate::{EvalError, FirstOrderWorld};
use emath_term::SymbolId;

/// CSA schema id for machine-readable artifacts.
pub const CSA_SCHEMA: &str = "emath.csa";
/// CSA schema version. Bump on any change to the mixing function or the
/// receipt layout: old values would silently stop reproducing otherwise.
pub const CSA_SCHEMA_VERSION: u32 = 1;
/// The labeling claim every CSA artifact must carry (labeling test below
/// pins it). CSA values witness totality; they never assert meaning.
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
}

/// Seeded concrete algebra over `u64`: total, deterministic, seed-keyed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeededCsaWorld {
    /// Seed keying the whole interpretation. The same seed always yields
    /// the same value for the same term; a different seed is the built-in
    /// negative control.
    pub seed: u64,
}

impl SeededCsaWorld {
    /// The documented default seed for baseline artifacts.
    #[must_use]
    pub const fn baseline() -> Self {
        Self { seed: 0xe4a7_0001 }
    }

    /// Deterministic value for a free variable (variables are part of the
    /// carrier too: CSA is total on open terms under this valuation).
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
}
