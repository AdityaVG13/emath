//! Language distribution installation: delegates to the exec-ir native
//! kernel installer, the single authority for kernel bindings.

use emath_exec_ir::language_image::LanguageDistribution;

/// Install a verified distribution. The native-kernel installer is the
/// single authority (see `emath-exec-ir/CONTRACT.md`); this seam exists
/// so sema consumers do not depend on exec-ir internals directly.
pub fn install_language_distribution(
    distribution: &LanguageDistribution,
) -> Result<(), emath_exec_ir::native_kernel::KernelBindingError> {
    emath_exec_ir::native_kernel::install_language_distribution(distribution)
}
