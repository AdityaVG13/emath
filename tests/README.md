# tests

Single home for this repository's behavioral integration tests, organized
by crate as \`tests/emath-<crate>/\`.

## Policy

- The shipping surface is production code under \`crates/\`; compile/lint
  hygiene is verified by the workspace build itself (\`cargo check --workspace\`,
  \`cargo clippy --workspace --all-targets -- -D warnings\`).
- \`tests/emath-<crate>/\` crates are workspace members that exercise the
  PUBLIC API only, run by \`cargo test --workspace\`. Behavioral tests,
  round-trips, and negative controls live here on purpose: this is their
  landing zone.
- A test that only reaches private internals is deleted, not moved; either
  exercise via the public surface or drop it.
- New crate-level integration tests are added under \`tests/emath-<crate>/\`.
- Generated-crate behavior is verified separately: \`build --verify\` /
  \`emath test\` refuse a generated crate with no \`#[test]\` tests
  (E-TLT-012), so \`tests:\` sections in specs are the mock-free assurance
  for generated code, backed by the \`scripts/validate.sh\` capstones.

## \`.emath\` fixtures

- \`tests/valid/\` contains \`.emath\` sources expected to parse/admit at the
  appropriate phase. \`affine_policy.emath\` is the phase-1 vertical-slice
  spec compiled by \`scripts/validate.sh\`, \`scripts/reproducible_lane.sh\`,
  the \`demo-host\` build script, and \`cargo xtask demo cache-policy\`;
  \`square.emath\` is the minimal function example.
- \`tests/invalid/\` names the required diagnostic in its first comment and is
  exercised by the negative controls in \`scripts/validate.sh\`. These are
  strategy fixtures until the full parser is implemented; Phase 1 copies
  supported fixtures into executable tests.
