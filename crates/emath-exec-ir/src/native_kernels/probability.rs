//! Domain-neutral numeric kernel leaves for the probability/statistics cutover.
//!
//! `native_kernel.rs` consumes these descriptors as immutable ABI data. The
//! leaves consume and produce only executable values and typed refusal strings;
//! they never choose presentation labels, worlds, evidence, exactness, or
//! authority state.

use crate::interp::Value;
use crate::native_kernel::NativeKernel;

/// Numeric leaves bound only through capsule `(kernel, signature, arity)` data.
pub static KERNELS: &[NativeKernel] = &[
    NativeKernel {
        kernel_id: "counter-stream-gaussian-transform",
        signature: "(Vector<Float64>,Float64,Float64,Text?)->Vector<Float64>",
        arity: 3,
        handler: sample_normal,
    },
    NativeKernel {
        kernel_id: "counter-stream-affine-transform",
        signature: "(Vector<Float64>,Float64,Float64,Text?)->Vector<Float64>",
        arity: 3,
        handler: sample_uniform,
    },
    NativeKernel {
        kernel_id: "counter-stream-threshold-transform",
        signature: "(Vector<Float64>,Float64,Float64,Text?)->Vector<Float64>",
        arity: 3,
        handler: sample_bernoulli,
    },
    density("gaussian-closed-form", density_normal),
    density("affine-support-closed-form", density_uniform),
    density("binary-mass-closed-form", density_bernoulli),
];

const fn density(
    kernel_id: &'static str,
    handler: fn(&[Value]) -> Result<Value, String>,
) -> NativeKernel {
    NativeKernel {
        kernel_id,
        signature: "(Vector<Float64>,Float64)->Float64",
        arity: 2,
        handler,
    }
}

fn sample(kind: u8, args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(params), Value::F64(seed), Value::F64(draws), tail @ ..] = args else {
        return Err(
            "E-TYPE-012: sampling requires (Vector<Float64>, Float64, Float64[, Text])".to_string(),
        );
    };
    let path = match tail {
        [] => "",
        [Value::Text(path)] => path.as_str(),
        _ => return Err("E-TYPE-012: sampling stream path must be Text".to_string()),
    };
    emath_rt::sample_distribution_in_stream(kind, params, *seed, *draws, path)
        .map(Value::Vector)
        .map_err(|error| error.code().to_string())
}

pub fn sample_normal(args: &[Value]) -> Result<Value, String> {
    sample(0, args)
}

pub fn sample_uniform(args: &[Value]) -> Result<Value, String> {
    sample(1, args)
}

pub fn sample_bernoulli(args: &[Value]) -> Result<Value, String> {
    sample(2, args)
}

fn probability_density(kind: u8, args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(params), Value::F64(point)] = args else {
        return Err("E-TYPE-012: density requires (Vector<Float64>, Float64)".to_string());
    };
    emath_rt::distribution_density(kind, params, *point)
        .map(Value::F64)
        .map_err(|error| error.code().to_string())
}

pub fn density_normal(args: &[Value]) -> Result<Value, String> {
    probability_density(0, args)
}

pub fn density_uniform(args: &[Value]) -> Result<Value, String> {
    probability_density(1, args)
}

pub fn density_bernoulli(args: &[Value]) -> Result<Value, String> {
    probability_density(2, args)
}
