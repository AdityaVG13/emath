# Phase 0 scope (emath CLI)

Mode: full (user asked to fix and commit locally).
Branch: current `main` only. No feature branch.
Workspace: this in-tree folder. No sibling directory.

Must not touch:
- JeffreySkills library
- Pre-existing dirty `Cargo.toml` / `emath-provenance` / `emath-search`
- Kernel, APFS, sqlite, graphdb, or parse/check latency work from other loops

In scope: `emath` CLI surfaces in `crates/emath-cli` (help, errors, flags, json, agent envelope).
Feature work is out of scope unless it is a required agent-facing contract surface
(`capabilities`, `robot-docs`, `--json` on read-side commands, mega-command).
