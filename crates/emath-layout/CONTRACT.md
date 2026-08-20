# emath-layout

## Purpose and layer

- Frontier lane for math *layout* input (SG-11 LaTeX, SG-12 PDF fixtures).
- Shared [`MathLayoutGraph`](src/graph.rs) is the IR both frontends emit.
- Depends on `emath-term`, `emath-genesis` (scoped binders), and `emath-world-ir` (`fnv1a64`).
- Fixture-driven: not a production LaTeX engine and not a PDF binary parser.

## Public types and semantics

- `LAYOUT_SCHEMA` = `emath.math-layout-graph` (same string as the disclosed emath-schema registry id), `LAYOUT_VERSION` = 1. `check_version` refuses unknown versions.
- `MathLayoutGraph`: ordered nodes and edges, retained source bytes, retained ambiguities, optional unlowered regions. `canonical()` is a versioned text encoding; `graph_id()` is FNV-1a64 over that form. `source()` is the original input (LaTeX) or the deterministic fixture serialization (PDF).
- `LayoutNode { id, content, source_span }`. `LayoutContent`: Glyph, Row, Superscript, Subscript, Fraction, Radical, BigOp(kind name), FormulaRegion.
- `LayoutEdge { parent, child, relation }` with `SpatialRelation`: RightOf, Above, Below, SuperscriptOf, SubscriptOf, Contains.
- `RetainedAmbiguity { node_id, reading_a, reading_b, reason }`: both readings stay on the graph.
- `UnloweredRegion { node_id, reason }`: formula extracted, term not fabricated.
- `parse_latex`: mixed documents detect `$...$` (inline) and `\[...\]` (display); bare math (no delimiters) is one formula. Structured subset only.
- `to_binder_term`: `\sum`/`\prod` → Structural binders (FiniteRange when bounds are integer literals, else Symbolic); `\int` → Integral / FiniteAnalogue; `\lim` → Limit / Conventional; infix `+ - * / =` → `Term::Apply`; letters/Greek → Variable; digits → Constant; non-binder `x^2` → `Apply("pow", [x, 2])`. A top-level `var = binder` equation lowers to the binder (term IR cannot wrap a binder in `Apply`).
- `PositionedGlyph` / `PdfPageFixture`: milli-unit integers. `reference_fixture()` is the supplied 2D formula `E = Σ_{i=1}^{3} i²`. `extract` groups y-bands, detects super/sub by offset + smaller font, keeps the 20–45% font-size band as a retained ambiguity, and marks formula vs prose runs.

## Invariants

- LaTeX `source()` is byte-exact with the input.
- Formula-region spans slice the source to the delimited region (`$...$` or `\[...\]`) exactly.
- Canonical form and `graph_id` are identical across independent rebuilds of the same input.
- Ambiguities are retained; the frontend never picks a single reading in the ambiguous band.
- Extraction never invents a binder term: failure is `LayoutError::Unlowered` (and is recorded on the PDF graph).

## Error model

- `LayoutError::UnknownVersion`: schema handshake.
- `UnexpectedToken { token, offset }`: character or token outside the subset.
- `UnknownMacro { name, offset }`: backslash command outside the subset (offset is the `\`).
- `UnterminatedDollar` / `UnterminatedDisplay`: missing closer.
- `Unlowered { reason }`: graph may still exist; no term is produced.

## Determinism class

- Deterministic: `BTree`-style sorted nodes/edges/ambiguities, sequential node ids, integer milli-units, FNV-1a64 identities.
- No floats in the graph or fixture coordinates.

## Cancellation behavior

- Not applicable; std-only synchronous crate with no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids unsafe_code.

## Feature flags

- None.

## Conformance tests

- `graph_unknown_version_refused`
- `graph_canonical_identical_across_rebuilds`
- `latex_source_preserved_byte_exact`
- `latex_sum_lowers_to_structural_finite_range_and_expands`
- `latex_unknown_macro_refused_with_offset`
- `latex_unterminated_dollar_refused`
- `latex_formula_region_spans_byte_exact`
- `pdf_reference_fixture_graph_id_deterministic`
- `pdf_superscript_detection_emits_relation`
- `pdf_ambiguous_band_retains_both_readings`
- `pdf_prose_only_has_zero_formula_regions`
- Production path: `cargo xtask demo math-layout` parses a mixed LaTeX document, extracts the PDF reference fixture, expands the LaTeX sum through `emath_genesis::run` / `FreeTermWorld`, records a typed unknown-macro refusal and a retained ambiguity, and emits `math-layout.json` with a tamper negative control.

## No-claim boundaries

- Not a general LaTeX engine: structured subset only (letters, digits, `+ - * / = ( )`, `^`/`_`, `\frac`, `\sqrt`, `\sum_{v=a}^{b}`, `\prod_{v=a}^{b}`, `\int_{a}^{b}`, `\lim_{v \to a}`, named Greek). Everything else is a typed refusal naming the token and byte offset; there is no recovery or guess.
- Not a PDF binary parser: positioned-glyph fixtures only. No page stream, font program, or content-stream interpretation.
- Ambiguities are retained, not resolved. The 20–45% font-size y-offset band emits both readings.
- There is no persisted layout store. `LAYOUT_VERSION` consumers refuse unknown versions (`check_version`), so rollback/migration is refuse-and-rebuild — an old artifact citing a different version is rejected, never reinterpreted.
