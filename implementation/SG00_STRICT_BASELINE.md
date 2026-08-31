# SG-00 Strict Baseline Freeze

The strict baseline is the deterministic semantic-genesis pipeline:

source bytes → glyphs → parse forest → signature → Term IR → free world →
5-world interpretation portfolio → answer receipt → CSA totality baseline
(ADR-003) → parametric Rust crate.

Artifacts are std-only and bit-identical across repeated runs of the same
source. The production path is `emath genesis` / `emath compile --parametric`;
the freeze verifier is `cargo xtask demo semantic-genesis`.

## Frozen evidence

Captured against HEAD via the real CLI (`emath genesis`) and
`cargo xtask demo semantic-genesis`.

**Reference source:** `tests/valid/arbitrary-glyphs.emath`
(`REFERENCE_SOURCE` in `xtask/src/main.rs`).

**Answer id** (`answer-receipt.json`): `447d467cf93fc4ce`

**Receipt closure** (`answer-receipt.json`, schema v2 / SG-09): the receipt
binds source, parse, signature, term, world, valuation, result, code
(`artifact_hash` over the rendered parametric crate), portfolio, trace,
authority, and VM cost; `receipt_id` = `bd3c31b4ebb619a8` (FNV-1a64 over the
documented preimage, recomputed independently by the demo verifier).

**World identities** (genesis stdout):

| World | Identity |
| --- | --- |
| `free_symbolic` | `361775e049cf1ff6` |
| `Boolean_algebra` | `f7d98f355f241691` |
| `modular_numeric` | `9b32e9f2132a5c34` |
| `one_point` | `909a6be3f247e488` |
| `csa_seeded` | `98dc2aff1691f4d4` |

**CSA baseline** (`csa-baseline.json`, verbatim):

| Field | Value |
| --- | --- |
| `seed` | `3836149761` |
| `value` | `118f84421f20b522` |
| `vm_steps` | `9` |
| `meaning_claim` | `totality-baseline; never author-intended meaning` |

**Differential oracle pins** (demo stdout):

| Lane | Pin |
| --- | --- |
| free | `apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))` |
| boolean | `false` |
| modular-17 | `6` |
| swapped-modular-17 | `5` |

## Verification

```
cargo xtask demo semantic-genesis
```

Re-runs genesis twice (byte-identical artifacts), parametric codegen, the
committed generated-crate fidelity check, generated-crate tests, and the
four oracle pins above. It then verifies the SG-09 receipt closure: the
answer receipt must self-verify (`receipt_id` recomputes from the bound
fields), a tampered result must fail recomputation (negative control), a
zero `artifact_hash` is refused, and the generated Rust answers must equal
the semantic VM's own portfolio answers for `free_symbolic`,
`Boolean_algebra`, and `modular_numeric` (VM/Rust differential). Pass is
`semantic-genesis demo: ok`.

## Receipt rollback and migration (SG-09)

Receipt schema v1 (implicit, no `schema_version` field) bound neither the
term, the code, nor the portfolio and carried no `receipt_id`. Schema v2
adds `schema_version`, `receipt_id`, `term_id`, `artifact_hash`,
`portfolio_hash`, and hex-string hash encodings. Consumers must key on
`schema_version`: absent means v1 (no self-verification available), `2`
means the documented preimage in `crates/emath-cli/src/genesis_cmd.rs`
applies. Rollback is reverting the emitter and the xtask verifier together
(the preimage comment in `genesis_cmd.rs` and the recomputation in
`xtask/src/main.rs` must always change in the same commit); there is no
mixed mode.

To recapture identities (scratch only; not the freeze verifier):

```
cargo run -q -p emath-cli -- genesis tests/valid/arbitrary-glyphs.emath --out <dir>
```

## Rollback

Any change to a pinned value above requires a stated semantic reason and a
version bump of the corresponding schema constant (current value `1`):

| Constant | Lives in |
| --- | --- |
| `TERM_IR_VERSION` | `crates/emath-term/src/lib.rs` |
| `CSA_SCHEMA_VERSION` | `crates/emath-genesis/src/csa.rs` |
| `VM_SCHEMA_VERSION` | `crates/emath-genesis/src/vm.rs` |
| `WORLD_ABI_VERSION` | `crates/emath-world-codegen-rust/src/lib.rs` |

Bump the constant that owns the changed encoding. Do not silently retune a
pin.

## No-claim boundary

This freeze certifies determinism and totality of the baseline lane. It does
not certify mathematical meaning. CSA is the ADR-003 totality witness;
`meaning_claim` travels with every CSA artifact so a consumer cannot read
the seeded value as author-intended interpretation.
