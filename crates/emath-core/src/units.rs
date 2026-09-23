//! Unit aliases and affine units (04 section 1.2 + 1.3):
//!
//! Kills the C/K bug class — the highest-frequency real-world unit error.
//!
//! Alias-as-identity (1.2): an `alias` is an identity, never an
//! approximation. `alias liter = L` makes liter and L the same unit (equal
//! FNV-1a64 identity over the target's canonical encoding). A conversion
//! that is not exact is a conversion function with a declared conversion
//! class, never an alias: rebinding a declared unit name over a different
//! scale is refused (`E-UNIT-ALIAS-CONFLICT`), and the original binding
//! survives. Units whose names differ by more than zero are two units
//! (`cal_th`, `cal_IT`).
//!
//! Affine units: the offset is ALWAYS pre-scale —
//! `SI = (value + offset) * scale`, so `degF` is
//! `K = (F + 459.67) * 5/9`, never `F * 5/9 + 459.67` (which differs by
//! 255.37 K). Conformance pinned: `32 degF == 273.15 K`.
//!
//! Difference typing: subtracting two affine quantities of the same unit
//! yields a *difference quantity* in the linear counterpart unit (`ΔdegC`:
//! multiplicative, scale equal to K's). Affine arithmetic refusals are
//! emitted by the capsules' kernels, not this registry.
//!
//! Determinism: f64 arithmetic, fixed conversion constants; comparisons in
//! tests use a 1e-9 tolerance (documented — f64 cannot represent 273.15
//! exactly). Deterministic for identical inputs.
//!
//! No-claim boundary: this module is the semantics layer in std. Surface
//! (`units:` section parsing and `emath check` admission) wires through
//! sema + `emath_ir::lookup_unit` and lands with the IR integration slice.
//!
//! Capsule authority (emath-ehpal.12): the NAMED unit catalog is capsule
//! data (`std.capability.units.catalog`,
//! `language/spec/capabilities/surface/units-catalog.emath`) parsed by
//! `seed_table`. Dimensional analysis is owned by the
//! `std.capability.units.*` capsules and their native kernels in
//! `emath-exec-ir/src/native_kernels/domain_science.rs`. What remains here
//! is the generic registry mechanics (alias-as-identity, affine/difference
//! typing) — data structure, not named-domain policy.

#![forbid(unsafe_code)]

use crate::hash::fnv1a64_bytes;

/// Refusal: rebinding a declared unit name over a different scale, or
/// aliasing to an unknown target.
pub const E_UNIT_ALIAS_CONFLICT: &str = "E-UNIT-ALIAS-CONFLICT";
/// Refusal: unknown unit name.
pub const E_UNIT_UNKNOWN: &str = "E-UNIT-104";

/// Alias/affine unit refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitRuleError {
    /// Stable code (`E-UNIT-ALIAS-CONFLICT`, `E-UNIT-AFFINE-1`,
    /// `E-UNIT-AFFINE-2`, `E-UNIT-DIM`, `E-UNIT-104`).
    pub code: &'static str,
    pub message: String,
}

/// Dimension vector in SI base exponents (m, kg, s, A, K, mol, cd).
pub type Dims = [i64; 7];

/// A declared unit. Affine semantics:
/// `to_si(value) = (value + offset) * scale` (offset pre-scale).
#[derive(Clone, Debug, PartialEq)]
pub struct UnitSpec {
    pub name: String,
    pub dims: Dims,
    pub scale: f64,
    pub offset: f64,
}

impl UnitSpec {
    pub fn new(name: &str, dims: Dims, scale: f64, offset: f64) -> Self {
        Self {
            name: name.to_string(),
            dims,
            scale,
            offset,
        }
    }

    /// SI conversion with the offset pre-scale.
    /// `degF`: `(32 + 459.67) * 5/9 = 273.15 K`. `degC`: `(0 + 273.15) * 1`.
    pub fn to_si(&self, value: f64) -> f64 {
        (value + self.offset) * self.scale
    }

    /// Inverse conversion: `from_si(si) = si / scale - offset`.
    pub fn from_si(&self, si: f64) -> f64 {
        si / self.scale - self.offset
    }

    pub fn is_affine(&self) -> bool {
        self.offset != 0.0
    }

    /// FNV-1a64 identity over the canonical encoding (dims, scale, offset,
    /// name). Aliases hash to their target's identity because resolution
    /// substitutes the target spec wholesale.
    pub fn identity(&self) -> u64 {
        let canonical = format!(
            "unit:{}:{:e}:{:e}:{}",
            self.dims
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
            self.scale,
            self.offset,
            self.name
        );
        fnv1a64_bytes(canonical.as_bytes())
    }
}

/// Whether a quantity is an absolute point on its unit's scale or a
/// difference between two points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantityKind {
    Absolute,
    Difference,
}

/// A quantity: value + unit + absolute/difference typing.
#[derive(Clone, Debug, PartialEq)]
pub struct Quantity {
    pub value: f64,
    pub unit: UnitSpec,
    pub kind: QuantityKind,
}

/// Unit registry with alias-as-identity.
#[derive(Clone, Debug, Default)]
pub struct UnitTable {
    units: Vec<UnitSpec>,
    /// Ordered alias bindings: `alias -> canonical`.
    aliases: Vec<(String, String)>,
}

impl UnitTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a unit. Redeclaring the same name with an identical spec is
    /// idempotent; redeclaring with a different scale/offset/dims is refused
    /// (two units whose names differ by more than zero are two units).
    pub fn declare_unit(&mut self, spec: UnitSpec) -> Result<(), UnitRuleError> {
        if let Some(existing) = self.units.iter().find(|unit| unit.name == spec.name) {
            if existing == &spec {
                return Ok(());
            }
            return Err(UnitRuleError {
                code: E_UNIT_ALIAS_CONFLICT,
                message: format!(
                    "`{}` is already declared with scale {:e} offset {:e}; \
                     a conversion that is not exact is a conversion function, never an alias",
                    spec.name, existing.scale, existing.offset
                ),
            });
        }
        self.units.push(spec);
        Ok(())
    }

    /// Declare `alias` as an identity alias of `canonical`. The target must
    /// exist; an alias name already bound to a different target is refused
    /// and the original binding survives.
    pub fn declare_alias(&mut self, alias: &str, canonical: &str) -> Result<(), UnitRuleError> {
        if self.resolve(canonical).is_err() {
            return Err(UnitRuleError {
                code: E_UNIT_ALIAS_CONFLICT,
                message: format!("cannot alias `{alias}` to `{canonical}`: target is not declared"),
            });
        }
        if let Some((_, existing)) = self.aliases.iter().find(|(name, _)| name == alias) {
            if existing == canonical {
                return Ok(());
            }
            return Err(UnitRuleError {
                code: E_UNIT_ALIAS_CONFLICT,
                message: format!(
                    "`{alias}` is already an alias of `{existing}`; \
                     rebinding over a different unit is refused"
                ),
            });
        }
        if self.resolve(alias).is_ok() {
            return Err(UnitRuleError {
                code: E_UNIT_ALIAS_CONFLICT,
                message: format!(
                    "`{alias}` is already a declared unit; \
                     rebinding it as an alias of `{canonical}` is refused"
                ),
            });
        }
        self.aliases
            .push((alias.to_string(), canonical.to_string()));
        Ok(())
    }

    /// Resolve a name through alias chains to its unit spec. An alias
    /// resolves to the target's spec wholesale, so identity hashes are
    /// equal by construction.
    pub fn resolve(&self, name: &str) -> Result<UnitSpec, UnitRuleError> {
        let mut current = name;
        let mut hops = 0;
        loop {
            if let Some((_, canonical)) = self.aliases.iter().find(|(alias, _)| alias == current) {
                hops += 1;
                if hops > self.aliases.len() {
                    // Cycle guard: a chain longer than the table can only be
                    // a cycle; refuse rather than loop.
                    return Err(UnitRuleError {
                        code: E_UNIT_ALIAS_CONFLICT,
                        message: format!("alias cycle resolving `{name}`"),
                    });
                }
                current = canonical.as_str();
                continue;
            }
            return self
                .units
                .iter()
                .find(|spec| spec.name == current)
                .cloned()
                .ok_or_else(|| UnitRuleError {
                    code: E_UNIT_UNKNOWN,
                    message: format!("unknown unit `{name}`"),
                });
        }
    }

    /// FNV-1a64 identity of the resolved unit: `liter` and `L` hash equal.
    pub fn identity(&self, name: &str) -> Result<u64, UnitRuleError> {
        Ok(self.resolve(name)?.identity())
    }
}

/// Named unit catalogs are leftover leftover. Constructor surface has no
/// SI/element `FeatureID`. Callers that need a table declare units locally.
#[must_use]
pub fn seed_table() -> UnitTable {
    UnitTable::new()
}
