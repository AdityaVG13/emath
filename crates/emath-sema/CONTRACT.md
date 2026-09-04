# emath-sema CONTRACT.md

## Purpose and layer

- Tier 2 semantic admission: syntax tree to typed neutral SIR.
- Owns generic package/import, declaration, section-schema, field, type,
  carrier, definition, equation, goal, and plan validation.
- Resolves executable calls only from capsule-installed FeatureID bindings or
  declared sibling functions. It does not recognize mathematical feature names,
  choose domain semantics, or own operation registries.
- Depends on `emath-core`, `emath-ir`, `emath-exec-ir`, and `emath-schema`.

## Public surface

- `CompilerSession`: source loading, parsing, checking, and planning.
- `SourcePackage`, `CheckResult`, and `PlanResult`.
- `CompilerPolicy`, `GeneratedCrate`, and `EmittedAnchor`.
- `SemanticTrace` and `TraceEntry`.
- `inspect_live_source`, `LiveConformanceRequest`,
  `LiveConformanceResponse`, and `StageStatus`.
- `language::install_language_distribution`: installs a verified distribution
  into the execution binding layer and derives sema's generic call bindings.
- `recognition::{KindDef, SchemaRule, admit_declaration, expr_text, type_text}`:
  generic declaration-schema and diagnostic rendering support.

`CheckResult` contains the admitted package, diagnostics, semantic trace, and
the effective per-declaration units-profile table (empty without
`@units_profile` attributes). Domain-policy side tables are not part of
mathematical admission; the profile table is declaration metadata for later
generic consumers.

## Authority boundary

- A verified Language Image is the sole source of executable mathematical
  FeatureIDs, aliases, arity, input/output contracts, and refusal diagnostics.
- Capsule-active capability entries are copied into the package capability arena
  before declaration lowering. A resolved call lowers to `ExprNode::Apply` with
  its `CapabilityId`.
- Unknown explicit `std::...` FeatureIDs refuse with `E-LANG-FEATURE`; other
  unresolved calls refuse with `E-TYPE-003`.
- No builtin arity table, mathematical call-name router, standard-kind registry,
  embedded domain package recognizer, or reaction/chemistry/finite recognizer
  exists in this crate. Item-attribute governance is typed-refusal admission,
  not domain semantics: unknown `@attributes` refuse (E-SYN-118), unknown
  capability keys refuse (E-PKG-065), `@experimental` without the declared
  `experimental-syntax` capability refuses (E-PKG-064), `@units_profile`
  validates its ladder and provenance requirements, and
  `@significant_figures` validates display/enforce contracts against retained
  presentation helpers. None of these compute mathematics or admit features.
- Source `emath feature` declarations are generic restricted capsule data.
  `emath-schema` validates their closed class table; sema does not branch on a
  feature or domain name.
- Local `emath kind` declarations provide structural section schemas only.
  Applications receive generic section/statement-shape checks and cannot mint
  mathematical, evidence, provider, or execution authority.
- Field packs are validated exported artifact descriptors and never enter the
  runnable declaration arena.

## Retained generic validation

- Package identity and library imports are recorded deterministically. File-path
  imports refuse with `E-PKG-050`.
- Empty and package-only source refuses with `E-PKG-081`.
- Duplicate, reserved, and confusable declaration names refuse with stable
  `E-NAME-*` diagnostics.
- Declaration schemas enforce required/optional section cardinality and allowed
  statement shapes. Unknown sections or declaration-level statements refuse
  with `E-SYN-101`.
- Field and result types, shapes, units as type carriers, definitions, equations,
  invariants, goals, source spans, and arena remapping remain checked generically.
- Option/Result constructors and projections retain direct structural carrier
  validation; they do not select mathematical meaning.
- Sibling function calls use declaration data and hygienic inline substitution,
  not a registry or compatibility alias.
- Missing source and parser installation are typed refusals, never silent empty
  admission.

## Determinism, errors, and safety

- Parsing/checking is deterministic for the same source, limits, edition, and
  installed Language Image.
- Planning uses deterministic native resolution ordering.
- Diagnostics use stable `E-*` codes through `emath_core::Diagnostics`; no
  unsupported construct is silently dropped.
- The crate is synchronous and has no cancellation surface.
- `unsafe` is forbidden.

## No-claim boundaries

- Sema does not prove mathematics, define domain operations, certify capsule
  authority, select kernels/providers, or interpret exactness/evidence/world
  policy. Those are authored image data and later generic execution concerns.
- Backend and artifact emission live outside this crate.
- Retired source files are retained as non-module tombstones because file
  deletion was not authorized; they contain no executable authority.
