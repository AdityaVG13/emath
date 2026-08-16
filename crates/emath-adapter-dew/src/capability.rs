//!: Dew capability census and optimization evidence.
//!
//! The capability descriptor is machine-readable and deterministic.
//! Backends not listed in the inventory are never claimed; promoted
//! optimizations require a certificate, a trusted rule inventory or
//! per-artifact differential validation.

use crate::backends::AcceleratorTarget;

/// Backend classes the adapter can select.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Backend {
    /// Rust source generation behind structured fragments.
    RustSource,
    /// Token-stream generation for proc-macro/build integration.
    TokenStream,
    /// Cranelift JIT through a provider capability.
    JitCranelift,
    /// Accelerator targets (WGSL/GLSL/CUDA/HIP/OpenCL subsets).
    Accelerator(AcceleratorTarget),
}

impl Backend {
    /// Stable backend token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RustSource => "rust-source",
            Self::TokenStream => "token-stream",
            Self::JitCranelift => "jit-cranelift",
            Self::Accelerator(target) => target.as_str(),
        }
    }
}

/// Machine capability descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DewCapability {
    /// Adapter identity.
    pub identity: String,
    /// Adapter version.
    pub version: String,
    /// Admitted domains.
    pub domains: Vec<String>,
    /// Admitted operator families.
    pub operators: Vec<String>,
    /// Backend inventory (never claim what is not listed).
    pub backends: Vec<Backend>,
    /// Determinism declaration.
    pub deterministic: bool,
    /// Boundary: what this descriptor does NOT claim.
    pub no_claim: NoClaimBoundary,
    /// Optimization evidence rules.
    pub optimization: OptimizationEvidence,
}

/// Explicit list of capabilities that are NOT claimed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoClaimBoundary {
    /// Optimizations that are not certified.
    pub uncertified_optimizations: Vec<String>,
    /// Backends that are not implemented.
    pub unimplemented_backends: Vec<String>,
}

/// Optimization evidence classification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptimizationEvidence {
    /// Certificated rewrite names.
    pub certificates: Vec<String>,
    /// Trusted rule inventory names.
    pub trusted_rules: Vec<String>,
    /// Rewrites requiring per-artifact differential validation before
    /// promotion.
    pub requires_differential: Vec<String>,
}

impl OptimizationEvidence {
    /// Whether a rewrite may be promoted: certified or trusted, or
    /// gated on per-artifact differential validation.
    #[must_use]
    pub fn may_promote(&self, rewrite: &str) -> bool {
        self.certificates.iter().any(|name| name == rewrite)
            || self.trusted_rules.iter().any(|name| name == rewrite)
            || self
                .requires_differential
                .iter()
                .any(|name| name == rewrite)
    }
}

/// Default capability descriptor for this adapter build.
#[must_use]
pub fn provide_capability() -> DewCapability {
    DewCapability {
        identity: "emath.adapter.dew".into(),
        version: "0.1.0".into(),
        domains: vec!["scalar-strict-f64".into(), "fixed-linear-algebra".into()],
        operators: vec![
            "add".into(),
            "sub".into(),
            "mul".into(),
            "div".into(),
            "pow".into(),
            "neg".into(),
            "abs".into(),
            "sqrt".into(),
            "exp".into(),
            "ln".into(),
            "is_finite".into(),
            "min".into(),
            "max".into(),
            "atan2".into(),
            "cmp".into(),
            "logical".into(),
            "if".into(),
            "dot".into(),
            "matvec".into(),
            "scale".into(),
        ],
        backends: vec![
            Backend::RustSource,
            Backend::TokenStream,
            Backend::JitCranelift,
            Backend::Accelerator(AcceleratorTarget::Wgsl),
            Backend::Accelerator(AcceleratorTarget::Glsl),
        ],
        deterministic: true,
        no_claim: NoClaimBoundary {
            uncertified_optimizations: vec![
                "fusion".into(),
                "pattern-rewrite-sin2cos2".into(),
                "hoisting".into(),
            ],
            unimplemented_backends: vec![
                "cuda".into(),
                "hip".into(),
                "opencl".into(),
                "jit-x86-sse2".into(),
                "jit-aarch64-neon".into(),
                "accelerator-shader-anytype".into(),
            ],
        },
        optimization: OptimizationEvidence {
            certificates: vec!["const-fold-ieee754".into()],
            trusted_rules: vec!["canonical-float-literal".into()],
            requires_differential: vec!["fusion".into(), "pattern-rewrite-sin2cos2".into()],
        },
    }
}

/// Selects a backend from the claimed inventory; anything outside is
/// refused (`E-PROV-031`).
pub fn select_backend(capability: &DewCapability, requested: Backend) -> Result<Backend, String> {
    if capability.backends.contains(&requested) {
        Ok(requested)
    } else {
        Err(format!(
            "E-PROV-031: backend `{}` is outside the Dew capability inventory",
            requested.as_str()
        ))
    }
}

/// Deterministic capability token for machine readers.
#[must_use]
pub fn capability_token(capability: &DewCapability) -> String {
    let mut backends: Vec<String> = capability
        .backends
        .iter()
        .map(|backend| backend.as_str().to_string())
        .collect();
    backends.sort_unstable();
    let mut operators = capability.operators.clone();
    operators.sort_unstable();
    format!(
        "dew-cap:v1:{}:{}:[{}]:[{}]:[{}]:{}",
        capability.identity,
        capability.version,
        backends.join(","),
        operators.join(","),
        capability.no_claim.unimplemented_backends.join(","),
        capability.deterministic
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_capability_is_deterministic_and_scoped() {
        let first = provide_capability();
        let again = provide_capability();
        assert_eq!(capability_token(&first), capability_token(&again));
        assert!(first.deterministic);
        // Only the inventory is claimable.
        assert!(select_backend(&first, Backend::RustSource).is_ok());
        assert!(select_backend(&first, Backend::Accelerator(AcceleratorTarget::Wgsl)).is_ok());
        let error =
            select_backend(&first, Backend::Accelerator(AcceleratorTarget::Cuda)).unwrap_err();
        assert!(error.contains("E-PROV-031"));
        assert!(error.contains("cuda"));
    }

    #[test]
    fn boundary_claims_nothing_unimplemented() {
        let capability = provide_capability();
        for backend in &capability.no_claim.unimplemented_backends {
            assert!(
                !capability
                    .backends
                    .iter()
                    .any(|claimed| claimed.as_str() == backend),
                "claimed {backend}"
            );
        }
    }

    #[test]
    fn optimization_promotion_needs_evidence() {
        let evidence = provide_capability().optimization;
        assert!(evidence.may_promote("const-fold-ieee754"));
        assert!(!evidence.may_promote("secret-rewrite"));
    }
}
