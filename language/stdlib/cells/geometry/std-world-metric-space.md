# `std.geometry.world.metric-space`; metric-space world slice

Status: language-layer capability cell (wave-16 worlds lane, bead
`emath-wave16-catalog-epic-fassw.26`, catalog item **metric-space
world** — "carrier with a distance satisfying metric laws").
HONEST THIN SLICE: the world is realized as user-declared `.emath`
capability data over the admitted strict-f64 surface (norm/dot
builtins + arithmetic); it is NOT a first-class `world` declaration —
the genesis/world machinery (`language/grammar/genesis.ebnf`) is
design-only today and the parser refuses unadmitted sections, so a
native world construct is a NAMED FENCE, not a silent gap.

## World contract (what the cell fixes down)

A metric-space instance is a pair (carrier X, distance d) where the
carrier is a declared Float64 vector domain and the metric laws are
executable witnesses pinned as exact signed slacks:

- identity of indiscernibles: `d(x, x) = 0` (pinned exactly);
- symmetry: `d(x, y) - d(y, x) == 0` (exact for the admitted forms);
- triangle inequality: `d(x, z) - (d(x, y) + d(y, z)) <= 0`, encoded
  as a signed slack (negative = strict, 0 = equality case);
- non-negativity enters through the squared-distance form and the
  zero pins (no comparison operators exist in evaluate targets).

## Instances realized

- `(R^3, L2)`: `d(p, q) = norm(p - q)` over Vector[3].  Lattice
  witness pins `d(0, (3,4,12)) = 13.0` exactly (integer coordinates,
  exact squares); irrational distances are single correctly-rounded
  sqrt literals verified independently (sqrt(29), sqrt(20), sqrt(17),
  sqrt(53) chain).
- `(unit sphere of R^3, chord)`: carrier restricted to unit vectors;
  chord distance via the componentwise chain, with the chord law
  `chord^2(u, v) = 2 - 2·dot(u, v)` pinned bit-for-bit at cardinal,
  antipodal, and degenerate points (4.0 / 0.0 / 2.0 witnesses).

## NAMED FENCES (formulas of record, instances pending)

- L1/Manhattan and L-infinity/Chebyshev instances: need componentwise
  `abs`/`max`, which ADMIT end-to-end in the strict-f64 emitter since
  emath-s9w1m + fpl60 (reprobed 2026-09-01: green on
  `language/examples/intro/clamp-distance-builtins.emath`); the
  instances enter with a cell revision, not an emitter change.
- Pseudometric / finite-space instances with boolean membership
  predicates: no boolean values in evaluate targets.
- First-class world machinery (world constructor, carrier admission,
  law statements as typed obligations): genesis design grammar only.

## Test shape

Each test-bearing declaration observes exactly one evaluate target;
metric axioms are pinned as signed slacks, one witness declaration per
instance, whole-vector equality in expects.  Mutation-checked: a
1-ULP pin flip is killed by the R3 probe witness.

## Contracts

- Pure, deterministic, strict-f64; zero core delta.
- Runnable artifact:
  `language/examples/geometry/worlds/metric-space-world.emath`
  (check + generated-crate tests pass; pin mutants killed).
- No-claim boundary: this cell does not provide world-level
  quantification over carriers ("for all x in X") — witnesses are
  pinned probes, not universally quantified proofs.
