# emath-lab-core CONTRACT

## Purpose and layer

Host-layer laboratory core. Three concerns, all std-only and deterministic:

- `measure`: deterministic measurement harness over injected samples.
- `stats`: the paired statistical protocol (warmup, seeded shuffle, MAD
  outlier trim, ratio statistics).
- `law_check`: finite-world law obligations with minimized counterexamples.

Production user: `emath-cli` diagnostics (`law_check`). Wall-clock timing
enters only as injected samples; the crate never reads a clock.

## Public types and semantics

- `measure`: `Measurement`, `MeasurementKind`, `Summary` (with
  `quarantined()` at `QUARANTINE_CV_PCT`).
- `stats`: `StatisticalProtocol`, `OutlierPolicy`, `PairedObservation`,
  `PairedResult`, `evaluate_paired`, and the pure helpers `mean`,
  `percentile`, `percentile_f64`.
- `identity`: `EngineIdentity`, `EngineRole`; `require_distinct` refuses a
  comparison of an engine with itself.
- `law_check`: `Law`, `WorldObligation`, `FiniteLawChecker`,
  `WorldCheckReport`, `LawVerdict`, `MinimizedCounterexample`,
  `ScopedAuthority`, `CheckerReceipt`, `CheckerError`.
- Crate root: `Sampler` (splitmix64-style seeded samples in `[-1, 1)`),
  `Authority` (lattice Structural < Tested < Certified < Proved), `LabError`.

## Invariants

- Every summary and paired result is a pure function of its inputs and seed.
- Zero-duration paired samples refuse; they never produce an infinite ratio.
- The first law violation over the sorted carrier is the lexicographically
  smallest counterexample.
- `Authority` is never escalated by ranking.

## Error model

`LabError { code, message }` with stable codes: `E-HOST-006` insufficient
evidence, `E-HOST-008` incomparable inputs, `E-HOST-012` invalid protocol,
`E-HOST-016` self-comparison. `law_check` returns `CheckerError`.

## Determinism class

Deterministic and byte-comparable for identical inputs.

## Cancellation behavior

None. All operations are bounded synchronous computations.

## Unsafe boundary

`#![forbid(unsafe_code)]`.

## Feature flags

None.

## Conformance tests

`tests/emath-lab-core/tests/`: `identity`, `law_check`, `laws`, `measure`,
`stats`.

## No-claim boundaries

- No timing claim: the crate summarizes samples it is given.
- Law checking covers finite carriers only; it proves nothing about
  infinite worlds.
