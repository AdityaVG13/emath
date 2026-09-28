//! Checked rational carrier bindings. Mathematical methods are authored cells.
use super::{NativeKernel, VerifiedKernelBinding};
use crate::interp::Value;

fn bad_value() -> String {
    "E-TYPE-012: value does not match the checked kernel carrier".into()
}

fn ratio_construct(args: &[Value]) -> Result<Value, String> {
    let [Value::I64(a), Value::I64(b)] = args else {
        return Err(bad_value());
    };
    let (num, den) = emath_rt::ratio_construct(*a, *b)?;
    Ok(Value::Rat { num, den })
}

fn ratio_norm(args: &[Value]) -> Result<Value, String> {
    let [Value::Rat { num, den }] = args else {
        return Err(bad_value());
    };
    let canonical = emath_rt::ratio_normalize(*num, *den)?;
    if canonical != (*num, *den) {
        return Err(bad_value());
    }
    Ok(Value::Rat {
        num: canonical.0,
        den: canonical.1,
    })
}

pub struct CheckedKernel {
    pub native: NativeKernel,
    pub rust_function: &'static str,
    pub rust_result: &'static str,
    pub borrowed: u64,
    semantic_hash: &'static str,
}

impl CheckedKernel {
    pub fn accepts(&self, binding: &VerifiedKernelBinding) -> bool {
        self.native.kernel_id == binding.kernel_id
            && self.native.signature == binding.signature
            && self.semantic_hash == binding.semantic_hash
    }
}

pub const BINDINGS: &[CheckedKernel] = &[
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "normalize-ratio",
            signature: "(Int,Int)->Rat",
            arity: 2,
            handler: ratio_construct,
        },
        rust_function: "ratio_construct",
        rust_result: "emath_rt::ExactRatio",
        borrowed: 0,
        semantic_hash: "sha256:6c9cd3b3d99351b74b02a5da1c1f7d13cc0fcfde3ebc710882fafcceb4291ce5",
    },
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "normalize-ratio",
            signature: "(Rat)->Rat",
            arity: 1,
            handler: ratio_norm,
        },
        rust_function: "ratio_norm",
        rust_result: "emath_rt::ExactRatio",
        borrowed: 0,
        semantic_hash: "sha256:ddef301862239823a8bfa961660ff2f76fcada3123f1df63870a49e24ddce23f",
    },
];

pub fn verified(capability: &str) -> Option<&'static CheckedKernel> {
    let identity = super::verified_kernel_binding(capability).ok()?;
    BINDINGS.iter().find(|binding| binding.accepts(&identity))
}
