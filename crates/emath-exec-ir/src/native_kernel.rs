//! Immutable static native-kernel registry for PURE capability cells
//! (approved architecture).
//!
//! New mathematics enters as `.emath` capability cells — this table is
//! the generic, domain-neutral ABI that lets a kernel-backed pure cell
//! resolve WITHOUT a new `EmirOp`, parser branch, or backend domain
//! switch. The registry is IMMUTABLE static data: no `register()` API,
//! no runtime mutation, no ambient state, no global mutable table.
//!
//! Contract:
//! - **Keyed by kernel ID and carrier signature**, never by a domain enum or
//!   FeatureID spelling.
//! - **Handler signature uses the existing exec-ir `Value`**
//!   (`fn(&[Value]) -> Result<Value, String>`); the exec-ir `Value`
//!   type is the single value type across the interpreted VM, so the
//!   registry duplicates nothing.
//! - **Arity check** happens before the handler: a wrong argument
//!   count refuses the SAME way the compiled-cell path refuses
//!   (a typed `Arithmetic` "capability argument count does not match
//!   the cell contract"), never a partial call.
//! - **Unknown name → `None`**: the caller (the capability
//!   application seam) keeps its current refusal; the registry never
//!   fabricates a handler. A handler `Err` propagates the refusal
//!   payload verbatim; `Ok` flows through unchanged.
//! Kernel implementations may use runtime numerical leaves, but binding,
//! arity, and refusal are independent of FeatureID spelling. Rust-backend
//! codegen for kernel-backed cells is an explicit no-claim.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::interp::Value;
use crate::language_image::LanguageDistribution;
use crate::term_compile::CompiledCell;

#[path = "native_kernels/calculus.rs"]
mod calculus;
#[path = "native_kernels/category.rs"]
mod category;
#[path = "native_kernels/domain_science.rs"]
mod domain_science;
#[path = "native_kernels/dynamics.rs"]
mod dynamics;
#[path = "native_kernels/einsum.rs"]
mod einsum;
#[path = "native_kernels/graph_optimization.rs"]
mod graph_optimization;
#[path = "native_kernels/linear.rs"]
mod linear;
#[path = "native_kernels/probability.rs"]
mod probability;
#[path = "native_kernels/program_optimize.rs"]
mod program_optimize;
#[path = "native_kernels/program_solve.rs"]
mod program_solve;

/// Generic argument-count contract parsed from capsule and kernel ABI data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelArity {
    Exact(usize),
    Bounded { min: usize, max: usize },
}

impl KernelArity {
    #[must_use]
    pub fn admits(self, found: usize) -> bool {
        match self {
            Self::Exact(expected) => found == expected,
            Self::Bounded { min, max } => (min..=max).contains(&found),
        }
    }
}

/// One immutable native-kernel descriptor: a domain-neutral kernel ID,
/// capsule signature, argument-count floor, and handler.
pub struct NativeKernel {
    /// Kernel ABI key (for example `euclidean-remainder`).
    pub kernel_id: &'static str,
    pub signature: &'static str,
    /// Exact arity, or minimum arity when the signature has trailing `?` inputs.
    pub arity: usize,
    /// The generic handler over exec-ir `Value`s.
    pub handler: fn(&[Value]) -> Result<Value, String>,
}

impl NativeKernel {
    /// Derive exact/range arity from the carrier signature. Optional arguments
    /// are trailing input spellings suffixed with `?`; no FeatureID is inspected.
    #[must_use]
    pub fn arity_contract(&self) -> KernelArity {
        let Some(inputs) = self
            .signature
            .strip_prefix('(')
            .and_then(|signature| signature.split_once(")->").map(|(inputs, _)| inputs))
        else {
            return KernelArity::Exact(self.arity);
        };
        if inputs.is_empty() {
            return KernelArity::Exact(0);
        }
        let mut depth = 0_u32;
        let mut count = 1_usize;
        for byte in inputs.bytes() {
            match byte {
                b'<' => depth = depth.saturating_add(1),
                b'>' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => count = count.saturating_add(1),
                _ => {}
            }
        }
        if inputs.split(',').any(|input| input.trim().ends_with('?')) {
            KernelArity::Bounded {
                min: self.arity,
                max: count,
            }
        } else {
            KernelArity::Exact(self.arity)
        }
    }

    #[must_use]
    pub fn admits_arity(&self, found: usize) -> bool {
        self.arity_contract().admits(found)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InstalledKernelBinding {
    kernel_id: String,
    signature: String,
    semantic_hash: String,
}

/// Verified, domain-neutral identity of an installed native kernel binding.
///
/// Consumers must match all three fields. In particular, a matching kernel
/// ID and signature with a different semantic hash is a stale artifact, not a
/// compatible binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedKernelBinding {
    pub kernel_id: String,
    pub signature: String,
    pub semantic_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KernelBindingError {
    MissingBinding(String),
    MissingKernel(String),
    StaleBinding(String),
    MissingSignature(String),
    ArityMismatch(String),
    SignatureMismatch(String),
    InvalidDistribution(String),
}

/// The immutable static table. Lookup is by `(kernel_id, signature)` so one
/// domain-neutral operation may expose capsule-declared carrier refinements
/// without inspecting a feature name (for example ratio normalization and
/// modular inverse).
static NATIVE_KERNELS: &[NativeKernel] = &[
    NativeKernel {
        kernel_id: "checked-add",
        signature: "(Int,Int)->Int",
        arity: 2,
        handler: checked_add,
    },
    NativeKernel {
        kernel_id: "normalize-ratio",
        signature: "(Int,Int)->Rat",
        arity: 2,
        handler: rat_construct,
    },
    NativeKernel {
        kernel_id: "add-ratios",
        signature: "(Rat,Rat)->Rat",
        arity: 2,
        handler: rat_add,
    },
    NativeKernel {
        kernel_id: "normalize-ratio",
        signature: "(Rat)->Rat",
        arity: 1,
        handler: rat_normalize,
    },
    NativeKernel {
        kernel_id: "bounded-product",
        signature: "(Int)->Int",
        arity: 1,
        handler: integer_factorial,
    },
    NativeKernel {
        kernel_id: "euclidean-remainder",
        signature: "(ExactInt,PositiveExactInt)->ExactInt",
        arity: 2,
        handler: integer_remainder,
    },
    NativeKernel {
        kernel_id: "extended-gcd-inverse",
        signature: "(ExactInt,PositiveExactInt)->ExactInt",
        arity: 2,
        handler: modular_inverse,
    },
    NativeKernel {
        kernel_id: "extended-gcd-inverse",
        signature: "(ExactInt,PrimeModulus)->ExactInt",
        arity: 2,
        handler: modular_inverse,
    },
    NativeKernel {
        kernel_id: "modular-power",
        signature: "(ExactInt,Nat,PositiveExactInt)->ExactInt",
        arity: 3,
        handler: modular_power,
    },
    NativeKernel {
        kernel_id: "modular-square-root",
        signature: "(ExactInt,PrimeModulus)->ExactInt",
        arity: 2,
        handler: modular_square_root,
    },
    NativeKernel {
        kernel_id: "euclidean-congruence",
        signature: "(ExactInt,ExactInt,PositiveExactInt)->Bool",
        arity: 3,
        handler: modular_congruence,
    },
    NativeKernel {
        kernel_id: "modular-horner",
        signature: "(Vector<ExactInt>,ExactInt,PositiveExactInt)->ExactInt",
        arity: 3,
        handler: modular_polynomial_eval,
    },
    NativeKernel {
        kernel_id: "modular-evaluation-sequence",
        signature: "(Vector<ExactInt>,Nat,PositiveExactInt)->Vector<ExactInt>",
        arity: 3,
        handler: reed_solomon_encode,
    },
    NativeKernel {
        kernel_id: "hamming-distance",
        signature: "(Vector,Vector)->Int",
        arity: 2,
        handler: hamming_distance,
    },
    NativeKernel {
        kernel_id: "euclidean-gcd",
        signature: "(Int,Int)->Int",
        arity: 2,
        handler: euclidean_gcd,
    },
    NativeKernel {
        kernel_id: "checked-lcm",
        signature: "(Int,Int)->Int",
        arity: 2,
        handler: checked_lcm,
    },
];

fn kernels() -> impl Iterator<Item = &'static NativeKernel> {
    NATIVE_KERNELS
        .iter()
        .chain(linear::LINEAR_KERNELS)
        .chain(dynamics::KERNELS)
        .chain(graph_optimization::KERNELS)
        .chain(category::KERNELS)
        .chain(domain_science::DOMAIN_SCIENCE_KERNELS)
        .chain(probability::KERNELS)
        .chain(calculus::KERNELS)
        .chain(program_solve::KERNELS)
        .chain(program_optimize::KERNELS)
        .chain(einsum::EINSUM_KERNELS)
}

thread_local! {
    static LANGUAGE_BINDINGS: RefCell<BTreeMap<String, InstalledKernelBinding>> =
        RefCell::new(BTreeMap::new());
    /// Installed authored reference cells (the verified Language Image's
    /// `reference_programs`, filtered to capsule-active capabilities),
    /// keyed by capability feature id string. Installed ONLY through
    /// [`install_language_distribution`] / [`install_reference_programs`]
    /// — no public injection API, no static table, no feature-name
    /// matching. The application seam consults these ONLY when no valid
    /// native binding exists.
    static REFERENCE_CELLS: RefCell<BTreeMap<String, CompiledCell>> =
        RefCell::new(BTreeMap::new());
}

/// The capsule-active capability reference programs of a distribution,
/// keyed by feature id string. Anything else in `reference_programs` —
/// non-capability, non-active, or lacking an authority entry — never
/// installs.
fn capsule_active_reference_cells(
    distribution: &LanguageDistribution,
) -> BTreeMap<String, CompiledCell> {
    distribution
        .capsules
        .iter()
        .filter(|capsule| {
            capsule.class == emath_ir::FeatureClass::Capability
                && distribution
                    .authority
                    .entries
                    .get(&capsule.feature_id)
                    .is_some_and(|entry| entry.state.as_str() == "capsule-active")
        })
        .filter_map(|capsule| {
            distribution
                .reference_programs
                .get(&capsule.feature_id)
                .map(|cell| (capsule.feature_id.to_string(), cell.clone()))
        })
        .collect()
}

/// Install ONLY the capsule-active capability reference programs of a
/// VERIFIED distribution (verify runs first; a failed verify installs
/// nothing). A successful install leaves NO native binding state: stale
/// bindings would silently defeat the deoptimized fallback world. This is
/// the only way reference cells enter the seam.
pub fn install_reference_programs(
    distribution: &LanguageDistribution,
) -> Result<(), KernelBindingError> {
    distribution
        .verify()
        .map_err(|error| KernelBindingError::InvalidDistribution(format!("{error:?}")))?;
    let references = capsule_active_reference_cells(distribution);
    REFERENCE_CELLS.with(|installed| *installed.borrow_mut() = references);
    LANGUAGE_BINDINGS.with(|installed| *installed.borrow_mut() = BTreeMap::new());
    Ok(())
}

/// The installed authored reference cell for a capability, or `None` — on
/// `None` the application seam's typed no-body refusal stays identical.
pub(crate) fn installed_reference_cell(capability: &str) -> Option<CompiledCell> {
    REFERENCE_CELLS.with(|installed| installed.borrow().get(capability).cloned())
}

pub fn install_language_distribution(
    distribution: &LanguageDistribution,
) -> Result<(), KernelBindingError> {
    distribution
        .verify()
        .map_err(|error| KernelBindingError::InvalidDistribution(format!("{error:?}")))?;
    let mut bindings = BTreeMap::new();
    for capsule in &distribution.capsules {
        let active = distribution
            .authority
            .entries
            .get(&capsule.feature_id)
            .is_some_and(|entry| entry.state.as_str() == "capsule-active");
        if !active || capsule.class != emath_ir::FeatureClass::Capability {
            continue;
        }
        let Some(emath_ir::CapsuleSlot::Value(semantics)) = capsule.slots.get("semantics") else {
            continue;
        };
        let Some(kernel_id) = semantic_field(semantics, "kernel") else {
            continue;
        };
        let Some(inputs) = semantic_field(semantics, "inputs") else {
            return Err(KernelBindingError::MissingSignature(
                capsule.feature_id.to_string(),
            ));
        };
        let Some(output) = semantic_field(semantics, "output") else {
            return Err(KernelBindingError::MissingSignature(
                capsule.feature_id.to_string(),
            ));
        };
        let Some(arity) = semantic_field(semantics, "arity").and_then(parse_kernel_arity) else {
            return Err(KernelBindingError::MissingSignature(
                capsule.feature_id.to_string(),
            ));
        };
        if !kernels().any(|kernel| kernel.kernel_id == kernel_id) {
            return Err(KernelBindingError::MissingKernel(kernel_id.to_string()));
        }
        let signature = format!("({inputs})->{output}");
        let Some(kernel) =
            kernels().find(|kernel| kernel.kernel_id == kernel_id && kernel.signature == signature)
        else {
            return Err(KernelBindingError::SignatureMismatch(
                capsule.feature_id.to_string(),
            ));
        };
        if kernel.arity_contract() != arity {
            return Err(KernelBindingError::ArityMismatch(
                capsule.feature_id.to_string(),
            ));
        }
        bindings.insert(
            capsule.feature_id.to_string(),
            InstalledKernelBinding {
                kernel_id: kernel_id.to_string(),
                signature,
                semantic_hash: capsule.semantic_hash.to_string(),
            },
        );
    }
    // All fallible work (verify, kernel resolution, reference filtering)
    // precedes both swaps: a failed install leaves no partial state.
    let references = capsule_active_reference_cells(distribution);
    LANGUAGE_BINDINGS.with(|installed| *installed.borrow_mut() = bindings);
    REFERENCE_CELLS.with(|installed| *installed.borrow_mut() = references);
    Ok(())
}

fn semantic_field<'a>(semantics: &'a str, field: &str) -> Option<&'a str> {
    semantics
        .split(';')
        .find_map(|part| part.trim().strip_prefix(field)?.strip_prefix('='))
}

fn parse_kernel_arity(value: &str) -> Option<KernelArity> {
    if let Some((min, max)) = value.split_once("..") {
        let min = min.parse().ok()?;
        let max = max.parse().ok()?;
        (min <= max).then_some(KernelArity::Bounded { min, max })
    } else {
        value.parse().ok().map(KernelArity::Exact)
    }
}

pub fn binding_semantic_hash(capability: &str) -> Option<String> {
    LANGUAGE_BINDINGS.with(|bindings| {
        bindings
            .borrow()
            .get(capability)
            .map(|binding| binding.semantic_hash.clone())
    })
}

/// Resolve an installed binding to its verified kernel identity.
///
/// Feature identity is used only as the Language Distribution lookup key.
/// Consumers dispatch on the returned kernel/signature/hash tuple.
pub fn verified_kernel_binding(
    capability: &str,
) -> Result<VerifiedKernelBinding, KernelBindingError> {
    let binding = LANGUAGE_BINDINGS
        .with(|bindings| bindings.borrow().get(capability).cloned())
        .ok_or_else(|| KernelBindingError::MissingBinding(capability.to_string()))?;
    if !kernels().any(|kernel| {
        kernel.kernel_id == binding.kernel_id && kernel.signature == binding.signature
    }) {
        return Err(KernelBindingError::StaleBinding(capability.to_string()));
    }
    Ok(VerifiedKernelBinding {
        kernel_id: binding.kernel_id,
        signature: binding.signature,
        semantic_hash: binding.semantic_hash,
    })
}

/// Look up a native kernel by capability name WITHOUT any arity check
/// (the application seam checks arity uniformly before calling).
///
/// Returns `None` for an unknown name — the caller's existing refusal
/// path stays identical.
pub fn native_kernel(capability: &str) -> Option<&'static NativeKernel> {
    let binding = LANGUAGE_BINDINGS.with(|bindings| bindings.borrow().get(capability).cloned())?;
    kernels().find(|kernel| {
        kernel.kernel_id == binding.kernel_id && kernel.signature == binding.signature
    })
}

fn checked_add(args: &[Value]) -> Result<Value, String> {
    match args {
        [Value::I64(left), Value::I64(right)] => left
            .checked_add(*right)
            .map(Value::I64)
            .ok_or_else(|| "E-ARITH-OVERFLOW: checked integer addition overflowed".to_string()),
        _ => Err("E-TYPE-012: checked-add arguments must be Int".to_string()),
    }
}

fn rat_construct(args: &[Value]) -> Result<Value, String> {
    match args {
        [Value::I64(num), Value::I64(den)] => canonical_rat(i128::from(*num), i128::from(*den)),
        _ => Err("E-TYPE-012: rat-construct arguments must be Int".to_string()),
    }
}

fn rat_add(args: &[Value]) -> Result<Value, String> {
    let [
        Value::Rat {
            num: left_num,
            den: left_den,
        },
        Value::Rat {
            num: right_num,
            den: right_den,
        },
    ] = args
    else {
        return Err("E-TYPE-012: rat-add arguments must be Rat".to_string());
    };
    let num = left_num
        .checked_mul(*right_den)
        .and_then(|left_term| {
            right_num
                .checked_mul(*left_den)
                .and_then(|right_term| left_term.checked_add(right_term))
        })
        .ok_or_else(|| "rational addition overflow (i128)".to_string())?;
    let den = left_den
        .checked_mul(*right_den)
        .ok_or_else(|| "rational addition overflow (i128)".to_string())?;
    canonical_rat(num, den)
}

fn rat_normalize(args: &[Value]) -> Result<Value, String> {
    match args {
        [Value::Rat { num, den }] => canonical_rat(*num, *den),
        _ => Err("E-TYPE-012: rat-normalize argument must be Rat".to_string()),
    }
}

fn canonical_rat(num: i128, den: i128) -> Result<Value, String> {
    if den == 0 {
        return Err("rat denominator must be nonzero".to_string());
    }
    let (num, den) = if den < 0 {
        (
            num.checked_neg()
                .ok_or_else(|| "rational arithmetic overflow (i128)".to_string())?,
            den.checked_neg()
                .ok_or_else(|| "rational arithmetic overflow (i128)".to_string())?,
        )
    } else {
        (num, den)
    };
    let gcd = gcd_u128(num.unsigned_abs(), den.unsigned_abs());
    Ok(Value::Rat {
        num: num / gcd as i128,
        den: den / gcd as i128,
    })
}

fn gcd_u128(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

/// Euclid over unsigned magnitudes; gcd(0, 0) = 0 by the
/// divisibility-lattice convention (0 divides only 0, and gcd is the
/// lattice meet). The one refusal is the 2^63 magnitude (|i64::MIN|)
/// paired with 0, whose gcd has no i64 carrier.
fn euclidean_gcd(args: &[Value]) -> Result<Value, String> {
    match args {
        [Value::I64(left), Value::I64(right)] => {
            let gcd = gcd_u128(
                u128::from(left.unsigned_abs()),
                u128::from(right.unsigned_abs()),
            );
            i64::try_from(gcd).map(Value::I64).map_err(|_| {
                "E-ARITH-OVERFLOW: euclidean-gcd result exceeds the i64 carrier".to_string()
            })
        }
        _ => Err("E-TYPE-012: euclidean-gcd arguments must be Int".to_string()),
    }
}

/// lcm(0, x) = 0; otherwise |a|/gcd · |b| in u128 intermediates
/// (|a|, |b| <= 2^63, so the widened product cannot wrap u128), and a
/// result past i64::MAX refuses typed instead of wrapping.
fn checked_lcm(args: &[Value]) -> Result<Value, String> {
    match args {
        [Value::I64(left), Value::I64(right)] => {
            let left = u128::from(left.unsigned_abs());
            let right = u128::from(right.unsigned_abs());
            if left == 0 || right == 0 {
                return Ok(Value::I64(0));
            }
            let lcm = left / gcd_u128(left, right) * right;
            i64::try_from(lcm).map(Value::I64).map_err(|_| {
                "E-ARITH-OVERFLOW: checked-lcm overflowed the i64 carrier".to_string()
            })
        }
        _ => Err("E-TYPE-012: checked-lcm arguments must be Int".to_string()),
    }
}

fn integer_factorial(args: &[Value]) -> Result<Value, String> {
    match args {
        [Value::I64(n)] => emath_rt::factorial_checked(*n)
            .map(Value::I64)
            .map_err(|detail| detail.to_string()),
        _ => Err("E-TYPE-012: integer-factorial argument must be Int".to_string()),
    }
}

fn integer_remainder(args: &[Value]) -> Result<Value, String> {
    let [value, modulus] = args else {
        return Err("E-TYPE-012: integer-remainder arguments must be exact integers".to_string());
    };
    if has_bigint(args) {
        let modulus = bigint_modulus(modulus, "int-rem: modulus must be positive")?;
        let value = bigint_field_element(value, &modulus)?;
        return emath_rt::big_int_rem_checked(&value, &modulus)
            .map(Value::BigInt)
            .map_err(|detail| detail.to_string());
    }
    match (value, modulus) {
        (Value::I64(value), Value::I64(modulus)) if *modulus > 0 => {
            Ok(Value::I64(value.rem_euclid(*modulus)))
        }
        (Value::I64(_), Value::I64(_)) => Err("int-rem: modulus must be positive".to_string()),
        _ => Err("E-TYPE-012: integer-remainder arguments must be exact integers".to_string()),
    }
}

fn modular_inverse(args: &[Value]) -> Result<Value, String> {
    let [value, modulus] = args else {
        return Err("E-TYPE-012: modular-inverse arguments must be exact integers".to_string());
    };
    if has_bigint(args) {
        let modulus = bigint_modulus(modulus, "mod_inv: modulus must be positive")?;
        let value = bigint_field_element(value, &modulus)?;
        return emath_rt::big_mod_inv_checked(&value, &modulus)
            .map(Value::BigInt)
            .map_err(|detail| detail.to_string());
    }
    match (value, modulus) {
        (Value::I64(value), Value::I64(modulus)) => emath_rt::mod_inv_checked(*value, *modulus)
            .map(Value::I64)
            .map_err(|detail| detail.to_string()),
        _ => Err("E-TYPE-012: modular-inverse arguments must be exact integers".to_string()),
    }
}

fn modular_power(args: &[Value]) -> Result<Value, String> {
    let [base, exponent, modulus] = args else {
        return Err("E-TYPE-012: modular-power arguments must be exact integers".to_string());
    };
    if has_bigint(args) {
        let modulus = bigint_modulus(modulus, "pow_mod: modulus must be positive")?;
        let base = bigint_field_element(base, &modulus)?;
        let exponent = bigint_exponent(exponent)?;
        return emath_rt::big_pow_mod_checked(&base, &exponent, &modulus)
            .map(Value::BigInt)
            .map_err(|detail| detail.to_string());
    }
    match (base, exponent, modulus) {
        (Value::I64(base), Value::I64(exponent), Value::I64(modulus)) => {
            emath_rt::pow_mod_checked(*base, *exponent, *modulus)
                .map(Value::I64)
                .map_err(|detail| detail.to_string())
        }
        _ => Err("E-TYPE-012: modular-power arguments must be exact integers".to_string()),
    }
}

fn modular_square_root(args: &[Value]) -> Result<Value, String> {
    let [value, modulus] = args else {
        return Err("E-TYPE-012: modular-square-root arguments must be exact integers".to_string());
    };
    if has_bigint(args) {
        let modulus = bigint_modulus(modulus, "sqrt_mod: modulus must be positive")?;
        let value = bigint_field_element(value, &modulus)?;
        return emath_rt::big_sqrt_mod_checked(&value, &modulus)
            .map(Value::BigInt)
            .map_err(|detail| detail.to_string());
    }
    match (value, modulus) {
        (Value::I64(value), Value::I64(modulus)) => emath_rt::sqrt_mod_checked(*value, *modulus)
            .map(Value::I64)
            .map_err(|detail| detail.to_string()),
        _ => Err("E-TYPE-012: modular-square-root arguments must be exact integers".to_string()),
    }
}

fn modular_congruence(args: &[Value]) -> Result<Value, String> {
    let [left, right, modulus] = args else {
        return Err("E-TYPE-012: modular-congruence arguments must be exact integers".to_string());
    };
    if has_bigint(args) {
        let modulus = bigint_modulus(modulus, "cong: modulus must be non-zero")?;
        let left = bigint_field_element(left, &modulus)?;
        let right = bigint_field_element(right, &modulus)?;
        let left =
            emath_rt::big_int_rem_checked(&left, &modulus).map_err(|detail| detail.to_string())?;
        let right =
            emath_rt::big_int_rem_checked(&right, &modulus).map_err(|detail| detail.to_string())?;
        return Ok(Value::Bool(left == right));
    }
    match (left, right, modulus) {
        (Value::I64(left), Value::I64(right), Value::I64(modulus)) if *modulus != 0 => {
            Ok(Value::Bool(
                i128::from(*left).rem_euclid(i128::from(*modulus))
                    == i128::from(*right).rem_euclid(i128::from(*modulus)),
            ))
        }
        (Value::I64(_), Value::I64(_), Value::I64(_)) => {
            Err("cong: modulus must be non-zero".to_string())
        }
        _ => Err("E-TYPE-012: modular-congruence arguments must be exact integers".to_string()),
    }
}

fn modular_polynomial_eval(args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(coefficients), point, modulus] = args else {
        return Err(
            "E-TYPE-012: modular-polynomial-eval expects Vector, ExactInt, ExactInt".to_string(),
        );
    };
    if has_bigint(&args[1..]) {
        let modulus = bigint_modulus(modulus, "poly_eval_mod: modulus must be positive")?;
        let point = bigint_field_element(point, &modulus)?;
        return emath_rt::big_poly_eval_mod_checked(coefficients, &point, &modulus)
            .map(Value::BigInt)
            .map_err(|detail| detail.to_string());
    }
    match (point, modulus) {
        (Value::I64(point), Value::I64(modulus)) => {
            emath_rt::poly_eval_mod_checked(coefficients, *point, *modulus)
                .map(Value::I64)
                .map_err(|detail| detail.to_string())
        }
        _ => Err(
            "E-TYPE-012: modular-polynomial-eval expects Vector, ExactInt, ExactInt".to_string(),
        ),
    }
}

fn reed_solomon_encode(args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(coefficients), Value::I64(length), modulus] = args else {
        return Err("E-TYPE-012: reed-solomon-encode expects Vector, Int, ExactInt".to_string());
    };
    if matches!(modulus, Value::BigInt(_)) {
        let modulus = bigint_modulus(modulus, "rs_encode: modulus must be positive")?;
        return emath_rt::big_rs_encode_checked(coefficients, *length, &modulus)
            .map(Value::BigVector)
            .map_err(|detail| detail.to_string());
    }
    match modulus {
        Value::I64(modulus) => emath_rt::rs_encode_checked(coefficients, *length, *modulus)
            .map(Value::Vector)
            .map_err(|detail| detail.to_string()),
        _ => Err("E-TYPE-012: reed-solomon-encode expects Vector, Int, ExactInt".to_string()),
    }
}

fn hamming_distance(args: &[Value]) -> Result<Value, String> {
    match args {
        [Value::Vector(left), Value::Vector(right)] => {
            emath_rt::hamming_distance_checked(left, right)
                .map(Value::I64)
                .map_err(|detail| detail.to_string())
        }
        _ => Err("E-TYPE-012: hamming-distance arguments must be Vector".to_string()),
    }
}

fn has_bigint(args: &[Value]) -> bool {
    args.iter().any(|value| matches!(value, Value::BigInt(_)))
}

fn bigint_modulus(value: &Value, invalid: &'static str) -> Result<emath_rt::UBig, String> {
    match value {
        Value::BigInt(value) if !value.is_zero() => Ok(value.clone()),
        Value::I64(value) if *value > 0 => Ok(emath_rt::UBig::from_u64(*value as u64)),
        Value::BigInt(_) | Value::I64(_) => Err(invalid.to_string()),
        _ => Err("E-TYPE-012: modulus must be an exact integer".to_string()),
    }
}

fn bigint_field_element(value: &Value, modulus: &emath_rt::UBig) -> Result<emath_rt::UBig, String> {
    match value {
        Value::BigInt(value) => Ok(value.clone()),
        Value::I64(value) => {
            emath_rt::big_int_rem_i64_checked(*value, modulus).map_err(|detail| detail.to_string())
        }
        _ => Err("E-TYPE-012: operand must be an exact integer".to_string()),
    }
}

fn bigint_exponent(value: &Value) -> Result<emath_rt::UBig, String> {
    match value {
        Value::BigInt(value) => Ok(value.clone()),
        Value::I64(value) if *value >= 0 => Ok(emath_rt::UBig::from_u64(*value as u64)),
        Value::I64(_) => Err("pow-mod: exponent must be non-negative".to_string()),
        _ => Err("E-TYPE-012: exponent must be an exact integer".to_string()),
    }
}
