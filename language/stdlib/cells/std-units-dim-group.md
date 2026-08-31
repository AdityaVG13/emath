# std::units — dim-group (dimensional analysis as a group)

Bead: `emath-sci-physics-lane-3f7v` (thin slice). Owner module:
`crates/emath-core/src/units.rs` (extends the alias/affine contract from
`emath-r3-unit-aliases-affine-tao6`).

## What this adds

Dimensional analysis is a **group**, not an exponent bag. The carrier is the
free abelian group **Z⁷** over the SI base dimensions (L, M, T, I, Θ, N, J =
`m, kg, s, A, K, mol, cd`):

- composition (`dim_add`) = multiplication of physical quantities;
- inverse (`dim_neg`) = reciprocal;
- identity (`dim_identity`) = the dimensionless element, notated `1`
  (`dim_notation` is canonical: fixed base order, zero exponents omitted,
  exponent 1 bare — `m^2*kg*s^-2`);
- power (`dim_pow`) = repeated composition;
- `dim_is_identity` decides dimensionlessness.

Compound units compose exactly: `J = force · length` is group composition,
not a lookup.

## Law-grade homogeneity receipts

`check_homogeneity(lhs, rhs)` proves `⟦lhs⟧ =symp ⟦rhs⟧` and returns a
`HomogeneityReceipt` carrying the shared witness dimension and its canonical
notation. Refusal is `E-UNIT-DIM` with **both** sides' notations in the
message — a law-grade diagnostic, never a bare code.

## Buckingham π-theorem (witness minimization)

`dimensionless_groups(vars)` computes the dimensionless groups as an integer
null-space basis of the variable×base-dimension matrix. Exactly
`n − dim_rank(vars)` groups; each is **witness-minimized** (primitive: gcd of
coefficients is 1) and sign-canonical (first nonzero coefficient positive).
The basis is scale-invariant: scaling every variable dim by k yields the same
normalized groups (pinned by test — this is a mutation canary for the
renormalization gate). Pure integer arithmetic, deterministic order, no
floats.

Classic shape verified: variables `v, ρ, r, μ, F` (rank 3, n = 5) give
exactly two groups — the Reynolds number `v·ρ·r/μ` and the inverse drag
coefficient `v²·ρ·r²/F`.

## Affine units are NOT a group (negative check, pinned)

Ratio (scale-only) units are closed under the group. Affine units (offset ≠
0, e.g. `degC`, `degF`) are a **torsor** over the group, never an element:
`20 degC × 2` **refuses** `E-UNIT-AFFINE-2` (there is no "40 degC" — affine
composition is meaningless). The difference unit `ΔdegC` IS multiplicative:
`ΔdegC × 2 = 40 ΔdegC`.

## Codes used (no new codes)

`E-UNIT-DIM` (homogeneity violation), `E-UNIT-AFFINE-2` (affine
multiplicative composition), `E-UNIT-104` (unknown unit — pre-existing).

## Fences (later slices of 3f7v — not claimed here)

- **tensor-geometry**: metric tensor, Christoffel symbols, Riemann/Ricci
  curvature, metric compatibility, Bianchi identities.
- **conservation-law**: Noether currents and on-shell conservation
  certificates. No conservation claim is made by this slice.
- **variational-action**: action functional, Euler-Lagrange operator, on-shell
  equality distinct from `==` (fenced with the
  `emath-r3-lagrangian-action-nf7s` design follow-up).
- Surface admission (`units:` section through sema/IR) lands with the IR
  integration slice, per the tao6 no-claim boundary.

## Determinism class

Pure integer group arithmetic; fixed iteration order; same inputs → same
receipts, same π basis, same refusal messages.
