# Corpus oracles

Hard contract, not a snapshot of today's `emath check`.

Pinned in `tests/emath-syntax/tests/official_examples_corpus.rs` via
`emath_test_harness::{demand_workspace_corpora, demand_language_gaps}`.

## Valid / examples

Every `.emath` under `tests/valid`, `tests/fixtures/language`, and
`language/examples` that declares `emath function` / `law` / `policy` must
admit and evaluate every `tests:` `expect` to Passed. Computed-without-expect
and symbolic fallback are failures. Field packs and custom lanes must admit.

## Invalid

Every `.emath` under `tests/invalid` pins `expect: E-XXX-NNN` in the header.
The probe demands each pinned code as Error. Missing pins are failures.

## Upgrade gaps

`demand_language_gaps` additionally demands RK45 stepping, range slices, and
interval endpoint arithmetic. Those rows are red until the engine grows.
Do not weaken them to match current no-claims.

## CLI oracles

The same file still demands `emath run` / `simulate` / `explain` on the
README rows. Exit codes and named diagnostics only — no golden numeric
snapshots of the current binary.
