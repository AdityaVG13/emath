# Current Repository Gap Audit

Baseline: `ffec253ab08e7a40d260798f348634e522a18e66`.

The audit recognizes substantial current work: project laws, router doctrine, bounded custom-kind lowering, capability cells, object packs, G4 proposal gates, deterministic artifacts, worlds, and extensive tests. V19 closes authority, generation, conformance, migration, and agent-control gaps without replacing those systems.

| ID | Severity | Finding | V19 correction |
|---|---|---|---|
| `GAP-001` | critical | Authority is distributed among project constitution, reference prose, grammar, capability matrix, executable object packs, tests, and code. | Introduce a feature-scoped authority lock with exactly one authority source per FeatureID. |
| `GAP-002` | high | The current maintenance order begins with implementation rather than a machine-readable semantic contract. | Move to capsule and conformance first; implementation completes the accepted contract. |
| `GAP-003` | high | The four-artifact rule covers reference, grammar, examples, and tests but omits exactness, worlds, reference semantics, providers, artifacts, diagnostics, migrations, and agent views. | Replace it with fifteen-projection closure while retaining current G4 gates during transition. |
| `GAP-004` | high | CAPABILITY.md is manually maintained and collapses parse, admit, execute, evidence, world, and artifact status. | Generate orthogonal status views from capsules, conformance, and implementation coverage. |
| `GAP-005` | high | Reference prose is normative over grammar, while selected standard-library object packs are also described as executable truth. | Use feature-scoped authority so each feature has one source; distinguish normative language definition from executable distribution. |
| `GAP-006` | high | The current surface grammar mixes stable syntax, implementation commentary, aspirational rules, and domain-specific productions. | Separate Stage-0 grammar, generated accepted surface, syntax-pack sources, and proposal-only grammar. |
| `GAP-007` | high | Syntax-pack growth is a decided doctrine but no complete generic pack-to-parser/lowering/formatter/LSP/conformance pipeline is the universal path. | Make syntax-pack compilation a V19 capstone. |
| `GAP-008` | critical | No stable FeatureID joins a concept across grammar, reference, code, worlds, diagnostics, tests, artifacts, and migration. | Adopt FeatureID as the cross-plane join key. |
| `GAP-009` | high | There is no complete Meaning Spine for dependency, reverse impact, load, migration, test, and agent closure. | Generate a typed feature graph. |
| `GAP-010` | high | There is no compiled Language Image containing the accepted language and exact identities. | Build deterministic partitioned Language Image and lock. |
| `GAP-011` | high | There is no explicit Stage-0 boundary or Stage1/Stage2 fixed-point gate. | Define minimal bootstrap and semantic fixed-point comparison. |
| `GAP-012` | high | ELPs are detailed prose but do not carry one canonical machine-readable Feature Capsule delta. | Adopt ELP v2 with capsule delta and projection closure. |
| `GAP-013` | high | No Spec Hole object prevents an implementation agent from deciding an unresolved semantic detail. | Make unresolved choices explicit and stable-build blocking when required. |
| `GAP-014` | medium | Quickstart examples use bare declaration names while the current surface EBNF excerpt appears to require a bracketed declaration head. | Resolve through authoritative source cases and goldens; do not let agents choose an interpretation. |
| `GAP-015` | high | Current status vocabulary mixes maturity, parse support, admission, execution, evidence, artifact disposition, and world coverage. | Use orthogonal status axes. |
| `GAP-016` | high | There is no standardized cross-layer golden from source bytes through CST, expansion, Core AST, typed HIR, worlds, result, and artifact. | Create a semantic-stack Conformance Corpus. |
| `GAP-017` | high | Tests are discovered by crate/file rather than indexed by FeatureID and projection. | Generate a Conformance Graph and targeted closure. |
| `GAP-018` | medium | Diagnostics have stable codes, but feature conditions, mathematical explanations, routes, and negative controls are not uniformly one machine record. | Use diagnostic capsules and generated constructors/reference. |
| `GAP-019` | high | World definitions and capability/evidence manifests are not uniformly one executable capsule per world. | Use World Capsules with applicability, representation, strategy, effects, and authority. |
| `GAP-020` | medium | User-defined worlds remain an open design question without a machine-visible blocker. | Represent it as a Spec Hole and experimental feature decision. |
| `GAP-021` | high | Phase-1 builtins still live in compiler tables while capability cells/object packs are the intended scalable path. | Migrate leaf builtins with dual-run reference semantics. |
| `GAP-022` | high | The V16 catalog has 1,224 entries but no uniform catalog-to-specified-to-conformant promotion ledger. | Generate a default catalog-only promotion ledger. |
| `GAP-023` | high | V15–V18 concepts can be implemented differently by models because the waves are prose authority only. | Provide cross-wave Feature Capsules and conformance slices. |
| `GAP-024` | high | No root machine manifest tells an agent authority, baseline, invariants, commands, and context budget. | Add AGENT_START.json. |
| `GAP-025` | high | No generated task-specific read and impact closure exists. | Add Task Capsule, Context Capsule, Impact Closure, and orientation tool. |
| `GAP-026` | medium | Agent decisions, failures, and learned constraints are not append-only language artifacts. | Add Change Receipts and Accretion Ledger. |
| `GAP-027` | medium | File paths and line-number search are brittle semantic anchors under refactors. | Use FeatureID and generated owner/projection anchors. |
| `GAP-028` | medium | No metrics measure agent orientation cost, unnecessary reads, tool calls, or rework. | Add agent-economy fields and acceptance targets. |
| `GAP-029` | high | Independent implementations lack a complete semantic conformance protocol. | Standardize cross-implementation inputs and comparisons. |
| `GAP-030` | medium | Generated-file policy is distributed and direct edits can be difficult to distinguish. | Use one generated header/lock and CI regeneration gate. |
| `GAP-031` | high | Legacy and capsule authority could overlap during migration. | Make authority transitions feature-scoped and reject dual authority. |
| `GAP-032` | medium | Router doctrine says nothing is refused at the door, while older documents and capability rows still use refusal language. | Generate a consistent distinction between parse failure, semantic contradiction, routed diagnostic, hole, and runtime fault. |
| `GAP-033` | medium | Current Float64 execution can be labeled numerical even when no rigorous error bound exists, while the public label vocabulary emphasizes approximate-with-bound. | Separate floating numerical result from certified bounded approximation in status/evidence. |
| `GAP-034` | high | Current traceability uses a legacy FNV identity chain while public/global semantic distribution needs versioned collision-resistant identity domains. | Version identity algorithms and migrate without invalidating legacy artifacts silently. |
| `GAP-035` | medium | Naming policy says aliases are removed while notation/import systems permit canonical aliases and V15 needs `cypher`→`cipher`. | Distinguish deprecated semantic synonyms from registered accept-many/canon-one surface aliases. |
| `GAP-036` | high | The standard-library README and capability matrix can drift; for example, support statements about Option/Result may differ by snapshot. | Generate both from the same feature and conformance data. |
| `GAP-037` | medium | Current `custom` documentation spans parser, genesis, and capability descriptions with differing execution status language. | Assign separate FeatureIDs for custom surface, genesis execution, and curated world support. |
| `GAP-038` | high | A feature can exist in grammar or code without a complete artifact/evidence consequence. | Require projection closure and artifact disposition. |
| `GAP-039` | medium | Current documentation gates have known pre-existing inconsistencies noted in the latest commit. | Make gap radar output a blocking or explicitly waived artifact rather than hidden CI debt. |
| `GAP-040` | high | There is no automatic check that a domain addition avoided core parser and stable-IR branches. | Add anti-domain-branch architectural lint keyed by FeatureID class. |
| `GAP-041` | medium | No canonical agent handoff/resume object exists. | Task/Context/Change Capsules make restart state explicit. |
| `GAP-042` | medium | Multi-agent semantic conflicts can appear as ordinary file merge conflicts. | Detect semantic and golden conflicts by FeatureID before merge. |
| `GAP-043` | high | A model may update goldens to match a bug. | Golden changes require ELP/identity/migration authority. |
| `GAP-044` | medium | A provider can be integrated without one generated coverage and evidence view. | Provider Capsules and capability/world matrices are generated. |
| `GAP-045` | medium | No uniform distinction exists between specification completeness and implementation completeness. | Feature maturity and projection/implementation coverage are independent. |
