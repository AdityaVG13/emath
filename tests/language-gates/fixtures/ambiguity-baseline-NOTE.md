# Ambiguity baseline classification note

Regenerated 2026-09-19 via `python3 scripts/ambiguity_scan.py
--emit-baseline tests/language-gates/fixtures/ambiguity-baseline.json
language/grammar/surface.ebnf language/grammar/genesis.ebnf` (52
signatures). Two things changed since the 2026-09-10 baseline: the
grammar surface and the scanner itself.

## Scanner fixes (why every signature was re-derived)

The shared EBNF parser mis-modeled four dialect rules, and every
first-set overlap containing a phantom terminal had to be re-derived:

- A bare `,` is the sequence separator between terms, never a
  terminal (real comma glyphs are quoted `","`). The scanner used to
  admit separator commas as literal terms, so any production with a
  nullable prefix acquired a phantom `,` first-set terminal.
- A bare `|` inside `(...)`, `[...]`, `{...}` is the choice operator;
  a group's first set is the union over its branches. A linear scan
  used to truncate at the first branch (which is why old overlaps
  showed `unit` without `dimension`).
- A first-position recursion back-edge contributes no terminals and
  is not nullable. The old recursion-approximates-as-nullable rule
  let the scan fall through the back-edge into the rest of the
  production graph.
- An alternative is nullable iff EVERY term in it is nullable. The
  old any-semantics declared `identifier { "::" identifier }`
  nullable because it contains a repeat, letting first sets fall
  through the mandatory leading identifier to phantom terminals (a
  record's `:`, an assignment's `.`/`[`, a quantity's space).

## Grammar drift since 2026-09-10

The expression surface grew the four binder families
(`function_abs` / `recur_expr` / `quote_expr` / `callable_binder`,
surface.ebnf:257-265) inside `primary_expr`, and `list_expr` gained
the open-range cons tail `[a, ..b]` (surface.ebnf:163).

## Classification: all 52 are parser-resolved, no two-parse sentence

- **Literal digit overlaps (`literal::*`, `complex_literal::1:2`)**:
  the lexer is a single tokenizer pass
  (`crates/emath-syntax/src/lexer/engine.rs`) that emits disjoint
  token kinds per number form — `Int` vs `Float` (the `i` imaginary
  suffix consumed into the number token, measurement parentheticals
  as one token); the parser folds rational and quantity forms
  afterwards (`crates/emath-syntax/src/parser/expr/literals.rs`).
  EBNF first-set overlap across the literal spellings is a
  documentation artifact, not a parse decision.
- **Literal/expression overlaps on expression openers** (`fn`, `if`,
  `unit`, `dimension`, `(`, `[`, `{`, `match`, `true`, `false`,
  `function`, `quote`, `recur`...): these all flow from ONE spelling —
  the complex sum form `expression ("+" | "-") (integer | decimal)
  "i"` (surface.ebnf:322) — which documents `a + 2i` with a leading
  expression, making EBNF first sets mutually reachable
  (literal → complex_literal → expression → primary_expr → literal).
  The parser never re-enters expression for the sum form's left
  side: the lexer folds `2i` into one number token and `a + 2i`
  parses as binary(path, +, complex-literal).
- **`primary_expr::*` shared openers**: `parse_primary`
  (`crates/emath-syntax/src/parser/expr/primary.rs`) dispatches on
  one token kind per alternative; inside shared openers, lookahead
  decides — tuple vs paren by the comma after the first element; set
  literal vs comprehension by comma/`}`/`:`/`in` after the first
  element; bare `{name: value}` refuses E-SYN-154 (records require
  the `Path : {` prefix); the binder forms are contextual
  identifiers that activate only before identifier + `in`, so
  `Point:{...}` (record) vs `Point x in ...` (callable binder) is a
  two-token decision; ranges bind postfix after a full expression.
- **`list_expr::2:3`**: the one real comma decision — after
  `[expr,` the next token either opens the `..` cons tail or
  continues the list (the sequence-cons arm checks comma-then-dotdot
  in `literals.rs`).
- **`statement::*`**: statement dispatch is by leading keyword
  (`crates/emath-syntax/src/parser/stmt.rs`) — `fn` always opens a
  declaration; statement-leading `match` refuses E-SYN-101.
- **`pattern::*`**: patterns are deliberately not full expressions
  (`crates/emath-syntax/src/parser/expr/forms.rs`): the pattern arm
  takes one literal token, the `_` wildcard, or a binding name; the
  `(` overlap flows only from the complex sum form's
  expression-leading spelling.
- **`type_primary::1:5`**: the `where` clause is a postfix
  continuation on any type_primary; the parser parses the type then
  checks for `where`.

## Grammar-vs-implementation gaps found while classifying

Recorded here because the pins above depend on them; both are
language-surface decisions for the owner, not gate fixes:

- `lambda_expr` (expression-position `fn params => body`) refuses
  E-SYN-110 today; the EBNF documents the spelling as contract.
- `match_statement` (statement-leading `match`) refuses E-SYN-101;
  match parses in expression position only.

## Verification

- `python3 scripts/ambiguity_scan.py --baseline
  tests/language-gates/fixtures/ambiguity-baseline.json
  language/grammar/surface.ebnf language/grammar/genesis.ebnf` →
  `0 NEW errors, 52 baseline`.
- Grammar battery controls: the identical-alternatives delta is
  refused, the clean additive delta is admitted (see validate.sh).
