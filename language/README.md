# emath Language Assets

Normative reference, grammar, examples, templates, and standard-library
contracts for the emath language.

| Directory / File | Purpose |
|------------------|---------|
| [`QUICKSTART.md`](QUICKSTART.md) | **Start here** - zero to working program in 60 seconds. |
| [`CAPABILITY.md`](CAPABILITY.md) | **What works today** - single-source capability matrix (parses / runs / refused). |
| [`NAMING.md`](NAMING.md) | Naming conventions: types, builtins, namespaces, error codes. |
| [`MAINTAINING.md`](MAINTAINING.md) | Checklist for adding a language feature (the four-artifact rule). |
| [`reference/`](reference/README.md) | Normative semantic specification, 16 chapters; the index there carries the canonical chapter order. |
| [`grammar/`](grammar/README.md) | EBNF grammars: authoritative surface grammar and genesis addendum; authority and supersession notes in the grammar README. |
| [`examples/`](examples/README.md) | Cross-domain example programs, grouped by category, with a curated reading order. |
| [`templates/`](templates/README.md) | Project, declaration, and provider scaffolds. |
| [`stdlib/`](stdlib/README.md) | Standard library package catalog and provider contracts. |

**Authority:** when the reference and the grammar disagree, the reference
chapters in [`reference/`](reference/README.md) are normative; grammar files
are the machine-checkable surface model that follows them.

Start with [`CAPABILITY.md`](CAPABILITY.md) for what works today, then
[`reference/overview.md`](reference/overview.md) (Chapter 1), then
follow the chapter index.

The 16 chapters describe the whole language we are building. The compiler
does not implement that whole language yet. What you can write, check,
run, simulate, and compile to Rust today is the smaller working subset
in the **Implemented today** sections of chapters 1, 5, and 7, plus
[`examples/README.md`](examples/README.md).

A file can parse and still not run. That is expected. emath compiles
math you write; it does not decide whether the answer is the one you
wanted. Worlds and later adapters (including Lean) can attach stronger
evidence later. They are not the product yet.
