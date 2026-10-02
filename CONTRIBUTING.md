# Contributing to emath

Contributions are welcome. Before you start, please reach out to the
author ([Aditya](https://x.com/adityavg13)) so we can agree on the
direction.

## Where changes go

- **New mathematics** is an ordinary `.emath` module under
  [`language/modules/`](language/modules/README.md) (a general tool) or
  [`language/domains/`](language/domains/README.md) (a worked object of
  study). It does not need Rust changes.
- **Language changes** follow the agent brief in
  [`language/AGENTS.md`](language/AGENTS.md): update the reference
  chapter, the grammar, and [`CAPABILITY.md`](language/CAPABILITY.md)
  in the same change.
- **Compiler changes** in `crates/` add only general machinery: parsing,
  admission, the IR, the VM, and code generation. No domain-specific
  operations.

## Tests

- Every module carries authored `tests:`. Run them with
  `emath test FILE`.
- Rust tests live under [`tests/`](tests/README.md). Run the narrowest
  package that covers your change (`cargo test -p PACKAGE --test NAME`),
  not the whole workspace.
- A new test should fail before the change that makes it pass.

## Documentation

User-facing docs live in [`language/`](language/README.md). Keep them
current with every change that adds, changes, or refuses a capability.
