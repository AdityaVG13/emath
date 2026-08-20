# Grammar

EBNF grammars for the emath language.

| File | Role |
|------|------|
| [surface.ebnf](surface.ebnf) | **Authoritative** surface grammar. |
| [genesis.ebnf](genesis.ebnf) | Grammar addendum for the semantic-genesis (custom-world) subsystem: `emath custom` / kind declarations, sections, and carriers. |

The surface grammar is the machine model of the language; the normative
semantic specification in [`../reference/`](../reference/README.md) wins on
any disagreement. The genesis grammar describes the compiler's custom-world
pipeline (see `crates/emath-syntax/src/genesis.rs`), not the user-facing
surface language.
