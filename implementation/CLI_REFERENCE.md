# CLI Reference (implemented surface)

`emath-cli` implements the commands below; `docs/P11_TOOLING_AND_DX.md`
tracks their status. Exit classes are stable: **0** ok, **1** refused or
admission/build diagnostics, **2** usage or io error. `--json` output is
deterministic sorted-key JSON via the in-tree writer.

## Semantic pipeline

| Command | Behavior |
| --- | --- |
| `emath check <file> [--json]` | parse + admission, no codegen; refuses with a typed `E-*` code on any diagnostic |
| `emath plan <file> [--json]` | admission + goal elaboration + deterministic native resolution plan (no artifact) |
| `emath planner <file> [--json] [--parametric]` | provider-registry planning: per-goal disposition. Refuses (exit 1) when any goal is unplanned; `--parametric` lifts missing operators to a provider trait |
| `emath build <file> --out <dir> [--verify] [--json]` | full pipeline: parse → admit → plan → generate → compile → artifact; `--verify` runs the generated tests |
| `emath run <file> [--out <dir>]` | build then execute the generated crate; library crates run their example tests |
| `emath test <file> [--out <dir>]` | build with `--verify`; generated crate with no tests is refused (E-TLT-012) |
| `emath bench <file>` | typed refusal E-TLT-004 (benchmark harness is Phase 4+) |
| `emath explain <file> [<symbol>]` | plan-level explanation of goals and plans |
| `emath diff <a.emath> <b.emath>` | content-id fingerprint comparison of parse-admitted sources |

## Tooling

| Command | Behavior |
| --- | --- |
| `emath new <name> [--out <dir>]` | deterministic project scaffold; refuses overwrite (E-TLT-011) |
| `emath fmt <file>` | canonical-form check via the lossless formatter (round-trip); stays a check in Phase 1 |
| `emath verify <artifact-dir>` | independent artifact re-verification |
| `emath inspect <dir>` | prints committed artifact manifests (E-TLT-005 if missing/none) |
| `emath doctor` | rustc/cargo/rustfmt/clippy presence checks |
| `emath vendor --out <dir>` | offline dependency lock snapshot (`forks/UPSTREAM_LOCK.json`); E-TLT-007 if lock missing; zero third-party deps |
| `emath provider list\|inspect <id>\|test <id>` | built-in provider descriptors; status table must agree with in-tree adapters |
| `emath artifact check <dir>` (`artifact battery`) | independent artifact checker; seeded negative-control battery |
| `emath fork status\|sync [--dry-run]` | upstream pin status; real sync refuses (E-TLT-006), dry-run allowed |

## Semantic Genesis family

| Command | Behavior |
| --- | --- |
| `emath parse --forest <file>` | bounded parse forest over the lexical/structural parse |
| `emath signature <file>` | signature inference of parses |
| `emath genesis <file> --out <dir>` | world interpretation + answer receipt (no invented `tested` authority; `keep: pareto N`) |
| `emath compile --parametric <file> --out <dir>` | compile a world via the parametric fallback into a generated crate |
| `emath world show` / `emath portfolio show` | inspect worlds / interpretation portfolio |
| `emath import modelica <file.mo> [--json]` | retain a Modelica subset source as foreign-model declarations (no rewrite) |
| `emath architecture` | print the neutral-IR architecture map |

## Agent envelope

| Command | Behavior |
| --- | --- |
| `emath agent check\|plan\|build <file> [--out <dir>]` | `emath.agent` over the same session/build paths; `build` defaults `--out` to `target/emath`; an agent cannot bypass admission, planning, or artifact checks |
| `emath help [<command>]` / `emath <command> --help` | full catalog, or one-command usage; unknown tokens print `did you mean` |
| `emath version` / `--version` / `-V` | crate version line (`emath <semver>`), no git SHA |

## LSP

`crates/emath-lsp` is a std-only deterministic LSP skeleton:
`initialize` (utf-8 position encoding), incremental text sync,
completion/hover, `publishDiagnostics` from the real admission path
(LSP and CLI agree), typed refusals `-32601`/`-32700`, and exit 0 only
after `shutdown` + `exit`.
