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

## Meaning and consumer boundaries

The current source admission authority is `emath-sema::CompilerSession`.
Its result retains syntax/parser diagnostics, the semantic package and an
error-free prepared constructor module. Syntax alone is not admission.
`emath-exec-ir` owns import scoping, immutable preparation, constructor type
admission, fresh evaluation and lowering. Sema invokes that machinery rather
than duplicating it. This is an explicit sema-to-execution dependency, not
yet one unified semantic/execution type representation.

CLI check/run/test/keep/compare/build, loop/dream/experiment hosts, Lean
translation/example checks/certification and WASM operations consume checked
results. Diagnostic and provenance consumers also use the semantic session.
Rust ABI/layout inference and Lean type/coercion/termination checks remain
target obligations; they may refuse an admitted definition, but cannot
independently admit source. Rust generation is optional user work, not
mathematical authority.

Root `MeaningId` is distinct from source bytes, dependency snapshots, target
artifacts and operational identities. Checked constructor identity includes
the retained scoped declaration graph, including private helpers and direct
and transitive imports. Legacy programmatic SIR includes dependency meanings
only when explicitly supplied. Neither path promises an atomic filesystem
snapshot. Native session export additionally compares retained dependency
sources, conservatively rejecting byte drift.
Serialized checkpoints and later ordinary library imports do not embed the
in-memory checked module.

## Implemented workspace crates (`crates/`)

The dependency column below records current normal first-party manifest
edges, not an ideal layering policy. Optional upstream dependencies are
described under shipping profiles. Inventory checked with offline Cargo
metadata on 2026-10-05.

### Tier 0: identity, diagnostics, transport

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-core` | `crates/emath-core` | IDs, syntax-tree records, spans, diagnostics, canonical primitives, source store, limits, JSON writer | no first-party dependency; unicode-normalization |
| `emath-artifact` | `crates/emath-artifact` | JSON parse-back reader for host documents (writer re-exported from core) | core |

### Tier 1: language

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-syntax` | `crates/emath-syntax` | lexer, layout, lossless tree, parser backend (`install_source_parser`), lossless formatter | core |
| `emath-schema` | `crates/emath-schema` | core declaration-kind section schemas and Feature Capsule records | core/term |
| `emath-term` | `crates/emath-term` | provider-neutral first-order term representation (canonical round-trip) | no first-party dependency |

### Tier 2: semantics and execution

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-ir` | `crates/emath-ir` | neutral SIR/GIR/plan/EMIR/evidence structures | core/schema/term |
| `emath-sema` | `crates/emath-sema` | source-session authority, semantic package and constructor admission | core/exec-ir/ir/schema |
| `emath-exec-ir` | `crates/emath-exec-ir` | import scoping, constructor preparation/evaluation/lowering, VM and executable regions | artifact/core/ir/rt/schema/syntax/term |
| `emath-rt` | `crates/emath-rt` | numeric/storage/execution carriers; includes authored-math migration debt | core |
| `emath-rust-backend` | `crates/emath-rust-backend` | Rust ABI/layout/rendering, constructor crate and engine-source packaging | core/exec-ir/rt |

### Tier 3: hosts and frontends

| Crate | Path | Responsibility | May depend on |
|---|---|---|---|
| `emath-build` | `crates/emath-build` | generated-crate host helpers: stable target dir, write-if-changed, bounded Cargo runs | (std only) |
| `emath-lab-core` | `crates/emath-lab-core` | measurement harness, paired statistics, finite law checking | core/ir/term |
| `emath-tui` | `crates/emath-tui` | loop/dream/experiment orchestration, scratch/native mirror, REPL/dashboard | artifact/build/core/exec-ir/rust-backend/sema/syntax |
| `emath-cli` | `crates/emath-cli` | production command routing, presentation, Lean translation | artifact/build/core/exec-ir/ir/lab-core/rust-backend/sema/syntax/term/tui |
| `emath-wasm` | `crates/emath-wasm` | checked in-memory constructor operations and confined C ABI | artifact/core/exec-ir/ir/rust-backend/sema/syntax |

## Compiled roots and shipping profiles

All 15 crates above have their production module root at `src/lib.rs`.
The CLI binary additionally starts at `src/main.rs`. Module declarations,
`cfg`, `include!` and build scripts, not physical source lines, determine
compilation. Crate `CONTRACT.md` files record each component's API, error,
determinism, resource and unsafe boundaries. This map is an ownership review,
not a claim that every line or optional configuration passed a quality gate.

| Profile | First-party normal dependency closure | Distribution boundary |
|---|---|---|
| Default `emath-cli` | 14 crates | Binary plus shipped `language/` tree; backend, lab core and TUI are linked dependencies; the terminal dashboard is the opt-in `dashboard` feature |
| `emath-wasm` | 11 crates | `cargo xtask build-web` builds wasm32 release and stages `web/dist/emath.wasm`; not full mathematical-capability parity |
| Direct generated Rust | Runtime source included when needed | Shared constructor emitter, named entries and manifest; target support/refusal remains explicit |
| Engine-backed generated Rust | 8 engine crates from exec-ir's manifest closure | `vendor/` first-party sources and a trimmed lockfile; third-party sources are not vendored, so an uncached machine is not guaranteed offline compilation |
| Native epoch host | Artifact plus sibling host crate | Scratch mirror is target-specific; full integer state, bounded i128 ledger projections, cross-lane parity tested |
| Lean export | Separate Lean project and toolchain | Translation is not proof; `--check` requires the Lean kernel, and laws also require emath certification |

The default CLI closure is core, syntax, term, schema, ir, sema, exec-ir,
rt, rust-backend, artifact, build, lab-core, cli and tui. WASM is core,
syntax, term, schema, ir, sema, exec-ir, rt, rust-backend, artifact and
wasm. Workspace libraries are `publish = false`, not a published
crate distribution. No universal release/archive assembly is established here.

`emath-rust-backend/build.rs` embeds every file under the engine closure's
`src/`, including unreferenced historical files. Uncompiled source can therefore
ship as data. `emath-rt::SOURCE` similarly concatenates runtime parts, including
core-owned numeric source. Removing a module declaration alone does not remove
its source from generated artifacts.

Optional profiles: CLI `dashboard` (non-default) enables TUI `dashboard`,
which adds ftui for the live `emath dream` terminal; the default CLI prints
the plain-text run status instead. Default transitive graphs already include
unicode-normalization and stacker. Release optimization settings in
`Cargo.toml` are not release assembly or passing-release evidence.

`xtask/generate` is a separate lightweight workspace behind the Cargo alias;
demo commands forward to the full feature-selected xtask. Tests, harnesses and
examples are build consumers, not shipped CLI modules. `bootstrap` is explicitly
excluded historical source. `forks/` records upstream acquisition, not automatic
linking. SBOM/notices follow `legal/LICENSE_POLICY.md`; the current dependency
inventory must be checked before redistribution.


## Absorbed in the 2026-08 consolidation

## Non-crate workspace members

| Member | Path | Responsibility |
|---|---|---|
| `tests/emath-build` | `tests/emath-build` | build-step public-API test suite |
| `tests/emath-cli` | `tests/emath-cli` | CLI public-API test suite and the error-code registry gate |
| `tests/emath-exec-ir` | `tests/emath-exec-ir` | executable-IR public-API test suite |
| `tests/emath-ir` | `tests/emath-ir` | IR canonicalization test suite |
| `tests/emath-lab-core` | `tests/emath-lab-core` | laboratory public-API test suite |
| `tests/emath-schema` | `tests/emath-schema` | schema public-API test suite |
| `tests/emath-rust-backend` | `tests/emath-rust-backend` | Rust backend public-API test suite |
| `tests/emath-rt` | `tests/emath-rt` | runtime-kernel public-API test suite |
| `tests/emath-sema` | `tests/emath-sema` | session/admission public-API test suite |
| `tests/emath-syntax` | `tests/emath-syntax` | syntax public-API test suite |
| `tests/emath-term` | `tests/emath-term` | term-parse back test suite |
| `tests/emath-wasm` | `tests/emath-wasm` | wasm public-API test suite |
| `tests/emath-tui` | `tests/emath-tui` | loop/dream/experiment/native-host test suite |
| `tests/harness` | `tests/harness` | shared test harness library |
| `xtask` | `xtask` | repository tasks: `generate-language`, `check-wasm`, `build-web`, `serve-web` |

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

- `emath-lab` → `emath-lab-core` (Tier 3).

No alias rows are used in this map: every implemented row names the
directory that actually exists on disk.
