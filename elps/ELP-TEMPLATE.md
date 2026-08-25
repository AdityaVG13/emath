# ELP-NNNN-<slug>

> Serial: 4 digits (`0001`, ...), one past the highest on `main`.
> Slug: kebab-case, matches the filename.
> Status: Draft — fill every section; `scripts/check_elp.py` refuses
> placeholders (`TODO`, `TBD`, `...` alone in a section).

## 1. Motivation and coverage claim

What math or science becomes expressible; which capability-matrix cells
move. Name the `language/CAPABILITY.md` rows this ELP changes.

## 2. Grammar delta

Unified diff against `language/grammar/surface.ebnf` (and
`genesis.ebnf` when the genesis surface is affected). Every new token or
literal lists its Unicode confusable class (NFC form + fold, per
`scripts/g4_confusable.py`).

## 3. Lowering and world interactions

The typed-IR node the new surface lowers to (or the refusal path), and
the `WorldIr` component it introduces, if any. If none is needed, say so
explicitly.

## 4. Meaning-preservation analysis

Proof sketch that no previously valid program reparses differently
(append-only argument: the delta adds productions without touching
existing ones), or the edition-gating declaration when the change is
not append-only.

## 5. Migration

`none needed (pure addition)` — or the migrate rule set with golden
tests.

## 6. Four-artifact plan

| Artifact | Files |
| --- | --- |
| Reference chapter | `language/reference/<chapter>.md` |
| Grammar | `language/grammar/surface.ebnf` |
| Examples / fixtures | `language/examples/...` (+ index row) |
| Tests | `tests/...` |

## 7. Refusals

New typed diagnostics with stable codes (registered in
`implementation/ERROR_CODES.md`) and their negative controls.

## Acceptance checklist (chapter-16 gates)

Map the proposal to the gates it touches (1 grammar/parser determinism,
2 formatter round-trip, 3 import/name resolution, 4 custom-kind
boundedness, 6 conflict-naming diagnostics, 7 definitions vs
goals/plans, 9 identity mutation matrix, 10 migration goldens,
11 no undocumented parser exceptions, 12 producer/consumer/negative/
artifact). Record G4 battery results and the E1 replay / E2 coverage-ledger
checks when applicable.
