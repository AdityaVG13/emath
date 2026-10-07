# emath-cli CONTRACT

Constructor-layer CLI. This file is the live command contract. Historical
planner, lab, genesis, and `goal_met` text is not admission.

## Purpose and layer

- Host entry is `run(&[String]) -> CliExit`.
- Live commands: `check`, `run`, `step`, `inspect`, `verify`, `test`,
  `build`, `api`, plus `new`, `fmt`, `migrate`, `help`, `version`,
  `doctor`, `diff`.
- `emath simulate`, `plan`, `planner`, `eval`, `sweep`, `genesis`,
  `solve`, `search`, and other extracted tokens refuse `E-KIND-GONE`
  and name `emath run`. There is no `emath-lab` execution surface.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Completed constructor value or successful inspect/help |
| 2 | Syntax, type, or input admission failure |
| 3 | Unmet, partial, or suspended requested answer |
| 4 | Execution or backend fault |
| 5 | Incompatible or corrupt checkpoint |

A suspended run is 3. `inspect` may return 0 when it reads a suspended
checkpoint.

## Receipt

`emath run --json` prints `emath.constructor.v1` with orthogonal fields
`execution`, `fulfillment`, `representation`, `payload`, `evidence`,
`remaining`. It does not print `goal_met`.

## Invariants

- Constructor files are admitted as `emath object`, `emath function`,
  or `emath query` only.
- `check` consumes the diagnostics and package identity from one
  `CompilerSession` checking result, including constructor admission.
  File checking uses the source loaded into that session; it does not reread
  the main file or repeat admission for display, meaning identity or declared
  data digests. A later independent check loads new source. In-memory checking
  uses the same semantic authority.
  `check --json` reports the constructor MeaningID from that checked package,
  including ordered executable semantics, complete types and resolved imports.
  It does not hash only signatures or reread dependency files to derive it.
  Conformance: `build_cmd::check_reports_executable_meaning_changes`.
  `run` and `test` use this same authority and execute its retained prepared
  module, without loading imports again. Name, section, attribute and type
  refusals remain admission errors on all three commands. Each execution starts
  fresh state; entry-only runs do not execute the module's authored tests.
  Work budgets, carried world selection and checkpoint accounting remain the
  existing evaluator policies. Query body faults remain execution faults,
  not missing-entry refusals. `run --json` retains its structured admission
  refusal response.
  Conformance: `check_snapshot`, `diagnostic_location`, `test_row_honesty`.
- `keep` and `compare` consume the same checked preparation. `compare`
  evaluates each world/grid point with fresh state over one resolved module;
  declared inputs come from that callable, not a second syntax-only table.
  Answer claims still carry unchecked status, not proof authority.
  `keep` derives required imports from the graph that produced its Code and
  checks the complete proposed library before writing it. A failed admission
  leaves the existing library untouched. Existing idempotence, overwrite and
  transitive/private-import rules remain in force.
  Conformance: `keep_cmd`, `compare_cmd`, `diagnostic_location`.
- Semantic commands load the verified `language/` image. The image is
  constructor contracts, scalar carriers, and the generic ABI. Discovery
  never suggests removed aliases or default recipes.
- `emath migrate` is format-only. It is not a recipe translator.
- `emath api --search` returns constructor contracts and imported
  module exports.
- Checkpoint resume requires the constructor-layer ABI
  (`constructor-layer/scoped-continuation-abi`). Function and expression frames
  remain distinct after serialization; older ABIs and foreign images refuse
  resume. Unsupported ABIs also refuse inspection.
  Checkpoints carry their original source path, search roots and dependency
  bytes. `step` resolves only those captured imports, not changed files or
  a different current directory, and preserves them across further stops.

## Public types

Native build, public `admitted_tree`, stdin formatting and source
fingerprinting consume the semantic-session result and original bytes.
Lean translation uses checked syntax and retained import sources. Its
derived example and law checks are re-admitted against those sources,
not reloaded from disk while Lean builds. Translation, examples and universal
law certification retain distinct evidence levels and target refusals.
Independently checked export roots must agree on shared dependency bytes,
including transitive imports; conflicting snapshots refuse before any output
is written instead of mixing emath checks with a different Lean translation.
Lean law checks reuse one shared, pinned Mathlib package set. Cache preparation
runs in a stable setup workspace with `.lake/packages` linked to that set,
so dependency artifacts do not land in each export. A file lock serializes
setup, existing revisions/toolchain are checked before Lake can update them,
and warm prebuilt checks skip cache fetching. Only the required law imports
and their dependencies are requested. Unchanged generated files preserve
mtimes and Lake's incremental outputs. Recovery may build the small cache
executable, not the full Mathlib library; incompatible installations refuse.
Conformance: `export_lean`, including real laws/rejections and isolated
shared-setup, unchanged-output and compatibility regression.

- `CliExit` and `run(&[String])`.
- Constructor evaluation lives in `emath_exec_ir::constructor_layer`.
- Constructor emission lives in `emath_exec_ir::constructor_emir`.

## No-claim

- Completing a run is not a theorem.
- `verify` checks published artifact files against their manifests; it
  does not re-run or prove anything.
- Reusing one checked module is not an atomic filesystem snapshot of its whole
  import graph. Serialized checkpoints capture the graph that module resolved;
  they are not authenticated or guaranteed to come from one filesystem instant.
- A kept library still imports its dependencies as ordinary source. Retaining
  checked import provenance does not freeze those files, make publication
  atomic against external writers or certify later imported definitions.
