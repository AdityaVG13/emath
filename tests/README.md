# tests

Single home for this repository's behavioral integration tests, organized
by crate as \`tests/emath-<crate>/\`.

## Policy

- The shipping surface is production code under \`crates/\`; compile/lint
  hygiene is verified by the workspace build itself (\`cargo check --workspace\`,
  \`cargo clippy --workspace --all-targets -- -D warnings\`).
- \`tests/emath-<crate>/\` crates are workspace members that exercise the
  PUBLIC API only, run by \`cargo test --workspace\`. Behavioral tests,
  round-trips, and negative controls live here on purpose — this is their
  landing zone.
- A test that only reaches private internals is deleted, not moved; either
  exercise via the public surface or drop it.
- New crate-level integration tests are added under \`tests/emath-<crate>/\`.
- Generated-crate behavior is verified separately: \`build --verify\` /
  \`emath test\` refuse a generated crate with no \`#[test]\` tests
  (E-TLT-012), so \`tests:\` sections in specs are the mock-free assurance
  for generated code, backed by the \`scripts/validate.sh\` capstones.
