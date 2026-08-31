# Conformance discrepancy ledger (DISC register)

Seeded by `emath-conform-pin-register-1iip` (skill-loop audit
`testing-conformance-harnesses`, 2026-08-23). This ledger records every
intentional divergence between the normative language specification
(`language/reference/**`, `language/grammar/**`) and the shipped
implementation, so XFAIL wiring and later harness work
(`emath-conform-harness-thin-lfpg`) attach to stable ids instead of
prose.

Protocol:

- Every entry carries: Reference position vs our implementation, impact,
  resolution, affected tests, review date.
- A resolution is one of `ACCEPTED` (spec is rewritten or fenced),
  `WILL-FIX` (implementation must converge; keep the DISC open),
  `CLOSED` (premise stale or divergence resolved).
- A DISC id is never reused for a different divergence.

---

## DISC-001; Ch2 tabs: spec rejects tabs; lexer was said to skip them

- **Reference vs our impl:** `language/reference/lexical-layout-and-source.md`
  says "Tabs are rejected in canonical source." The audit claimed the
  lexer silently skipped tabs. Current binary probe (2026-08-29): a file
  indented with a tab is **not** silently accepted; the tab breaks
  indentation semantics and the parser refuses with `E-SYN-112`
  (expected an indented block).
- **Impact:** none; spec and implementation agree that a tab-indented
  source is refused rather than skipped.
- **Resolution:** ACCEPTED (premise stale). Spec wording stands: tabs are
  rejected via the layout grammar, not silently consumed.
- **Tests affected:** probe-only evidence (`emath check` on a
  tab-indented fixture); no dedicated repo fixture yet; the
  `assert_invalid` battery may add one.
- **Review date:** 2026-08-29.

## DISC-002; Ch2 NFC: spec normalization vs refuse-not-normalize

- **Reference vs our impl:** Ch2 says identifiers "are normalized to NFC
  for identity." The implementation never re-normalizes: a combining
  mark refuses at the lexer (`E-SYN-115`, probe 2026-08-29: "source must
  be NFC"), non-ASCII identifiers warn (`E-SYN-114`), and a lookalike
  fold collision refuses (`E-NAME-024`). Ch2's enforcement ladder and
  Ch12 negative-space rule 5 already document refuse-not-normalize.
- **Impact:** low; identity is NFC-equivalent for all admitted sources,
  and the non-admitting path is typed, so identity claims stay sound.
- **Resolution:** ACCEPTED. The refusal ladder **is** the normalization
  policy; ch2 wording stands.
- **Tests affected:** `tests/invalid/combining_mark.emath`
  (`validate.sh` negative control, `E-SYN-115`),
  `tests/invalid/confusable_decl.emath` (`E-NAME-024`).
- **Review date:** 2026-08-29.

## DISC-003; Ch11 length-framed/canonical binary: spec overclaim claim

- **Reference vs our impl:** The audit claimed Ch11 promised
  length-framed canonical binary without an implementation. The
  implementation is real: `emath.meaning.canonical.v1` length-framed
  canonical bytes live in `crates/emath-ir/src/meaning.rs` (with
  `push_framed` framing in `crates/emath-ir/src/canonical.rs`), and
  Ch11's "Implemented today" paragraph already fences exactly that.
- **Impact:** none.
- **Resolution:** CLOSED (premise stale; implemented).
- **Tests affected:** identity round-trip tests under
  `crates/emath-ir` (package identity lanes).
- **Review date:** 2026-08-29.

## DISC-004; Rumoca Phase-1 native stand-in (no upstream engine)

- **Reference vs our impl:** `crates/emath-adapter-rumoca` consumes no
  upstream Rumoca engine; the Modelica subset scanner, causalizer, and
  MSL ladder helpers are native stand-ins, documented as such in
  `census.rs` ("no upstream parser", "no upstream engine") and
  `crates/emath-adapter-rumoca/CONTRACT.md`.
- **Impact:** honesty surface only; the adapter must never claim
  upstream conformance while the stand-in is in place.
- **Resolution:** ACCEPTED. The locked intended upstream commit
  (`5bafcd90f3410654f258fded7783ca493c3f4a77`,
  `forks/UPSTREAM_LOCK.json`) is now recorded in
  `crates/emath-adapter-rumoca/CONTRACT.md`, and
  `scripts/check_upstream_lock.py` enforces that binding textually.
- **Tests affected:** `crates/emath-adapter-rumoca` census/conformance
  unit tests; `scripts/check_upstream_lock.py` seam-binding lane.
- **Review date:** 2026-08-29.

## DISC-005; MSL ladder role: no MSL corpus CI

- **Reference vs our impl:** The lock previously labeled the
  Modelica Standard Library row `required: "conformance"` while nothing
  consumes an MSL corpus in any gate; an implied conformance claim
  with no executable behind it.
- **Impact:** misreadable as language conformance to MSL.
- **Resolution:** ACCEPTED fence. The lock row is relabeled
  `required: "future"` with an explicit no-MSL-CI note (respecting the
  closed `emath-gauntlet-04-q22w` guidance: do not invent Rumoca MSL
  CI). `evaluate_msl` remains an adapter capability, not a gate.
- **Tests affected:** `implementation/schemas/upstream-lock.schema.json`
  enum (`core|optional|future`; `conformance` no longer valid);
  `scripts/check_upstream_lock.py`.
- **Review date:** 2026-08-29.
