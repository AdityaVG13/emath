//! Control transfer, gain, and stability coverage migrated to the authored
//! capability seam: see `tests/emath-exec-ir/tests/dynamics_capsule_cutover.rs`
//! (`control_calls_preserve_values_and_typed_refusals_through_the_seam`).
//! The native `emath_rt::control` wrappers this file exercised no longer exist;
//! identical fixtures execute through `std.capability.control.*` reference cells.
