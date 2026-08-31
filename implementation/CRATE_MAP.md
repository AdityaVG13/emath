# Canonical Rust Crate Map

Pinned against HEAD by the `crate-map + API inventory` lane in
`scripts/validate.sh` (gauntlet-08 gate). The gate fails when:

1. an implemented row lacks its directory on disk (or a recorded alias);
2. a non-hidden directory under `crates/` is missing from the map
   (dot-directories such as `crates/.asgrep` are tool caches, not crates);
3. a workspace member (from `[workspace] members` in `Cargo.toml`) is
   missing from the map.

Planned rows are never certifying: a planned name is a statement of
intent, not an implemented surface.

## Implemented workspace crates (`crates/`)

### Tier 0 — identity, diagnostics, transport

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-core` | `crates/emath-core` | IDs, spans, diagnostics, canonical primitives, source store, limits | std only |

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
| `emath-rust-backend` | `crates/emath-rust-backend` | EMIR to Rust IR and rendering | rust-ir |
| `emath-artifact` | `crates/emath-artifact` | Cargo package, manifests, source maps, SBOM | core/schema |
| `emath-rt` | `crates/emath-rt` | pre-compiled math kernels, embedded verbatim into generated crates and used by the interpreter | std only |

### Tier 5 — integration

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-build` | `crates/emath-build` | build-step backend and artifact emission (`GeneratedCrate` host) | sema/artifact |
| `emath-macro` | `crates/emath-macro` | procedural macro convenience (corrected name; formerly `emath-macros`) | — |
| `emath-lab-core` | `crates/emath-lab-core` | experiments, metrics, promotion, drift, keep-gate identity (corrected name; formerly `emath-lab`) | core |
| `emath-registry` | `crates/emath-registry` | package/provider registry slice | provider-api |
| `emath-cli` | `crates/emath-cli` | command-line application | sema/build/lab-core |

### Tier 6 — semantic genesis substrate

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-genesis` | `crates/emath-genesis` | minimal Semantic Genesis evaluator and built-in example worlds | term/world-ir |
| `emath-world-ir` | `crates/emath-world-ir` | provider-neutral World IR and meaning-hole structures; owns FittedTable and re-exports core's fnv1a64 (o7a6) | ir/core |

### Tier 7 — governance and operations

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|

### Tier 8 — infrastructure adapters (feature-gated)

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-store` | `crates/emath-store` | evidence/artifact state store (frankensqlite, `sqlite-store` feature) | core/ir |

Feature-gated: each crate's default build is std-only; the upstream engine
(Dicklesworthstone franken*) arrives only behind the named feature.

### Tier 9 — layout and playground frontends

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-wasm` | `crates/emath-wasm` | C-ABI wasm engine for the browser playground (`em_alloc`/`em_free`/`em_run`) | sema/syntax/exec-ir/rust-backend |


## Absorbed in the 2026-08 consolidation

## Non-crate workspace members

| Member | Path | Responsibility |
|---|---|---|
| `tests/emath-adapter-dew` | `tests/emath-adapter-dew` | Dew adapter public-API test suite |
| `tests/emath-adapter-rumoca` | `tests/emath-adapter-rumoca` | Rumoca adapter public-API test suite |
| `tests/emath-agent-protocol` | `tests/emath-agent-protocol` | agent-protocol public-API test suite |
| `tests/emath-artifact` | `tests/emath-artifact` | artifact test suite |
| `tests/emath-build` | `tests/emath-build` | build-step public-API test suite |
| `tests/emath-builder` | `tests/emath-builder` | builder public-API test suite |
| `tests/emath-checker` | `tests/emath-checker` | checker public-API test suite |
| `tests/emath-cli` | `tests/emath-cli` | CLI public-API test suite |
| `tests/emath-evidence` | `tests/emath-evidence` | evidence public-API test suite |
| `tests/emath-exec-ir` | `tests/emath-exec-ir` | executable-IR public-API test suite |
| `tests/emath-genesis` | `tests/emath-genesis` | genesis public-API test suite |
| `tests/emath-hir` | `tests/emath-hir` | HIR public-API test suite |
| `tests/emath-holes` | `tests/emath-holes` | holes public-API test suite |
| `tests/emath-ir` | `tests/emath-ir` | IR canonicalization test suite |
| `tests/emath-lab-core` | `tests/emath-lab-core` | laboratory public-API test suite |
| `tests/emath-law-check` | `tests/emath-law-check` | law-check public-API test suite |
| `tests/emath-layout` | `tests/emath-layout` | layout public-API test suite |
| `tests/emath-lsp` | `tests/emath-lsp` | language-server public-API test suite |
| `tests/emath-portfolio` | `tests/emath-portfolio` | portfolio public-API test suite |
| `tests/emath-registry` | `tests/emath-registry` | registry public-API test suite |
| `tests/emath-provider-api` | `tests/emath-provider-api` | provider-API public-API test suite |
| `tests/emath-rust-backend` | `tests/emath-rust-backend` | Rust backend public-API test suite |
| `tests/emath-rust-ir` | `tests/emath-rust-ir` | Rust IR public-API test suite (targets `emath-rust-backend::rust_ir`) |
| `tests/emath-rt` | `tests/emath-rt` | runtime-kernel public-API test suite |
| `tests/emath-search` | `tests/emath-search` | search public-API test suite |
| `tests/emath-sema` | `tests/emath-sema` | session/admission public-API test suite |
| `tests/emath-core` | `tests/emath-core` | core std-layer test suite (units, measure, statistics, geometry, signal, integral, stochastic, linprog, numtheory) |
| `tests/emath-store` | `tests/emath-store` | store public-API test suite |
| `tests/emath-syntax` | `tests/emath-syntax` | syntax public-API test suite |
| `tests/emath-term` | `tests/emath-term` | term-parse back test suite |
| `tests/emath-trust-gates` | `tests/emath-trust-gates` | trust gate test suite |
| `tests/emath-tuning` | `tests/emath-tuning` | tuning public-API test suite |
| `tests/emath-wasm` | `tests/emath-wasm` | wasm public-API test suite |
| `tests/harness` | `tests/harness` | shared test harness library |
| `tests/emath-world-codegen-rust` | `tests/emath-world-codegen-rust` | parametric codegen public-API test suite |
| `tests/emath-world-ir` | `tests/emath-world-ir` | World IR public-API test suite |
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
