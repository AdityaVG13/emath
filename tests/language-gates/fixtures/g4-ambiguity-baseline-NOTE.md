# g4-ambiguity baseline classification note

Regenerated 2026-09-10 via `python3 scripts/g4_ambiguity.py --emit-baseline
tests/language-gates/fixtures/g4-ambiguity-baseline.json language/grammar/surface.ebnf`
after the surface.ebnf changes that added the 16 signatures
`primary_expr::{1:7,1:8,3:11,5:6,7:8}` and `literal::{1:6,1:10,2:6,2:10,3:6,3:10,4:6,4:10,5:6,5:10,6:10}`.

Classification: all 16 are parser-resolved, no genuine two-parse sentence.

- **Literal digit overlaps** (`literal::*`): the lexer is a single
  tokenizer pass (`crates/emath-syntax/src/lexer/engine.rs`) that emits
  disjoint token kinds per number form — `Int` vs `Float` (incl. the
  `i` complex suffix and `±` measurement fold) vs
  `FloatUncertainty` (CODATA parenthetical, one token). The parser folds
  rational (`Int` + `SlashSlash`) and quantity (`Int`/`Float` + path) in
  `crates/emath-syntax/src/parser/expr/literals.rs` (`parse_primary_literal`).
  EBNF first-set overlap across `integer|decimal|float_literal|…` is a
  documentation artifact, not a parse decision.
- **`primary_expr::1:7` / `1:8` / `5:6` / `3:11` / `7:8`**:
  `parse_primary` (`crates/emath-syntax/src/parser/expr/primary.rs`)
  dispatches on one disjoint token kind per alternative; inside the
  shared-openers, LL(2) lookahead decides — tuple vs paren by `Comma`
  after the first element (`literals.rs`, `LParen` arm), set literal vs
  comprehension by `Comma`/`RBrace`/`Colon`/membership re-read after the
  first element (`primary.rs`, `LBrace` arm; bare-record ambiguity
  refuses E-SYN-154, records require the `Path : {` prefix).

No grammar or parser change accompanied this baseline refresh.
