# ELP; emath Language Proposals

Every capability change to the emath language lands through an ELP. The
pipeline makes the finish-line gates of
[`language/reference/language-acceptance.md`](../language/reference/language-acceptance.md)
(chapter 16) part of the proposal itself, so an accepted syntax change
arrives already satisfying its slice of the twelve gates; never as a
silent drift between grammar, reference, examples, and tests.

## Lifecycle

1. **Draft**; write `elps/ELP-NNNN-<slug>.md` from the template
   (`ELP-TEMPLATE.md`); the next free serial is one past the highest
   `ELP-` prefix on `main`.
2. **Gate**; `scripts/check_elp.py` validates the document shape; the
   G4 audit battery (`scripts/g4_ambiguity.py`, `scripts/g4_confusable.py`,
   `scripts/g4_precedence.py`) runs against the proposed grammar delta;
   the four-artifact rule (`scripts/check_four_artifact.py`) is enforced
   by CI on the commit.
3. **Adopt**; accepted proposals land either as stable surface
   (append-only, meaning-preserving) or behind the experimental lane
   (below). The commit closes the ELP and updates
   `language/` per RULE 0.3 of AGENTS.md.
4. **Retire**; experimental syntax that survives two release cycles
   without promotion is retired (the feature and its gate are removed).

## ELP document

One file per proposal: `elps/ELP-NNNN-<slug>.md` with seven mandatory
sections (validated by `scripts/check_elp.py`):

1. **Motivation and coverage claim**; what math/science becomes
   expressible; which capability-matrix cells move.
2. **Grammar delta**; unified diff against `language/grammar/*.ebnf`;
   new tokens carry their Unicode confusable class.
3. **Lowering and world interactions**; typed-IR node it lowers to,
   which `WorldIr` component it introduces, if any.
4. **Meaning-preservation analysis**; proof sketch that no previously
   valid program reparses (append-only argument), or the edition-gating
   declaration when the change is not append-only.
5. **Migration**; `none needed (pure addition)` or the migrate rule set
   with golden tests.
6. **Four-artifact plan**; the exact files touching reference, EBNF,
   examples/fixtures, and tests.
7. **Refusals**; new typed diagnostics with stable codes and negative
   controls.

## The G4 audit ("one glyph, many meanings")

Four mechanical sub-tests, run by the scripts under `scripts/` (each is
proved by a negative-control lane in `scripts/validate.sh`):

| Sub-test | Script | Fatal on |
| --- | --- | --- |
| Ambiguity scan | `g4_ambiguity.py` | duplicate production definitions and overlapping first-sets (an alternative that can start the same way as a sibling), including under a unified diff |
| Confusable scan | `g4_confusable.py` | a new glyph whose NFC/confusable fold collides with an existing grammar glyph |
| Precedence surprise | `g4_precedence.py` | operators without an explicit `notation_decl` precedence; regenerates the pinned boundary corpus |
| Hidden interpretation | `g4_hidden_interpretation.py` | a glyph gaining a second role (production); an unregistered "many meanings" form; or a registered glyph's role set drifting from its registry entry. Every multi-role glyph must declare how its meaning is pinned: `parser-context` (operator table / lexical position), `worlds-machinery` (routing through a notation pack, declared world, or portfolio artifact), or `typed-refusal` (a documented E-code) |

The ambiguity, confusable, and hidden-interpretation scans are first-pass
mechanical detectors of the C2–C15 class (redefinitions, lookalike
glyphs, precedence surprises, silent meaning picks); they are not a
proof of unambiguity; the boundary corpus plus human review of the
meaning-preservation section covers the residual.

## Four-artifact rule (CI)

A commit touching `language/grammar/*.ebnf` that does not also touch at
least one reference chapter, one example or fixture, and one test fails
the gate (`scripts/check_four_artifact.py`). A reference chapter change
with no grammar delta and no example/test companions is accepted only
with the `Reference-Only: true` trailer in the commit message.

## Experimental lane

Experimental syntax ships behind the `experimental-syntax` capability:

```
@capabilities(experimental-syntax)
@experimental
emath function Foo:
    ...
```

- The capability is declared on any item in a source file and applies
  file-scope (`admit_capability_gates` in `crates/emath-sema`).
- `@experimental` without the declared capability is refused with typed
  code `E-PKG-064`, never silently admitted.
- Unknown attributes (`E-SYN-118`), unknown capability keys (`E-PKG-065`),
  and malformed attribute arguments (`E-SYN-117`) are typed refusals.

**Implemented today vs deferred design elements** (kept honest per
RULE 0.3: what computes, what is refused, what is still design):

| Element | Status |
| --- | --- |
| Capability declaration + file-scope gate | Implemented (`@capabilities(experimental-syntax)`, `admit_capability_gates`) |
| Typed refusals for the capability matrix | Implemented (E-SYN-117/118, E-PKG-064/065) |
| Nightly-vs-stable channel enforcement | Deferred; lands with the version-stack deck (`emath-r3-version-stack-9z1a`) |
| `edition: experimental` provenance marking on artifacts | Deferred; lands with the version-stack deck |
| SG-15-style structural quarantine of experimental artifacts | Not implemented; today the mechanism is the capability gate plus quarantine-by-review |
| Two-release-cycle retirement without promotion | Procedure stated in Lifecycle step 4; not yet mechanically enforced (no clock/tracking) |

The reference vocabulary for the deferred rows is normative in
`language/reference/declarations-sections-and-attributes.md`
("design vocabulary (not yet admitted)"); nothing in the deferred rows
compiles silently today.

## Codes

| Code | Meaning | Emitter |
| --- | --- | --- |
| `E-SYN-117` | attribute argument outside the identifier/string/list subset | `emath-syntax` parser, `emath-sema` recognition |
| `E-SYN-118` | unknown item attribute | `emath-sema` recognition |
| `E-PKG-064` | `@experimental` without the `experimental-syntax` capability | `emath-sema` recognition |
| `E-PKG-065` | unknown capability key in `@capabilities` | `emath-sema` recognition |

## Scripts

- `scripts/check_elp.py`; document-shape gate (seven sections, serial,
  slug, four-artifact commitments).
- `scripts/g4_ambiguity.py`; grammar/delta ambiguity scan.
- `scripts/g4_confusable.py`; glyph confusability scan (NFC + fold).
- `scripts/g4_hidden_interpretation.py`; multi-role glyph registry scan
  (the "one glyph, many meanings" gate, pinning each shared glyph's
  interpretation policy).
- `scripts/g4_precedence.py`; precedence boundary corpus generator.
- `scripts/check_four_artifact.py`; commit-range four-artifact gate.
- `scripts/validate.sh`; `elp`, `g4-*`, and `four-artifact` lanes with
  negative controls.
