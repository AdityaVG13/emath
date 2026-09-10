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
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "euclidean-gcd",
            signature: "(Int,Int)->Int",
            arity: 2,
            handler: super::super::native_kernel::euclidean_gcd,
        },
        rust_function: "gcd_checked",
        rust_result: "i64",
        borrowed: 0,
        semantic_hash: "sha256:41308178c87b9cb4e026632197fd92f15d4a3a8e6b1fae445de0bc9d8922cd72",
    },
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "checked-lcm",
            signature: "(Int,Int)->Int",
            arity: 2,
            handler: super::super::native_kernel::checked_lcm,
        },
        rust_function: "lcm_checked",
        rust_result: "i64",
        borrowed: 0,
        semantic_hash: "sha256:e1644da444694dd481df5099e93ac5f950787db1771cc107d39f056f7bf9cd6a",
    },
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "euclidean-remainder",
            signature: "(ExactInt,PositiveExactInt)->ExactInt",
            arity: 2,
            handler: super::super::native_kernel::integer_remainder,
        },
        rust_function: "int_rem_checked",
        rust_result: "i64",
        borrowed: 0,
        semantic_hash: "sha256:e0c12531bac8c7e595bf72791bc8ea11ac645396064c7607da6c858c1db8ae62",
    },
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "extended-gcd-inverse",
            signature: "(ExactInt,PositiveExactInt)->ExactInt",
            arity: 2,
            handler: super::super::native_kernel::modular_inverse,
        },
        rust_function: "mod_inv_checked",
        rust_result: "i64",
        borrowed: 0,
        semantic_hash: "sha256:0b98150fb65bc146aed8f11c49e3343c140efd3c6850ea4f2cbd196d67a0f284",
    },
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "extended-gcd-inverse",
            signature: "(ExactInt,PrimeModulus)->ExactInt",
            arity: 2,
            handler: super::super::native_kernel::modular_inverse,
        },
        rust_function: "mod_inv_checked",
        rust_result: "i64",
        borrowed: 0,
        semantic_hash: "sha256:4063f6e8dc87c739bcac6c756b065ff33fddcedb2cfe3c8901aef56782d6f60d",
    },
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "bounded-product",
            signature: "(Int)->Int",
            arity: 1,
            handler: super::super::native_kernel::integer_factorial,
        },
        rust_function: "factorial_checked",
        rust_result: "i64",
        borrowed: 0,
        semantic_hash: "sha256:e70e54cc15d3ad1834ed82504d8930eaba970ac00f2a1c24e027ed6f5bb2e473",
    },
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "modular-power",
            signature: "(ExactInt,Nat,PositiveExactInt)->ExactInt",
            arity: 3,
            handler: super::super::native_kernel::modular_power,
        },
        rust_function: "pow_mod_checked",
        rust_result: "i64",
        borrowed: 0,
        semantic_hash: "sha256:033fa3d2ffe3462e7ff28c5e30aee50f0156ce40e24c24ce9314e5ed0ceadda5",
    },
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "modular-square-root",
            signature: "(ExactInt,PrimeModulus)->ExactInt",
            arity: 2,
            handler: super::super::native_kernel::modular_square_root,
        },
        rust_function: "sqrt_mod_checked",
        rust_result: "i64",
        borrowed: 0,
        semantic_hash: "sha256:45f11c89602c8e8831fd37c2d7754926590863ca9907b61d6fd53d2863da8e67",
    },
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "euclidean-congruence",
            signature: "(ExactInt,ExactInt,PositiveExactInt)->Bool",
            arity: 3,
            handler: super::super::native_kernel::modular_congruence,
        },
        rust_function: "congruence_checked",
        rust_result: "bool",
        borrowed: 0,
        semantic_hash: "sha256:670119465dd7c056209dfa8a226ede70c76d6ba91ad7a4f67180cd4e3ae0b987",
    },
    CheckedKernel {
        native: NativeKernel {
            kernel_id: "hamming-distance",
            signature: "(Vector,Vector)->Int",
            arity: 2,
            handler: super::super::native_kernel::hamming_distance,
        },
        // The rt twin takes borrowed slices; both operands borrow.
        rust_function: "hamming_distance_checked",
        rust_result: "i64",
        borrowed: 0b11,
        semantic_hash: "sha256:c55adc8f3cc713024cb0c12cda0afa7c29191cb3fe7874aa662a19d248d6dc09",
    },
];

pub fn verified(capability: &str) -> Option<&'static CheckedKernel> {
    let identity = super::verified_kernel_binding(capability).ok()?;
    BINDINGS.iter().find(|binding| binding.accepts(&identity))
}
