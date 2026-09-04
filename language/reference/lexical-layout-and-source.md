# Chapter 2: Lexical, Layout and Source Rules

## Encoding

- Source is UTF-8.
- Canonical line ending is LF.
- A byte-order mark is not canonical in packages (diagnosed `E-SYN-113`).
- Identifiers use Unicode XID rules and are normalized to NFC for identity; original spelling remains available for diagnostics.
- Confusable identifier linting is enabled by default for public declarations.

### Confusables (A5, normative)

No visually confusable glyphs in one namespace. The enforcement ladder:

- a combining mark (U+0300–U+036F) in an identifier diagnoses with
  `E-SYN-115`; such a spelling is canonically non-NFC by construction
  and cannot be re-normalized without a Unicode table;
- any other non-ASCII identifier warns with `E-SYN-114` (confusable
  Unicode lookalikes are a quality hazard);
- a declaration name that folds onto an already-seen lookalike
  (Latin `a` vs Cyrillic `а` vs Greek `α`, per the `confusable_fold`
  seed map) diagnoses with `E-NAME-024`; the public API would present two
  visually indistinguishable names.

No profile or capability disables the ladder; a package may only narrow
it (stricter admission), never widen it (anti-proposal A5, chapter 12).

## Comments and documentation

```emath
# ordinary line comment
/// documentation attached to the following item
```

Only line comments are admitted. Block comments are diagnosed.

## Colon charter

`:` means exactly two things, nothing else:

1. **Binder head separator**; after a bound name in a declaration head,
   section field, or section-suite statement: `emath function f() -> Float64:`,
   `x: Float64`, `unit Token = base "token":`-style heads, constructor
   params, `given` names.
2. **Section head separator**; between a section/command name and its
   indented payload: `definitions:`, `inputs:`, `goals:`, `compile:`.

Record-literal fields use path-prefixed `{}` (U3), not `:`. Every other
`:` use is outside the grammar; the parser diagnoses it with `E-SYN-111`
(expected `:`) only where one of the two meanings was required; never by
inventing a third meaning.

## `in` charter

`in` means exactly two things, nothing else:

1. **Binder keyword**; after a bound name in a binder head, before the
   domain: `sum n in 0..10: n`, and inside set comprehensions,
   `{n in 0..100 if is_prime(n)}`.
2. **Membership operator**; infix between two expressions: `v in s`
   (ASCII for ∈). It is parsed by the comparison operator tier, never by
   the binder grammar.

The positions are provably disjoint: the binder form requires a bound
identifier to its left inside a binder head; the operator form requires
two complete operands. One ELP ambiguity scan covers both spellings per
X13 (`tests/emath-syntax/tests/sets.rs`:
`binder_in_stays_binder_not_membership`,
`elp_x12_both_brace_forms_share_one_profile`). No third meaning: any
other placement diagnoses with the enclosing construct's expected-token
code (e.g. `E-SYN-111`), never a new interpretation.

## Layout

Indentation is semantic after a header ending in `:`. The canonical indentation unit is four spaces. Tabs are not canonical source.

`NEWLINE` is suppressed:

- inside `()`, `[]`, `{}`;
- after an incomplete assignment or infix operator;
- where the grammar explicitly consumes a continuation.

A formatter emits canonical indentation and never changes semantic identity.

## Literals

Supported literal families include:

```text
integer           123, 1_000
exact rational    3//7
decimal           1.25
fixed float       1.25f64
quantity          10 m, 3//2 s
complex           2 + 3i
string/character
boolean
```

Literal spelling, parsed exact value and target representation are distinct. A decimal literal does not become binary floating point until a numeric profile or explicit suffix requires it.

Juxtaposition (A-bonus, normative): a unit identifier binds a numeric
literal only across whitespace (`10 m`, `3//2 s`). `2x` is diagnosed with a
suggestion naming `2 * x`; never a silent product or a silent quantity
with unit `x` (chapter 12, anti-proposals; C15 pins reaction-line `2H2`
to section grammar).

## Resource limits

The parser exposes configurable caps for file bytes, token count, literal bytes, identifier bytes, nesting, indentation depth and recovery nodes. Diagnoses carry stable codes and spans.

## Stage-0 Rust boundary

The bootstrap parser permanently owns only UTF-8/source preservation;
indent/newline layout; generic `emath <kind> Name:` declarations and named
sections; `use` and qualified paths; identifiers/literals; calls, indexing,
field access, lists, records, local `let`; registry-driven operator slots; one
generic binder skeleton; holes; and unknown-glyph preservation.

Unknown mathematical glyphs retain exact UTF-8 bytes and byte spans. Stage-0
does not infer their meaning. A mounted syntax pack/operator registry may assign
meaning later; without one, admission returns a typed hole/diagnosis.

Cipher, puzzle, protocol, campaign, frontier, criterion, reaction-network,
nabla, braket, graph-literal, softmax, and Hodge-specific grammar are explicitly
outside Stage-0. Those names belong in Feature Capsules and syntax packs. A
structural gate rejects direct feature-name matching added to parser, sema, or
backend code outside approved generic registries.

A rare Stage-0 extension requires a reviewed language-design decision proving
the form cannot be represented by a capsule/syntax pack, benefits at least
three domains, passes ambiguity/precedence/confusable analysis, and documents
migration and cost. Otherwise add a capsule or syntax pack, not Rust grammar.

Accepted extension: one generic binder skeleton used by sums, products,
quantifiers, and integrals. Rejected extension: a `cipher` keyword branch or a
special parser arm for one named transform. Registry-driven operator slots are
accepted; matching `"softmax"` in parser code is rejected.


## Empty source

A file that contains only comments and whitespace is not a package. After
lexing, it has zero items. A file whose only items are `package` / `use` /
`notation` (no `emath function` / `policy` / `model` / `kind`) also has no
declarations. `check` and `plan` diagnose both with `E-PKG-081`
(`source has no declarations`) instead of admitting vacuously (build used
to fail later with "package has no declarations"). `eval` and `simulate`
also diagnose empty source. An empty file is invalid, not a successful no-op.

## Paths and modules

Paths use `::`. Filesystem paths do not define semantic package identities by themselves. Package/module mapping is declared in the manifest and loader.

## Reserved: literate cell fences (05 §7.3, seed only)

Literate `.emath` is a reserved future shape, not implemented surface: a
literate file is a Markdown document whose fenced code blocks compile as
one `.emath` module. The reservation below fixes the grammar and
extraction rules NOW so no later syntax can collide with them.

- **Cell fence (reserved).** A cell is a fenced code block whose info
  string is exactly `emath` (case-sensitive, no additional tokens).
  The fence itself; the backtick delimiter and info string; lives
  OUTSIDE the module text: the loader strips it before lexing. A
  backtick is not a `.emath` token and the info string is parsed by
  the loader, not the module lexer, so no `.emath` keyword, operator,
  or literal grammar can ever conflict with the fence. Nothing inside
  a cell changes: cell content is ordinary module text under all
  chapter-2 rules (encoding, layout, comments).
- **Extraction order (reserved).** Cells are extracted in source
  order; the order the fences appear in the document; and each
  cell's text is normalized to NFC before compilation, the same
  identity rule as any other source (Encoding, above). The extracted
  cells compile as ONE module; cell boundaries are not module
  boundaries and carry no semantics of their own.
- **Prose is never identity (policy).** Any machine-checkable claim
  must live inside a cell; prose that outruns the compiled cells is a
  documentation error, not evidence. A receipt embedded in the
  document is checked like any other receipt (Tamper behavior,
  chapter 11): a hash that no longer matches the current build is
  flagged, never silently accepted.
- **No claim today.** `emath check` reads `.emath` source only; no
  loader extracts cells yet. This section reserves syntax and order;
  it admits nothing.
