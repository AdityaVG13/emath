//! Exact-rational `Rat` kernel coverage lane, pruned with the cutover:
//! `emath_rt::rat` (the i128 exact-rational kernel, `fba34ea`) no longer
//! exists anywhere in the tree. Canonical rational arithmetic is a
//! representation primitive class (AGENTS.md), so if it returns it
//! lands in `emath-core` with a fresh failure-first pin — this husk
//! keeps the history discoverable without compiling against a deleted
//! module.
