# Canonical Rust Crate Map

Pinned against HEAD by the `crate-map + API inventory` lane in
`scripts/validate.sh` (gauntlet-08 gate). The gate fails when:

1. an implemented row lacks its directory on disk (or a recorded alias);
2. a directory under `crates/` is missing from the map;
3. a workspace member (from `[workspace] members` in `Cargo.toml`) is
   missing from the map.

Planned rows are never certifying: a planned name is a statement of
intent, not an implemented surface.

## Implemented workspace crates (`crates/`)

### Tier 0 — identity, diagnostics, transport

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-core` | `crates/emath-core` | IDs, spans, diagnostics, canonical primitives, source store, limits | std only |
| `emath-source` | `crates/emath-source` | source files, line maps, human-readable diagnostic rendering | core |

### Tier 1 — language

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-syntax` | `crates/emath-syntax` | lexer, layout, lossless tree, parser backend (`install_source_parser`), lossless formatter | core |
| `emath-schema` | `crates/emath-schema` | custom kind schemas and restricted lowering (thirteen-schema registry) | core |
| `emath-hir` | `crates/emath-hir` | resolved declaration representation | core/ir |
| `emath-term` | `crates/emath-term` | provider-neutral first-order term representation (canonical round-trip) | core |

### Tier 2 — semantics

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-ir` | `crates/emath-ir` | neutral SIR/GIR/plan/EMIR/evidence structures | core |
| `emath-sema` | `crates/emath-sema` | orchestration and constructor/invariant admission (`CompilerSession`) | core/ir/syntax/goal |
| `emath-exec-ir` | `crates/emath-exec-ir` | executable target-independent regions | ir |
| `emath-goal` | `crates/emath-goal` | request elaboration and goal schemas | ir |
| `emath-plan` | `crates/emath-plan` | compatibility, decomposition, ranking, fallback | ir |

### Tier 3 — goals and providers

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-provider-api` | `crates/emath-provider-api` | descriptors, provider/adapter/checker traits | core/ir |
| `emath-adapter-dew` | `crates/emath-adapter-dew` | Dew scalar backends (std-only native lanes) | provider-api |
| `emath-adapter-rumoca` | `crates/emath-adapter-rumoca` | Rumoca subset-import adapter | provider-api |

### Tier 4 — evidence and artifacts

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-evidence` | `crates/emath-evidence` | claims, assumptions, bundles, freshness | ir |
| `emath-checker` | `crates/emath-checker` | artifact/result/certificate checking | evidence/artifact |
| `emath-rust-ir` | `crates/emath-rust-ir` | structured Rust target AST | ir |
| `emath-rust-backend` | `crates/emath-rust-backend` | EMIR to Rust IR and rendering | rust-ir |
| `emath-artifact` | `crates/emath-artifact` | Cargo package, manifests, source maps, SBOM | core/schema |
| `emath-runtime` | `crates/emath-runtime` | Outcome, budgets, continuations, provider runtime | core |

### Tier 5 — integration

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-build` | `crates/emath-build` | build-step backend and artifact emission (`GeneratedCrate` host) | sema/artifact |
| `emath-builder` | `crates/emath-builder` | programmatic Rust builder API (laboratory surface) | build |
| `emath-macro` | `crates/emath-macro` | procedural macro convenience (corrected name; formerly `emath-macros`) | — |
| `emath-lab-core` | `crates/emath-lab-core` | experiments, metrics, promotion, drift, keep-gate identity (corrected name; formerly `emath-lab`) | core |
| `emath-registry` | `crates/emath-registry` | package/provider registry slice | provider-api |
| `emath-lsp` | `crates/emath-lsp` | language server | sema/cli |
| `emath-cli` | `crates/emath-cli` | command-line application | sema/build/lab-core |

### Tier 6 — semantic genesis substrate

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-genesis` | `crates/emath-genesis` | minimal Semantic Genesis evaluator and built-in example worlds | term/world-ir |
| `emath-world-ir` | `crates/emath-world-ir` | provider-neutral World IR and meaning-hole structures | ir |
| `emath-world-codegen-rust` | `crates/emath-world-codegen-rust` | deterministic parametric Rust world artifact generation (Genesis G3) | world-ir/lab-core |
| `emath-meaning-provider-api` | `crates/emath-meaning-provider-api` | stable contracts for meaning proposal and world checking | provider-api |
| `emath-portfolio` | `crates/emath-portfolio` | deterministic interpretation portfolios | meaning-provider-api |

### Tier 7 — governance and operations

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-agent-protocol` | `crates/emath-agent-protocol` | agent-native meaning proposals (admission envelope) | provider-api |
| `emath-calibration` | `crates/emath-calibration` | semantic calibration | portfolio |
| `emath-law-check` | `crates/emath-law-check` | independent world checking (`WorldChecker`): law admission | meaning-provider-api |
| `emath-holes` | `crates/emath-holes` | meaning holes and finite synthesis | world-ir |
| `emath-tuning` | `crates/emath-tuning` | semantic and joint tuning | portfolio |
| `emath-plugin-sdk` | `crates/emath-plugin-sdk` | plugin SDK slice: descriptors, sandbox policy decisions | provider-api |

### Tier 8 — infrastructure adapters (feature-gated)

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-store` | `crates/emath-store` | evidence/artifact state store (frankensqlite, `sqlite-store` feature) | core/ir |
| `emath-provenance` | `crates/emath-provenance` | goal → plan → artifact lineage graph (frankengraphdb, `graphdb` feature) | ir |
| `emath-search` | `crates/emath-search` | artifact corpus hybrid search (frankensearch, `search` feature) | ir |

Feature-gated: each crate's default build is std-only; the upstream engine
(Dicklesworthstone franken*) arrives only behind the named feature.

## Non-crate workspace members

| Member | Path | Responsibility |
|---|---|---|
| `tests/emath-term` | `tests/emath-term` | term-parse back test suite |
| `tests/emath-ir` | `tests/emath-ir` | IR canonicalization test suite |
| `tests/emath-trust-gates` | `tests/emath-trust-gates` | trust gate test suite |
| `tests/emath-artifact` | `tests/emath-artifact` | artifact test suite |
| `examples/demo-host` | `examples/demo-host` | build-time pipeline host + promotion + negative control |
| `examples/demo-host-independent` | `examples/demo-host-independent` | fingerprint-free behavioral-assert host |
| `examples/provider-skeleton` | `examples/provider-skeleton` | provider adapter skeleton |
| `examples/generated/semantic-genesis-worlds` | `examples/generated/semantic-genesis-worlds` | generated parametric worlds crate (Phase 4 golden) |
| `xtask` | `xtask` | demo/tooling carrier (`cargo xtask demo ...`) |

## Planned crates (not yet in `crates/`, never certifying)

| Crate | Status | Responsibility |
|---|---|---|
| `emath-canonical` | planned | versioned encoders/decoders and hashing |
| `emath-format` | planned | canonical formatter (lossless formatter currently lives in `emath-syntax`) |
| `emath-package` | planned | manifests, locks, modules, imports, resolver |
| `emath-types` | planned | type/refinement inference |
| `emath-units` | planned | dimensions/quantities/affine units |
| `emath-shapes` | planned | shape/layout constraints |
| `emath-domains` | planned | domains/measures/branches |
| `emath-provider-host` | planned | static/component/process/remote provider hosting |
| `emath-host` | planned | trait/FFI/host binding generation |

### Adapter/provider repositories (external, planned)

```text
emath-provider-wrenfold
emath-provider-frankensim-*
emath-provider-frankenjax
emath-provider-frankennumpy
emath-provider-frankenscipy-*
emath-provider-frankenlean
emath-provider-frankenengine
```

Provider crates may depend on upstream forks and emath adapter APIs. Core
crates never depend on providers.

## Name corrections (SURF-0003)

- `emath-macros` → `emath-macro` (Tier 5).
- `emath-lab` → `emath-lab-core` (Tier 5).

No alias rows are used in this map: every implemented row names the
directory that actually exists on disk.
