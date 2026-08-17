# emath-ir

## Purpose and layer

Tier 2 neutral semantic IR (SIR), goal IR (GIR), resolution plans and
evidence IR. Provider-free by constitution: no upstream type may appear here.

## Public types and semantics

- `Constructor`, `Field`, `TestCase`, `Visibility`: constructor structure.
- `ExprNode`, `Literal`, `BinaryOp`, `UnaryOp`, `BinderKind`,
  `BinderVariable`: neutral expression trees.
- `Goal`, `GoalKind`, `GoalRequirements`, `RequestSpec`, `ResolutionPlan`,
  `PlanNodeDef`, `PlanOperation`, `CompileSpec`: goal and resolution planning.
- `KindSchema`, `SectionSchema`, `CoreKind`, `PayloadPolicy`, `RepeatPolicy`:
  kind/type schema representations shared by compiler and builder.
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
