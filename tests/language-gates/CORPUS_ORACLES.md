# Corpus Oracles; `language/examples` acceptance map (emath-ail.1)

Every README example row, the acceptance command, the pinned oracle, and the
current verdict. Pinned in code by
`tests/emath-syntax/tests/official_examples_corpus.rs` (exit-code oracles) and
`scripts/validate.sh` phase `corpus-oracles` (pinned numeric final rows +
exact refusal codes).

Oracle capture: repository-local binary (`cargo run -q -p emath-cli --`,
target/debug/emath; the global `emath` is NOT on PATH), 2026-08-30.

## Admits check

All 25 corpus files under `language/examples/*/*.emath` admit `emath check`
with no `E-*` diagnostics (re-scanned 2026-08-30 with the local binary).
`intro/scratch.emath` additionally carries the `N-HOLE-001` open-hole note
(the hole is the example).

## Refuse-pinned examples

None currently: the corpus refresh (2026-08-30) removed the pinned-refusal
fixtures (`intro/v9_06_2rdq_17.emath`, `intro/sets-records.emath`'s
`E-TYPE-113` pin; sets-records now admits check with the `{}` surface
executing; its ambiguity row is covered by
`tests/invalid/r3_sets_tub8_ambiguous.emath` E-SYN-154). Any new intentional
refusal example re-enters through a `-> E-XXX-NNN` header, which the gate
honors.

## Runs rows (pinned Ok oracles)

| Example | Command + pinned args | Oracle |
|---|---|---|
| physics/newton-second.emath | `emath run <f>` | exit 0; generated crate test 1 passed |
| intro/hello-square.emath | `emath run <f>` | exit 0 |
| algebra/symbolic-cas.emath | `emath run <f>` | exit 0; artifact `fnv1a64:4c2dd710423ddec4` |
| numerical/explicit-mass-spring.emath | `emath simulate <f> --set m=1 --set k=1 --set c=0 --set s=[1,0] --dt 0.01 --t1 3.141592653589793` | final row exactly `t=3.141592653589793 s=[-0.9999999999978184, -0.0000000002616344400786959]` (x(π) = −1, cos) |
| numerical/heat-rod-sim.emath | `emath simulate <f> --set alpha=1.0 --set u=[1,0,0,0,0] --dt 0.01 --t1 1.0` | exit 0; verified 2026-08-30 final row `t=1.0 u=[0.5237781093210822, 0.3085124869062955, 0.12206440671862484, 0.035903549585294535, 0.009741447468702879]` |
| numerical/dae-rc-circuit.emath | `emath simulate <f> --model RCCircuit --set voltage=10 --set resistance=1 --set capacitance=1 --set threshold_voltage=5 --set current=0 --set charge=0 --method backward-euler --dt 0.1 --t1 1.0` | exit 0; `charge` 0→6.144567105792, `current` 10→3.855432894389 (BE, physics-exact `10(1−0.9^10)`) |
| numerical/solver-methods.emath | `emath simulate <f> --model StiffDecay --set y=1 --method backward-euler --dt 0.1 --t1 0.3` | exit 0; `y` 1→0.0046296296295476325 (= 1/6³, exact BE recursion) |
| numerical/solver-methods.emath | `emath simulate <f> --model HarmonicOscillator --set q=1 --set v=0 --method velocity-verlet --dt 0.01 --t1 6.283185307179586` | exit 0; `q=0.999999999544588 v=-0.00002620375716343333` at t=2π |
| science/observations.emath | `emath explain <f> --provenance` | exit 0; DAG edge `PkSingleDose.plasma_conc -> InstrumentRun(file=pk_run_041.csv, processing=LC-MS/MS, area ratio, sha256=e706a0172e0ef6f8b748eca6a55763eced959b6eb055b442b30d3c1e313acb2f)` |
| intro/scratch.emath | `emath run <f>` | **Refused E-GOAL-043**; open hole never claims a produced crate |

## Event locator oracle (declared events vs model variables)

| Command (dae-rc-circuit bindings as above) | Result |
|---|---|
| `--event current=5` | exit 0; crossing sample `t=0.7262836955484092 charge=5.000000000366084 current=5.000000000092301` |
| `--event ThresholdCrossed=5` | exit 1: `error: event state `ThresholdCrossed` is missing`; locator keys on model variables, NOT declared event names |

## Divergence ledger; README promises not yet true on HEAD

These rows pin CURRENT failure; a flip requires consciously re-pinning.

| Example | README claim | Current behavior | Blocker |
|---|---|---|---|
| intro/autodiff.emath | Forward-mode derivative runs | `emath run` exit 1 (re-verified 2026-08-30): generated `auto_diff_parabola` test fails | forward-mode dual-number backend defect |

Removed 2026-08-30 with the corpus re-org (no longer in the gate):
`intro/v9_06_2rdq_11.emath` (was: Mod17 law lowering gap) and the
heat-plate/heat-volume simulate rows (fixtures deleted; heat-rod remains the
pinned spatial row).

### Healed (history; fixtures since reorganized)

`numerical/heat-rod-sim.emath` etc. healed 2026-08-28 (`self_` / `der_u` /
Stencil3d fixes; mutation-evidence: reverting `SelfValue` → `Var("self")`
failed the gate). `dae-rc-circuit.emath` check record superseded by the
executed record in `numerical/dae-rc-circuit-check.md` (2026-08-30,
SilentBear + BronzeCoyote probes).

## Doc-hygiene notes (fold into parent emath-ail)

- `tests/invalid/` `# expect:` headers vs actual `check` codes diverge for
  ~12 strategy fixtures (parse-first `E-PKG-081` before the intended
  semantic code). Full header reconciliation belongs to the parent bead.
- `validate.sh` negative-controls lane pins (`function_type` → E-TYPE-110,
  `named_call_arg` → E-SYN-121, `missing_state_assignment` → E-CTOR-030,
  `recursive_kind` → E-KIND-100) need re-verification against a fresh binary
  once the emath-ir capability WIP (VioletGorge) lands.
