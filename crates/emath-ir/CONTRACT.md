# emath-ir

## Purpose and layer

Tier 2 neutral semantic IR (SIR), goal IR (GIR), resolution plans and
evidence IR. Provider-free by constitution: no upstream type may appear here.

## Public types and semantics

- `Constructor`, `Field`, `TestCase`, `Visibility`: constructor structure.
- `ObligationClass` (static/runtime/solver/certificate/deferred), `ObligationKind` (precondition/postcondition), `ConstructionObligation`, `ConstructionReceipt`: the constructor obligation taxonomy. `Constructor::obligation_matrix` classifies every textual `require`/`ensure`/`invariant` (all `Runtime` in Phase 1); `Constructor::receipt` produces the receipt; `ConstructionReceipt::compose` merges delegate obligations first and never drops one; only `Deferred` obligations remain open after construction; `ConstructionReceipt::identity` is a deterministic content id.
- `mig`: `Mig` / `MigNode` / `MigEdge` / `MigNodeKind` / `MigEdgeKind` — the mathematical intent graph (schema `emath.mig.v1`), derived deterministically from a `SemanticPackage`. Every semantic plane is represented by node kinds (definition, construction, goal, evidence, execution, evolution); every non-declaration node is owned by a declaration node (spine property). `Mig::identity` excludes presentation-only changes by construction (no spans enter the derivation; expression content enters via span-free `canonical_expr`).
- `layers`: `IrLayer` — the ten-layer IR stack registry (syntax, HIR, MIG, SIR, GIR, resolution, EIR, evidence, Rust IR, artifact) with durable schema base ids (matching strings already written into artifacts), explicit schema versions, `versioned_schema()` ids and owning crates.
- `ExprNode`, `Literal`, `BinaryOp`, `UnaryOp`, `BinderKind`,
  `BinderVariable`: neutral expression trees.
- `Goal`, `GoalKind`, `GoalRequirements`, `RequestSpec`, `ResolutionPlan`,
  `PlanNodeDef`, `PlanOperation`, `CompileSpec`: goal and resolution planning.
- `KindSchema`, `SectionSchema`, `CoreKind`, `PayloadPolicy`, `RepeatPolicy`:
  kind/type schema representations shared by compiler and builder.
  `KindSchema::core_function` treats `outputs:` as `AtMostOne` with default
  `definitions` (omitted outputs expose every definition).
- `TypeScheme`, `TypeExpr`, `TypeConstraints`, `TypeVar`, `TypeNode`,
  `SchemeBody`, `SchemeField`: type representation and `unify` / `canonical_of`
  / `render`.
- `Domain`, `Interval`, `BranchConvention`, `Unit`, `UnitDim`, `NumericType`:
  domains, units and numeric types (not exhaustive; see modules).
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
through `emath_core::Diagnostics` where appropriate.

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

No `tests/` directory in this crate. Inline `#[cfg(test)]` unit tests live in
the `src` modules (not enumerated). IR canonicalization also has a
dedicated non-crate test member at `tests/emath-ir`.

## No-claim boundaries

Neutral representation only: no provider semantics, no certification of
correctness beyond structure. Executable regions and world semantics are
outside as per crate map.
