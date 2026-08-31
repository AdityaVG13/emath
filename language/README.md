# emath Language

This directory is the user-facing source of truth for the language.

| Path | Purpose |
|---|---|
| [`QUICKSTART.md`](QUICKSTART.md) | Build and run a first program |
| [`CAPABILITY.md`](CAPABILITY.md) | Current parse, run, and refusal matrix |
| [`reference/`](reference/README.md) | Normative syntax and semantics |
| [`grammar/`](grammar/README.md) | Machine-readable surface grammar |
| [`examples/`](examples/README.md) | Runnable teaching programs (indexed)
| [`templates/`](templates/README.md) | Project and declaration scaffolds |
| [`stdlib/`](stdlib/README.md) | Standard-library and provider contracts |
| [`NAMING.md`](NAMING.md) | Naming and diagnostic conventions |

When reference prose and grammar differ, the reference is normative.
`CAPABILITY.md` states whether a documented form computes or is refused.

Start with the quickstart, then the first four examples. Use the reference for exact syntax, semantics, and diagnostics.

A file may parse but still refuse semantic admission or execution. Such boundaries are documented explicitly; emath never substitutes an unintended interpretation.
