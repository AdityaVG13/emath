# emath-ir

## Purpose and layer

Tier 2 neutral semantic IR (SIR), goal IR (GIR), resolution plans and
evidence IR. Provider-free by constitution: no upstream type may appear here.

## Public types and semantics

- `Constructor`, `Field`, `TestCase`, `Visibility`: constructor structure. `TestCase.expect` is `Option<ExprId>`: `Some` is an asserted example test; `None` is a worked example (given only).
- `ObligationClass` (static/runtime/solver/certificate/deferred), `ObligationKind` (precondition/postcondition), `ConstructionObligation`, `ConstructionReceipt`: the constructor obligation taxonomy. `Constructor::obligation_matrix` classifies every textual `require`/`ensure`/`invariant` (all `Runtime` in Phase 1); `Constructor::receipt` produces the receipt; `ConstructionReceipt::compose` merges delegate obligations first and never drops one; only `Deferred` obligations remain open after construction; `ConstructionReceipt::identity` is a deterministic content id.
- `mig`: `Mig` / `MigNode` / `MigEdge` / `MigNodeKind` / `MigEdgeKind`; the mathematical intent graph (schema `emath.mig.v1`), derived deterministically from a `SemanticPackage`. Every semantic plane is represented by node kinds (definition, construction, goal, evidence, execution, evolution); every non-declaration node is owned by a declaration node (spine property). `Mig::identity` excludes presentation-only changes by construction (no spans enter the derivation; expression content enters via span-free `canonical_expr`).
- `layers`: `IrLayer`; the ten-layer IR stack registry (syntax, HIR, MIG, SIR, GIR, resolution, EIR, evidence, Rust IR, artifact) with durable schema base ids (matching strings already written into artifacts), explicit schema versions, `versioned_schema()` ids and owning crates.
- `ExprNode`, `Literal`, `BinaryOp`, `UnaryOp`,
  `BinderKind`,
  `BinderVariable`: neutral expression trees.
  `ExprNode::Apply { capability, arguments }` is the capability-cell
  application term: the payload is a stable `CapabilityId` into the owning
  package's `capabilities` arena, so adding a domain cell appends data and
  never adds a core enum variant. A dangling capability id refuses with
  `MeaningError::MissingCapability` at the `meaning_id` seam (canonical
  bytes stay deterministic with a `<missing-capability>` marker); the
  native symbolic fragment refuses capability applications with `E-SYM-003`.
- `Capability`, `CellSchema`, `CellClass`, `MigrationPolicy`,
  `canonical_capability`, `canonical_cell`, `cell_id`, `admit_cell`,
  `admit_cell_mutation`, `AdmissionRefusal`: the capability-cell layer
  (schema `emath.capability-cell.v1`). Ten closed classes (`pure` …
  `artifact`); every descriptor carries a required schema version and an
  explicit migration policy. Identity (`cell_id`, FNV-1a64 over the
  length-framed canonical preimage) covers exactly the identity-affecting
  fields (name, class, version, migration policy token, arity); `about` is
  presentation-only. Bounded admission refuses unknown class (`E-CELL-001`),
  missing version (`E-CELL-002`), policy-refused identity mutation
  (`E-CELL-003`), arity over `MAX_CELL_ARITY` (`E-CELL-004`), and
  namespace-less names (`E-CELL-005`). Interned cells enter the MeaningID
  preimage by name, and every `ExprNode::Apply` must reference an interned
  cell or `meaning_id` refuses with `MeaningError::MissingCapability`.
- `SymbolicExpr`, `RewritePattern`, `RewriteRule`, `SymbolicOracleContract`:
  provider-neutral symbolic contracts. Native v1 structurally simplifies
  exact integer scalar expressions and exactly decides bounded univariate
  polynomial identities.
- `Goal`, `GoalKind`, `GoalRequirements`, `RequestSpec`, `ResolutionPlan`,
  `PlanNodeDef`, `PlanOperation`, `CompileSpec`: goal and resolution planning.
- `KindSchema`, `SectionSchema`, `CoreKind`, `PayloadPolicy`, `RepeatPolicy`:
  kind/type schema representations shared by compiler and builder.
  `KindSchema::core_function` treats `inputs:` as `AtMostOne` (omitted
  inputs is a constant-only declaration) and `outputs:` as `AtMostOne` with
  default `definitions` (omitted outputs expose every definition). Policy
  inherits those pins; `state:` / `constructors:` stay `ExactlyOne`.
- `TypeScheme`, `TypeExpr`, `TypeConstraints`, `TypeVar`, `TypeNode`,
  `SchemeBody`, `SchemeField`: type representation and `unify` / `canonical_of`
  / `render`.
- `Measured<T>`, `DistributionKind`, `Provenance`, `BindingSite`, and
  `core_measure_schemes`: provider-neutral measurement values and the closed
  six-variant `core::measure` schema. `Measured::unstated` makes missing
  provenance explicit.
- `Domain`, `Interval`, `BranchConvention`, `Unit`, `UnitDim`, `UnitFamily`,
  `NumericType`, `NumericProfile`, `NumericBehavior`: domains, units and
  numeric computation models (not exhaustive; see modules).
  `NumericProfile::StrictF64` is the Phase 1 default; `IntervalF64` is
  explicit only. `parse_numeric_profile("")` yields the default.
  `lookup_unit` / `per_unit` refuse unknown or ill-formed units.
  `Interval::checked` and `Shape::declare` refuse inverted/empty shapes;
  `branch_point` resolves branch conventions; `Extent` / `SparseLayout`
  complete the shape surface.
- `ContractRegistry`, `ProviderRepresentationContract`, `EvidenceBundle`,
  `EvidenceClaim`, `ClaimVerdict`, `SemanticPackage`, `ImportEntry`.
  `SemanticPackage::binding_provenance` participates in canonical package
  identity; it is intentionally excluded from mathematical MeaningID.
- `meaning_id` / `SemanticPackage::meaning_id`: SHA-256 MeaningID over
  `emath.meaning.canonical.v1` length-framed admitted semantics and sorted
  dependency MeaningIDs. Presentation, local/declaration/binder names,
  tests, evidence attachments, prose, spans, and host bindings are excluded.
- `MeaningError`: malformed SIR refusal for missing/cyclic expression/type
  references; malformed graphs never receive a MeaningID.
- Modules: `canonical`, `capability`, `constructor`, `contracts`, `domains`,
  `evidence`, `expression`, `goal`, `ids`, `kind_schema`, `numeric`,
  `operator`, `package`, `provenance`, `shapes`, `symbolic`, `type_system`,
  `types`, `units`.

## Invariants

- IR is neutral and provider-free: no upstream type may appear.
- Canonical forms exist for expressions, operators and goals (round-trip).
- Meaning identity alpha-normalizes local and binder names, resolves imported
  aliases, and binds meaning-affecting numeric/goal/world policy.
- Same type set shared across compiler and builder admission.
- Core function/policy kind schemas are the admission source of truth for
  which sections are required vs optional.

## Error model

Named structured error types with stable identities: `DomainError`,
`NumericError`, `InferenceError`, `ShapeError`, `UnitError`. Diagnostics flow
through `emath_core::Diagnostics` where appropriate. Numeric-model refusals
use `E-NUM-001` (unknown model), `E-NUM-002` (precision demand),
`E-NUM-003` (error-limit demand). Unit catalog misses are `E-UNIT-104`;
ill-formed `Per<U>` is `E-UNIT-105`. Ill-formed intervals are `E-DOM-002`;
ill-formed declared shapes are `E-SHAPE-004`.

## Determinism class

Canonicalization is deterministic and round-trip stable via `canonical_of`,
`canonical_operator` and `plan_identity`; covered by canonical round-trip
tests.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. Crate root is `#![forbid(unsafe_code)]`.

## Feature flags

None.

## Conformance tests

No `tests/` directory in this crate and no `#[cfg(test)]` module in `src/`.
Coverage lives in workspace test members `tests/emath-ir` (canonical,
goal, layers, numeric_models, symbolic, constructor, mig, domain_logic, containment,
including `interval_containment_holds_on_seeded_grid` for
`Interval`/`Domain` membership; and `capability_id_terms.rs` for
CapabilityId terms, name-based cell identity, legacy sin/exp compat, and the
dangling-capability typed refusal) and `tests/emath-store` (MeaningID
presentation/alpha/alias stability plus semantic-policy/dependency changes).

## No-claim boundaries

Neutral representation only: no provider semantics, no certification of
correctness beyond structure. Executable regions and world semantics are
outside as per crate map. `NumericProfile` / `NumericBehavior` are
computation descriptors (rounding, overflow, precision ceiling). They are
never claims about real-number arithmetic, and `Real` is not silently
`f64` without a selected profile (the omitted-`numeric:` default is the
explicit `strict-f64` model).
MeaningID is declared structural identity, not a proof of general
mathematical equivalence; stronger equivalence belongs in evidenced relations.
The native symbolic decision is complete only for the documented exact
univariate degree-64 fragment. It makes no Gröbner, CAD, transcendental, or
general first-order decision claim.
