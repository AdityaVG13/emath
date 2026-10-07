# Security

This page describes what emath treats as untrusted, the defenses in
place today, the defenses planned for later stages, and how to report a
problem.

## What is untrusted

- `.emath` source files and package manifests.
- Generated Rust and anything a build executes, including build scripts.
- Saved checkpoints and run directories read back by `step`, `inspect`,
  and `dream resume`.
- Inputs supplied through `--set`, `--set-file`, and data files.
- Programs constructed at run time with `quote` during discovery.

## Defenses in place

- **Bounded execution.** The reference VM counts work in deterministic
  steps and stops with `budget_exhausted` at the limit. Call depth is
  capped, and deep recursion is a named fault, not a crash.
- **Fail-closed admission.** Parsing and admission refuse with a named
  diagnostic rather than guessing. An unknown command, flag, or import
  refuses.
- **No hidden code paths.** Mathematics is authored in `.emath`; the
  compiler does not dispatch on subject names.
- **Checked checkpoints.** An incompatible or corrupt checkpoint refuses
  to resume (exit code `5`).
- **Self-contained artifacts.** `emath build` emits a crate with the
  runtime embedded in its source. Only a function that uses
  `quote.cost` or `quote.world` also vendors the reference engine, with
  a pinned `Cargo.lock`.
- **Overwrite guards.** `emath new` and `emath keep` refuse to overwrite
  existing files unless asked.
- **Determinism.** Runs replay exactly, so a result can be reproduced and
  audited.

## Planned

These belong to later stages and are not implemented yet:

- Sandboxing for external providers and plugins.
- Content-locked package resolution for a package registry.
- Signed, content-addressed evidence bundles.

Native crates built by `emath build` do not enforce the reference VM's
work budget. Treat a generated crate like any other code you compile and
run.

## Reporting a vulnerability

Please do not open a public issue for a security problem, and do not
post exploit inputs publicly. Contact the author
([Aditya](https://x.com/adityavg13)) privately with a description and a
minimal reproduction. A dedicated reporting address and disclosure policy
will be published before the first public release.
