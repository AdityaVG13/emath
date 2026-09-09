//! Typed ODE stepping: error model and safe wrappers over numerical kernels.
//!
//! The backward-Euler and velocity-Verlet stepping algorithms execute from
//! authored reference bodies in
//! `language/spec/capabilities/numerics/dynamics-control-pde.emath`;
//! this module retains no native implementations. Callers must go through
//! the installed capability seam (`std.capability.ode.backward-euler`,
//! `std.capability.ode.velocity-verlet`), never a direct Rust entry point.
