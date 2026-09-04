# Language Specification

Normative semantic specification of the emath language, in 18 chapters.
Chapters are canonically ordered; this index is the source of order.
Filenames are intentionally prefix-free, so listings sort alphabetically,
not semantically.

| # | Chapter | File |
|---|---------|------|
| 1 | emath Language Overview | [overview.md](overview.md) |
| 2 | Lexical, Layout and Source Rules | [lexical-layout-and-source.md](lexical-layout-and-source.md) |
| 3 | Packages, Modules and Imports | [packages-modules-and-imports.md](packages-modules-and-imports.md) |
| 4 | Declarations, Sections and Attributes | [declarations-sections-and-attributes.md](declarations-sections-and-attributes.md) |
| 5 | Types, Units, Shapes and Domains | [types-units-shapes-and-domains.md](types-units-shapes-and-domains.md) |
| 6 | Constructors and Valid-State Semantics | [constructors-and-valid-state.md](constructors-and-valid-state.md) |
| 7 | Expressions, Equations, State and Events | [expressions-equations-state-and-events.md](expressions-equations-state-and-events.md) |
| 8 | Custom Kinds, Schema and Lowering | [custom-kinds-schema-and-lowering.md](custom-kinds-schema-and-lowering.md) |
| 9 | Goals, Requests, Strategies and Resolution | [goals-requests-strategies-and-resolution.md](goals-requests-strategies-and-resolution.md) |
| 10 | Evidence, Budgets, Compilation and Host Sections | [evidence-budgets-compile-and-host.md](evidence-budgets-compile-and-host.md) |
| 11 | Canonicalization, Identity and Serialization | [canonicalization-identity-and-serialization.md](canonicalization-identity-and-serialization.md) |
| 12 | Diagnostics and Tooling Contract | [diagnostics-and-tooling-contract.md](diagnostics-and-tooling-contract.md) |
| 13 | Standard Library Constitution | [standard-library-constitution.md](standard-library-constitution.md) |
| 14 | Rust Interop and Generation | [rust-interop-and-generation.md](rust-interop-and-generation.md) |
| 15 | Total Compilation Protocol | [total-compilation-protocol.md](total-compilation-protocol.md) |
| 16 | Language Acceptance | [language-acceptance.md](language-acceptance.md) |
| 17 | House Style | [house-style.md](house-style.md) |
| 18 | Using emath as a Mathematical Probe Lab | [probe-lab-workflow.md](probe-lab-workflow.md) |

Each chapter carries its canonical number in its H1 title
(e.g. `# Chapter 7: Expressions, Equations, State and Events`).
