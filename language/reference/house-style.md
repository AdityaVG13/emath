# Chapter 17: House Style

## 0. What house style is

House style is what `emath fmt` accepts as canonical; nothing more
inventive. The formatter is the arbiter: a file either round-trips
(`fmt` confirms `canonical form`) or comes back diagnosed `NOT canonical`
(never silently rewritten). This chapter documents that canonical style
so a reader can write it correctly the first time. Where this chapter
and the formatter disagree, the formatter wins and this chapter must be
fixed in the same change.

## 1. Section order

The declaration-envelope reading order is:

```text
inputs → state → constants → definitions → equations → goals
```

`tests:` and `compile:` follow the semantic sections; `about:` and
`use:` lead. Canonical layout puts one blank line between sections
(the `intro/hello-square.emath` shape) and none between a `///` comment
block and the declaration it documents. This order is a convention, not
a legality: the kind schema (ch. 8) decides which sections a kind
admits and in what multiplicity. Examples follow the convention; a file
that orders sections differently still admits if its schema allows it.

## 2. Naming

- Bindings, fields, inputs, state, outputs: `snake_case`
  (`state.scale`, `force`, `tau`).
- Declaration names (kinds, functions, policies, models): `PascalCase`
  (`Square`, `Defaults`, `AffineScorer`).
- Test/example names: `snake_case` inside angle brackets
  (`example <three_squared>:`).
- Everything is XID + NFC-clean per chapter 2; the confusable ladder
  applies unchanged.

## 3. Meaning versus work

Chapter 1 §4 is normative: a `definitions:` entry states meaning; a
`goals:` entry asks the compiler for work. House style adds one rule:
never mix them. A section that both defines and requests is a style
diagnosis waiting to happen; the payload-shape table (ch. 7, F3) already
routes any payload that does not match its family to its code.

## 4. Decision table: `=`, `==`, and the goal verb

| You are writing… | Spelling | Lives in |
|---|---|---|
| a value/rate a name carries | `name = expr` | `definitions:`, rate rows |
| a constraint/residual | `lhs == rhs` or bare `expr` | `equations:`, `invariant:` |
| an assertion about admitted values | `expect lhs == rhs` | `tests:` |
| work the compiler should do | `verb <target>:` + commands | `goals:` |

`==` never defines (ch. 7, F6); the diagnostic names both readings when
the spellings are mixed. A goal verb is never a one-line heading; the
flat-goal sugar routes to `E-SYN-112`.

## 5. `///` scope

A `///` doc comment attaches to the next item (chapter 2). One comment
block per declaration, immediately above it; blank lines between a
comment and its item break the attachment. Doc comments are prose, not
payload: they never carry semantics.

## 6. Vertical idioms

- One binding per line; the `definitions:` payload shape is exactly
  `name = expr` per line (ch. 7, F3).
- Single spaces around `=` and binary operators; the canonical style has
  no aligned-`=` column and the formatter neither produces nor requires
  alignment. (The earlier "align the equals signs" proposal is retired:
  it is not formatter-canonical.)
- A long right-hand side may hang a trailing operator across the line
  break; `NEWLINE` is suppressed after an incomplete assignment or
  infix operator (ch. 2 layout, C4). Bracketed continuation is equally
  canonical; pick one per file.
- Indentation is the canonical four spaces; tabs are not canonical (ch. 2).

## 7. `emath fmt` is the contract

`emath fmt <file>` confirms canonicality: canonical files round-trip
byte-identically; anything else comes back `NOT canonical` with no
rewrite (the diagnostic lists the expected canonical lines). The
formatter is idempotent, edition-aware, preserves comments, and never
changes semantic identity (ch. 12). Honesty fence: the lossless
canonicality *gate* pins named fixtures (`tests/valid/square.emath`,
`tests/valid/affine_scorer.emath`, and each newly added example), so a
drift there fails the build; the wider example corpus is being
canonicalized incrementally and a non-canonical example there is
recorded debt, not a green light; do not cite the corpus as
formatter-canonical until the sweep closes.
