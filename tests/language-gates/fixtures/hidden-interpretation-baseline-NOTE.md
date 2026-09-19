# Hidden-interpretation baseline NOTE (2026-09-19)

## What happened

Two changes since the 2026-09-10 registry required a full regen:

1. The shared EBNF parser was fixed to skip separator commas (a bare
   `,` is the sequence operator of the dialect, never a glyph
   terminal). The old `,` entry carried ~120 roles, most of them EBNF
   metasyntax positions rather than language surface; it is now the
   honest 23 productions that actually contain a quoted `","` glyph.
2. The expression surface grew the four binder families
   (`function_abs` / `recur_expr` / `quote_expr` / `callable_binder`)
   and the open-range list tail `[a, ..b]`, changing the multi-role
   surface: six glyphs drifted in role set and two became multi-role
   for the first time.

## Classification (51 multi-role glyphs, all `parser-context`)

No glyph required `worlds-machinery` or `typed-refusal`; no real
same-context ambiguity was found. Every multi-role glyph is
disambiguated by production position or by the introducer that opens
its context. New entries and role changes, by mechanism:

- `..` — list open-range tail vs range operator; after `[expr,` the
  next token decides (the sequence-cons arm), and ranges bind
  postfix after a full expression.
- `function` — declaration introducer (`emath function F`) vs the
  binder expression keyword (`function n in T: body`); declaration
  vs expression position, contextual-identifier lookahead
  (identifier + `in`).
- `in` — gained the four binder-family domain roles; the enclosing
  rule fixes the meaning as before.
- `if` — lost the old binder-guard role; guards live in the
  comprehension, list-comprehension, and for positions.
- `,` `:` `(` `)` — role-set refresh from the honest comma model and
  the binder forms (`:` gained the binder domain role; `(` gained
  quote's parenthesized form).
- `fn` `match` — notes updated to the honest parse status:
  expression-position `fn` refuses E-SYN-110 and statement-leading
  `match` refuses E-SYN-101; both EBNF spellings are contract-only
  today (recorded in the ambiguity NOTE as well).

Registry notes carry per-glyph disambiguation stories and current
line citations; entries with empty notes are pure position
delimiters whose meaning is fixed by the production that opens them.

## Verification

- `python3 scripts/hidden_interpretation_scan.py --baseline
  tests/language-gates/fixtures/hidden-interpretation-baseline.json
  language/grammar/surface.ebnf language/grammar/genesis.ebnf` →
  `0 errors, 51 registered, 0 stale`.
- Grammar battery controls: the new-role delta is refused, the
  registered-glyph drift delta is refused, the single-role glyph
  addition is admitted (see validate.sh).
