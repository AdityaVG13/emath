# Seedling — Tier-0 teaching profile

Seedling is the ten-construct teaching tier: an afternoon course that
builds a working model by the end. It is **deletion-defined**: everything
in Seedling is ordinary emath; the profile is defined by what it removes
from full emath, never by new syntax. A Seedling file is a full-emath
file, so `emath check` / `fmt` / `run` / `simulate` on a lesson are the
conformance proof — one compiler, no divergent behavior to reconcile.

## The ten constructs

| # | Lesson | Construct | File |
|---|--------|-----------|------|
| 1 | Numbers | `Float64` bindings (`Real` is the concept, ch. 1 §0) | `lesson-01-numbers.emath` |
| 2 | Arithmetic | `+ - * / ^`, parens | `lesson-02-arithmetic.emath` |
| 3 | Definitions | `name = expr` binding | `lesson-03-definitions.emath` |
| 4 | Functions | `emath function Name:` | `lesson-04-functions.emath` |
| 5 | Inputs/outputs | typed fields, multiple outputs | `lesson-05-inputs-outputs.emath` |
| 6 | Conditionals | `cases x: \| cond => v \| else => v` (C1 spelling) | `lesson-06-conditionals.emath` |
| 7 | Vectors | `Vector<n>`, `v[i]`, `length(v)` | `lesson-07-vectors.emath` |
| 8 | Folds | `sum i in a..b:`, `product` | `lesson-08-folds.emath` |
| 9 | Simple ODEs | `emath model`, `der(s)`, rate rows | `lesson-09-odes.emath` |
| 10 | Goals | `evaluate <t>:`, `produce`, `tests:`, `compile:` | `lesson-10-goals.emath` |

## What Tier-0 removes

Everything not listed above: quantities and units, matrices/tensors,
records and sets, custom kinds, notations, provenance/evidence,
strategies, worlds, policies, autodiff/solve/minimize, exact rationals,
complex numbers, pattern matching. A learner who needs more graduates to
the full reference (ch. 1 onward) — no unlearning required, because
nothing here is Seedling-specific syntax.

## Conformance

- **Pair-test, honest form:** Seedling and full emath share one
  compiler, so "same result under both profiles" holds by construction;
  the executable evidence is each lesson passing `emath check` and
  `emath fmt` canonical round-trip (lesson 9 additionally simulates:
  `emath simulate lesson-09-odes.emath --set k=0.5 --set s=2.0 --dt
  0.01 --t1 1.0`).
- **Negative control, honest fence:** refusing a non-Seedling construct
  *by profile* needs an enforcing checker (a Tier-0 admission mode), which
  does not exist yet. Until it lands, "Tier-0" is enforced by this
  corpus staying inside the deletion set — not by the compiler. The
  checker is named future work; do not claim profile refusal today.
- Lesson 6 uses the resolved C1 conditional spelling
  (`cases`/`else`, totality enforced at parse time; reference ch. 7),
  so the C1 gate is satisfied.
