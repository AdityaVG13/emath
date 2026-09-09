//! Typed spectral-PDE solve: the error model and safe wrapper over the strict-f64 kernel.
//!
//! The discrete sine-diagonalization Poisson solve executes from the authored
//! reference body in
//! `language/spec/capabilities/numerics/dynamics-control-pde.emath`;
//! this module retains no native implementation. Callers must go through
//! the installed capability seam (`std.capability.pde.poisson-sine`),
//! never a direct Rust entry point.
