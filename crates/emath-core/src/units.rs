//! Unit aliases and affine units (bead emath-r3-unit-aliases-affine-tao6,
//! 04 section 1.2 + 1.3).
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
//! Affine units (1.3) with the C13 fix: the offset is ALWAYS pre-scale —
//! `SI = (value + offset) * scale`, so `degF` is
//! `K = (F + 459.67) * 5/9`, never `F * 5/9 + 459.67` (which differs by
//! 255.37 K). Conformance pinned: `32 degF == 273.15 K`.
//!
//! Difference typing: subtracting two affine quantities of the same unit
//! yields a *difference quantity* in the linear counterpart unit (`ΔdegC`:
//! multiplicative, scale equal to K's). Adding two affine quantities is a
//! typed refusal (`E-UNIT-AFFINE-1`); multiplying/dividing affine operands
//! is refused (`E-UNIT-AFFINE-2`); affine + difference is admitted.
//!
//! Determinism: f64 arithmetic, fixed conversion constants; comparisons in
//! tests use a 1e-9 tolerance (documented — f64 cannot represent 273.15
//! exactly). Deterministic for identical inputs.
//!
//! No-claim boundary: this module is the semantics layer in std. Surface
//! (`units:` section parsing and `emath check` admission) wires through
//! sema + `emath_ir::lookup_unit` and lands with the IR integration slice.

#![forbid(unsafe_code)]

use crate::hash::fnv1a64_bytes;

/// Refusal: rebinding a declared unit name over a different scale, or
/// aliasing to an unknown target.
pub const E_UNIT_ALIAS_CONFLICT: &str = "E-UNIT-ALIAS-CONFLICT";
/// Refusal: cannot add two affine (absolute-temperature-class) quantities.
pub const E_UNIT_AFFINE_ADD: &str = "E-UNIT-AFFINE-1";
/// Refusal: cannot multiply or divide affine operands.
pub const E_UNIT_AFFINE_MUL: &str = "E-UNIT-AFFINE-2";
/// Refusal: dimension mismatch between operands.
pub const E_UNIT_DIM: &str = "E-UNIT-DIM";
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

/// A declared unit. Affine semantics per the C13 fix:
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

    /// SI conversion with the C13 order: offset pre-scale.
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
                .map(|e| e.to_string())
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

const M: Dims = [1, 0, 0, 0, 0, 0, 0];
const S: Dims = [0, 0, 1, 0, 0, 0, 0];
const KG: Dims = [0, 1, 0, 0, 0, 0, 0];
const KELVIN: Dims = [0, 0, 0, 0, 1, 0, 0];
const JOULE: Dims = [2, 1, -2, 0, 0, 0, 0];

/// Seed table with the temperature family (C13 order) and SI bases.
#[must_use]
pub fn seed_table() -> UnitTable {
    let mut table = UnitTable::new();
    let _ = table.declare_unit(UnitSpec::new("K", KELVIN, 1.0, 0.0));
    let _ = table.declare_unit(UnitSpec::new("degC", KELVIN, 1.0, 273.15));
    let _ = table.declare_unit(UnitSpec::new("degF", KELVIN, 5.0 / 9.0, 459.67));
    let _ = table.declare_unit(UnitSpec::new("m", M, 1.0, 0.0));
    let _ = table.declare_unit(UnitSpec::new("kg", KG, 1.0, 0.0));
    let _ = table.declare_unit(UnitSpec::new("s", S, 1.0, 0.0));
    let _ = table.declare_unit(UnitSpec::new("min", S, 60.0, 0.0));
    let _ = table.declare_unit(UnitSpec::new("J", JOULE, 1.0, 0.0));
    table
}

fn dims_equal(left: &Dims, right: &Dims) -> bool {
    left == right
}

/// The linear (difference) unit underlying `spec`: same dims and scale,
/// zero offset. `ΔdegC` is multiplicative and equal in scale to `K`
/// (both scale 1). ΔdegF has scale 5/9.
#[must_use]
pub fn difference_unit(spec: &UnitSpec) -> UnitSpec {
    UnitSpec {
        name: format!("Δ{}", spec.name),
        dims: spec.dims,
        scale: spec.scale,
        offset: 0.0,
    }
}

/// SI magnitude of a quantity (differences already carry SI-scaled values).
fn to_si(quantity: &Quantity) -> f64 {
    quantity.unit.to_si(quantity.value)
}

/// Add two quantities.
///
/// - affine + affine: refused (`E-UNIT-AFFINE-1` — cannot add absolute
///   temperatures; did you mean a difference or a mixture average?).
/// - affine + difference (compatible delta scale): admitted, result affine
///   (`22.0 degC + 10.0 K` = `32.0 degC`).
/// - difference + difference: admitted, result difference.
/// - dimension mismatch: refused (`E-UNIT-DIM`).
pub fn add(left: &Quantity, right: &Quantity) -> Result<Quantity, UnitRuleError> {
    if !dims_equal(&left.unit.dims, &right.unit.dims) {
        return Err(UnitRuleError {
            code: E_UNIT_DIM,
            message: format!(
                "dimension mismatch: {} vs {}",
                left.unit.name, right.unit.name
            ),
        });
    }
    match (left.kind, right.kind) {
        (QuantityKind::Absolute, QuantityKind::Absolute) => {
            if left.unit.is_affine() || right.unit.is_affine() {
                return Err(UnitRuleError {
                    code: E_UNIT_AFFINE_ADD,
                    message: format!(
                        "cannot add absolute temperatures `{} {}` + `{} {}`; \
                         did you mean a difference or a mixture average?",
                        left.value, left.unit.name, right.value, right.unit.name
                    ),
                });
            }
            // Linear absolute + linear absolute: same scale family.
            let value = to_si(left) + to_si(right);
            Ok(Quantity {
                value: left.unit.from_si(value),
                unit: left.unit.clone(),
                kind: QuantityKind::Absolute,
            })
        }
        (QuantityKind::Absolute, QuantityKind::Difference)
        | (QuantityKind::Difference, QuantityKind::Absolute) => {
            let (point, delta) = if left.kind == QuantityKind::Absolute {
                (left, right)
            } else {
                (right, left)
            };
            // Delta scales must match the point unit's scale exactly
            // (ΔdegC and K both scale 1; ΔdegF is scale 5/9 and cannot mix
            // with a degC point).
            if delta.unit.scale != point.unit.scale {
                return Err(UnitRuleError {
                    code: E_UNIT_DIM,
                    message: format!(
                        "difference scale mismatch: Δ{} (scale {:e}) vs {} (scale {:e})",
                        delta.unit.name, delta.unit.scale, point.unit.name, point.unit.scale
                    ),
                });
            }
            let value = to_si(point) + delta.value * delta.unit.scale;
            Ok(Quantity {
                value: point.unit.from_si(value),
                unit: point.unit.clone(),
                kind: QuantityKind::Absolute,
            })
        }
        (QuantityKind::Difference, QuantityKind::Difference) => {
            if left.unit.scale != right.unit.scale {
                return Err(UnitRuleError {
                    code: E_UNIT_DIM,
                    message: format!(
                        "difference scale mismatch: {} (scale {:e}) vs {} (scale {:e})",
                        left.unit.name, left.unit.scale, right.unit.name, right.unit.scale
                    ),
                });
            }
            Ok(Quantity {
                value: left.value + right.value,
                unit: left.unit.clone(),
                kind: QuantityKind::Difference,
            })
        }
    }
}

/// Subtract two quantities.
///
/// - affine - affine (same unit scale): difference quantity in the linear
///   counterpart (`ΔdegC`), value carried in that unit's scale.
/// - affine - difference: affine (`22 degC - 10 ΔdegC` = `12 degC`).
/// - difference - difference: difference.
/// - linear absolute - linear absolute: difference in the same unit.
pub fn sub(left: &Quantity, right: &Quantity) -> Result<Quantity, UnitRuleError> {
    if !dims_equal(&left.unit.dims, &right.unit.dims) {
        return Err(UnitRuleError {
            code: E_UNIT_DIM,
            message: format!(
                "dimension mismatch: {} vs {}",
                left.unit.name, right.unit.name
            ),
        });
    }
    match (left.kind, right.kind) {
        (QuantityKind::Absolute, QuantityKind::Absolute) => {
            if left.unit.is_affine() || right.unit.is_affine() {
                // T - T_room: difference in the shared linear base.
                if left.unit.scale != right.unit.scale {
                    return Err(UnitRuleError {
                        code: E_UNIT_DIM,
                        message: format!(
                            "affine subtraction scale mismatch: {} (scale {:e}) vs {} (scale {:e})",
                            left.unit.name, left.unit.scale, right.unit.name, right.unit.scale
                        ),
                    });
                }
                let si_left = to_si(left);
                let si_right = to_si(right);
                Ok(Quantity {
                    value: (si_left - si_right) / left.unit.scale,
                    unit: difference_unit(&left.unit),
                    kind: QuantityKind::Difference,
                })
            } else {
                Ok(Quantity {
                    value: left.value - right.value,
                    unit: left.unit.clone(),
                    kind: QuantityKind::Difference,
                })
            }
        }
        (QuantityKind::Absolute, QuantityKind::Difference) => {
            if right.unit.scale != left.unit.scale {
                return Err(UnitRuleError {
                    code: E_UNIT_DIM,
                    message: format!(
                        "difference scale mismatch: Δ{} (scale {:e}) vs {} (scale {:e})",
                        right.unit.name, right.unit.scale, left.unit.name, left.unit.scale
                    ),
                });
            }
            let value = to_si(left) - right.value * right.unit.scale;
            Ok(Quantity {
                value: left.unit.from_si(value),
                unit: left.unit.clone(),
                kind: QuantityKind::Absolute,
            })
        }
        (QuantityKind::Difference, QuantityKind::Absolute) => {
            // A difference minus a point is not a well-formed quantity
            // (ΔT - T has no interpretation as either an absolute point or
            // a difference of the same family); refused.
            Err(UnitRuleError {
                code: E_UNIT_DIM,
                message: format!(
                    "cannot subtract absolute `{}` from a difference quantity",
                    right.unit.name
                ),
            })
        }
        (QuantityKind::Difference, QuantityKind::Difference) => {
            if left.unit.scale != right.unit.scale {
                return Err(UnitRuleError {
                    code: E_UNIT_DIM,
                    message: format!(
                        "difference scale mismatch: {} (scale {:e}) vs {} (scale {:e})",
                        left.unit.name, left.unit.scale, right.unit.name, right.unit.scale
                    ),
                });
            }
            Ok(Quantity {
                value: left.value - right.value,
                unit: left.unit.clone(),
                kind: QuantityKind::Difference,
            })
        }
    }
}

/// Multiply two quantities. Any affine operand is refused
/// (`E-UNIT-AFFINE-2`): multiplying absolute temperatures is meaningless.
pub fn mul(left: &Quantity, right: &Quantity) -> Result<Quantity, UnitRuleError> {
    if left.unit.is_affine() || right.unit.is_affine() {
        return Err(UnitRuleError {
            code: E_UNIT_AFFINE_MUL,
            message: format!(
                "cannot multiply affine quantity `{} {}` by `{} {}`; \
                 convert to a difference or a linear unit first",
                left.value, left.unit.name, right.value, right.unit.name
            ),
        });
    }
    if !dims_equal(&left.unit.dims, &right.unit.dims) {
        // Multiplication composes dims; composing requires a dims-product
        // constructor, which is out of scope for this module's contract —
        // refuse rather than emit a wrong dimension.
        return Err(UnitRuleError {
            code: E_UNIT_DIM,
            message: format!(
                "dimension composition not supported here: {} vs {}",
                left.unit.name, right.unit.name
            ),
        });
    }
    Ok(Quantity {
        value: left.value * right.value,
        unit: left.unit.clone(),
        kind: left.kind,
    })
}

// ---------------------------------------------------------------------------
// dim-group (bead emath-sci-physics-lane-3f7v, thin slice)
//
// Dimensional analysis is a group, not an exponent bag: the carrier is the
// free abelian group Z^7 over the SI base dimensions (L, M, T, I, Θ, N, J =
// m, kg, s, A, K, mol, cd), with composition = multiplication of physical
// quantities, inverse = reciprocal, identity = dimensionless. Ratio units
// (scale-only) are closed under it; affine units are a torsor over the
// group, never an element of it (pinned by the affine NC in the 3f7v suite).
//
// Law-grade receipts: homogeneity is a receipt carrying the shared witness
// dimension and its canonical notation, not a bare bool. Buckingham
// π-theorem: the dimensionless groups are exactly an integer null-space
// basis of the variable×base dimension matrix; each basis vector is
// witness-minimized (primitive, sign-canonical). Deterministic: fixed
// iteration order, pure integer arithmetic, no floats anywhere below.
//
// Fences: tensor-geometry (curvature/covariance), Noether conservation
// certificates, and the variational action/Euler-Lagrange operator are
// later slices of this epic — none are claimed here.
// ---------------------------------------------------------------------------

/// SI base symbols in the fixed carrier order (m, kg, s, A, K, mol, cd).
pub const BASE_SYMBOLS: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];

/// The group identity: the dimensionless vector.
#[must_use]
pub fn dim_identity() -> Dims {
    [0; 7]
}

/// Group composition (multiplication of quantities): exponents add.
#[must_use]
pub fn dim_add(a: Dims, b: Dims) -> Dims {
    let mut out = [0; 7];
    for (o, (x, y)) in out.iter_mut().zip(a.iter().zip(b.iter())) {
        *o = x + y;
    }
    out
}

/// Group inverse (reciprocal): exponents negate.
#[must_use]
pub fn dim_neg(a: Dims) -> Dims {
    a.map(|e| -e)
}

/// Repeated composition with itself `k` times (`k < 0` composes the inverse).
#[must_use]
pub fn dim_pow(a: Dims, k: i64) -> Dims {
    a.map(|e| e * k)
}

/// Whether `a` is the identity (dimensionless).
#[must_use]
pub fn dim_is_identity(a: Dims) -> bool {
    a == [0; 7]
}

fn gcd(a: i64, b: i64) -> i64 {
    let (a, b) = (a.abs(), b.abs());
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Canonical multiplicative notation over the base symbols: `m^2*kg*s^-2`;
/// the identity notates as `1` (the free group's identity written
/// multiplicatively). Deterministic: fixed base order, zero exponents
/// omitted, exponent 1 bare.
#[must_use]
pub fn dim_notation(a: Dims) -> String {
    let parts: Vec<String> = BASE_SYMBOLS
        .iter()
        .zip(a.iter())
        .filter(|(_, e)| **e != 0)
        .map(|(sym, e)| {
            if *e == 1 {
                (*sym).to_string()
            } else {
                format!("{sym}^{e}")
            }
        })
        .collect();
    if parts.is_empty() {
        "1".to_string()
    } else {
        parts.join("*")
    }
}

/// Divide out the common factor of the exponents and fix the sign
/// (first nonzero exponent positive). This is the witness minimization for
/// a single dimension vector: `m^2*kg^2*s^-4` and `m*kg*s^-2` are the same
/// group element; the primitive form is the canonical witness.
#[must_use]
pub fn dim_primitive(a: Dims) -> Dims {
    let d: i64 = a.iter().fold(0, |acc, e| gcd(acc, *e));
    let mut out = if d > 1 { a.map(|e| e / d) } else { a };
    if let Some(first) = out.iter().copied().find(|e| *e != 0) {
        if first < 0 {
            out = dim_neg(out);
        }
    }
    out
}

/// Witness-minimization predicate for a group coefficient vector (length =
/// number of variables, not 7): primitive when the gcd of its absolute
/// entries is 1 (or it is the zero vector).
#[must_use]
pub fn dim_group_is_primitive(coefficients: &[i64]) -> bool {
    let d = coefficients.iter().fold(0, |acc, e| gcd(acc, *e));
    d <= 1
}

/// A law-grade homogeneity receipt: ⟦lhs⟧ =symp ⟦rhs⟧ holds, and the
/// receipt names the shared witness dimension and its canonical notation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomogeneityReceipt {
    /// The shared dimension vector both sides carry.
    pub witness: Dims,
    /// Canonical notation of the witness (e.g. `m^2*kg*s^-2`).
    pub notation: String,
}

/// Homogeneity check for a physical law: both sides must be the same group
/// element. Refusal is `E-UNIT-DIM` with BOTH sides' canonical notations in
/// the message (a law-grade diagnostic, not a bare code).
pub fn check_homogeneity(lhs: Dims, rhs: Dims) -> Result<HomogeneityReceipt, UnitRuleError> {
    if lhs != rhs {
        return Err(UnitRuleError {
            code: E_UNIT_DIM,
            message: format!(
                "homogeneity violated: ⟦lhs⟧ = {} vs ⟦rhs⟧ = {} \
                 (a law must be homogeneous in the base dimensions)",
                dim_notation(lhs),
                dim_notation(rhs)
            ),
        });
    }
    Ok(HomogeneityReceipt {
        witness: lhs,
        notation: dim_notation(lhs),
    })
}

/// Row-echelon rank of the variable×base-dimension matrix (rows = the given
/// dimension vectors), computed by fraction-free elimination with gcd
/// reduction. Deterministic.
#[must_use]
pub fn dim_rank(vars: &[Dims]) -> usize {
    // Basis kept in echelon form: each row's first nonzero (pivot) column
    // is strictly increasing down the basis.
    let mut basis: Vec<Dims> = Vec::new();
    'vars: for v in vars {
        let mut w = dim_primitive(*v);
        for b in &basis {
            let pivot = b
                .iter()
                .position(|e| *e != 0)
                .expect("echelon row has a pivot");
            if w[pivot] != 0 {
                // Cross-multiply to kill the pivot column, then re-primitive
                // to bound growth.
                let bp = b[pivot];
                let wp = w[pivot];
                w = dim_primitive(dim_add(dim_pow(*b, wp), dim_pow(w, -bp)));
            }
        }
        if dim_is_identity(w) {
            continue 'vars;
        }
        let pivot = w
            .iter()
            .position(|e| *e != 0)
            .expect("non-identity row has a pivot");
        let pos = basis
            .iter()
            .position(|b| b.iter().position(|e| *e != 0).expect("pivot") > pivot)
            .unwrap_or(basis.len());
        basis.insert(pos, w);
    }
    basis.len()
}

/// Buckingham π-theorem: the dimensionless groups of a set of physical
/// variables. Each variable is a dimension vector (a group element); a
/// dimensionless group is an integer coefficient vector `c` over the
/// variables with `Σ c_i · var_i = 1` (the identity). The returned basis
/// spans the integer null space of the variable matrix; there are exactly
/// `n − rank` of them, each witness-minimized (`dim_group_is_primitive`)
/// and sign-canonical (first nonzero coefficient positive). Deterministic:
/// fixed elimination order, pure integer arithmetic.
#[must_use]
pub fn dimensionless_groups(vars: &[Dims]) -> Vec<Vec<i64>> {
    let n = vars.len();
    // Incremental kernel basis over the 7 base-dimension equations: start
    // with the kernel of no equations (all of Z^n), intersect with each
    // equation's kernel in turn.
    let mut kernel: Vec<Vec<i64>> = (0..n)
        .map(|i| {
            let mut e = vec![0i64; n];
            e[i] = 1;
            e
        })
        .collect();
    for d in 0..7 {
        // Equation d: Σ_i c_i · V[i][d] = 0. Score each current kernel
        // vector against this base-dimension column of the variable matrix.
        let scores: Vec<i64> = kernel
            .iter()
            .map(|k| {
                k.iter()
                    .zip(vars.iter())
                    .fold(0i64, |acc, (c, var)| acc + c * var[d])
            })
            .collect();
        let Some(p) = scores.iter().position(|s| *s != 0) else {
            continue; // every kernel vector already kills this equation
        };
        let sp = scores[p];
        let mut next: Vec<Vec<i64>> = Vec::new();
        for (j, k) in kernel.iter().enumerate() {
            if j == p {
                continue;
            }
            if scores[j] == 0 {
                next.push(k.clone());
            } else {
                // k' = s_j * k_p − s_p * k_j kills the row exactly.
                let combo: Vec<i64> = k
                    .iter()
                    .zip(kernel[p].iter())
                    .map(|(kj, kp)| scores[j] * kp - sp * kj)
                    .collect();
                next.push(primitive_coefficients(combo));
            }
        }
        kernel = next;
    }
    kernel
        .into_iter()
        .map(|mut c| {
            if let Some(first) = c.iter().copied().find(|e| *e != 0) {
                if first < 0 {
                    c = c.iter().map(|e| -e).collect();
                }
            }
            c
        })
        .collect()
}

/// Divide a coefficient vector by its common factor (witness minimization).
fn primitive_coefficients(mut c: Vec<i64>) -> Vec<i64> {
    let d = c.iter().fold(0i64, |acc, e| gcd(acc, *e));
    if d > 1 {
        c.iter_mut().for_each(|e| *e /= d);
    }
    c
}
