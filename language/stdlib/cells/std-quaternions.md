# `core::algebra`; quaternion, dual, Clifford nucleus (B44, xx0x.6-adjacent)

Status: **emath-core reference nucleus landed** (bead
`emath-r3-quaternions-cgvg`). Contract-first: the sema admission table
does not admit `quat`/`qi`/`qj`/`qk`/`Dual`/`Clifford` names yet, so
`.emath` models calling them refuse with the standard
unknown-function diagnostic until the admission-table follow-up (the
special-functions seam pattern).

## C18 resolution (the `i/j/k` collision)

NO new literal suffix exists. The complex `Ni` production (B14) keeps
`i`; `3j` and `4k` were never complex literals. Quaternions spell via
the constructor and (at admission time) named constants:

```text
quat(w, x, y, z)          - the constructor
qi, qj, qk                - named basis constants (admission follow-up)
```

`1 + 2i + 3j + 4k` therefore does NOT parse as a quaternion (or as a
complex literal; `j`/`k` are not complex): the negative control is a
PARSE-level refusal, exactly the collision-freedom the bead requires.

## Contract

| Type | Carrier | Laws | Boundaries |
|---|---|---|---|
| `Quaternion` (`quat(w,x,y,z)`) | f64 ×4 | Hamilton: `i²=j²=k²=ijk=−1`, `i·j=k`; **non-commutative is a pinned contract** (`i·j ≠ j·i`) | `normalize`/`inverse` of zero REFUSE (no NaN laundering); `rotate_vector` is `q v q̄` (Hamilton, active); a non-unit `q` scales by `‖q‖²`; documented, not hidden |
| `Dual` (`Dual::new(value, ε)`, `::variable(x)` = `(x, 1)`) | f64 ×2 | **ε² = 0 exactly** (algebraic truncation, no tolerance): `(a+bε)(c+dε) = ac + (ad+bc)ε`; division `(a+bε)/(c+dε) = a/c + (bc−ad)/c²·ε` | zero real part in a divisor: the operator carries the fault visibly (NaN), `checked_div` is the typed refusal seam; exact first-order derivatives; no FD error, no step size |
| `CliffordBasis::new(p, q)` + `MultiVector` | f64 coefficients, sparse blade terms | multiplication table DERIVED from `(p, q)`: `e_i² = +1` (i ≤ p) / `−1` (i > p); `e_i·e_j = −e_j·e_i` (i ≠ j); never hand-listed | `blade_count = 2^(p+q)` (exponential cost is explicit, no silent truncation); `Clifford<p, q>` const-generic surface binds at admission (C10 CLOSED) |

Checks the pins enforce: `i·j = k` and `j·i = −k`; `i² = j² = k² = −1`;
`q·q̄ = ‖q‖²`; 90°-about-z rotation of `(1,0,0) → (0,1,0)`;
`(2+ε)³ = 8 + 12ε` (exact); `Cl(2,0)` vector self-product = squared
norm with vanishing wedge; `Cl(0,2)` reproduces the quaternion laws
(e1² = e2² = −1, e1·e2 = −e2·e1).

## Implementation

`crates/emath-core/src/{quaternion, dual, clifford}.rs`; std-only,
deterministic, no allocation beyond the sparse term vector. Blade
reduction terminates because each step either shrinks the index list
(annihilation) or strictly increases its sortedness (swap).

## No-claim boundaries

- f64 carrier, labeled: quaternion norms and Clifford coefficients
  are floating-point, not exact algebra. An exact-rational Clifford
  layer would be a different contract.
- `Dual` is first-order ONLY (ε² = 0 by definition): second-order
  information does not exist in the carrier (use the reverse-AD tape
  or a hyper-dual extension; both follow-ups, neither silent).
- `rotate_vector` with a non-unit quaternion is a rotation+scale; the
  nucleus does not auto-normalize (caller-visible choice).
- Clifford geometric-ALGEBRA surface (rotors, meet/join, Hodge star)
  is the algebra-world follow-up; the nucleus ships the product
  structure and sparse multivectors.
- No grammar change: nothing here extends the `.emath` parser (the
  C18 avoidance IS the absence of new suffixes).
