//! Dew breadth-backend adapter.
//!
//! Reuses Dew's expression/code-generation machinery through a stable
//! adapter seam (`seam`) while preserving emath semantics and
//! artifacts. Like the Rumoca adapter, no upstream type appears here;
//! Dew is referenced only by provider identity string.
//!
//! - [`capability`]: machine capability descriptor,
//!   no-claim boundary and optimization-evidence classification.
//! - [`seam`]: versioned adapter-facing API with a
//!   patch ledger.
//! - [`dexpr`]: exact scalar mapping and explicit
//!   linear-algebra mapping with shape/layout conversions; unsupported
//!   emath nodes are refused before Dew execution.
//! - [`backends`]: Rust source and token
//!   stream backends, the Cranelift JIT capability with fallback, and
//!   the accelerator inventory (WGSL/GLSL/CUDA/HIP/OpenCL) with
//!   explicit target/numeric semantics and device transfer plans.
//! - [`mapping`]: SIR -> Dew -> generated symbol/span
//!   source map with deterministic anchors.
//! - [`oracle`]: boundary-case scan of the reference
//!   evaluator, injected semantic-drift detection, scalar evaluation
//!   for the native↔Dew differential lane, and a planted-value
//!   wrong-result control. No upstream Dew engine is consumed.

#![forbid(unsafe_code)]

pub mod backends;
pub mod capability;
pub mod dexpr;
pub mod mapping;
pub mod oracle;
pub mod seam;

pub use backends::{
    AcceleratorTarget, BackendSelection, DeviceTransferPlan, JitCapability, JitTarget,
    RustFragment, TokenStream, accelerator_inventory, jit_capability, render_rust_fragment,
    render_tokens,
};
pub use capability::{
    Backend, DewCapability, NoClaimBoundary, OptimizationEvidence, provide_capability,
    select_backend,
};
pub use dexpr::{
    CmpOp, DewExpr, Layout, LinearOp, MappingIssue, Shape, map_expression, map_linear,
};
pub use mapping::{SourceMapEntry, build_source_map};
pub use oracle::{
    DifferentialFinding, EvalValue, MutantDrift, ScanCase, ScanProfile, detect_drift,
    detect_seeded_wrong_result, evaluate_scalar, run_boundary_cases, scan_reference_boundaries,
};
pub use seam::{AdapterSeam, PatchLedger, PatchOutcome, ProviderVersion, SeamError};
