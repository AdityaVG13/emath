# `std.analysis.finite.opnorms`; operator norms on finite-dimensional matrices

Status: std-layer capability cell (authoring draft). Pure surface
contract over ADMITTED primitives only (`abs`, `dot`, `norm`,
`singular_values`, fixed-shape matrix literals): no core IR/op-enum
change, no new binder kind. Finite-dimensional operator norms are
finite sums/maxima; the exact-vs-approximate boundary of each norm is
stated per norm, not blurred.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.analysis.finite.opnorms` |
| Class | `pure` (fixed small-n matrix norms) |
| Version | `0.1.0` |
| Migration | `experimental` |

## The four norms and their honesty class

For `A : Matrix[m, n]` with finite entries:

| Norm | Definition | Honesty class |
|---|---|---|
| `\|A\|_1` (max column abs-sum) | `max_j sum_i abs(a[i][j])` | EXACT finite max of finite sums (single rounding class: one `abs`/`+` per entry in the f64 carrier) |
| `\|A\|_inf` (max row abs-sum) | `max_i sum_j abs(a[i][j])` | EXACT, same class as `\|A\|_1` |
| `\|A\|_F` (Frobenius) | `sqrt(sum a[i][j]^2)` | one `sqrt` after an exact-shape sum; the square `\|A\|_F^2` is the honest comparable quantity |
| `\|A\|_2` (spectral) | `sigma_max(A)` | APPROXIMATE on the f64 carrier: `sigma_max = singular_values(A)[0]` (descending order per the linalg contract). No exactness claim, no certificate. |

`\|A\|_2` is exact only in structured cases (normal matrices: the
largest `|eigenvalue|`; rank-1 `A = u v^T`: `sqrt(dot(u,u)*dot(v,v))`).
The cell states the structured shortcuts and refuses to claim
general-n exactness.

## Laws (finite-dimensional, checkable on fixed shapes)

1. **Submultiplicativity, induced norms.**
   `\|A B\| <= \|A\| \|B\|` for `\|.\|` in {1, inf, 2} on compatible
   fixed shapes. On finite dims this is a finite inequality; the cell's
   tests check instances, not the general proof.
2. **Consistency.** `\|A x\|_2 <= \|A\|_2 \|x\|_2` and
   `\|A x\|_2 <= \|A\|_F \|x\|_2` (Cauchy-Schwarz on the sum).
3. **Norm comparison on finite dims.** For the f64 carrier with
   nonnegative entries the bounds
   `max(a_ij) <= sigma_max <= \|A\|_F <= sqrt(m n) max(a_ij)` and
   `\|A\|_2 <= \|A\|_F` hold; the spectral norm is bracketed, never
   assumed.
4. **Distinct norms are distinct.** On the demo matrix
   `[[3,0],[4,5]]` the four values (7, 9, sqrt(50), sqrt(45)) are all
   different: no norm silently substitutes for another.

## Refusals (typed, never silent)

- Non-finite entry: refuses before any sum (norm of a non-finite matrix
  is not a number).
- Shape mismatch in a product/consistency check: refuses at admission
  (shape inference), not at evaluation.
- A test claiming `\|A\|_2` exactness where the carrier is f64: contract
  violation; the honest spelling compares squares with an explicit
  tolerance, or uses a structured case where the value is exact.

## Surface spelling

```emath
emath function opnorms_2x2:
    inputs:
        m00: Float64
        m01: Float64
        m10: Float64
        m11: Float64
    outputs:
        n1: Float64
        ninf: Float64
        fro2: Float64
        sig2: Float64
    definitions:
        A = [[m00, m01], [m10, m11]]
        # `abs`/`max` admit in the strict-f64 subset since emath-s9w1m;
        # this spelling keeps the original exact composition from the
        # admitted 2-norm: |x| = norm([x]) and
        # max(a, b) = (a + b + |a - b|) / 2 (exact for these inputs).
        a0 = norm([m00]) + norm([m10])
        a1 = norm([m01]) + norm([m11])
        n1 = (a0 + a1 + norm([a0 - a1])) / 2.0
        r0 = norm([m00]) + norm([m01])
        r1 = norm([m10]) + norm([m11])
        ninf = (r0 + r1 + norm([r0 - r1])) / 2.0
        fro2 = m00 * m00 + m01 * m01 + m10 * m10 + m11 * m11
        # sigma_max^2 = largest eigenvalue of A^T A = [[25,20],[20,25]]
        sig = singular_values(A)[0]
        sig2 = sig * sig
```

Runnable demonstration with exact max/sum tests and explicit-tolerance
spectral checks: `language/examples/analysis/finite-opnorms.emath`.

## Named fences (deliberately open)

- No matrix-entry indexing primitive is claimed beyond fixed literal
  shapes; a general `A[i][j]` access surface is compiler admission work.
- No condition-number or condition-estimate cell (depends on this one
  plus honest reciprocal bounds; separate slice).
- No induced-norm OPTEXIMIZATION (max over unit vectors is not
  enumerated; only the closed forms above).
- Rat-carrier matrices (exact norms over the rationals) need the exact
  matrix carrier to land; until then Frobenius/1/inf exactness claims
  are scoped to the f64 sum rounding class, and the spectral norm stays
  approximate.
