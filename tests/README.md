# tests

Single home for this repository's tests, organized by crate.

## Policy (intent-not-fluff, LIQUIDATE)

- The shipping surface is production code under `crates/`. Compilation
  is verified by the workspace build itself (`cargo check --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `scripts/validate.sh`).
- Only the exact tests required for the program to compile may live
  here — i.e. minimal compile witnesses of a crate's public surface,
  organized as `tests/emath-<crate>/`.
- Behavioral suites, golden files, corpus sweeps, and internal-structure
  tests are not shipped. A test that only reaches private internals is
  deleted, not moved.
- New crate-level tests are added under `tests/emath-<crate>/` as
  integration tests using the public API only.
