//! Dew breadth-backend adapter.
//!
//! Reuses Dew expression/code-generation machinery through a stable seam
//! while preserving emath semantics. No upstream type appears here; Dew
//! is referenced only by provider identity string. Unsupported emath
//! nodes are refused (`E-PROV-030`) before Dew execution.

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
