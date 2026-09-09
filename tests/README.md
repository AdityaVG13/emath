# tests

Failure-first intent library. Not a 1:1 mirror of production functions.

## Law

- Tests live here, never next to production code.
- Demand the intended math. A test that passes against today's incomplete
  compiler is fluff. The probe stays red until the engine upgrades.
- One `#[test]` runs many functions, many files, many stages. Do not ship a
  test per assertion.
- Never skip, ignore, or XFAIL. A known hole is a recorded failure, not a
  green row.
- Independent oracles (algebra, named diagnostics). Never snapshot the code
  under test and call that expected.

## Library

`tests/harness` (`emath-test-harness`) is the builder:

```rust
use emath_test_harness::{boot, Probe, Source};

#[test]
fn dense_index_and_oob_are_one_intent() {
    boot();
    let mut p = Probe::new("rank-polymorphic index; OOB is E-SHAPE-006");
    Source::from_str("ok", "...").eval_tests(&mut p);
    Source::from_str("oob", "...").must_refuse(&mut p, &["E-SHAPE-006"]);
    p.finish();
}
```

`Probe` collects every mismatch and panics with all of them. `Source` is
parse → admit → eval. `demand_workspace_corpora` walks `tests/valid`,
`tests/invalid`, `tests/fixtures/language`, and `language/examples`.
`demand_language_gaps` is the upgrade ratchet (RK45, range slices, interval
arithmetic): it is supposed to fail in multiple places.

## Corpora

- `tests/valid` — runnable math must admit and `Passed` every `expect`.
  Catalog shells (field packs, custom) must admit.
- `tests/invalid` — header pins `expect: E-XXX-NNN`; every pinned code must
  fire as Error.
- `tests/fixtures/language` — same contract as valid.
- `language/examples` — teaching set; same hard eval contract.
- `tests/conformance/DISCREPANCIES.md` — spec vs impl ledger. WILL-FIX rows
  are probe failures, not skips.

## Layout

`tests/emath-<crate>/` are workspace members. They exercise the public API
through the harness. Named `cargo test -p <crate> --test <file>` is the
only targeted run; never a workspace-wide suite as routine verification.
