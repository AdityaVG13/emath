# Grammar

EBNF grammars for the emath language.

| File | Role |
|------|------|
| [surface.ebnf](surface.ebnf) | **Authoritative** surface grammar (design grammar v5). |
| [genesis.ebnf](genesis.ebnf) | Grammar addendum for the semantic-genesis (custom-world) subsystem: `emath custom` / kind declarations, sections, and carriers. |
| (archive) | Superseded dialect snapshots (design grammar v4) are removed; see the `surface.ebnf` header for version lineage. |

The surface grammar is the machine model of the language; the normative
semantic specification in [`../reference/`](../reference/README.md) wins on
any disagreement. The genesis grammar describes the compiler's custom-world
pipeline (see `crates/emath-syntax/src/genesis.rs`), not the user-facing
surface language.
