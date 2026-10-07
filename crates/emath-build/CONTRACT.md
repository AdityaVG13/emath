# emath-build CONTRACT

## Purpose and layer
- Host helpers for generated Rust crates, shared by `emath build`,
  `emath export`, and the emath-tui export and experiment hosts.
- Std-only: no first-party or third-party dependencies.

## Public API
- `generated_crate_target_dir(key)`: persistent Cargo target dir
  `$CWD/target/emath-cargo/<key>` (`/` and `:` in the key become `-`), so
  incremental rustc output survives across runs that wipe their staging
  directories.
- `write_generated_file(path, contents)`: writes only when the contents
  differ, preserving the mtime of identical files so Cargo can reuse its
  fingerprints in a stable output directory. A failed read falls back to
  writing. This is not a binary cache and does not promise cross-directory
  reuse or concurrent-writer isolation.
- `run_cargo_timed(command, timeout)`: runs a Cargo command under a
  wall-clock budget that covers spawn and stdout/stderr drain.

## Error model
- `run_cargo_timed` returns `Result<Output, String>` with concrete
  spawn/wait messages; an exceeded budget is `E-RES-120`, including when the
  parent exited but inherited output handles are still open.
- `write_generated_file` returns the underlying `std::io::Error`.

## Resource defaults
- Commands default to one build job and one libtest thread, even outside
  the repository. `CARGO_BUILD_JOBS` and `RUST_TEST_THREADS` inherit the
  caller environment; explicit per-command settings take precedence. These
  are concurrency defaults, not CPU or memory quotas.

## Cancellation behavior
- Nonblocking Unix socket pairs replace blocking reader threads, so no
  detached thread outlives a run. On timeout a live child's direct children
  are killed first (`pkill -P`), then the child, which is reaped. A reaped pid
  is never signalled. The child stays in the terminal's process group so
  Ctrl-C reaches Cargo and rustc.

## Unsafe boundary
- None: `#![forbid(unsafe_code)]`.

## Feature flags
- None.

## Conformance tests
- `tests/emath-build/tests/lib.rs`: live-child timeout, exited parent with
  inherited output, nested descendants, in-budget descendant output capture,
  resource-default overrides, ordinary completion, and mtime preservation for
  unchanged generated files.

## No-claim boundaries
- Bounded stdio requires Unix socket pairs; other hosts refuse before
  spawning. Descendant cleanup is direct children only, not recursive
  containment; reparented descendants may survive but cannot extend the
  output drain.
