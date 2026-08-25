//! Provider constellation waves and maturity ladder.
//!
//! Waves A..H with capability census, no-claim boundaries, disable/rollback,
//! version locks, and a deterministic P0..P5 maturity ladder. Composition is
//! provider-type-free: plans reference providers by id strings only.

use crate::descriptor::ProviderLock;
use emath_core::{ContentId, fnv1a64_bytes};
use std::collections::BTreeMap;

/// Provider maturity level (Phase 7 ladder).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaturityLevel {
    /// Descriptor only.
    P0,
    /// Adapter compiles and unit tests.
    P1,
    /// Differential/certificate validation.
    P2,
    /// Integrated artifact workflow.
    P3,
    /// Protected host usage.
    P4,
    /// Supported/stable provider.
    P5,
}

impl MaturityLevel {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::P0 => "P0",
            Self::P1 => "P1",
            Self::P2 => "P2",
            Self::P3 => "P3",
            Self::P4 => "P4",
            Self::P5 => "P5",
        }
    }

    /// Promotion criteria for reaching the next level.
    #[must_use]
    pub fn next_criteria(self) -> &'static [&'static str] {
        match self {
            Self::P0 => &[
                "adapter compiles",
                "unit tests green",
                "representation/copy/resource model",
            ],
            Self::P1 => &[
                "differential or certificate validation",
                "checker/evidence contract",
            ],
            Self::P2 => &[
                "integrated artifact workflow",
                "positive/negative capability fixtures",
            ],
            Self::P3 => &["protected host usage", "public workflow demo"],
            Self::P4 => &["independent negative controls", "promotion owner assigned"],
            Self::P5 => &["supported/stable provider agreement"],
        }
    }

    /// Whether all criteria are satisfied by the proof checklist.
    #[must_use]
    pub fn criteria_met(self, proofs: &[String]) -> bool {
        self.next_criteria()
            .iter()
            .all(|criterion| proofs.iter().any(|proof| proof == criterion))
    }
}

/// One provider in the constellation (provider-type-free census entry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstellationProvider {
    /// Provider id.
    pub id: String,
    /// Wave (`A`..`H`).
    pub wave: char,
    /// Capability summary.
    pub capability_summary: String,
    /// Explicit no-claim boundary: things this provider never claims.
    pub no_claim_boundary: Vec<String>,
    /// Maturity level.
    pub maturity: MaturityLevel,
    /// Disabled state; disabled providers roll back cleanly.
    pub disabled: bool,
    /// Version lock; promotion keeps the last known compatible version.
    pub lock: ProviderLock,
    /// Promotion owner.
    pub promotion_owner: String,
}

impl ConstellationProvider {
    /// Canonical census row.
    #[must_use]
    pub fn census(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.id,
            self.wave,
            self.maturity.name(),
            if self.disabled { "disabled" } else { "enabled" },
            self.capability_summary,
            self.no_claim_boundary.join(";"),
            self.promotion_owner,
        )
    }
}

/// A version entry in a provider lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionEntry {
    /// Version string.
    pub version: String,
    /// Whether this version is still known compatible.
    pub valid: bool,
}

/// Constellation lock: preserves the last known compatible version.
#[derive(Clone, Debug, Default)]
pub struct ConstellationLock {
    /// Known versions, newest first.
    pub versions: Vec<VersionEntry>,
}

impl ConstellationLock {
    /// Records a version as compatible (newest first order).
    pub fn record_compatible(&mut self, version: String) {
        self.versions.insert(
            0,
            VersionEntry {
                version,
                valid: true,
            },
        );
    }

    /// Marks all versions except the newest incompatible (rollback).
    pub fn invalidate_from(&mut self, broken_version: &str) {
        for entry in &mut self.versions {
            if entry.version == broken_version {
                entry.valid = false;
            }
        }
    }

    /// Last known compatible version, if any.
    #[must_use]
    pub fn last_compatible(&self) -> Option<&String> {
        self.versions
            .iter()
            .find(|entry| entry.valid)
            .map(|entry| &entry.version)
    }
}

/// Maturity registry: providers -> level with promotion checks.
#[derive(Clone, Debug, Default)]
pub struct MaturityRegistry {
    entries: BTreeMap<String, ConstellationProvider>,
    locks: BTreeMap<String, ConstellationLock>,
}

impl MaturityRegistry {
    /// Registers a census entry. Only a P0 (descriptor-only) claim may be
    /// registered directly; any higher maturity must arrive through the
    /// promote ladder with proofs.
    pub fn register(&mut self, provider: ConstellationProvider) -> Result<(), ConstellationError> {
        // Refuse overwrite: a second P0 register would silently demote a
        // promoted entry back to P0 and discard its maturity climb.
        if self.entries.contains_key(&provider.id) {
            return Err(ConstellationError {
                code: "E-PROV-525",
                message: format!(
                    "provider `{}` already registered; maturity changes go through promote",
                    provider.id
                ),
            });
        }
        if provider.maturity != MaturityLevel::P0 {
            return Err(ConstellationError {
                code: "E-PROV-524",
                message: format!(
                    "provider `{}` registers maturity {} without proofs; census entries start at P0 and climb via promote",
                    provider.id,
                    provider.maturity.name()
                ),
            });
        }
        self.locks
            .entry(provider.id.clone())
            .or_default()
            .record_compatible(format!("lock-{}", provider.maturity.name()));
        self.entries.insert(provider.id.clone(), provider);
        Ok(())
    }

    /// Maturity level of a provider.
    #[must_use]
    pub fn maturity_of(&self, id: &str) -> Option<MaturityLevel> {
        self.entries.get(id).map(|provider| provider.maturity)
    }

    /// Promotes a provider to a higher level when all criteria are proven;
    /// returns the new level or a typed refusal.
    pub fn promote(
        &mut self,
        id: &str,
        target: MaturityLevel,
        proofs: &[String],
    ) -> Result<MaturityLevel, ConstellationError> {
        let Some(provider) = self.entries.get_mut(id) else {
            return Err(ConstellationError {
                code: "E-PROV-521",
                message: format!("unknown provider `{id}`"),
            });
        };
        let current = provider.maturity;
        if target <= current {
            return Err(ConstellationError {
                code: "E-PROV-522",
                message: format!(
                    "promotion target {} not above current {} for `{id}`",
                    target.name(),
                    current.name()
                ),
            });
        }
        // Criteria must be proven level by level (no skipped rungs).
        let mut level = current;
        while level < target {
            if !level.criteria_met(proofs) {
                return Err(ConstellationError {
                    code: "E-PROV-523",
                    message: format!(
                        "promotion of `{id}` to {} blocked: criteria for {} unmet",
                        target.name(),
                        level.name()
                    ),
                });
            }
            level = next_level(level).ok_or_else(|| ConstellationError {
                code: "E-PROV-523",
                message: format!(
                    "promotion of `{id}` to {} blocked: maturity ladder exhausted at {}",
                    target.name(),
                    level.name()
                ),
            })?;
        }
        provider.maturity = target;
        if let Some(lock) = self.locks.get_mut(id) {
            lock.record_compatible(format!("lock-{}", target.name()));
        }
        Ok(target)
    }

    /// Disables a provider (rollback); the entry stays in the census with
    /// its lock preserved.
    pub fn disable(&mut self, id: &str) -> Result<(), ConstellationError> {
        let Some(provider) = self.entries.get_mut(id) else {
            return Err(ConstellationError {
                code: "E-PROV-521",
                message: format!("unknown provider `{id}`"),
            });
        };
        provider.disabled = true;
        Ok(())
    }

    /// Re-enables a provider.
    pub fn enable(&mut self, id: &str) -> Result<(), ConstellationError> {
        let Some(provider) = self.entries.get_mut(id) else {
            return Err(ConstellationError {
                code: "E-PROV-521",
                message: format!("unknown provider `{id}`"),
            });
        };
        provider.disabled = false;
        Ok(())
    }

    /// Provides sorted by id.
    #[must_use]
    pub fn providers(&self) -> Vec<&ConstellationProvider> {
        self.entries.values().collect()
    }

    /// Lock of a provider.
    #[must_use]
    pub fn lock_of(&self, id: &str) -> Option<&ConstellationLock> {
        self.locks.get(id)
    }

    /// Rollback story: invalidates a version and preserves the last
    /// known compatible one.
    pub fn rollback(&mut self, id: &str, broken_version: &str) -> Result<(), ConstellationError> {
        let Some(lock) = self.locks.get_mut(id) else {
            return Err(ConstellationError {
                code: "E-PROV-521",
                message: format!("unknown provider `{id}`"),
            });
        };
        lock.invalidate_from(broken_version);
        Ok(())
    }
}

/// Next maturity level.
fn next_level(level: MaturityLevel) -> Option<MaturityLevel> {
    match level {
        MaturityLevel::P0 => Some(MaturityLevel::P1),
        MaturityLevel::P1 => Some(MaturityLevel::P2),
        MaturityLevel::P2 => Some(MaturityLevel::P3),
        MaturityLevel::P3 => Some(MaturityLevel::P4),
        MaturityLevel::P4 => Some(MaturityLevel::P5),
        MaturityLevel::P5 => None,
    }
}

/// Constellation refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstellationError {
    /// Stable code (`E-PROV-521`..`E-PROV-523`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Composition outcome: deterministic artifact identity or a parametric
/// lift when a provider in the chain is missing/disabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositionOutcome {
    /// Deterministic artifact identity over the composed chain.
    Artifact(ContentId),
    /// Parametric artifact: first missing provider id.
    Parametric { missing: String },
}

/// Composes a provider chain into a deterministic artifact identity,
/// without provider types leaking: only ids and lock versions contribute.
pub fn compose_chain(chain: &[String], registry: &MaturityRegistry) -> CompositionOutcome {
    let mut payload = String::from("chain");
    for id in chain {
        let Some(provider) = registry.entries.get(id) else {
            return CompositionOutcome::Parametric {
                missing: id.clone(),
            };
        };
        if provider.disabled {
            return CompositionOutcome::Parametric {
                missing: id.clone(),
            };
        }
        let version = registry
            .locks
            .get(id)
            .and_then(ConstellationLock::last_compatible)
            .cloned()
            .unwrap_or_else(|| "unversioned".to_string());
        payload.push('\n');
        payload.push_str(&provider.id);
        payload.push('@');
        payload.push_str(&version);
    }
    CompositionOutcome::Artifact(ContentId(format!(
        "fnv1a64:{:016x}",
        fnv1a64_bytes(payload.as_bytes())
    )))
}

/// Default constellation: one provider per wave with a no-claim boundary.
#[must_use]
pub fn default_constellation() -> MaturityRegistry {
    let mut registry = MaturityRegistry::default();
    for (wave, id, summary, boundary, maturity, owner) in [
        (
            'A',
            "phase4.symbolic",
            "symbolic: conversion subset, simplification/CSE, derivatives/Jacobians",
            vec![
                "no proof checking",
                "no interval arithmetic",
                "no DAE structural analysis",
            ],
            MaturityLevel::P0,
            "backends",
        ),
        (
            'B',
            "phase2.expression",
            "expression: rust source/tokens, optional JIT/accelerator targets (not yet implemented)",
            vec!["no tensor derivatives", "no ODE solvers"],
            MaturityLevel::P0,
            "execution",
        ),
        (
            'C',
            "phase3.structural",
            "structural: neutral DAE subset instantiation/flattening, DAE/structural analysis",
            vec!["no symbolic integration", "no proof transport"],
            MaturityLevel::P1,
            "structural",
        ),
        (
            'D',
            "phase4.tensor.jax",
            "tensor: dtype/shape representations, tracing, JVP/VJP/Jacobian/Hessian",
            vec!["no certified numerics", "no FEEC meshes"],
            MaturityLevel::P0,
            "tensor",
        ),
        (
            'D',
            "phase4.tensor.ndarray",
            "tensor: ndarray ops, broadcasting, dtype promotion",
            vec!["no autodiff transforms", "no sparse solvers"],
            MaturityLevel::P0,
            "tensor",
        ),
        (
            'E',
            "phase5.numerics",
            "numerical: root solving, quadrature, ODE/BVP, linear/sparse solvers, optimization, special functions",
            vec!["no symbolic manipulation", "no proof checking"],
            MaturityLevel::P0,
            "numerics",
        ),
        (
            'F',
            "phase6.simulation",
            "simulation: typed operators, FEEC/mesh/physics models, interval/certified numerics",
            vec!["no general autodiff", "no remote execution"],
            MaturityLevel::P0,
            "simulation",
        ),
        (
            'G',
            "phase7.proof",
            "proof: statement/proof transport, kernel checking, theorem/certificate evidence",
            vec!["no numerical execution", "no tensor ops"],
            MaturityLevel::P0,
            "proof",
        ),
        (
            'H',
            "phase7.runtime",
            "runtime evidence: deterministic replay, signed evidence, guarded execution/promotion",
            vec!["no math semantics", "no codegen"],
            MaturityLevel::P0,
            "runtime",
        ),
    ] {
        // ubs:ignore — static census table; register/promote only fail on internal conflicts.
        let _ = registry.register(ConstellationProvider {
            id: id.to_string(),
            wave,
            capability_summary: summary.to_string(),
            no_claim_boundary: boundary.into_iter().map(String::from).collect(),
            maturity: MaturityLevel::P0,
            disabled: false,
            lock: ProviderLock::Unlocked,
            promotion_owner: owner.to_string(),
        });
        // Claims above P0 climb the ladder with the full criteria set;
        // register() itself no longer accepts proof-free maturity claims.
        if maturity != MaturityLevel::P0 {
            let mut proofs: Vec<String> = Vec::new();
            let mut level = MaturityLevel::P0;
            while level < maturity {
                proofs.extend(level.next_criteria().iter().copied().map(String::from));
                let Some(next) = next_level(level) else {
                    break;
                };
                level = next;
            }
            let _ = registry.promote(id, maturity, &proofs);
        }
    }
    registry
}
