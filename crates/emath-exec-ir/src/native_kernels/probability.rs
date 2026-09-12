//! Domain-neutral numeric kernel leaves for the probability/statistics cutover.
//!
//! `native_kernel.rs` consumes these descriptors as immutable ABI data. The
//! leaves consume and produce only executable values and typed refusal strings;
//! they never choose presentation labels, worlds, evidence, exactness, or
//! authority state.
//!
//! Distribution transforms (Box–Muller, affine support, Bernoulli
//! threshold) and closed-form densities execute from authored reference
//! bodies. This table retains only the counter-stream unit-interval
//! primitive.

use crate::interp::Value;
use crate::native_kernel::NativeKernel;

/// Numeric leaves bound only through capsule `(kernel, signature, arity)` data.
pub static KERNELS: &[NativeKernel] = &[NativeKernel {
    kernel_id: "counter-stream-unit-interval",
    signature: "(Float64,Float64,Text?)->Vector<Float64>",
    arity: 2,
    handler: unit_interval,
}];

fn unit_interval(args: &[Value]) -> Result<Value, String> {
    let (seed, draws, path) = match args {
        [Value::F64(seed), Value::F64(draws)] => (*seed, *draws, ""),
        [Value::F64(seed), Value::F64(draws), Value::Text(path)] => (*seed, *draws, path.as_str()),
        _ => {
            return Err(
                "E-TYPE-012: unit-interval stream requires (Float64, Float64[, Text])".to_string(),
            );
        }
    };
    emath_rt::unit_interval_stream(seed, draws, path)
        .map(Value::Vector)
        .map_err(|error| error.code().to_string())
}
