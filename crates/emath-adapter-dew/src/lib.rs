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
//! - [`oracle`]: differential oracle over boundary
//!   cases; negative semantic-drift fixtures are detected.

#![forbid(unsafe_code)]

pub mod backends;
pub mod capability;
pub mod dexpr;
pub mod mapping;
pub mod oracle;
pub mod seam;

pub use backends::{
    accelerator_inventory, jit_capability, render_rust_fragment, render_tokens, AcceleratorTarget,
    BackendSelection, DeviceTransferPlan, JitCapability, JitTarget, RustFragment, TokenStream,
};
pub use capability::{
    provide_capability, select_backend, Backend, DewCapability, NoClaimBoundary,
    OptimizationEvidence,
};
pub use dexpr::{
    map_expression, map_linear, CmpOp, DewExpr, Layout, LinearOp, MappingIssue, Shape,
};
pub use mapping::{build_source_map, SourceMapEntry};
pub use oracle::{
    detect_drift, differential_scan, run_boundary_cases, DifferentialFinding, MutantDrift,
    ScanCase, ScanProfile,
};
pub use seam::{AdapterSeam, PatchLedger, PatchOutcome, ProviderVersion, SeamError};
