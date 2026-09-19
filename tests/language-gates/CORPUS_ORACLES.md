# Corpus oracles

> History note (2026-09-19): the numeric shell oracles (spring/heat-plate
> final rows, the scratch E-GOAL-043 run) were cut from scripts/validate.sh
> when their fixtures under tests/fixtures/language were removed in the
> restructure. The in-process contracts below (official_examples_corpus,
> the Valid/Invalid rules) still govern the corpora.

Hard contract, not a snapshot of today's `emath check`.

`demand_workspace_corpora` is pinned in
`tests/emath-syntax/tests/official_examples_corpus.rs` (the
`workspace_corpora` case). It walks the four on-disk corpora and fails
on every broken contract row; the current red census is the pre-cutover
fixture mass (files written for the removed `model`/`policy`/`kind`
surface and dead diagnostic codes), not a machine defect — reconcile
those files with the current surface to turn the lane green.

`demand_language_gaps` is defined in the harness but deliberately NOT
wired into the suite: its rows (RK45 stepping, range slices, interval
endpoint arithmetic) are red by design until the engine grows, and a
red-by-design demand cannot sit in a green test. Wire it the day those
capabilities land, or split it into its own expect-red test.

## Valid / examples

Every `.emath` under `tests/valid`, `tests/fixtures/language`, and
`language/examples` that declares `emath function` / `law` / `policy` must
admit and evaluate every `tests:` `expect` to Passed. Computed-without-expect
and symbolic fallback are failures. Field packs and custom lanes must admit.

## Invalid

Every `.emath` under `tests/invalid` pins `expect: E-XXX-NNN` in the header.
The probe demands each pinned code as Error. Missing pins are failures.

## Upgrade gaps

`demand_language_gaps` (unwired; see above) demands RK45 stepping, range
slices, and interval endpoint arithmetic. Those rows are red until the
engine grows. Do not weaken them to match current no-claims.

## CLI oracles

The same file still demands `emath run` / `simulate` / `explain` on the
README rows. Exit codes and named diagnostics only — no golden numeric
snapshots of the current binary.
