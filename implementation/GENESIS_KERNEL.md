# Semantic Genesis Kernel

The kernel is the trusted deterministic core of semantic genesis:
fixed inputs yield fixed outputs. It constructs candidate
interpretations honestly and never invents semantics inside the
trusted scope. Genesis admission is an honest constructed-world
outcome, never a silent strict fallback.

## Inputs

- Raw UTF-8 genesis source (`emath custom Name:` grammar):
  `parse_genesis` / `GenesisFile` in `crates/emath-syntax/src/genesis.rs`.
- `Limits` (`emath_core::limits::Limits`): `max_source_bytes`,
  `max_tokens`, `max_nesting` (`crates/emath-core/src/limits.rs`).
- `ForestLimits` (`emath_genesis::forest::ForestLimits`): `max_nodes`,
  `max_alternatives`, `max_depth` (`crates/emath-genesis/src/forest.rs`).
  Genesis admission uses `65_536 / 128 / 128`, not the type default.
- `VmBudget` (`emath_genesis::VmBudget`): per-run step ceiling;
  `seed_default()` is `max_steps: 4096` (`crates/emath-genesis/src/vm.rs`).

## Outputs

`genesis_cmd` (`crates/emath-cli/src/genesis_cmd.rs`) writes this set:

| File | Schema |
| --- | --- |
| `source-artifact.json` | `emath.source-artifact` |
| `parse-forest.json` | `emath.parse-forest` |
| `signature.json` | `emath.signature` |
| `free-term.json` | `emath.free-term` |
| `meaning-problem.json` | `emath.meaning-problem` |
| `interpretation-portfolio.json` | `emath.interpretation-portfolio` |
| `g7-portfolio-receipt.txt` | G7 `evaluate` receipt (`replay` input) |
| `world-admission.jsonl` | `emath.world-admission` |
| `answer-receipt.json` | `emath.answer-receipt` |
| `csa-baseline.json` | `emath.csa` |
| `world-candidates/<id>.json` | `emath.world-candidate` |

The ten files are the `files` array in `genesis_cmd`;
`world-candidates/` is written beside them. Parametric Rust codegen
is downstream (`compile --parametric`), not inside the kernel.

## Trusted scope

Trusted: structural parse of the genesis body (bounded forest),
signature inference from a unique term, free-term construction,
metered VM evaluation (`emath.vm`), portfolio ranking
(`InterpretationPortfolio::new`), and receipts (answer + CSA).

Not trusted:

- Asserting intended meaning. `CSA_MEANING_CLAIM` in
  `crates/emath-genesis/src/csa.rs` is
  `totality-baseline; never author-intended meaning`.
- Granting authority above `Authority::Structural` without checker
  receipts. The ladder in `crates/emath-portfolio/src/lib.rs` is
  Structural < Tested < Certified < Proved; genesis writes
  `checker_receipts: []` and stamps Structural only.
- Executing providers. World candidates carry `provider_id`
  `builtin-seed`; the kernel does not spawn providers.

## Determinism / replay

`run_demo_semantic_genesis` in `xtask/src/main.rs` runs `emath genesis`
twice on `language/examples/integration/arbitrary-glyphs.emath` into dirs `a`
and `b`, then `diff_dirs(&a, &b, "genesis determinism")`. Pass requires
byte-identical artifacts.

Pinned by `implementation/SG00_STRICT_BASELINE.md`:

- answer_id `447d467cf93fc4ce`
- worlds: `free_symbolic` `361775e049cf1ff6`, `Boolean_algebra`
  `f7d98f355f241691`, `modular_numeric` `9b32e9f2132a5c34`,
  `one_point` `909a6be3f247e488`, `csa_seeded` `98dc2aff1691f4d4`
- CSA: seed `3836149761`, value `118f84421f20b522`, vm_steps `9`

2026-08-18 `cargo xtask demo semantic-genesis`:

```
semantic-genesis demo: ok
```

## First implementation boundary

Content ids are bootstrap FNV-1a64 (`emath_world_ir::fnv1a64`), not
cryptographic; production replaces them with the canonical identity
service. The kernel is std-only: no `unsafe`, no extra features
(`crates/emath-genesis/CONTRACT.md`).
