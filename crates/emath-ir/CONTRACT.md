# emath-ir

## Purpose and layer

Tier 2 neutral semantic IR (SIR), goal IR (GIR), resolution plans and
evidence IR. Provider-free by constitution: no upstream type may appear here.

## Public types and semantics

- `Constructor`, `Field`, `TestCase`, `Visibility`: constructor structure. `TestCase.expect` is `Option<ExprId>`: `Some` is an asserted example test; `None` is a worked example (given only).
- `ObligationClass` (static/runtime/solver/certificate/deferred), `ObligationKind` (precondition/postcondition), `ConstructionObligation`, `ConstructionReceipt`: the constructor obligation taxonomy. `Constructor::obligation_matrix` classifies every textual `require`/`ensure`/`invariant` (all `Runtime` in Phase 1); `Constructor::receipt` produces the receipt; `ConstructionReceipt::compose` merges delegate obligations first and never drops one; only `Deferred` obligations remain open after construction; `ConstructionReceipt::identity` is a deterministic content id.
- `mig`: `Mig` / `MigNode` / `MigEdge` / `MigNodeKind` / `MigEdgeKind` — the mathematical intent graph (schema `emath.mig.v1`), derived deterministically from a `SemanticPackage`. Every semantic plane is represented by node kinds (definition, construction, goal, evidence, execution, evolution); every non-declaration node is owned by a declaration node (spine property). `Mig::identity` excludes presentation-only changes by construction (no spans enter the derivation; expression content enters via span-free `canonical_expr`).
- `layers`: `IrLayer` — the ten-layer IR stack registry (syntax, HIR, MIG, SIR, GIR, resolution, EIR, evidence, Rust IR, artifact) with durable schema base ids (matching strings already written into artifacts), explicit schema versions, `versioned_schema()` ids and owning crates.
- `ExprNode`, `Literal`, `BinaryOp`, `UnaryOp`, `BinderKind`,
  `BinderVariable`: neutral expression trees.
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
- Modules: `canonical`, `constructor`, `contracts`, `domains`, `evidence`,
  `expression`, `goal`, `ids`, `kind_schema`, `numeric`, `operator`, `package`,
  `shapes`, `type_system`, `types`, `units`.

## Invariants

- IR is neutral and provider-free: no upstream type may appear.
- Canonical forms exist for expressions, operators and goals (round-trip).
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
Coverage lives in the workspace test member `tests/emath-ir` (canonical,
goal, layers, numeric_models, constructor, mig, domain_logic, containment
— including `interval_containment_holds_on_seeded_grid` for
`Interval`/`Domain` membership).

## No-claim boundaries

Neutral representation only: no provider semantics, no certification of
correctness beyond structure. Executable regions and world semantics are
outside as per crate map. `NumericProfile` / `NumericBehavior` are
computation descriptors (rounding, overflow, precision ceiling). They are
never claims about real-number arithmetic, and `Real` is not silently
`f64` without a selected profile (the omitted-`numeric:` default is the
explicit `strict-f64` model).
