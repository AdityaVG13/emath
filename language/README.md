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
`CAPABILITY.md` states what each documented form does today and which
worlds can run it.

Start with the quickstart, then the first four examples. Use the
reference for exact syntax, semantics, and diagnostics.

Nothing you write is refused at the door: everything enters the
language, and every answer comes back labeled with what it means
(`exact`, `approximate(±bound)`, `symbolic-only`, `hole-open`, `fault`).
Where a capability cannot compute something yet, the docs say so explicitly:
the response is a routed diagnosis pointing at the world
that can, never a silent guess. The governing doctrine lives in
[`../implementation/VISION.md`](../implementation/VISION.md).
