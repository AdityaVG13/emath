# Module acceptance fixtures

Failure-first and expected-refusal fixtures for the optional
`language/modules/` families. Positive fixtures run green:

    emath test tests/fixtures/modules/evolution_exact_steps.emath
    emath test tests/fixtures/modules/evolution_clipping.emath
    emath test tests/fixtures/modules/evolution_stream_vs_stored.emath
    emath test tests/fixtures/modules/evolution_reverse.emath
    emath test tests/fixtures/modules/evolution_parity_probe.emath
    emath test tests/fixtures/modules/fitting_exact_chain.emath
    emath test tests/fixtures/modules/fitting_unique_minimizer.emath
    emath test tests/fixtures/modules/fitting_planted_nonminimizer.emath
    emath test tests/fixtures/modules/fitting_verified_fit.emath
    emath test tests/fixtures/modules/fitting_heldout_zero.emath
    emath test tests/fixtures/modules/fitting_nonidentifiable_set.emath
    emath test tests/fixtures/modules/fitting_mismatch_bias.emath
    emath test tests/fixtures/modules/reachability_witness_paths.emath
    emath test tests/fixtures/modules/reachability_forced_attractor.emath
    emath test tests/fixtures/modules/reachability_forced_strategy.emath
    emath test tests/fixtures/modules/reachability_enumeration_crosscheck.emath
    emath test tests/fixtures/modules/integrator_contract_gate.emath
    emath test tests/fixtures/modules/integrator_contract_unique.emath
    emath test tests/fixtures/modules/integrator_contract_heldout.emath
    emath test tests/fixtures/modules/adaptive_refine_decay.emath
    emath test tests/fixtures/modules/adaptive_refine_ballistic.emath
    emath test tests/fixtures/modules/adaptive_cash_karp_equivalence.emath
    emath test tests/fixtures/modules/adaptive_budget_stops_incomplete.emath

Mixed fixtures (positive cases pass; the refusal cases fail with the
NAMED refusal codes — nonzero exit is the intended signature, same
convention as the refusal fixtures below):

    emath test tests/fixtures/modules/linear_systems.emath    # singular_matrix, ill_conditioned
    emath test tests/fixtures/modules/implicit_stiff.emath    # verification_budget_exhausted, singular_jacobian, nonconvergence
    emath test tests/fixtures/modules/intervals.emath        # h_out_of_range, negative_radicand, refinement_budget_exhausted, bad_bracket

Expected-refusal fixtures exit nonzero with the NAMED refusal code in
the failure line (the constructor layer has no positive
refusal-assert for functions; the named code in the output IS the
observable):

    emath test tests/fixtures/modules/evolution_refuse_zero_step.emath      # non_positive_step
    emath test tests/fixtures/modules/evolution_refuse_dim_mismatch.emath  # dim_mismatch
    emath test tests/fixtures/modules/evolution_refuse_direction.emath     # direction_mismatch
    emath test tests/fixtures/modules/evolution_refuse_bad_tableau.emath   # invalid_tableau
    emath test tests/fixtures/modules/evolution_refuse_order.emath         # order_conditions_unmet
    emath test tests/fixtures/modules/fitting_refuse_empty_domain.emath    # empty_domain
    emath test tests/fixtures/modules/fitting_refuse_seed_outside.emath    # seed_outside_domain
    emath test tests/fixtures/modules/fitting_refuse_nonidentifiable.emath # non_identifiable
    emath test tests/fixtures/modules/fitting_refuse_local_minimum.emath   # local_minimum
    emath test tests/fixtures/modules/adaptive_refuse_zero_scale.emath    # zero_scale
    emath test tests/fixtures/modules/adaptive_refuse_min_step.emath      # min_step
    emath test tests/fixtures/modules/adaptive_refuse_bad_bounds.emath    # bad_step_bounds
    emath test tests/fixtures/modules/adaptive_refuse_bad_budget.emath    # bad_attempt_budget
    emath test tests/fixtures/modules/reachability_refuse_not_forced.emath # not_forced
    emath test tests/fixtures/modules/reachability_refuse_bad_graph.emath  # bad_graph
    emath test tests/fixtures/modules/reachability_refuse_bad_start.emath  # bad_start
    emath test tests/fixtures/modules/reachability_refuse_empty_domain.emath # empty_domain

`evolution_budget_stop.emath` is the budget-contract fixture: it
asserts the rollout STOPS at the step budget reporting
`complete = false` with the last accepted time (`1/2`), never
stretching to the requested final time (`2`), plus the
`step_budget_ok` gate from `analysis.evolution.control`. Dropping the
budget stop (mutation check b) makes it run to completion and fail
those assertions.

## Fitting fixtures (bounded inverse fit reproduction)

The `fitting_*` fixtures reproduce the bounded inverse fit at full
scale over `search.fitting`: system `x' = v, v' = -x - c*v`, Euler
observations at truth `c = 1/2`, `h = 1/8`, 16 steps, initial state
`(1,0)`, domain `c = k/8, k = 0..16`.

- `fitting_exact_chain.emath` — the hillclimb from `c = 0` accepts
  exactly `0 -> 1/8 -> 1/4 -> 3/8 -> 1/2` with monotonically
  decreasing exact SSE ending at exactly zero.
- `fitting_unique_minimizer.emath` — exhaustive 17-candidate scan:
  `c = 1/2` is the unique minimizer.
- `fitting_planted_nonminimizer.emath` — a planted claim (`c = 3/8`)
  is rejected by the exhaustive verification.
- `fitting_verified_fit.emath` — `fit_unique` returns the accepted
  record (scope 1, exact-zero score and bound, witness ordinal).
- `fitting_heldout_zero.emath` — the recovered `c = 1/2` reproduces
  the held-out initial state `(2,1)` over 8 steps with zero error.
- `fitting_nonidentifiable_set.emath` — from rest `(0,0)` all 17
  coefficients fit: the feasible set is the whole domain (17-way tie).
- `fitting_mismatch_bias.emath` — observations from exact RK4 while
  fitting Euler: best coefficient `5/8` with a POSITIVE exact
  residual that beats `c = 1/2`. Exact arithmetic did not remove the
  discretization bias.

Named mutation checks over these fixtures: (a) flipping the
`argmin_first` tie rule to last-wins fails the ordered `[2,1,0]`
module test; (b) removing the exhaustive verification fails the
planted-nonminimizer fixture; (c) dropping the mismatch control
(fitting with RK4 too) makes the `5/8` bias silently disappear —
`best` flips to `4` and the fixture fails.

Each fixture file stays under the shared 1M work budget of
`emath test` (one exhaustive 17-candidate scan plus its data per
file; doubling scans in one file exhausts the budget, which is why
the planted check is its own fixture).

## Reachability fixtures (reference game-graph reproduction)

The `reachability_*` fixtures reproduce the reference backward
reachability results over `discrete.reachability` on a declared
12-state game graph (ids A..L = 0..11; controllers A, C, E, H, I, K;
adversaries B, D, F, J; goal G; dead end L):

- `reachability_witness_paths.emath` — every state except the dead
  end has SOME path to the goal; the forward depth-first check agrees
  from every start.
- `reachability_forced_attractor.emath` — least-fixed-point waves
  `{G}, {C,H}, {B,J}, {A,E}`; forced region exactly `{A,B,C,E,G,H,J}`;
  the outcome table labels D, F, I, K witness-only, L unreachable,
  H forced despite its self-loop.
- `reachability_forced_strategy.emath` — the rank-decreasing strategy
  edges `A->B, C->G, E->B, H->G`, each strictly decreasing rank.
- `reachability_enumeration_crosscheck.emath` — 16 controller
  policies x 16 adversary policies x 12 starts = 3,072 complete
  plays agree with the attractor region on every start.

Named mutation checks over these fixtures: (a) removing the
nonempty-successor (vacuity) check makes the dead end win vacuously —
the attractor fixture fails; (b) removing the rank-decreasing choice
lets the strategy loop inside the winning set (H picks itself) — the
strategy fixture fails; (c) reading adversary nodes existentially
wrongly forces D and F — the attractor fixture fails AND the
3,072-play enumeration cross-check disagrees.

## Integrator-contract fixtures (museum reproduction)

The `integrator_contract_*` fixtures reproduce the contract-gated
integrator-family search over `search.order_contract_demo` (the
proper-name dogfooding demo): the three-stage second-order family
`b = (alpha, 1 - 2*alpha, alpha)` on `y' = -y`, scored by certified
one-step error intervals against a Taylor enclosure of `exp(-h)`,
gated by the frozen order-3 contract.

- `integrator_contract_gate.emath` — the receipt: training winner
  `n = 16` (`alpha = 2/15`), contract winner `n = 20` (`alpha = 1/6`),
  `promoted = false`, and the winner's certified error more than
  100x below the `alpha = 0` baseline.
- `integrator_contract_unique.emath` — exhaustive 31-candidate scan:
  `alpha = 2/15` is the unique training-grid minimizer.
- `integrator_contract_heldout.emath` — held-out `h = 1/10`: the
  contract member's certified UPPER bound is below `1/100000` while
  the training winner's LOWER bound is above it — single-case tuning
  picked the wrong method contract.

Named mutation check over these fixtures: weakening the order-3 gate
to vacuously true (`order3_admissible` returns `true`) promotes the
training winner (`promoted = true`) and admits the whole grid — four
module tests and the gate fixture fail.

## Engine-seam regression provenance (multi-output tail call)

Authoring this demo surfaced and fixed an engine defect, locked by
`tests/fixtures/constructor/tail_call_multi_output.emath` + the
`multi_output_tail_call_packs_record` case in
`tests/emath-exec-ir/tests/constructor_carrier.rs`: a multi-output
function whose LAST definition is a bare named call was hijacked by
tail-call optimization — the frame jumped into the callee and
returned the callee's value raw, skipping the result-record packing,
so callers bound the last output's value and `enc.lo` style
projections faulted as `unbound`. Arithmetic, if-branch, and
closure-call last definitions were unaffected (which is why every
earlier module worked). The fix refuses the TCO path for multi-output
declarations; single-output tail recursion is untouched (the
`tail_call_reuses_frame` carrier case still passes). Failure-first
evidence: the minimal probe failed pre-fix and passes post-fix;
`order_contract_demo` went 10/15 to 15/15.

## Receipt vocabulary observed (recorded for a later decision)

What the module's output naturally distinguishes (recorded, not
frozen as public vocabulary): witness vs forced vs unreachable
outcome labels; wave structure as evidence (`attractor_waves` in
iteration order); per-state ranks; one rank-decreasing strategy edge
as a certificate; and "no witness in domain" (exhaustive over the
declared finite graph) as a mathematical answer, distinct from the
engine's `budget_exhausted` fault for a budget-limited miss.

## Failure-first provenance

The refusal fixtures fail-open against the pre-family
`numerics.euler` / `numerics.rk4` scalar wrappers: the old modules had
no refusal machinery at all (zero step returned y unchanged; no vector
state, no tableau admission, no order evidence, no budget). These
fixtures encode the family contracts; each named refusal is the
observable that the old modules could not produce.

## Budget interpretation

The refusal inventory lists "unbounded work (budget)". That budget is
enforced at two layers: the engine's work budget (`budget_exhausted`,
continuation-capable) and the authored step-budget contract here —
negative counts refuse `bad_step_count`, and exceeding `max_steps`
stops the rollout with an incomplete result whose `accepted_t` never
pretends to be the target. Later stages (adaptive acceptance) extend
the same result shape.

## Parity boundary

`evolution_parity_probe.emath` proves VM values (`emath run` prints
canonical `clip_t=3/4, clip_y=3/8, rk4_y=65/24`). Emitted-runnable
parity is deferred: `emath build` marks any closure/record/`use`
program not-runnable at HEAD (only flat scalar arithmetic lowers),
including the pre-existing numerics modules.

## Adaptive stepping fixtures (embedded pairs + controller)

The `adaptive_*` fixtures cover the first half: the Cash-Karp 4(5)
embedded pair over `analysis.evolution.one_step`, the componentwise
scaled error norm and halve/double controller in
`analysis.evolution.control`, and the adaptive rollout driver.

- `adaptive_refine_decay.emath` — refinement study on y' = -y: at BOTH
  declared tolerances (1/1024 and 1/1000000) the final value lies
  strictly inside the certified alternating-Taylor bracket
  (11/30, 53/144) of exp(-1); the fine run rejects before completing.
- `adaptive_refine_ballistic.emath` — refinement study on the 2D
  ballistic problem (x' = v, v' = -2): declared goal EXACT zero error
  at both tolerances (an order-5 method is exact on degree-2
  polynomial solutions), exercising the two-component scaled norm.
- `adaptive_cash_karp_equivalence.emath` — museum-contract equivalence:
  the four-rate packing (k1,k3,k4,k6) is the advance and satisfies
  order-5 evidence exactly (halving identity dev5(1) == 64*dev5(1/2)
  on y' = -y; the bush condition b.c^4 = 1/5 by hand: 3/920 + 3/110 +
  2401/14168); the five-rate set is genuinely order 4; both carry
  complete rooted-tree order-4 evidence. The two weight sets were
  initially swapped by folklore labels — the exact evidence caught it.
- `adaptive_budget_stops_incomplete.emath` — two budgeted attempts
  both reject: the run stops INCOMPLETE reporting the last accepted
  time (0) and the UNCHANGED initial state.

Named mutation checks: (a) committing the state on a REJECTED step
makes the budget-stop fixture fail (t == 0 and y == [1] hold only when
rejected steps never mutate the committed trajectory); (b) dropping
the attempt-budget check (never out of budget) makes the same fixture
fail (the run stops unbounded instead of incomplete).

BOUNDARY FOUND (recorded for the bit-budget lane): exact-rational
adaptive rollouts are practical only for rate laws linear in y. On
y' = y*y each RK stage squares the previous stage's denominator
exponents — ONE Cash-Karp step from y = 1 already produces ~330-bit
rationals, and digits grow doubly exponentially per step. The drafted
Riccati refinement study was replaced by the ballistic study for this
reason.

Second engine-seam fix of the session (same carrier file):
`stepped.next.y` — three-segment record paths — faulted as unbound
because the constructor Path arm only projected one field. The arm now
folds projection through every segment (two-segment behavior
unchanged); locked by the `nested_record_path_projects` carrier case.
Failure-first: the adaptive driver's `unbound stepped.next.y` was the
probe evidence.

## Dense output + event fixtures

The `events_*` fixtures cover the second half: the cubic Hermite
dense interpolant in `analysis.evolution.interpolation` and the event
machinery in `analysis.evolution.events` running over accepted
adaptive Cash-Karp segments.

- `events_terminal_exact.emath` — y' = -1 from y = 1, guard y - 1/2
  falling, terminal: the interpolant of a line is the line and 1/2 is
  dyadic, so the bracket returns t = 1/2 EXACTLY and the run stops
  there (outcome 3).
- `events_reset_continues.emath` — guard y - 3/4 rises at t = 3/4
  (dyadic, exact), the reset adds 1, the rollout continues and
  completes at t = 1 with y = 2 exactly.
- `events_initial_root_dedup.emath` — a root AT the initial time
  (theta = 0) fires exactly once; the same-instant refire on the next
  segment is suppressed.
- `events_chatter_budget.emath` — the reset moves the state back below
  the threshold every segment; the third firing exhausts the event
  budget and reports the explicit chatter outcome (2) at t = 9/4.
- `events_direction_filter.emath` — a falling crossing against a
  RISING event stays silent; the rollout completes untouched.
- `events_accepted_segments_only.emath` — y' = -y, tolerance 1/1000000:
  the h = 1 and h = 1/2 attempts REJECT before the first accepted
  segment; the falling guard y - 1/2 fires inside the third accepted
  segment with the event time certified inside (2/3, 7/10) —
  ln(2) = 0.6931 — and the run reports rejections >= 2.
- `events_refuse_empty_set.emath` — refuses `empty_event_set`.
- `events_refuse_bad_direction.emath` — refuses `bad_direction`.

Named mutation checks on `events.emath`: (a) removing direction
filtering (`crosses` accepts any sign-change window) is caught by the
direction-filter fixture plus two module tests (the wrong-sign
terminal fires and stops the rollout early); (b) evaluating guards on
ATTEMPTED segments (the full event handling moved ahead of the
rejection check) is caught by the accepted-segments-only fixture —
the guard fires inside the rejected h = 1 attempt at t ~ 0.57,
outside the certified window. Mutation (b) needed a HONEST mutant: a
first, timid version that pre-fired only non-terminal events (leaving
terminal events behind the acceptance check) SURVIVED the fixture —
the discriminating fixture's event is terminal, so the partial mutant
behaved identically to the original. The mutation must reproduce the
full defect, or the check proves nothing.

Engine seam of this half: closure-typed `Event` object fields
(`events[i].guard(...)` chained calls through indexed sequence
projection) parse and evaluate with no engine change — probed before
authoring (a `sequence(Rat -> Rat)` type does not parse; the
closure-typed field route does).

## Composition fixtures (splitting/reversible composition)

- `composition_reversal.emath` — exact reversal contracts and honest boundaries, all values hand-derived from the museum recipe: drift inversion `step(-1/2, step(1/2, (0,2))) == (0,2)`; harmonic Verlet reversal; museum-value equivalence ((7/8, -15/32), two steps (17/32, -105/128)); momentum involution (the museum's free-particle numbers + `R o step(h) o R == step(-h)` on the harmonic); energy NOT exact (Verlet delta pinned -15/2048); damped delta pinned -959/51200, strictly more dissipative.
- `composition_refuses.emath` — the four named refusal signatures (the refusal convention: each `failed <name> -- mathematical method refused: <code>` line IS the expected evidence): `non_separable_acceleration` (p-dependent acceleration caught by the two-momenta probe), `non_separable_carrier` (three-component state), `non_advancing_step` (h = 0), `negative_step_non_reversible` (backward damped step — the museum api.rs law).

## Structural spike fences (three-arm comparison)

- `structural_spike_fences.emath` — structural-spike fences, authored failure-first (first run: `E-USE-ADMISSION: unbound import 'search.structural_spike'`). `cubic_never_force_fit`: the out-of-grammar cubic target keeps positive training/holdout/rollout error in all three arms AND every arm's reported holdout equals the holdout-recomputed score of its selected candidate (a training-scored report is a fake win — catches the train-scored mutation). `equal_budgets_all_arms`: all nine arm runs carry cap 65, stay within it, and the scans spend exactly 65 (catches the extra-evaluations mutation).

## L1 closed-code fixtures (quote.evaluate executable)

`l1_closed_code.emath` (constructor fixtures) covers the L1
service: guarded `quote.evaluate` (closed code unchanged; open code
refuses `unbound_code` instead of leaking the ambient environment —
probes pinned the pre-L1 dynamic-scope leak at 21/81), `quote.identity`
(de Bruijn-normalized canonical identity; alpha-equivalent quotes
share it), and the `closed_code` query method (query inputs ARE the
explicit input environment; function-valued results consume the
ordered inputs). Carrier cases: stale_dependency (two-tree capture vs
evaluate), fuel (budgeted nested evaluation suspends
budget_exhausted), type_mismatch (receipt diagnostic verbatim).
Mutation checks: (a) stale-check drop CAUGHT (carrier stale case);
(c) unbound-guard drop CAUGHT (the named refusal signatures vanish); (b)
fuel-drop is vacuous by construction — L1 shares the engine work
budget (charge), so a fuel-drop mutant breaks the whole engine and is
caught by every case, not by an L1-specific seam. Binder domains are
TYPE expressions, never free names (a domain path like Rat initially
faulted as unbound — fixed in the walker). The carrier now runs its
cases on a dedicated 64 MiB stack thread: the 256-frame native
depth-fault margin with large per-frame values no longer fits the
2 MiB test-thread default on this tree (regression recorded for
follow-up; macOS-local exposure, rch/Linux 8 MiB marginal).

## Linear-systems fixtures (dense exact solves)

- `linear_systems.emath` — authored failure-first (first run:
  `E-USE-ADMISSION: unbound import 'algebra.linear.systems'`).
  Positives: triangular solves pinned ((2,1) lower, (5/2,1) upper),
  dense 2x2 = (4/5, 7/5) by hand, zero-pivot column swaps rows
  ([[0,1],[1,0]] x = (2,3) -> (3,2)), and the floor-zero policy
  disabled case verified by exact residual r0 = r1 = 0 (exact
  arithmetic needs no pivoting — the floor is caller policy).
  Refusal signatures: the singular [[1,2],[2,4]] system refuses
  `singular_matrix`; the 1/1000 pivot under a 1/100 floor refuses
  `ill_conditioned`. Mutation check: dropping the singular gate
  CAUGHT — the dead-column pivot index surfaces as a sequence index
  fault instead of the named refusal.

## Implicit/stiff fixtures (backward Euler + Newton + Picard control)

- `implicit_stiff.emath` — authored failure-first (first run:
  `E-USE-ADMISSION: unbound import 'analysis.equations.iteration'`).
  The reference stiff probes in exact rationals: the linear stiff
  case y' = -1000y, h = 1/10 is EXACT in one Newton update (root
  1/101, kind 0, residual 0 — also the value any emitted comparison
  must reproduce identically); the vector linear family gives
  (1/101, 10/11) exactly through the dense solve; the Picard control
  AMPLIFIES the error by exactly 100 per update (z1 = -99) — the
  control that rejects Picard for stiffness; the quadratic stiff
  case y' = -1000y^2 pins its first two Newton iterates (101/201,
  1060501/4100601, coprime by the Euclidean run) and its trip: the
  third iterate's denominator is exactly 886553220781401 (50 bits),
  so the authored bit budget 40 refuses `verification_budget_exhausted`
  at 3 updates, before constructing a fourth whose ratio intermediates
  would cross the engine's checked i128 boundary — the trace is the
  evidence, never a promoted unverified winner (the reference
  environment's 1,734/27,800-bit bignum figures are recorded as
  environment context, not engine claims). Receipt kinds are kept
  DISTINCT: exact (1/101) vs rational_approximation (17/12,
  residual 1/144) vs certified_enclosure ([1,2] IVT bracket).
  Refusal signatures: `verification_budget_exhausted`,
  `singular_jacobian` (x^2 + 1 from 0), `nonconvergence` (the Newton
  cycle x^3 - 2x + 2 from 0 alternating 0 <-> 1). Mutation checks:
  (a) bit-budget drop CAUGHT (the updates == 3 pin fails; honest
  note — the engine completed all 12 updates without faulting, so
  the authored budget is a semantic guard and the PIN is what
  discriminates, not an engine crash); (b) Picard substituted for
  Newton in the implicit step CAUGHT at both levels (module and
  fixture refuse `nonconvergence` where the exact receipt was
  pinned).

## Interval fixtures (certified enclosures)

- `intervals.emath` — authored failure-first (first run:
  `E-USE-ADMISSION` unbound import `analysis.intervals`). Positives:
  the endpoint algebra pinned exactly ([1/2,3/4]+[1/4,1] = [3/4,7/4];
  negation swaps and negates; -2 x [1/2,1] = [-2,-1]; the cross-sign
  product [-1,2]x[2,3] = [-3,6] via all four candidates; disjoint
  hull [0,3]); sqrt(2) one-shot bracket [1,2] and TWO bisections to
  [5/4,3/2] with 7/5 inside and 8/5 outside; the exact square 9/4
  pinches to the zero-width certificate [3/2,3/2]; the 2-norm of
  (3,4) certifies 5 exactly and (1,1) brackets sqrt(2) — one root,
  no width stacking; exp(-1) bracketed with placement cross-checked
  against the adaptive fixtures' certified bracket (11/30, 53/144)
  and the width pinned EXACTLY 1/41! against an independent
  fixture-local integer factorial (the module derives it by the
  term recurrence dividing by (j+1) stepwise — a cross-check, not a
  tautology). Refusal signatures: `h_out_of_range` (h = 0 and
  h = 5/4 — the alternating certificate needs 0 < h <= 1),
  `negative_radicand`, `refinement_budget_exhausted` (budget 4
  admits the first refinement at mid 3/2 and refuses the second at
  mid 5/4 BEFORE evaluating its square), `bad_bracket` (lo > hi).
  Mutation checks: (a) negative-radicand gate drop CAUGHT — the
  fixture case changes from the named refusal to a value mismatch,
  and the unguarded path returns the garbage receipt sqrt(-1) in
  [0,1] (the gate exists because the powers roots silently clamp
  negatives); (b) budget gate neutralized CAUGHT at the fixture
  level (the named code vanishes and the unbudgeted run completes
  all 8 refinements — honest note: the engine finished without
  faulting, the budget is a semantic work contract and the PIN
  discriminates; the module's own tests use generous budgets and
  stay green, so this gate's coverage lives here); (c) bisection
  side-flip CAUGHT at both levels (the module pin [5/4,3/2] and the
  fixture pin both fail — the mutant keeps the wrong half).

## Reference-reproduction fixtures (research-loop calculations)

These fixtures re-execute the research-loop reference calculations as
ordinary authored `.emath` programs (no module imports — plain
language over the scalar ABI), pinning the reference values exactly.
Budget lanes: the four heavy files run under `emath test ...
--work 20000000`; everything else fits the shared 1M default.
Fifty-one authored tests across eleven files.

- `replay_countermodel.emath` (3/3) — the training/future split
  countermodel: both worlds replay identically on training
  observations and diverge exactly on the future observation.
- `batch_order_determinism.emath` (7/7) — all 120 orderings of a
  5-batch workload fold to the same winner under first-wins
  strict-improvement selection (60/60 split check, zero deviants),
  reconciled against an independent selection-sort enumeration.
- `explicit_state_continuation.emath` (3/3, budget lane) — the
  1024-batch/32-epoch/8-candidates-per-batch workload as an explicit
  `emath object` state record: field-wise state equality at five
  resume cuts with reversed completion order, final counters
  8191/0/8192/8192, archive tail `[8184..8191]` pinned. At the
  default 1M budget the file refuses `budget_exhausted` — the
  boundary is the pin, same convention as `work_budget_row`.
- `cache_budget_verdict.emath` (3/3) — cold cache refuses under
  ACTUAL-cost charging; warm zero-work returns; the verdict pins the
  charging identity.
- `rk_family_flat_objective.emath` (5/5) — five a-values of the
  three-stage second-order family: all five order conditions exact
  AND all five nonlinear errors byte-exact against the reference.
- `enclosure_bisect.emath` (bisection arm, 4/4, budget lane) —
  512 cases (p = 1..64, q = 1..8): 512/0/9511/28533
  cases/refusals/steps/units, max_bits within the 25-bit grid bound,
  sqrt(2) at [92681/65536, 46341/32768] in 16 steps / 48 units.
- `enclosure_newton.emath` (exact-Newton arm, 7/7, budget lane) —
  sqrt(2) Newton bracket [816/577, 577/408] in 3 steps / 12 units,
  width 1/235416; perfect squares admit at entry. HONEST BOUNDARY:
  the reference accepted endpoints to 128 bits by CONSTRUCTING them
  under bignum arithmetic and refusing after the fact (56/512
  refused, max attempted endpoint 208 bits); the engine's checked
  machine arithmetic cannot construct those endpoints at all, so the
  native arm refuses BY PREDICTION at a 2^57 endpoint-part guard —
  the native success/refusal split is STRICTER than the reference
  456/56 and is reported, deliberately not pinned (only the
  split-consistency pin `successes + refusals == 512` and
  `all_valid`). The named refusal is pinned twice: as data
  (`refused == 1` on a = 64) and as the named refusal signature
  `representation_budget` through `constructor_refuse`.
- `enclosure_dyadic_newton.emath` (rounded arm, 5/5, budget lane)
  — the constructive repair: exact Newton upper iterate rounded
  UPWARD onto the 2^20 dyadic grid (ceil/floor binary searches), the
  lower bound reconstructed from the ROUNDED upper endpoint. 512/0/
  2306/18448 exactly (entry-guarded accounting: each performed
  rounding counts one step — a rounding-first loop undercounts by
  one step per case, 1802/14416 native, and the a = 1 diagonal cases
  must count zero), max_bits 23 within the grid bound, sqrt(2) at
  [1482907/1048576, 1482913/1048576] width 3/524288.
- `curriculum_ablation.emath` (6/6) — ordered curriculum
  135/1495 vs adaptive 64/6848/480 with per-target pins and both
  bounds checks.
- `float_alias_recovery.emath` (4/4) — the Float64 2^53 + 1
  aliasing countermodel: Float64 residuals alias at 0.0 while exact
  Int/Rat residuals stay 1, and the exact path recovers the wrong
  first-winner.
- `reciprocal_domain_audit.emath` (4/4) — the reciprocal domain
  audit positives; the three undefined cases are RUN-LANE probes
  (`emath run ... --set lane=0 --set x=-1/1 --set n=2` etc.): the
  fault line is the observable (`error: division_by_zero: exact
  division by zero`), the test lane catches faults without printing
  codes.

Authored-against-the-engine findings (all fixed in the fixtures, no
engine change needed): `obs` and `cases` are reserved words
(`E-SYN-110` at the field declarations) — renamed `evals` /
`total_cases`; a first `floor_search` recurred forever at `hi = lo+1`
(`mid = half_down(lo + hi)` equals `lo`, so the predicate-true branch
re-enters the identical range — found by reading the suspension
checkpoint's frame stack, 10.5 KB of `floor_search` frames at
`steps=0`); the dyadic loop was initially rounding-first and
undercounted steps by one per case (native 1802/14416 vs reference
2306/18448 — the difference is exactly 504 = 512 minus the eight
a = 1 diagonal cases, one final counted rounding each).

Mutation check: flipping the dyadic upper rounding direction
(ceil -> floor) is CAUGHT — the bracket pin, the step/unit totals,
and `every_enclosure_valid` all fail (a downward-rounded upper
endpoint drops below sqrt(a)); reverted, all five pass.

## Research-loop fixtures (S8 target-agnostic loop modules)

These fixtures drive the real `search.research` loop modules through
the ordinary module test lane (module imports, one fixture per
acceptance control plus the engine-generality and replay
disciplines). Nine authored tests across six files; the targets
file takes ~25 s — the reference VM deep-clones captured closure
environments per application, so the two targets are two functions
(one shared body would nest the fit closure tree inside every reach
closure clone: minutes, not seconds).

- `research_loop_valley.emath` (2/2) — the E5a archive-valley
  control: incumbent-only mode plateaus at score 4 then exhausts the
  domain; archive mode retains the worse candidates as stepping
  stones, crosses 000 -> 001 -> 011 -> 111, and promotes exactly the
  baseline plus the winner (score 6).
- `research_loop_false_memory.emath` (1/1) — the E5b control: the
  domain-audit curriculum grows the frozen case set {1,2} ->
  {0,1,2}; freshness re-scores the whole archive, the unconditional
  x/x -> 1 rule is quarantined at case 0 (promotion withdrawn, the
  counterexample recorded), the guarded rule survives and is
  promoted; the batch that grew the set reports running, never
  goal_attained.
- `research_loop_forged.emath` (1/1) — the E5c control: a forged
  record claiming score 0 is re-verified to the exact 2 and never
  promoted (9 units charged); the honest key 1 wins; the
  reduced-coverage key 3 is rejected by the hard gate.
- `research_loop_aliasing.emath` (1/1) — the X8 alias pair through
  the loop: 2^53 and 2^53 + 1 both probe to residual 0.0 on the
  rounded views, both are retained, exact verification decides, and
  the tie loser stays retained at exact residual 1.
- `research_loop_targets.emath` (2/2) — engine generality: the SAME
  imported `research_batch` walks the Stage 2 fitting target
  (0 -> 1/4 -> 1/2 by strict improvement, exact SSE zero,
  parent-linked archive chain) and the Stage 3 reachability
  counterexample target (state 0 is witness but not forced;
  witness_set [0,1,2] vs attractor [2]).
- `research_loop_replay.emath` (2/2) — the X1/X3 disciplines: a
  budget halt mid-batch commits its partial-but-valid records
  (verdict 4); resuming from the halted state reaches the same goal
  and winner; the same checkpoint continued twice is identical, and
  the straight run equals the checkpointed run.

Authored-against-the-engine findings (both fixed at root cause): a
single body holding both targets' definitions exploded in cost
(closure capture deep-cloning compounds per definition — split into
two functions); the verdict chain originally checked goal_attained
before the case-set advance, letting a batch that grew the audit
declare goal on stale pre-audit scores — the false-memory fixture
caught it and `set_changed` now precedes `goal_met` in
`research_batch`.

Mutation checks (each applied to the loop or the fixture target,
confirmed to kill the named test, then reverted — all six pass
green after the sweep): pruning the probe-tier tie loser in
`probe_scan` (a temporary tier-0 tie gate on the append) kills the
aliasing test — the 2^53 tie loser is never retained at exact
residual 1; making `source_ords` incumbent-only in both modes kills
the valley archive arm (score 4, domain exhausted, one promotion —
the incumbent-only arm still passes, which is exactly the
discrimination the fixture claims); removing the x != 0 guard from
the fixture's guarded rule kills the false-memory test (both rules
quarantine at case 0, the guarded survivor pin dies); trusting the
stored score in `refresh_scan` (`v = r.score`, units still charged)
kills the forged test — the forged record wins at its claimed 0.
