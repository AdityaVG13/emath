//! Finite-category capability adapters.
//!
//! Certification and diagram commutativity execute from authored
//! reference bodies in `language/spec/capabilities/algebra/category.emath`.
//! This module retains only the (currently empty) descriptor registry as a
//! stable owned path placeholder; it is no longer a mathematical
//! implementation. Native mathematical implementations live here no longer.

use crate::native_kernel::NativeKernel;

/// Descriptors to append to the immutable native-kernel registry.
///
/// Empty: both public category capabilities resolve to their authored
/// reference programs through the installed Language Image.
pub const KERNELS: &[NativeKernel] = &[];
