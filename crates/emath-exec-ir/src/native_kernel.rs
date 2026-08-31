//! Immutable static native-kernel registry for PURE capability cells
//! (emath-r3-sde-control-zxkl, approved architecture).
//!
//! New mathematics enters as `.emath` capability cells — this table is
//! the generic, domain-neutral ABI that lets a kernel-backed pure cell
//! resolve WITHOUT a new `EmirOp`, parser branch, or backend domain
//! switch. The registry is IMMUTABLE static data: no `register()` API,
//! no runtime mutation, no ambient state, no global mutable table.
//!
//! Contract:
//! - **Keyed by capability data** (`&'static str`), not by a domain
//!   enum — there is no `SdeKind`/`EmirOp::Sde*` anywhere.
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
//! - **Genericity proof**: a toy `std.stochastic.toy_double` entry
//!   (pure doubling of one scalar) sits beside the SDE entry, proving
//!   the table is a generic mechanism, not an SDE branch. Both entries
//!   are Pure-class cells with the SAME arity/refusal discipline.
//!
//! The SDE adapter is the ONLY place that knows the emath-rt
//! `stochastic` kernel; every other layer (registry, arity, refusal)
//! is SDE-agnostic. Rust-backend codegen for kernel-backed cells is
//! an explicit NO-CLAIM: generated crates that meet these cells get
//! the existing typed refusal (no domain switch added to the backend).

use crate::interp::Value;

/// One immutable native-kernel descriptor: the capability name the
/// cell registers under, the exact argument count, and the handler.
pub struct NativeKernel {
    /// The capability data key (e.g. `std.stochastic.euler_maruyama`).
    pub capability: &'static str,
    /// Exact argument count; checked before the handler runs.
    pub arity: usize,
    /// The generic handler over exec-ir `Value`s.
    pub handler: fn(&[Value]) -> Result<Value, String>,
}

/// The immutable static table. Order is irrelevant (lookup is by name);
/// names are unique by construction (a duplicate name is a compile-time
/// review issue — asserting uniqueness here would be dead logic).
static NATIVE_KERNELS: &[NativeKernel] = &[
    NativeKernel {
        capability: "std.stochastic.toy_double",
        arity: 1,
        handler: toy_double,
    },
    NativeKernel {
        capability: "std.stochastic.euler_maruyama",
        arity: 7,
        handler: sde_euler_maruyama,
    },
    NativeKernel {
        capability: "std.stochastic.stratonovich",
        arity: 7,
        handler: sde_stratonovich,
    },
];

/// Look up a native kernel by capability name WITHOUT any arity check
/// (the application seam checks arity uniformly before calling).
///
/// Returns `None` for an unknown name — the caller's existing refusal
/// path stays identical.
pub fn native_kernel(capability: &str) -> Option<&'static NativeKernel> {
    NATIVE_KERNELS.iter().find(|k| k.capability == capability)
}

/// The toy genericity proof: `[x]` → `[2x]`, with a typed refusal for
/// a non-finite input (the PURE-cell guard discipline — the same
/// strict-f64 policy the compiled-cell path applies).
fn toy_double(args: &[Value]) -> Result<Value, String> {
    match args {
        [Value::F64(x)] if x.is_finite() => Ok(Value::F64(*x * 2.0)),
        [Value::F64(_)] => {
            Err("E-CELL-006: all native-kernel inputs must be finite".into())
        }
        _ => Err("E-TYPE-012: native-kernel arguments must be Float64".into()),
    }
}

/// SDE adapter (Itô): invokes the OWNED `emath_rt::stochastic` kernel,
/// mapping its typed error to a refusal string. This function is the
/// only place that knows the SDE kernel; the registry is generic.
fn sde_euler_maruyama(args: &[Value]) -> Result<Value, String> {
    sde_adapter(emath_rt::stochastic::SdeRule::Ito, args)
}

/// SDE adapter (Stratonovich): the mathematically distinct rule,
/// same 7-argument contract, same kernel.
fn sde_stratonovich(args: &[Value]) -> Result<Value, String> {
    sde_adapter(emath_rt::stochastic::SdeRule::Stratonovich, args)
}

/// Shared SDE adapter: `(drift_vec, diffusion_vec, x0, h, steps, seed, stream)`.
/// The stream label is accepted (root/declared) for future split
/// topology; today the kernel uses the root stream (the vnqo
/// contract's default). Arity is enforced by the registry before the
/// handler runs; seed/finite/domain validation has ONE authority (the
/// emath-rt kernel), so refusal texts cannot drift between layers.
/// The only check that must live here is the whole-number test on
/// `steps`: the kernel takes `usize` and cannot see a fractional
/// argument.
fn sde_adapter(rule: emath_rt::stochastic::SdeRule, args: &[Value]) -> Result<Value, String> {
    if args.len() != 7 {
        return Err("capability argument count does not match the cell contract".into());
    }
    // Parse the strict-Float64 sub-parts (the language's Phase-1 shape):
    // drift and diffusion are vectors; x0, h, steps, seed are scalars.
    let Value::Vector(drift) = &args[0] else {
        return Err("E-TYPE-012: SDE carriers must be Float64 vectors".into());
    };
    let Value::Vector(diffusion) = &args[1] else {
        return Err("E-TYPE-012: SDE carriers must be Float64 vectors".into());
    };
    let Value::Vector(_) = &args[6] else {
        return Err("E-TYPE-012: the SDE stream label must be a vector".into());
    };
    let scalar = |v: &Value| match v {
        Value::F64(x) => Ok(*x),
        _ => Err("E-TYPE-012: SDE scalar arguments must be Float64".to_string()),
    };
    let x0 = scalar(&args[2])?;
    let h = scalar(&args[3])?;
    let steps = scalar(&args[4])?;
    let seed = scalar(&args[5])?;
    // steps must be a non-negative integer (pre-cast; the kernel's
    // usize signature cannot express this check).
    if steps < 0.0 || steps.fract() != 0.0 {
        return Err(format!(
            "{}: a whole number of steps is required",
            emath_rt::stochastic::SdeError::Domain.code()
        ));
    }
    let trajectory = emath_rt::stochastic::sde_euler_maruyama(
        rule,
        drift,
        diffusion,
        x0,
        h,
        steps as usize,
        Some(seed),
    )
    .map_err(|e| e.to_string())?;
    Ok(Value::Vector(trajectory))
}
