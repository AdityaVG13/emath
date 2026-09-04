# `std.analysis.finite.genfun`; generating functions as exact coefficient arithmetic

Status: std-layer capability cell (authoring draft). Pure surface
contract over ADMITTED primitives only (`generating_function`,
`convolution`, `coefficient`): no core IR/op-enum change, no new
binder kind. The generating-function lane of the advanced-analysis
pack: a linear recurrence, its rational generating function, and the
Cauchy product are ONE exact coefficient identity, not an analytic
claim.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.analysis.finite.genfun` |
| Class | `pure` (formal power series over the admitted f64 Sequence carrier) |
| Version | `0.1.0` |
| Migration | `experimental` |

## World contract

`generating_function(num, den, count)` returns the first `count`
coefficients of the formal power series with

```text
Num(x) / Den(x) = sum_{n >= 0} x[n] x^n
```

where `num`, `den` are coefficient vectors in ASCENDING power order
and `den[0] != 0`. The recurrence reading is exact: for
`den = [d0, d1, ..., dk]` the coefficients satisfy the linear
recurrence

```text
d0 * x[n] + d1 * x[n-1] + ... + dk * x[n-k] == 0   (n >= k, with num
supplying the initial values)
```

so `den = [1, 1]`, `num = [0, 1]` is EXACTLY `x[n] = x[n-1] + x[n-2]`
(Fibonacci). All values here are integer-valued; within the f64
carrier they are exact (every partial sum below 2^53), so the
identities are checked with `==`/`congruence`-style exact equality and
no tolerance.

## Laws (exact, finite)

1. **Cauchy product.** `convolution(a, b, count)[n] ==
   sum_{i+j=n} a[i] * b[j]` for every `n < count` — the coefficient of
   `x^n` in `A(x) B(x)`. Verified against direct unrolled sums at
   fixed small indices; the direct sum is the definition, `convolution`
   must agree exactly.
2. **Recurrence encoding.** The generated coefficients satisfy the
   denominator recurrence exactly: for the Fibonacci world,
   `coefficient(g, n) - coefficient(g, n-1) - coefficient(g, n-2) == 0`
   at every pinned `n >= 2 < count`, and the closed value
   `coefficient(g, 7) == 13` pins the initial conditions.
3. **Convolution diagonal identity (self-convolution).** For the
   Fibonacci series `g`, `(g*g)[n] = sum_{i} fib[i] fib[n-i]` gives
   `0, 0, 1, 2, 5, 10, ...`; the unrolled sum and `convolution` agree
   exactly at the pinned indices (a discriminatinng check against an
   off-by-one in either sum).
4. **Shift/denominator correspondence.** Multiplying the series by
   `(1 - x - x^2)` annihilates it: `g[n] - g[n-1] - g[n-2] == 0` — the
   same identity read backwards; the denominator IS the recurrence.

## Refusals (typed, never silent)

- `den[0] == 0` (or a leading-zero denominator): no formal inverse
  exists; the call refuses upstream rather than emitting a shifted
  series silently.
- Non-finite or non-integer-valued intermediate values: outside the
  exactness claim of this cell (the honesty boundary is 2^53; values
  beyond it are the float-analysis lane's problem, not silently
  blessed here).
- Formal-series EVALUATION at a point is NOT claimed: the admitted
  `series_at` belongs to the measured-Series world (declared
  interpolation/extrapolation semantics), not to formal power series.
  A pointwise sum `sum_n x[n] t^n` with a truncation bound is future
  cell work; this cell refuses to blur the two worlds.
- KNOWN BACKEND DEFECT (escalated, not worked around here): the
  generated-Rust path for `generating_function`/`convolution` fails to
  compile — `codegen_render/op_domains.rs` passes the scalar budget
  through `operand_ref` as `&(16.0)` while
  `emath_rt::sequence_generate(initial: &[f64], recurrence: &[f64],
  budget: f64)` takes the count BY VALUE (`error[E0308]: expected
  f64, found &{float}`). `emath check` and the reference-VM `emath
  eval` are unaffected; only `emath test`'s generated crate refuses.
  The backend test (`sequence_recurrence_generates_shared_runtime_call`)
  greps for the call name and never compiles it, which is why the
  defect survived.

## Surface spelling

```emath
emath function genfun_cauchy:
    inputs:
        n: Int
    outputs:
        cauchy_ok: Bool
    definitions:
        g = generating_function([0.0, 1.0], [1.0, 1.0], 16.0)
        square = convolution(g, g, 16.0)
        # (g*g)[3] = fib0 fib3 + fib1 fib2 + fib2 fib1 + fib3 fib0 = 2
        d3 = (coefficient(g, 0.0) * coefficient(g, 3.0)
              + coefficient(g, 1.0) * coefficient(g, 2.0)
              + coefficient(g, 2.0) * coefficient(g, 1.0)
              + coefficient(g, 3.0) * coefficient(g, 0.0))
        cauchy_ok = ((square[2] == 1.0) and (square[3] == d3))
```

Runnable demonstration with exact Cauchy, recurrence, and
self-convolution checks: `language/examples/analysis/finite-genfun.emath`.

## Named fences (deliberately open)

- No closed-form Binet evaluation (float sqrt; approximate lane).
- No rational-function arithmetic beyond the admitted
  `generating_function` call (no gcd/partial-fraction surface yet).
- No formal-series evaluation/`series_at` tie-in (measured-Series
  world fence, stated above).
- No asymptotics or growth-rate claims; the cell is finite
  coefficient arithmetic only.
