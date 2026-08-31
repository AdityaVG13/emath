# `core::game_theory` — finite-carrier game claims (B41, t9m8)

Status: **emath-core reference nucleus landed** (bead
`emath-r3-game-theory-t9m8`). Contract-first: the sema admission table
does not admit game names yet (`BimatrixGame`, `nash`, …), so `.emath`
models calling them refuse with the standard unknown-function
diagnostic until the admission-table follow-up (special-functions seam
pattern).

## The design stance (per the bead)

**Nash equilibrium is a CHECKABLE CLAIM, not a computation.** The
checker verifies a claimed profile — "is this profile an equilibrium?"
— and never promises to FIND one. Equilibrium SEARCH (support
enumeration for mixed Nash, Lemke–Howson, etc.) is a different
contract and a declared follow-up; conflating the two is how
"solvers" quietly become oracles.

## C10 and C8 status

- **C10 CLOSED**: `BimatrixGame<M, N>` value generics can now bind at
  the `.emath` surface — the admission-table follow-up lands the
  spelling; the nucleus carries the same runtime-shaped finite carrier
  (row/column payoff matrices).
- **C8** (`exists s2 in S`): the binder grammar already owns `in` for
  quantifier carriers (`exists i in 0..n`), so NO parser change was
  needed — the bead's parser half is already satisfied by the
  quantifier slice; this bead lands the game-theory CARRIER.

## Contract

| Item | Signature | Semantics | Boundaries |
|---|---|---|---|
| `BimatrixGame` | two `PayoffMatrix` (row-major) | two-player finite game | ragged/mismatched matrices refuse at `validate`; out-of-range profiles are `Err`, never `false` (a typo is not game theory) |
| `is_nash_equilibrium(row, col)` | pure-profile claim | true iff NO unilateral deviation strictly improves either player | the strictness is the definition (an equal-payoff deviation does NOT break Nash — pinned); checks BOTH players (a row-only checker mutant dies on the pin) |
| `is_mixed_nash(row_mix, col_mix)` | mixed-profile claim | support condition: every strategy in a player's support maximizes expected utility against the opponent's mix (tolerance 1e-12) | a pure profile is a degenerate mix (the same checker covers both); **column strategies index COLUMNS** — the utility convention pin caught the first draft scoring the row player's payoffs for the column check |
| `best_responses(matrix, column)` | → set of row indices | ALL maximizing rows, ascending | **ties are a set** — a silent argmax pick would be an undisclosed choice; ragged/empty carriers yield an empty set, never a fabricated index |
| `MixedStrategy::new(weights)` | validated distribution | non-negative, finite, mass 1 within 1e-9 | **never renormalized** (the `core::probability` discipline); pure strategies are degenerate mixes |
| `expected_utility(matrix, row_mix, col_mix)` | exact bilinear form | `Σᵢⱼ rᵢ·cⱼ·payoff[i][j]` | shape mismatches refuse |

## Honesty fences (the infinite-oracle boundary)

- The carrier is FINITE (`m×n` matrices): infinite/continuous games,
  extensive-form games, repeated/stochastic games, and general Nash
  oracles are OUT by construction — the carrier cannot represent them,
  and nothing here approximates them silently.
- The claim checker answers "is THIS profile an equilibrium?" — it
  does not answer "does an equilibrium exist?" (that is
  Nash's theorem territory and a search computation; both refuse here
  by not existing).
- Zero-sum minimax, subgame perfection, and mechanism design are
  outside this bead's scope (P3 thin slice).

## Implementation

`crates/emath-core/src/game_theory.rs` — std-only, deterministic. The
mixed-Nash checker is O(m·n + m + n) per claim; `expected_utility` is
O(m·n). No randomness; no floating-point branching beyond the declared
1e-9 mass and 1e-12 comparison tolerances (both pinned).

## No-claim boundaries

- A `true` verdict is a THEOREM-CHECKED property of the given carrier
  (deviation enumeration), not a search result — there is no
  numerical-tolerance caveat on pure profiles beyond exact f64
  comparisons.
- Mixed-strategy equilibrium EXISTENCE is never claimed by this cell
  (Nash's theorem is mathematics, not this code); the checker only
  verifies profiles you bring it.
- Support enumeration, correlated equilibrium, and repeated-game folk
  theorems are declared follow-ups.
