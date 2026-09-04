# `std.combinatorics.census`; inclusion-exclusion and derangement censuses

Status: **law pack + example landed** (Wave 16, discrete-math lane,
selective). Inclusion-exclusion censuses over finite sets: the three-set
union census and the derangement census in both its alternating
inclusion-exclusion form and its recurrence form, evaluated in exact
i64 integer arithmetic. No asymptotic or rounding claim.

## Contract (finite sets, exact Int carrier)

| Object | Form | Semantics | Boundaries |
|---|---|---|---|
| `InclusionExclusionThreeSets` | law, seven counts → `union_count` | `|A∪B∪C| = Σ singles − Σ pairs + triple` as an evaluated census | the seven inputs are the Venn-cell totals of the same three finite sets |
| `DerangementAlternatingCensus` | law, `n → d_n` | the five-term alternating sum `n!/k! − … + n!/4!` at the fixed four-element shape (`D_4 = 9`) | shape is fixed at four elements; no asymptotic or rounding claim |
| `DerangementRecurrenceStep` | law, `seed0, seed1 → d5` | the two-term recurrence `D_m = (m−1)(D_{m−1} + D_{m−2})` unrolled to `D_5 = 44` | fixed seeds `D_0 = 1`, `D_1 = 0`; exact Int arithmetic |

## Executable surface

- Laws: `language/stdlib/laws/combinatorics-derangements.emath`
  (package `combinatorics.derangements.laws`).
- Example: `language/examples/discrete/catalan_tables.emath`
  (`DerangementAlternatingCensus`, `DerangementRecurrenceCensus`,
  `InclusionExclusionCensus`): the alternating chain 24 − 24 + 12 − 4
  + 1 = 9, the recurrence unrolled to `D_5 = 44`, and the three-set
  union census `12` from dyadic Venn counts.

## No-claim boundaries

- Exact census arithmetic on small finite sets only; no asymptotic
  inclusion-exclusion bounds, no floating-point approximations.
- The general inclusion–exclusion censuses over infinite families are
  out of scope; each delivered law evaluates a fixed finite shape.
- Matroid/design/geometry combinatorics fields from the same gap
  section are untouched by this slice (selective, not bulk import).
