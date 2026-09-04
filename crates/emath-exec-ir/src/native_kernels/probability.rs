//! Domain-neutral numeric kernel leaves for the probability/statistics cutover.
//!
//! `native_kernel.rs` consumes these descriptors as immutable ABI data. The
//! leaves consume and produce only executable values and typed refusal strings;
//! they never choose presentation labels, worlds, evidence, exactness, or
//! authority state.

use emath_core::{
    KernelDomainRefusal, KernelSpecialFn, evaluate_special_kernel, kernel_mean, kernel_median,
    kernel_quantile, kernel_variance,
};

use crate::interp::Value;
use crate::native_kernel::NativeKernel;

/// Numeric leaves bound only through capsule `(kernel, signature, arity)` data.
pub static KERNELS: &[NativeKernel] = &[
    special("stirling-reflection-value", 1, gamma_value),
    special("stirling-reflection-bound", 1, gamma_bound),
    special("gamma-ratio-value", 2, beta_value),
    special("gamma-ratio-bound", 2, beta_bound),
    special("alternating-odd-series-value", 1, erf_value),
    special("alternating-odd-series-bound", 1, erf_bound),
    special("eta-series-value", 1, zeta_value),
    special("eta-series-bound", 1, zeta_bound),
    special("principal-product-log-inverse-value", 1, lambert_w0_value),
    special("principal-product-log-inverse-bound", 1, lambert_w0_bound),
    special("agm-first-integral-value", 1, elliptic_k_value),
    special("agm-first-integral-bound", 1, elliptic_k_bound),
    special("agm-second-integral-value", 1, elliptic_e_value),
    special("agm-second-integral-bound", 1, elliptic_e_bound),
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
    statistic("finite-average", statistic_mean),
    statistic("type7-middle-order-statistic", statistic_median),
    statistic("centered-square-n-minus-one", statistic_sample_variance),
    statistic("centered-square-n", statistic_population_variance),
    NativeKernel {
        kernel_id: "type7-order-statistic",
        signature: "(Vector<Float64>,Float64)->Float64",
        arity: 2,
        handler: statistic_quantile,
    },
];

const fn special(
    kernel_id: &'static str,
    arity: usize,
    handler: fn(&[Value]) -> Result<Value, String>,
) -> NativeKernel {
    NativeKernel {
        kernel_id,
        signature: if arity == 1 {
            "(Scalar)->Float64"
        } else {
            "(Scalar,Scalar)->Float64"
        },
        arity,
        handler,
    }
}

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

const fn statistic(
    kernel_id: &'static str,
    handler: fn(&[Value]) -> Result<Value, String>,
) -> NativeKernel {
    NativeKernel {
        kernel_id,
        signature: "(Vector<Float64>)->Float64",
        arity: 1,
        handler,
    }
}

fn scalars(args: &[Value]) -> Result<Vec<f64>, String> {
    args.iter()
        .map(|arg| match arg {
            Value::F64(value) => Ok(*value),
            Value::I64(value) => Ok(*value as f64),
            _ => {
                Err("E-TYPE-012: numeric approximation arguments must be real scalars".to_string())
            }
        })
        .collect()
}

fn special_eval(function: KernelSpecialFn, bound: bool, args: &[Value]) -> Result<Value, String> {
    let args = scalars(args)?;
    let evaluated = evaluate_special_kernel(function, &args).map_err(special_refusal)?;
    Ok(Value::F64(if bound {
        evaluated.error_bound
    } else {
        evaluated.value
    }))
}

fn special_refusal(refusal: KernelDomainRefusal) -> String {
    let code = match refusal {
        KernelDomainRefusal::Pole { .. } => "E-SPECIAL-POLE",
        KernelDomainRefusal::OutsideCarrier { .. } => "E-SPECIAL-DOMAIN",
        KernelDomainRefusal::NotImplemented { .. } => "E-SPECIAL-NOT-IMPLEMENTED",
        KernelDomainRefusal::Arity { .. } => "E-SPECIAL-ARITY",
    };
    code.to_string()
}

macro_rules! special_handlers {
    ($value:ident, $bound:ident, $function:expr) => {
        pub fn $value(args: &[Value]) -> Result<Value, String> {
            special_eval($function, false, args)
        }
        pub fn $bound(args: &[Value]) -> Result<Value, String> {
            special_eval($function, true, args)
        }
    };
}

special_handlers!(gamma_value, gamma_bound, KernelSpecialFn::Gamma);
special_handlers!(beta_value, beta_bound, KernelSpecialFn::Beta);
special_handlers!(erf_value, erf_bound, KernelSpecialFn::Erf);
special_handlers!(zeta_value, zeta_bound, KernelSpecialFn::Zeta);
special_handlers!(
    lambert_w0_value,
    lambert_w0_bound,
    KernelSpecialFn::LambertW0
);
special_handlers!(
    elliptic_k_value,
    elliptic_k_bound,
    KernelSpecialFn::EllipticK
);
special_handlers!(
    elliptic_e_value,
    elliptic_e_bound,
    KernelSpecialFn::EllipticE
);

fn sample(kind: u8, args: &[Value]) -> Result<Value, String> {
    let [
        Value::Vector(params),
        Value::F64(seed),
        Value::F64(draws),
        tail @ ..,
    ] = args
    else {
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

fn sample_values(args: &[Value]) -> Result<&[f64], String> {
    match args {
        [Value::Vector(values)] => Ok(values),
        _ => Err("E-TYPE-012: statistic requires one Vector<Float64>".to_string()),
    }
}

pub fn statistic_mean(args: &[Value]) -> Result<Value, String> {
    kernel_mean(sample_values(args)?).map(Value::F64)
}

pub fn statistic_median(args: &[Value]) -> Result<Value, String> {
    kernel_median(sample_values(args)?).map(Value::F64)
}

pub fn statistic_sample_variance(args: &[Value]) -> Result<Value, String> {
    kernel_variance(sample_values(args)?, true).map(Value::F64)
}

pub fn statistic_population_variance(args: &[Value]) -> Result<Value, String> {
    kernel_variance(sample_values(args)?, false).map(Value::F64)
}

pub fn statistic_quantile(args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(values), Value::F64(probability)] = args else {
        return Err("E-TYPE-012: quantile requires (Vector<Float64>, Float64)".to_string());
    };
    kernel_quantile(values, *probability).map(Value::F64)
}
