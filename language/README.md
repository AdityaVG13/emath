# emath Language Assets

Normative reference, grammar, examples, templates, and standard-library
contracts for the emath language.

| Directory | Purpose |
|-----------|---------|
| [`reference/`](reference/README.md) | Normative semantic specification, 16 chapters; the index there carries the canonical chapter order. |
| [`grammar/`](grammar/README.md) | EBNF grammars: authoritative surface grammar and genesis addendum; authority and supersession notes in the grammar README. |
| [`examples/`](examples/README.md) | Cross-domain example programs, grouped by category, with a curated reading order. |
| [`templates/`](templates/README.md) | Project, declaration, and provider scaffolds. |
| [`stdlib/`](stdlib/README.md) | Standard library package catalog and provider contracts. |

**Authority:** when the reference and the grammar disagree, the reference
chapters in [`reference/`](reference/README.md) are normative; grammar files
are the machine-checkable surface model that follows them.

Start with [`reference/overview.md`](reference/overview.md) (Chapter 1), then
follow the chapter index. Examples illustrate intended semantics; the phase
documents state when each becomes executable.
