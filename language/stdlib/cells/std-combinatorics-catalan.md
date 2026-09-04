# `std.combinatorics.catalan`; recurrence-table slice

Status: **law pack + example landed** (Wave 16, discrete math /
combinatorics, selective). Catalan numbers evaluated two ways — the
Segner convolution recurrence step and the closed-form central-binomial
ratio — as exact small-integer arithmetic. No asymptotic growth claim.

## Contract (small exact Int, i64 fold carrier)

| Object | Form | Semantics | Boundaries |
|---|---|---|---|
| `CatalanRecurrenceStep` | law, `c0, c1, c2, c3 → c4` | one convolution step `C(n+1) = Σ C_k C_{n−k}` from the seed row `1, 1, 2, 5` to `C_4 = 14` | fixed four-term step shape; exact Int arithmetic |
| `CatalanClosedForm` | law, `n → c_n` | `C(2n,n)/(n+1)` via the exact factorial fold | `2n <= 20`; the division is exact at every test value |

## Executable surface

- Laws: `language/stdlib/laws/combinatorics-catalan.emath`
  (package `combinatorics.catalan.laws`): `CatalanRecurrenceStep`,
  `CatalanClosedForm`.
- Example: `language/examples/discrete/catalan_tables.emath`
  (`CatalanTwoAuthorities`): the closed form and the convolution step
  must agree exactly at `n = 4` (both 14).

## No-claim boundaries

- Only the recurrence evaluation and the closed-form slice are claimed;
  no asymptotic growth rate, no asymptotic enumeration.
- The general sequence table (Stirling, Bell) is a declared follow-up
  slice; only the convolution step and closed form are claimed.
- The recurrence is evaluated at fixed indices; no symbolic closed form
  is proved.
