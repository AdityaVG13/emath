//! SDE kernel coverage lane, pruned with the cutover (`a2581b3`): the
//! native `emath_rt::stochastic` kernels (Ito/Stratonovich rules,
//! `sde_euler_maruyama`) and the `emath_core::stochastic` public surface
//! no longer exist, and no authored capability cell replaced them — the
//! keep-set constructor pins live in
//! `tests/emath-exec-ir/tests/constructor_cutover.rs`. If SDE math
//! returns, it returns as authored `.emath` definitions (AGENTS.md
//! language ownership), not as Rust kernels.
