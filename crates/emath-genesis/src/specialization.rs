//! Deterministic specialization cache keyed by [`WorldId`].
//!
//! [`SpecializationCache::challenge`] refuses reuse when the presented
//! identity differs from the bound identity.

use std::collections::BTreeMap;

use emath_world_ir::WorldId;

/// Why a cached specialization cannot be reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecializationChallenge {
    /// No specialization is stored for this world.
    Missing { world: WorldId },
    /// Presented identity differs from the bound one.
    IdentityChanged { bound: WorldId, presented: WorldId },
}

/// Hit/miss/challenge counters, deterministic per operation sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpecializationStats {
    pub hits: u64,
    pub misses: u64,
    pub challenges: u64,
}

/// Process-local cache keyed by compiled-against [`WorldId`].
#[derive(Debug, Clone)]
pub struct SpecializationCache<T> {
    entries: BTreeMap<WorldId, T>,
    stats: SpecializationStats,
}

impl<T> Default for SpecializationCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SpecializationCache<T> {
    /// Empty cache, zero counters.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            stats: SpecializationStats::default(),
        }
    }

    /// Current counters.
    #[must_use]
    pub const fn stats(&self) -> SpecializationStats {
        self.stats
    }

    /// Store (or replace) a specialization bound to `world_id`.
    pub fn insert(&mut self, world_id: WorldId, artifact: T) {
        self.entries.insert(world_id, artifact);
    }

    /// Exact-key lookup. Counts a hit or miss; never a challenge.
    pub fn get(&mut self, world_id: WorldId) -> Option<&T> {
        if self.entries.contains_key(&world_id) {
            self.stats.hits += 1;
            self.entries.get(&world_id)
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// Reuse the artifact bound to `bound` only when `presented` matches.
    pub fn challenge(
        &mut self,
        bound: WorldId,
        presented: WorldId,
    ) -> Result<&T, SpecializationChallenge> {
        if bound != presented {
            self.stats.challenges += 1;
            return Err(SpecializationChallenge::IdentityChanged { bound, presented });
        }
        match self.get(bound) {
            Some(artifact) => Ok(artifact),
            None => Err(SpecializationChallenge::Missing { world: bound }),
        }
    }
}
