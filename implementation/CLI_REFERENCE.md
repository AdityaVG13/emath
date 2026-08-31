# CLI Reference (implemented surface)

`emath-cli` implements the commands below; `docs/P11_TOOLING_AND_DX.md`
tracks their status. Exit classes are stable: **0** ok, **1** refused or
admission/build diagnostics, **2** usage or io error. `--json` output is
deterministic sorted-key JSON via the in-tree writer.

## Semantic pipeline

| Command | Behavior |
| --- | --- |
| `emath check <file> [--verify-data] [--json]` | parse + admission, no codegen; refuses with a typed `E-*` code on any diagnostic (empty / comment-only source is `E-PKG-081`, not a vacuous admit); `--verify-data` (04 §5.2) re-hashes every `sha256` declared in InstrumentRun provenance against the data file (resolved relative to the source file) and refuses drift or an unreadable file as `E-OBS-HASH`; without the flag, provenance is declared, not verified; prints the effective honesty table (`honesty: units_profile <decl>=<level>`) when declarations carry `@units_profile`; `--json` always includes a `diagnostics` array of `{code, severity, message}` plus the `units_profiles` rows |
| `emath eval <file> [--world <name>] [--function NAME] [--set name=value] [--json]` | two lanes: a genesis-format reference file evaluates on the semantic VM (default world `free_symbolic`; `--world` selects an admitted world; `--json` is the `emath.eval-answer` envelope); a standard function-spec file executes an admitted `emath function` through the generic EMIR/reference-VM stack; `--set name=value` binds declared inputs (finite decimal scalar or `[vector]`; every input must be bound), `--function NAME` selects among several declarations, and plain `eval` runs the spec's own single worked example as the input oracle (or evaluates a zero-input function). The `--json` receipt is schema `emath.eval-function` v1 (`function`, `entrypoint`, `inputs_from`, `meaning_id`, `inputs{}`, `outputs{}`); typed refusals cover unsupported (`E-EVAL-001`), unknown named (`E-EVAL-002`), ambiguous (`E-EVAL-003`), missing input (`E-EVAL-004`), malformed/unknown/duplicate `--set` (`E-EVAL-005`), unsupported input type (`E-EVAL-006`), lowering/evaluation fault or failing example (`E-EVAL-007`), and `--world` misuse (`E-EVAL-008`). Missing files are `E-PKG-080` |
| `emath plan <file> [--json]` | admission + goal elaboration + deterministic native resolution plan (no artifact) |
| `emath planner <file> [--json] [--parametric]` | provider-registry planning: per-goal disposition. Refuses (exit 1) when any goal is unplanned; `--parametric` lifts missing operators to a provider trait |
| `emath build <file> --out <dir> [--verify] [--json]` | full pipeline: parse → admit → plan → generate → compile → artifact; `--verify` runs the generated tests |
| `emath run <file> [--out <dir>]` | build then execute the generated crate; library crates run their example tests |
| `emath test <file> [--out <dir>]` | build with `--verify`; generated crate with no tests is refused (E-TLT-012) |
| `emath bench <file>` | typed refusal E-TLT-004 (benchmark harness is Phase 4+) |
| `emath explain <file> [<symbol>] [--json]` | plan-level explanation of goals and plans; `--provenance` renders the binding-provenance DAG; `--show-defaults` prints the effective-defaults table (7 rows, each labeled with its source: language default / declaration attribute / planner default) plus one `units-profile: <decl>=<level>` row per declaration that overrides, `--json` emits the same under `defaults` + `declaration_overrides` |
| `emath diff <a.emath> <b.emath> [--json]` | content-id fingerprint comparison of parse-admitted sources |
| `emath simulate <file.emath> [--dt N] [--t0 N] [--t1 N] [--method euler\|rk4\|rk45] [--atol N] [--rtol N] [--dt-max N] [--event name=value] [--set name=value] [--json]` | integrate an admitted `emath model`; default is fixed-step classic RK4; `--set` binds scalars or `[vector]`/`[[matrix]]` literals; `--atol/--rtol` opt into adaptive RK45; `--event` locates one scalar crossing; missing files are `E-PKG-080`; `--json` includes diagnostic `code`s on admission refusal |

## Tooling

| Command | Behavior |
| --- | --- |
| `emath new <name> [--out <dir>]` | deterministic project scaffold; refuses overwrite (E-TLT-011) |
| `emath fmt <file>` | canonical-form check via the lossless formatter (round-trip); stays a check in Phase 1 |
| `emath migrate <file> [--fix] [--check] [--receipt <path>]` / `emath migrate --list-rules` | receipt-driven rewrites (05 §5): `--check` reports without rewriting; `--fix` applies formatter respells only after byte-identical MeaningId verification; the registry also classifies edition-major semantic corrections, which must receipt a checked before/after MeaningId delta; ambiguous semantic sites refuse as E-MIG-AMBIGUOUS-SITE with candidates; refusing source is E-MIG-SOURCE-REFUSES; receipt = canonical replay-stable `emath.migration-receipt v1` JSON |
| `emath verify <artifact-dir>` | independent artifact re-verification |
| `emath inspect <dir> [--json]` | prints committed artifact manifests (E-TLT-005 if missing/none) |
| `emath doctor [--json]` | rustc/cargo/rustfmt/clippy presence checks |
| `emath vendor --out <dir>` | offline dependency lock snapshot (`forks/UPSTREAM_LOCK.json`); E-TLT-007 if lock missing; zero third-party deps |
| `emath provider list\|inspect <id>\|test <id> [--json]` | built-in provider descriptors; status table must agree with in-tree adapters |
| `emath artifact check <dir>` (`artifact battery`) | independent artifact checker; seeded negative-control battery |
| `emath fork status\|sync [--dry-run] [--json]` | upstream pin status; real sync refuses (E-TLT-006), dry-run allowed |

## Semantic Genesis family

| Command | Behavior |
| --- | --- |
| `emath parse --forest <file>` | bounded parse forest over the lexical/structural parse |
| `emath signature <file>` | signature inference of parses |
| `emath genesis <file> --out <dir>` | world interpretation + answer receipt (no invented `tested` authority; `keep: pareto N`; G7 `evaluate`; `E-GEN-095` if a single answer would hide several kept worlds) |
| `emath compile --parametric <file> --out <dir> [--world LABEL]` | compile a world via the parametric fallback into a generated crate; `--world` selects one compiled world (`free_symbolic`, `Boolean_algebra`, `modular_numeric`); missing files are `E-PKG-080` |
| `emath world show` / `emath portfolio show` | inspect worlds / interpretation portfolio |
| `emath meaning list\|set\|unset\|explain` | project-local interpretation lock (`.emath/meaning.lock`); `set` refuses disqualified worlds (`E-LOCK-005`); drifted/tampered/malformed locks refuse (`E-LOCK-*`) and never silently fall back |
| `emath import modelica <file.mo> [--json]` | retain a Modelica subset source as foreign-model declarations (no rewrite) |
| `emath architecture [--json]` | print the neutral-IR architecture map |

## Agent envelope

| Command | Behavior |
| --- | --- |
| `emath agent check\|plan\|build\|triage <file> [--out <dir>]` | `emath.agent` over the same session/build paths; `triage` is the mega-command (doctor+check+plan); `build` defaults `--out` to `target/emath`; an agent cannot bypass admission, planning, or artifact checks |
| `emath help [<command>]` / `emath <command> --help` | full catalog, or one-command usage; unknown tokens print `did you mean` |
| `emath version` / `--version` / `-V` | crate version line (`emath <semver>`), no git SHA |
| `emath capabilities [--json]` | machine contract (always JSON): commands, exit codes, env vars |
| `emath robot-docs [guide]` | paste-ready agent handbook |

## LSP

`crates/emath-lsp` is a std-only deterministic LSP skeleton:
`initialize` (utf-8 position encoding), incremental text sync,
completion/hover, `publishDiagnostics` from the real admission path
(LSP and CLI agree), typed refusals `-32601`/`-32700`, and exit 0 only
after `shutdown` + `exit`.
