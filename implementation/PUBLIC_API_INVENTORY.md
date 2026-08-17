# Public API Inventory

Pinned against HEAD by the `crate-map + API inventory` lane in
`scripts/validate.sh` (gauntlet-08 gate). The compiler session block
below is extracted from `crates/emath-sema/src/session.rs` and must
match it method-for-method (name and receiver kind); any drift makes
the gate fail. This inventory is the reference for what is implemented
in HEAD; planned surfaces are marked `[planned]` and are never
presented as implemented.

## Compiler session — implemented (`crates/emath-sema/src/session.rs`)

```rust
pub struct CompilerSession {
    pub store: SourceStore,
    pub limits: Limits,
}

impl CompilerSession {
    pub fn new(limits: Limits) -> Self;
    pub fn load_package(&mut self, path: impl AsRef<Path>) -> Result<SourcePackage, String>;
    pub fn load_text(&mut self, name: impl Into<String>, text: impl Into<String>) -> FileId;
    pub fn parse_text(&self, text: &str) -> (emath_core::tree::SyntaxTree, Diagnostics);
    pub fn check(&mut self, file: FileId) -> CheckResult;
    pub fn check_owned(&mut self, name: &str, text: &str) -> CheckResult;
    pub fn plan(&mut self, file: FileId) -> PlanResult;
}
```

The session is `load → check → plan`; the build step (backend +
artifact emission) lives in `emath-build`, which consumes `PlanResult`
and produces `GeneratedCrate`.

## Named types on the session path — implemented

`SourcePackage`, `CompilerPolicy`, `PlanResult`, `GeneratedCrate`,
`EmittedAnchor`, `CheckResult` (`crates/emath-sema/src/session.rs`,
`crates/emath-sema/src/admit.rs`), `Limits`, `FileId`, `Diagnostics`,
`Span`, `SourceStore` (`crates/emath-core`), `SemanticPackage`,
`RequestSpec`, `ResolutionPlan`, `GoalId` (`crates/emath-ir`).

## Request-typed surface — Partial (not on the session)

The promise-typed request surface is NOT on the session: the methods
above take paths and `FileId`s, not request structs. Documenting a
request-typed `load_package` or a session `build` as implemented would
be dishonest (that is exactly the drift the gate exists to refuse);
these stay Partial until a behavior-changing API redesign adopts them
(out of scope for the doc gate).

```rust
pub struct LoadRequest;    // [planned] not on the session
pub struct GoalRequest;    // [planned] not on the session
pub struct BuildRequest;   // [planned] not on the session
```

## Provider API — crate exists, surface evolving (not gate-pinned)

`crates/emath-provider-api`: `ProviderDescriptor`, `CapabilityReport`,
`Provider`, `Adapter<Source, Target>`, `ResultChecker`,
`ProviderResult`, `ProviderError`.

## Runtime — crate exists, surface evolving (not gate-pinned)

`crates/emath-runtime`: `Outcome<T, E>`, `Budget`, `Cancellation`,
`ContinuationHandle`, `EvidenceHandle`, `UnresolvedReason`.

## Artifacts — crate exists, surface evolving (not gate-pinned)

`crates/emath-artifact`: `ArtifactManifest`, `ArtifactClass`,
`SourceMapEntry`, `ArtifactChecker`, `ArtifactBuilder`,
`GeneratedPackage`.

## Laboratory — crate exists, surface evolving (not gate-pinned)

`crates/emath-lab-core`: `ExperimentManifest`, `Observation`,
`MetricDefinition`, `PromotionPolicy`, `PromotionDecision`,
`PromotionReceipt`, `RuntimeSelector`, `DriftPolicy`.

## Neutral semantics — planned surface (not gate-pinned)

`PackageIdentity`, `DeclarationId`, `TypeId`, `ExprId`, `Declaration`,
`Constructor`, `TypeNode`, `ExprNode`, `Goal`, `GoalRequirements`,
`ResolutionPlan`, `PlanNode`, `EvidenceLevel`, `ExactnessPolicy`,
`TargetProfile`, `FallbackPolicy` — names may evolve; responsibilities
may not disappear silently.

## Compatibility

Every durable/public struct is non-exhaustive or version-enveloped
until 1.0. Serialization uses schemas rather than Rust layout. Trait
object ABI is not considered stable across dynamic libraries;
component/process boundaries use versioned transport.
