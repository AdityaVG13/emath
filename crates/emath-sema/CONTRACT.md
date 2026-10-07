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
  Disk loading searches ancestor edition manifests using a lexical absolute
  source path, including for relative inputs; display names remain unchanged.
  Loading changed bytes under the same display name allocates a new `FileId`;
  older ids retain their source snapshots and line maps. Unchanged bytes reuse
  the latest id. Stored checks carry that id through parser and admission spans.
  `check_package` resolves loaded siblings to their latest matching revisions.
  `parse_text` is anonymous, allocates no source, and uses `FileId(0)` spans.
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

It also retains `syntax` and `parse_diagnostics` from the same check. Syntax
can remain present after semantic refusal and is not permission to execute or
translate. Consumers that claim admission must require the error-free result
and its `prepared` module. Live conformance inspection follows this rule,
including query receipts, rather than independently admitting a second parse.

For the current constructor language, session checking includes constructor
admission and its resolved imported object names before producing `CheckResult`.
Hosts should consume that result rather than parse and admit the main source
again. Its `prepared` field retains the constructor module resolved and admitted
by that check. It is `None` on any parse, load or semantic error, including
multi-file package pre-diagnostics; warnings alone do not remove it. Retained
generic semantic checks still run before a module is exposed for execution.
The package and prepared module are two current representations of one checking
result, not a claim that semantic and execution type models have been unified.
This does not promise an atomic filesystem snapshot of all imported files.
Constructor `package` declarations retain legacy SIR signature projections,
paired with `constructor_semantics`: the complete checked scoped declaration
graph from `PreparedModule::meaning_tree`, including private/unselected helpers,
resolved alias edges and direct/transitive imported bodies. No source file is
reread to assign meaning or seal content. Constructor meaning structurally
encodes full types, ordered bodies, lexical binding and policy; source spans,
comments and tests are excluded. This is identity of admitted mathematics,
not an extensional-equivalence decision or proof of a behavior-preserving rewrite.

## Authority boundary

- A verified Language Image is the sole source of executable mathematical
  FeatureIDs, aliases, arity, input/output contracts, and refusal diagnostics.
- Capsule-active capability entries are copied into the package capability arena
  before declaration lowering. A resolved call lowers to `ExprNode::Apply` with
  its `CapabilityId`.
- Unknown explicit `std::...` FeatureIDs refuse with `E-LANG-FEATURE`; other
  unresolved calls refuse with `E-TYPE-003`.
- Named mathematics is not admitted by a handwritten FeatureID router.
  Remaining compiler primitives intern or apply *authored* data:
  `emath capability` cells intern into the package arena; local names
  shadow image aliases; `series_from_csv` /
  `series_at` map string literals onto universal `ExprNode::Series` /
  `SeriesSample`; rank-3 stencil sugar is a spelling table that builds
  weight carriers and `Apply`s existing kernels; Option/Result and
  `length`/`norm`/`not` are universal IR. None of these mint a FeatureID.
- `emath reaction_network` admission is labeled-multiset schema (species
  closure, section shape, element-spelling balance). It is not a chemistry
  kernel and does not select a FeatureID. Moving the element spelling
  table into a capsule is diet follow-up, not a dispatch registry.
- Item-attribute governance is typed-refusal admission, not domain
  semantics: unknown `@attributes` refuse (E-SYN-118), unknown capability
  keys refuse (E-PKG-065), `@experimental` without the declared
  `experimental-syntax` capability refuses (E-PKG-064), `@units_profile`
  validates its ladder and provenance requirements, and
  `@significant_figures` validates display/enforce contracts against
  retained presentation helpers. None of these compute mathematics or
  admit features.
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
- Undeclared leftover recognition/call files were deleted on 2026-09-04
  with user authorization (`emath-5gz4o`). They are not modules and
  contained no executable authority.
