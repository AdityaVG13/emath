# `std.text.report`; text values and deterministic report emitters

Status: IMPLEMENTED . The compiler,
reference VM, generated Rust path, and WASM serializer carry text and
report values.

## What exists today (landed, honest)

- **Interpolation grammar + purity fences**:
  a hole carries only a name or dotted path (`{x}`, `{model.coeff}`);
  expressions, calls, and indexing refuse at parse time (`E-SYN-101`,
  naming the purity rule). The format spec is fixed (`.Nf`, nothing
  else); `{{`/`}}` escape; an unparsed hole is never silently text.
  Side effects are impossible **by grammar**, not by discipline.
- **Identifier NFC** at the lexer: combining marks refuse (`E-SYN-115`).
- Runtime interpolation substitutes admitted values with fixed,
  locale-independent formatting.

## `core::text` contract (Phase B acceptance)

- A `Text` value carrier: string values admit, lower to `Literal::Text`,
  and round-trip losslessly through the formatter.
- Interpolation evaluation: holes evaluate admitted scalars with the
  fixed `.Nf` spec; the emitted digits are exact fixed-point formatting
  of the admitted value (no ambient locale, no ambient precision).
- **NFC value identity**: canonically equivalent text values hash to
  the same meaning because literals normalize before semantic identity.
  their NFC-normalized contents match; MeaningId hashes the normalized
  form (lexer precedent: identifiers are NFC by construction).
- Unicode operations are declared, not ambient: `text_length` counts
  Unicode scalar values, `nfc` normalizes, and `==` compares values.
  Anything outside this minimal set refuses by name.

## `core::report` contract (Phase B acceptance)

- `section(heading, body)` and `document(title, section)` construct
  report values from admitted expressions. A
  report node binds names that already have provenance; it never
  computes new numbers (that boundary keeps reports evidence-grade;
  same rule as observations).
- `render_markdown` and `render_latex` are deterministic: same admitted model in,
  byte-identical artifact out; no timestamps, no locale, no map
  iteration order in output.
- A report is data, not code: emitting has no side effects and the
  emitted bytes hash into provenance like any other artifact.

## Refusals

Reports are pure values. There is no file-writing or callback operation;
`render_file` and other side-effecting spellings are unknown functions
and refuse rather than performing I/O.
