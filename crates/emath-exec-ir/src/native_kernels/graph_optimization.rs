//! Graph, finite-optimization, and finite-game capability adapters.
//!
//! All fourteen methods (traversal, shortest paths, sparse carriers,
//! Bland simplex, Pareto front, and finite-game claim checks) execute from
//! authored reference bodies in
//! `language/spec/capabilities/discrete/graph-optimization.emath`.
//! This module retains only the (currently empty) descriptor registry as a
//! stable owned path placeholder; it is no longer wired into the shared
//! native-kernel registry, and native mathematical implementations live
//! here no longer.

use crate::native_kernel::NativeKernel;

/// Descriptors to append to the immutable native-kernel registry.
///
/// Empty: every graph-optimization capability resolves to its authored
/// reference program through the installed Language Image.
pub const KERNELS: &[NativeKernel] = &[];
