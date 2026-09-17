//! Control transfer, gain, and stability coverage lane, pruned with the
//! cutover (`a2581b3`): the native `emath_rt::control` wrappers this
//! file exercised no longer exist, and the migrated capsule cutover
//! test was removed with them (no authored `std.capability.control.*`
//! cells shipped). The keep-set constructor pins live in
//! `tests/emath-exec-ir/tests/constructor_cutover.rs`. If control math
//! returns, it returns as authored `.emath` definitions (AGENTS.md
//! language ownership), not as Rust kernels.
