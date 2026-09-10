# g4-hidden-interpretation baseline NOTE (2026-09-10)

Pass 3 of the running-the-gauntlet-on-your-rust-port battery (greenfield mode).

## What happened

The grammar delta since the previous baseline (reaction networks, braket
notation, nabla packs, matrices/tables/graphs/sets, match expressions)
changed the multi-role surface: 11 glyphs became multi-role with no registry
entry and 15 registered glyphs drifted in role set. The baseline was
regenerated from the shipped grammar with `--emit-baseline` and each glyph's
`resolved_by` was classified honestly.

## Classification summary (49 multi-role glyphs, all `parser-context`)

No glyph required `worlds-machinery` or `typed-refusal`. No real
same-context ambiguity was found: every multi-role glyph is
disambiguated by production position or by the introducer that opens
its context. New entries, by mechanism:

- `0`, `1` — qubit carrier label vs numeric digit; lexer emits a digit,
  parser slot decides (`surface.ebnf:202`).
- `;` — matrix row separator, graph edge-list separator, type tuple
  separator in distinct productions (`surface.ebnf:167,239`).
- `match` — match expression vs match statement; expression vs statement
  position (`surface.ebnf:224`).
- `true`, `false` — boolean literal vs match-arm pattern shorthand;
  arm head vs expression position (`surface.ebnf:225,293`).
- `use` — import item vs notation pack mount with fixed path literals
  (`surface.ebnf:181,196`).
- `{`, `}` — delimiters for graph / hole / match / record /
  set-comprehension / set / use-tree; each opened only after its
  introducer keyword (`surface.ebnf:213,224,239,250`).
- `|` — ket bar, bra bar, sandwich bar, table cell delimiter, type union;
  position-disambiguated inside braket/table/type productions
  (`surface.ebnf:173,198-201`).
- `⟩` — closer for ket / bra-form / sandwich, opened by `⟨` or the `|`
  bar (`surface.ebnf:198-201`).

Drifted glyphs (`(`, `)`, `+`, `,`, `-`, `->`, `.`, `:`, `=`, `=>`, `[`,
`]`, `emath`, `if`, `in`) kept their existing `resolved_by` and notes; only
their role lists were refreshed.

## Verification

- `python3 scripts/g4_hidden_interpretation.py --baseline
  tests/language-gates/fixtures/g4-hidden-interpretation-baseline.json
  language/grammar/surface.ebnf language/grammar/genesis.ebnf` →
  `171 glyphs; 49 multi-role; 0 errors, 49 registered, 0 stale`.
- `./scripts/validate.sh` → all lanes green, exit 0.
