# Error and Refusal Code Registry

| Prefix | Area | Examples |
|---|---|---|
| `E-SYN-*` | syntax/layout | bad indentation, unclosed delimiter, BOM rejection (E-SYN-113), confusable identifier lint (E-SYN-114), NFC violation refused (E-SYN-115), attribute-arg subset (E-SYN-117), unknown attribute refused (E-SYN-118), named call argument refused (E-SYN-121), head-args mixed with `inputs:`/`outputs:` (E-SYN-122), head-args on a stateful or non-function declaration (E-SYN-123) |
| `E-PKG-*` | package/lock | missing dependency, checksum mismatch, experimental without capability (E-PKG-064), unknown capability key (E-PKG-065) |
| `E-NAME-*` | names/visibility | ambiguous import, private access |
| `E-KIND-*` | custom kind | missing section, recursive expansion |
| `E-SEC-*` | sections | section outside the Phase 1 subset |
| `E-TYPE-*` | type/refinement | mismatch, unsatisfied bound |
| `E-UNIT-*` | units | dimensional mismatch, affine misuse |
| `E-SHAPE-*` | shapes/layout | rank mismatch, invalid broadcast |
| `E-DOM-*` | domains | branch/domain violation |
| `E-NOTATION-*` | notation declarations | glyph not lexable as one identifier (E-NOTATION-GLYPH), reserved core glyph (E-NOTATION-RESERVED), ambiguous glyph→target (E-NOTATION-AMBIG), precedence below the custom floor (E-NOTATION-PRECEDENCE) |
| `E-NUM-*` | numeric models | unknown model, precision/error-limit, silent Real map |
| `E-CTOR-*` | constructors | missing assignment, invariant bypass |
| `E-GOAL-*` | requests/planning | no eligible plan, weakened requirement |
| `E-PROV-*` | providers/adapters | version drift, unsupported subset |
| `E-EVID-*` | evidence/checkers | stale/tampered/wrong-goal certificate |
| `E-CODEGEN-*` | backend/artifact | unsupported lowering, manifest mismatch |
| `E-RES-*` | resources | budget, memory, cancellation |
| `E-HOST-*` | host/lab | invalid experiment manifest (003), engine policy (015), ABI mismatch (001), incomparable experiment (008), self-comparison refused (016) |
| `E-TLT-*` | tooling/CLI | bench harness absent (E-TLT-004), manifest missing (E-TLT-005), network sync disabled (E-TLT-006), lock unreadable (E-TLT-007), invalid package name (E-TLT-010), scaffold overwrite refused (E-TLT-011), unknown provider id (E-TLT-016) |
| `E-SCHEMA-*` | schema registry | unknown schema name (E-SCHEMA-001) |
| `E-PROV-001/002` | adapter seam | upstream fork version drift; uncategorized patch set |
| `E-REG-*` | package/provider registry | unknown package (020), lock mismatch (021), yanked (022), revoked (023), unsatisfied constraint (024), kind schema missing (030), provider capability missing (031) |
| `E-PLG-*` | plugin SDK | component runtime absent (001), sandbox violation (002), capability outside allowed set / none declared (003), interface core mismatch (004), empty/control-bearing plugin id (005) |
| `E-LOCK-*` | meaning lock | malformed lock (001), unknown schema version (002), tampered fingerprint/lock_id (003), drifted/inadmissible locked world (004), disqualified set (005), unknown candidate (006) |

Codes are stable identifiers. Messages can improve without changing code meaning. A code is never repurposed.

## Issued codes (implementation evidence)

Tooling (`crates/emath-cli/src/tooling_cmd.rs`):

- `E-TLT-004` — `bench` requires the Phase 4+ benchmark harness.
- `E-TLT-005` — `inspect`/`verify`: artifact state or manifest missing.
- `E-TLT-006` — `fork sync` refuses network/source sync (offline-first);
  `--dry-run` allowed.
- `E-TLT-007` — `vendor`/`fork`: upstream lock file unreadable.
- `E-TLT-010` — `new`: invalid package name.
- `E-TLT-011` — `new`: refuses to overwrite an existing scaffold.
- `E-TLT-012` — `build --verify` / `test`: generated crate has no
  `#[test]` tests; refused so an empty test surface is never reported as
  passing (add a `tests:` section to the spec).
- `E-TLT-013` — `provider test`: no in-CLI negative-control battery
  exists, so no "suite: ok" is printed; the command refuses and points at
  the workspace test suites.
- `E-TLT-016` — unknown provider id (`provider inspect|test`); distinct
  from `E-PROV-001` (adapter seam version drift).

Genesis (`crates/emath-cli/src/genesis_cmd.rs`):

- `E-GEN-090` — a `matrix`/`graph` world is deferred in the genesis
  admission lane (not silently admitted).
- `E-GEN-091` — another admitted world label is deferred in the genesis
  admission lane.
- `E-GEN-092` — `semantic genesis --world <label>` named a world outside
  the admitted set; refused.
- `E-GEN-093` — `keep: pareto 0` keeps no candidates; the genesis run is
  refused instead of emitting an empty or winner-less portfolio receipt.
- `E-GEN-094` — `compile --parametric`: a world declares operator
  semantics the label-based emission cannot honor, so the unused WorldIr
  is refused (SURF-0008) instead of silently dropped.
- `E-GEN-095` — genesis would have collapsed several kept worlds to a
  single answer without `answer: return interpretation_portfolio` or a
  user lock; refused instead of `kept.first()`.

Meaning lock (`crates/emath-portfolio/src/meaning_lock.rs`,
`crates/emath-cli/src/meaning_cmd.rs`, genesis/eval/compile resolution):

- `E-LOCK-001` — malformed meaning lock (truncated JSON, unknown or
  missing fields, unreadable file); never best-effort parsing.
- `E-LOCK-002` — unknown `schema_version`; refuse rather than adapt.
- `E-LOCK-003` — stored `lock_id` does not match the identity body
  (tampered fingerprint or other identity field).
- `E-LOCK-004` — locked world drifted or inadmissible; hint to re-open
  the portfolio with `emath meaning unset` (never a silent fallback).
- `E-LOCK-005` — `meaning set` targeted a checker/guard-disqualified
  world; diagnostic includes the disqualification ledger entry.
- `E-LOCK-006` — `meaning set` named a fingerprint that is not in the
  current portfolio.

HIR migration (`crates/emath-hir/src/migrate.rs`):

- `E-MIGR-001` — declaration carried a legacy bootstrap schema tag; the
  stable edition must regenerate it (V3/Phase1 examples gate).
- `E-MIGR-002` — section renamed by migration (`request:` →
  `goals:` with a nested block; `input:` → `inputs:`; `output:` →
  `outputs:`).
- `E-MIGR-003` — inline constructor lifted into a `constructors:`
  section by migration.

Resources (`crates/emath-plan/src/planner.rs`, `crates/emath-build/src/lib.rs`,
`crates/emath-cli/src/tooling_cmd.rs`, `crates/emath-holes/src/synth.rs`):

- `E-RES-100` — planner: plan DAG exceeds the `max_nodes` budget;
exhausted outcome with a resume continuation instead of a silently
oversized artifact.
- `E-RES-110` — hole synthesis: carrier exceeds `MAX_CARRIER_SIZE`;
table space (`carrier^(carrier²)`) would be unbounded.
- `E-RES-111` — hole synthesis with an empty law set refused: every
  table would vacuously "satisfy" it, so the outcome is a typed refusal,
  never an invented `Contradictory`.
- `E-RES-120` — cargo (verify/test/run) exceeded the wall-clock timeout
and was killed; a session never blocks forever on a generated crate.

Registry (`crates/emath-registry/src/lib.rs`):

- `E-REG-020` — unknown package in index snapshot.
- `E-REG-021` — lock snapshot fingerprint or pin does not verify.
- `E-REG-022` — pinned version is yanked.
- `E-REG-023` — pinned version is revoked.
- `E-REG-024` — no usable version satisfies the constraint.
- `E-REG-030` — package does not serve the required kind schema.
- `E-REG-031` — package does not serve the required provider capability.

Plugin SDK (`crates/emath-plugin-sdk/src/lib.rs`):

- `E-PLG-001` — component runtime absent in the Phase 1 subset (execution
  contract; typed refusal).
- `E-PLG-002` — sandbox violation: network without permission, or
  wrong/missing fuel (untrusted plugin without positive fuel, and
  `execute` requiring positive fuel under every trust class — claiming
  `Trust::Local` can never skip the fuel gate).
- `E-PLG-003` — capability outside the sandbox's allowed set, or no
  capabilities declared.
- `E-PLG-004` — plugin interface core does not match the SDK's.
- `E-PLG-005` — plugin id is empty or contains ASCII control characters.

Requests (`crates/emath-goal/src/lib.rs`):

- `E-GOAL-041` — `evaluate` request missing a target in `<...>`.
- `E-GOAL-042` — `evaluate` request without `produce rust.library`, or
  with a produce target outside the Phase 1 subset (the request is
  refused, not silently accepted).
- `E-GOAL-043` — request kind outside the Phase 1 subset
  (supported: `evaluate`, `differentiate`, `benchmark`).
- `E-GOAL-044` — `differentiate` request missing `wrt [names]`.
- `E-GOAL-045` — `benchmark` request missing `against <path>`.

Admission (`crates/emath-sema/src/admit.rs`):

- `E-SEC-101` — section outside the Phase 1 subset (`inputs`,
  `outputs`, `state`, `definitions`, `equations`, `constructors`,
  `goals`, `exports`, `tests`, `compile`, `about`, `evidence`,
  `host`); refused instead of silently dropped.
  The pre-`goals:` spellings `request:` and `requests:` refuse with a
  migration hint to `goals:`.

Session (`crates/emath-sema/src/session.rs`):

- `E-PKG-080` — `check`/`plan` on a source file that was never loaded;
  refused with a typed diagnostic instead of returning an empty-source
  plan that passes admission. `eval`/`compile --parametric`/`simulate`
  use the same code when the source path cannot be read, instead of an
  uncoded `cannot read` line.

- `E-PKG-081` — `check`/`plan` on a file that parses to zero items
  (empty, comment-only, or whitespace-only). Refused instead of a
  vacuous admit: `eval`/`simulate` already refuse empty source, and
  K-8 treats one-error-one-OK as a hard failure.

- `E-SYN-120` — `check`/`plan`/`parse_text` refuse when no source-parser
  backend is installed: a host must wire
  `emath_syntax::install_source_parser` once per process (CLI and LSP
  do this at startup). The refusal is typed; never a silent empty parse.

Codegen (`crates/emath-macro/src/lib.rs`, `crates/emath-builder/src/lib.rs`):

- `E-CODEGEN-011` — `emath!` expansion input does not parse as a syntax
  package; the macro fails compilation with a typed message instead of
  expanding to nothing.
- `E-CODEGEN-012` — builder `compile:` spec outside the Phase 1
  rust/library subset; the model is refused, never adapted.

Generated-crate profiles (`crates/emath-rust-ir/src/profiles.rs`,
checked on the build path via `CrateProfile::Library`):

- `E-CODEGEN-002` — generated module contains `unsafe` while the profile
  is safe (every profile bans unsafe); the build fails before any
  artifact is staged.
- `E-CODEGEN-003` — profile name not recognized
  (`parse_profile("...")`); unknown profiles are typed refusals, never
  a silent default.
- `E-CODEGEN-004` — a public item in the generated module lacks a source
  anchor; source-map round-trips are incomplete, so the build fails.

Syntax (`crates/emath-syntax/src/lexer.rs`, `crates/emath-syntax/src/parser.rs`):

- `E-SYN-101` — malformed argument list in a section head; the whole
  statement is refused instead of recording `args: None`.
- `E-SYN-121` — a named call argument (`f(name = value)` in call position)
  is outside the Phase 1 subset; the call is refused instead of stripping
  the name and silently losing the binding.
- `E-SYN-122` — declaration head arguments (`name(args)`) mixed with an
  `inputs:` section, or a head `-> T` mixed with an `outputs:` section;
  the two spellings are identity-equivalent and cannot both appear.
- `E-SYN-123` — declaration head arguments on a stateful declaration
  (`state:` / `constructors:`) or on a non-`function` kind (policy,
  world, builder, method). Head-args are only admitted on stateless
  `emath function`.
- `E-SYN-115` — an identifier contains a combining mark (U+0300–U+036F);
  such a spelling is canonically non-NFC by construction and cannot be
  verified without a Unicode table, so the identifier is refused instead
  of entering the pipeline with an identity the toolchain cannot check.
- `E-SYN-117` — an item attribute argument is outside the
  identifier/string-literal/bracket-list subset (`@experimental(deep)`,
  `@capabilities(x = 1)`); the attribute is refused instead of dropping
  the argument and silently changing meaning.
- `E-SYN-118` — unknown item attribute (`@bogus`): the front-end does
  not understand it, so it is refused instead of silently discarded.
- `E-SYN-107` — `wrt` (derivative-variable list) attached to a binder
  that is not `derivative`, `solve`, or `optimize`; the outer form is
  refused instead of silently dropping the `wrt` clause.
- `E-TYPE-110` — function types (`fn(params) -> T`) are outside the Phase 1
  subset; refused instead of recording a lossy `Path(["fn"])`.
- `E-TYPE-111` — type aliases (`type X = T`) are outside the Phase 1 subset;
  refused instead of dropping the right-hand side.
- `E-TYPE-112` — generic `extern operator` declarations are outside the
  Phase 1 subset; refused instead of discarding the generic parameters.

Notation (`crates/emath-syntax/src/parser.rs`):

- `E-NOTATION-RESERVED` — a notation glyph or alias shadows a core
  token (N3: `+ - * / ^ == != < <= > >= and or not = := -> => :: . ..
  ..= ?`); the core vocabulary cannot be rebound, so the declaration is
  refused instead of silently overloading an operator.
- `E-NOTATION-GLYPH` — a glyph or alias does not lex as a single
  identifier (e.g. `!`, `++`); such a spelling could never appear in an
  expression without re-lexing as other syntax, so it is refused at the
  declaration instead of registering a dead operator.
- `E-NOTATION-AMBIG` — the same glyph is bound to two different target
  paths in one scope (N4); the glyph must map to exactly one canonical
  operator, so the later binding is refused instead of resolved by
  precedence or fixity.
- `E-NOTATION-PRECEDENCE` — the declared integer precedence is below
  the custom-operator floor `CUSTOM_OP_MIN_PRECEDENCE` (11); tiers
  1–10 belong to the core lexical ladder and a lower declaration would
  parse without ever binding, so it is refused up front.

Schema registry (`crates/emath-schema/src/registry.rs`):

- `E-SCHEMA-001` — unknown schema name: the thirteen canonical
  `emath.<name>` registry entries are fixed; any other name is a
  typed refusal instead of a guessed document.

Providers (`crates/emath-adapter-rumoca/src/provider.rs`,
`crates/emath-adapter-rumoca/src/import.rs`):

- `E-PROV-230` — missing parameter value, or unknown variable during
  evaluation.
- `E-PROV-234` — unresolvable or mis-targeted initial condition.
- `E-PROV-235` — simulation plan shape refused: fewer than two states,
  non-finite or non-positive `dt`, or a state without a derivative row.
- `E-PROV-236` — assignment to a non-state variable, or an equation whose
  left-hand side is not a variable or `der` reference; the simulation fails
  instead of silently dropping the assignment.
- `E-PROV-237` — provider model refused by the validation gate before any lowering or simulation runs; untrusted provider output never becomes `Resolved`.
- `E-PROV-238` — simulation plan with more than two states refused: the
  fixture-time recorder represents at most two states and never silently
  drops extra states.
- `E-PROV-240` — malformed Modelica subset import: missing model name or
  missing `end` terminator.
- `E-PROV-241` — Modelica subset import refuses a construct that is not in
  the semantic mapping table (or is classified unsupported); the import is
  rejected instead of retained with the construct dropped.

- `E-PROV-030` — provider-supplied IR refused before splicing/generation:
  integer literal not finite in strict-f64, a variable name that is not a
  safe Rust identifier, or a non-scalar linear-algebra node under the
  scalar backends; the adapter emits a typed refusal instead of invalid
  code or a numeric placeholder stub.
- `E-PROV-031` — requested capability outside the advertised inventory:
  a backend or accelerator target with no admitted subset
  (`capability::select_backend`, `backends::admit_target`).
- `E-PROV-033` — linear-algebra shape mismatch in `map_linear` (e.g.
  `dot` on non-vectors, `matvec` with incompatible columns/rows).
- `E-PROV-501` — provider registration rejected because the descriptor failed
  self-validation (duplicate capability names, empty representations, empty
  semantic subsets); untrusted descriptors never enter the registry.
- `E-PROV-515` — provider excluded because its declared exactness cannot serve
  the goal's exactness policy (bounded/checked-numeric/any-explicit goals never
  fall back to estimate-only or undeclared exactness).


Rumoca dynamic-model subset (`crates/emath-adapter-rumoca/src/subset.rs`):

- `E-KIND-310` — variable role outside the dynamic-model subset (only
  `Parameter`/`State` are in scope).
- `E-KIND-311` — component construct outside the dynamic-model subset.
- `E-KIND-312` — discrete event outside the Phase 1 subset; only basic
  continuous events are accepted (distinct from `E-KIND-310`, which is
  about variable roles).

Adapter seams (`crates/emath-adapter-dew/src/seam.rs`):

- `E-PROV-001` — upstream fork version drift between the packed lock and
  the adapter's expected baseline; the seam refuses to map a mismatched
  world.
- `E-PROV-002` — uncategorized upstream patch set blocks the seam; the
  patch table must be curated before mapping.

Evidence (`crates/emath-evidence/src`, `crates/emath-checker/src`):

- `E-EVID-101` — artifact file content does not hash to its declared
  content id.
- `E-EVID-102` — artifact identity does not recompute from the manifest
  body under the independent checker.
- `E-EVID-103` — evidence bundle is not scoped to the frozen
  goal/source package.
- `E-EVID-110` — source-map schema is not `emath.source-map`.
- `E-EVID-111` — provider lock record missing or not matching the
  manifest's provider dependency.
- `E-EVID-112` — source map does not reference the manifest's source
  package (distinct from `E-EVID-110`, which is about the schema).
- `E-EVID-403` — checker contract registers with an empty `admits` list;
  a contract that admits nothing is refused.
- `E-EVID-507` — certifier output refused by the corpus gate:
  known-unsound pattern or non-UTF-8 payload.

Kinds (`crates/emath-schema/src/load.rs`, `crates/emath-sema/src/admit.rs`,
`crates/emath-hir/src/open.rs`):

- `E-KIND-010` — function declarations cannot have state or constructors in
  Phase 1 (semantic admission in `admit.rs`; the builder embeds the same
  predicate verbatim). One predicate, one code; the HIR manifest schema
  violations use their own codes (`E-KIND-011`, `E-KIND-016`).
- `E-KIND-011` — kind schema is missing a
  `RepeatPolicy::ExactlyOne` section (`SectionManifest::check` and
  semantic admission in `admit.rs`). `inputs:` and `outputs:` are
  `AtMostOne` on the core function schema, so omitting either is not
  this refusal.
- `E-KIND-016` — HIR section manifest declares a section that is not part
  of the kind schema (`SectionViolationReason::UnknownSection`). Split from
  `E-KIND-010` so unknown-section (schema conformance) and
  constructors/state (Phase 1 subset) are never the same code. Live
  `request:` / `requests:` use this code with a `goals:` migration hint.
- `E-KIND-032` — schema load refuses a kind whose expansion would recurse.
- `E-KIND-100` — `emath custom <K> as kind` is refused by admission: the
  Phase 1 grammar only accepts `item_kind == "kind"`, so the documented
  custom-kind form dies with a typed refusal instead of a silent
  non-`KindDef` node.

Units (`crates/emath-ir/src/units.rs`,
`crates/emath-adapter-rumoca/src/structural.rs`):

- `E-UNIT-100` — unknown variable in dimensional analysis. A missing
  variable is refused, never treated as dimensionless.
- `E-UNIT-101` — dimension mismatch: two operands that must share
  dimensions do not (unit compatibility in `units.rs`; sums and equations
  in the rumoca structural check). The predicate is identical at every
  site, so one code serves all of them.
- `E-UNIT-102` — affine unit misuse: multiplying or dividing by a unit
  with a non-zero offset.

Names/constructors/units (`crates/emath-sema/src/admit.rs`):

- `E-NAME-020` — duplicate symbol/field in a package or structural model
  (also emitted by `emath-hir` notation and the rumoca structural check).
- `E-NAME-022` — duplicate declaration name in one admission: two
  declarations with the same name would collide in generated Rust, so the
  second is refused instead of silently overwriting the first.
- `E-NAME-023` — declaration named `_`: `_` cannot be escaped into a Rust
  type name, so the declaration is refused up front. Also reused for a
  declared output that has no definition.
- `E-NAME-025` — continuous `emath model` is missing a
  `derivative(state)` / `der(state)` equation for a declared state
  field. Distinct from `E-NAME-023` so `_` names and missing rates are
  never the same code.
- `E-NAME-024` — declaration name differs from an already-seen name only
  by confusable glyphs (e.g. Latin `o` vs Cyrillic `о`, per the
  `confusable_fold` seed map); the second public declaration is refused
  so the API never presents two visually indistinguishable names.
Dimensional mismatch (e.g. adding `Length` to `Duration`) is `E-UNIT-101`.
The old `E-UNIT-001` spelling is retired and is not emitted.
- `E-UNIT-104` — unknown unit name in a quantity literal (`1 furlong`)
  or catalog lookup.
- `E-UNIT-105` — ill-formed unit constructor (`Per<>` arity, affine
  inverse).
- `E-CTOR-030` — missing state assignment in a constructor (also emitted
  by the builder for generated-code invariants).

Types (`crates/emath-ir/src/type_system.rs`):

- `E-TYPE-312` — `unify` refuses two refined types whose predicates differ,
  or whose discharge is different (e.g. a statically discharged refinement
  against a construct-time one); the discharge is never silently dropped.

Provider API (`crates/emath-provider-api/src`):

- `E-PROV-510` — registration refused: claim contradicts advertised table isolation, or advertised isolation not allowed by the registry policy.
- `E-PROV-518` — provider registry refuses a duplicate provider id (silent overwrite would replace isolation/capabilities under the same lookup key).
- `E-PROV-524` — constellation register attempts a maturity above P0 without proofs; census entries start at P0 and climb via promote.
- `E-PROV-525` — constellation register refuses a duplicate provider id (a second P0 register must not silently demote a promoted entry).

Lab (`crates/emath-lab-core/src`, `crates/emath-rust-ir/src/host.rs`):

- `E-HOST-001` — rust-ir host gate: toolchain/ABI incompatible with what
  the crate declares (typed refusal, never a silent fallback).
- `E-HOST-003` — invalid experiment manifest: wrong schema, duplicate
  partitions/metrics/kill-rule ids, non-positive thresholds, malformed
  canonical JSON.
- `E-HOST-004` — manifest not frozen before measurement, or baseline and
  candidate are not distinct artifacts.
- `E-HOST-005` — promotion quality gate blocked the candidate (default
  gate failure code; `E-EVID-*` check codes take precedence).
- `E-HOST-006` — insufficient evidence: too few samples after
  warmup/cull, or an empty sample set given to `percentile` /
  `percentile_f64` (typed refusal, never an index underflow).
- `E-HOST-007` — metric ratio regressed beyond the policy floor/ceiling
  (median, p99, memory, energy).
- `E-HOST-008` — incomparable inputs (paired analysis under an unpaired
  protocol, zero-duration samples, no paired comparison).
- `E-HOST-010` — runtime drift detected; the candidate was demoted
  instead of promoted.
- `E-HOST-011` — receipt/drift record does not verify against the
  recorded hash.
- `E-HOST-012` — invalid statistical protocol configuration (repetitions
  below the minimum, warmup below repetitions, MAD trim not positive and
  finite, field does not fit `usize`); distinct from manifest validation.
- `E-HOST-013` — invalid canary routing configuration (canary outcome
  without a positive canary interval).
- `E-HOST-014` — invalid drift band tolerance (non-finite or
  non-positive).
- `E-HOST-015` — invalid engine policy (median ratio bounds, regression
  and memory ceilings outside `[1.0, inf)`).
- `E-HOST-016` — refuse self-comparison: the subject and oracle of a
  comparison must be distinct engine identities (honest comparator
  separation; never a silent self-comparison).

LSP (`crates/emath-lsp/src`):

- LSP/JSON-RPC uses the standard `-32601` (method not found) and `-32700`
  (parse error) codes; message text is deterministic.

Units/shapes/domains (`crates/emath-ir`, `crates/emath-adapter-rumoca`):

- `E-UNIT-103` — event condition is not dimensionless (Rumoca structural
  validation); distinct from `E-UNIT-102` (affine multiplication/division
  misuse in `units.rs`).

### Codegen (`crates/emath-build`)

- `E-CODEGEN-005` — git/registry dependency denied by dependency policy
  (forbidden source).
- `E-CODEGEN-006` — dependency used but not declared.
- `E-CODEGEN-007` — version conflict among mapped dependencies.
- `E-CODEGEN-008` — locked build script cannot satisfy a dependency that
  needs network access while locked (offline-first honored).
- `E-CODEGEN-009` — absolute-path dependency refused (absolute-path
  leak).
- `E-CODEGEN-010` — generated build script wrote outside `$OUT_DIR`
  (isolation refusal).
- `E-CODEGEN-051` — compile target outside the Phase 1 subset (`rust`
  only).
- `E-CODEGEN-052` — compile profile outside the subset (`library`).
Unknown numeric profile names are `E-NUM-001`. The old `E-CODEGEN-053`
spelling is retired and is not emitted.
- `E-CODEGEN-054` — safety profile outside the subset
  (`forbid-unsafe`).
- `E-CODEGEN-055` — `unresolved <disposition>` outside the subset
  (native only).

### Constructors (`crates/emath-builder`, `crates/emath-sema/src/admit.rs`)

- `E-CTOR-031` — policy declarations require a `constructors:` section
  with a public `new`.
- `E-CTOR-032` — `require`/`ensure` must be a Boolean expression.
- `E-CTOR-033` — default value reads a target that is not a state field.
- `E-CTOR-034` — multiple constructors named `new`, or duplicate
  constructor parameter.
- `E-CTOR-035` — duplicate assignment for a state field.
- `E-CTOR-036` — Phase 1 admits exactly one public `new` constructor;
  the primary constructor must be named `new`.
- `E-CTOR-037` — delegating constructor targets an unknown constructor.
- `E-CTOR-038` — delegating constructor assigns state directly
  (refused).
- `E-CTOR-039` — default supplied for an undeclared parameter.

### Domains (`crates/emath-ir`)

- `E-DOM-001` — a value falls outside its declared domain.
- `E-DOM-002` — ill-formed domain declaration (inverted or non-finite
  interval; compile `domain lo..hi`).

### Numeric models (`crates/emath-ir/src/numeric.rs`, `crates/emath-sema/src/admit.rs`)

- `E-NUM-001` — unknown numeric model name. Known models: `strict-f64`
  (Phase 1 default when `numeric:` is omitted) and explicit
  `interval-f64`. Other names are refused, never silently coerced.
- `E-NUM-002` — precision demand no selected model can honor (zero bits,
  or more significand bits than the model provides).
- `E-NUM-003` — error-limit demand no selected model can honor (strict-f64
  cannot certify a bound tighter than machine epsilon; interval-f64
  cannot honor a zero/exact bound; non-finite limits refused).
- `E-NUM-004` — `representation Real` without a named model. `Real` is
  not mapped to `f64` without profile evidence.

### Evidence and artifacts (`crates/emath-checker`, `crates/emath-evidence`)

- `E-EVID-104` — certificate for a claim is stale (fresh-until passed).
- `E-EVID-105` — required artifact file is missing.
- `E-EVID-106` — claim class is not supported by any checker.
- `E-EVID-107` — resolved claim has no checker bound.
- `E-EVID-108` — manifest schema is not `emath.artifact`.
- `E-EVID-109` — manifest declares no files, or declares a path with no
  file present.
- `E-EVID-113` — required/declared artifact path is a symlink (refused).
- `E-EVID-114` — artifact document or declared file is not valid UTF-8.
- `E-EVID-201` — claim language stronger than the available evidence
  (downgrade suggested).
- `E-EVID-301` — translation mismatch (no equivalence witness).
- `E-EVID-302` — witness cannot be independently verified.
- `E-EVID-401` — unknown certificate contract kind/version.
- `E-EVID-402` — duplicate versioned checker contract refused.
- `E-EVID-404` — incomplete computation cannot become resolved evidence.
- `E-EVID-405` — assumption already registered under a different class.
- `E-EVID-501` — unknown evidence record id.
- `E-EVID-502` — duplicate append-only revocation marker.
- `E-EVID-503` — content-identity mismatch (bootstrap identity).
- `E-EVID-504` — double supersession (append-only conflict).
- `E-EVID-505` — stale record refused for promotion (revalidation
  required).
- `E-EVID-506` — proof provider unavailable (optional-path refusal).

### Genesis (`crates/emath-cli/src/genesis_cmd.rs`)

- `E-GEN-080` — genesis parse refused.
- `E-GEN-081` — genesis body expression is empty.
- `E-GEN-082` — reference body is not unique (ambiguity reported).
- `E-GEN-083` — signature inference refused.
- `E-GEN-084` — inferred signature rejects the term.

### Goals (`crates/emath-goal`, `crates/emath-plan`)

- `E-GOAL-011` — goal schema requires at least one output.
- `E-GOAL-012` — budget limit without a work unit.
- `E-GOAL-013` — duplicate output name in the goal schema.
- `E-GOAL-201` — no eligible plan; an exhausted budget yields a
  continuation/diagnostic per the goal's fallback policy.

### Host/lab (`crates/emath-lab-core`, `crates/emath-rust-ir`)

- `E-HOST-002` — host-binding layer failure beyond ABI version mismatch
  (family code owned by `emath-rust-ir`; no construction site in HEAD).

### Custom kinds (`crates/emath-schema`, `crates/emath-sema`)

- `E-KIND-001` — declaration kind is not supported by this front-end.
- `E-KIND-002` — declaration could not be admitted.
- `E-KIND-003` — one honest refusal, no contradiction.
- `E-KIND-012` — malformed directive or unknown token (repeat policy).
- `E-KIND-013` — duplicate section spec.
- `E-KIND-014` — duplicate default for a section.
- `E-KIND-015` — default or predicate references an undeclared section.
- `E-KIND-020` — invalid lowering operation.
- `E-KIND-021` — rename source is not a declared (core) section.
- `E-KIND-022` — lowering bound exceeded (op limit or recursive hoist).
- `E-KIND-030` — unknown kind at schema load.
- `E-KIND-031` — incompatible schema version refused at load.

### Names/visibility (`crates/emath-sema`, `crates/emath-hir`)

- `E-NAME-021` — Phase 1 exports must be `public` (also: empty operator
  symbol refused in notation mount).
- `E-NAME-026` — `given` name is not an input or constructor parameter.
  (`expect` is optional: a `given`-only example is a worked example.
  The former "has no expect" refusal on this code is removed.)
- `E-NAME-027` — policy example must supply the constructor parameter
  via `given`.

### Packages (`crates/emath-schema`, `crates/emath-sema`)

- `E-PKG-020` — schema lock checksum mismatch.
- `E-PKG-050` — external file import outside the front-end subset
  (library-path imports only).
- `E-PKG-064` — `@experimental` on an item while the source file does
  not declare the `experimental-syntax` capability; experimental syntax
  never compiles silently in a stable package (ELP experimental lane).
- `E-PKG-065` — unknown capability key in `@capabilities(...)`
  (declared: `experimental-syntax`); the key is refused instead of
  being silently dropped.

### Providers (`crates/emath-adapter-rumoca`, `crates/emath-provider-api`, `crates/emath-ir`, `crates/emath-plan`)

- `E-PROV-210` — connection cardinality violation or unknown port
  (structural census).
- `E-PROV-220` — underdetermined: no equation produces the expected
  variable.
- `E-PROV-221` — algebraic cycle refused (tearing candidates reported).
- `E-PROV-222` — multiple equations produce the same identifier.
- `E-PROV-223` — equation has no single unknown on its left-hand side.
- `E-PROV-231` — non-finite value during evaluation.
- `E-PROV-232` — assignment to an unknown derivative.
- `E-PROV-233` — division by zero during evaluation.
- `E-PROV-300` — provider diagnostic mapped with provider code and
  component preserved.
- `E-PROV-310` — source-map loss: no emath span for a component (never
  silently dropped).
- `E-PROV-401` — provider seam version drift refused.
- `E-PROV-402` — provider seam identity mismatch refused.
- `E-PROV-410` — bit-identical primitive conversion contract
  (float64/i64).
- `E-PROV-411` — value-conserving affine unit conversion contract (degC
  ↔ kelvin).
- `E-PROV-412` — index-conserving sparse matrix conversion (csc-matrix).
- `E-PROV-502` — duplicate capability name in a descriptor.
- `E-PROV-503` — capability declares no representations or no semantic
  subset.
- `E-PROV-511` — lock-mismatched provider registration refused.
- `E-PROV-512` — capability filtered: goal kind/subset exclusion.
- `E-PROV-513` — capability filtered: evidence/checker exclusion.
- `E-PROV-514` — capability filtered: no capability serves the target
  family.
- `E-PROV-516` — capability filtered: determinism exclusion.
- `E-PROV-517` — no conversion path between representations (or cycle
  refused).
- `E-PROV-521` — unknown provider id in the provider constellation.
- `E-PROV-522` — promotion target is not above the provider's current
  level.
- `E-PROV-523` — promotion of a provider blocked: selection criteria
  unmet.

### Shapes (`crates/emath-ir`)

- `E-SHAPE-001` — matrix product requires rank-2 operands.
- `E-SHAPE-002` — inner extents differ.
- `E-SHAPE-003` — slice bounds invalid (start exceeds end).
- `E-SHAPE-004` — declared shape is not well-formed (empty tensor rank,
  zero extent, or `Zero` symbolic extent).
- `E-SHAPE-005` — elementwise shape mismatch: vector/matrix add or sub
  extents differ, or a matrix literal has ragged rows.
- `E-SHAPE-006` — index rank or index type is wrong (`v[i, j]` on a
  vector, non-numeric subscript). Out-of-range evaluation is an
  interpreter fault, not this code.

### Syntax/layout (`crates/emath-syntax`, `crates/emath-sema`, `crates/emath-hir`, `crates/emath-genesis`)

- `E-SYN-100` — inconsistent indentation: dedent does not match an
  enclosing block.
- `E-SYN-102` — expected `)` to close a parameter list.
- `E-SYN-103` — duplicate section refused.
- `E-SYN-105` — unterminated string literal.
- `E-SYN-106` — nesting limit exceeded.
- `E-SYN-108` — token limit exceeded (source too complex).
- `E-SYN-109` — invalid string escape.
- `E-SYN-110` — expected a package path after `package`.
- `E-SYN-111` — expected `:` after the declaration head.
- `E-SYN-112` — expected an indented block.
- `E-SYN-113` — UTF-8 BOM rejected at the start of a source file.
- `E-SYN-114` — non-ASCII identifier refused (confusable lookalike
  hazard).
- `E-SYN-116` — source exceeds `Limits::max_source_bytes`; the lexer
  refuses before scanning so huge inputs cannot burn O(n) work.
- `E-SYN-201` — malformed genesis header (expected `emath custom
  Name:`).
- `E-SYN-202` — clause outside `construct meaning:` (or unsupported
  clause).
- `E-SYN-203` — malformed `keep:` clause (expected `pareto <u32>`).
- `E-SYN-204` — malformed `answer:` clause (expected `return <name>`).
- `E-SYN-205` — genesis file has no `body:` section.
- `E-SYN-206` — genesis file has no `answer:` section.
- `E-SYN-207` — genesis source exceeds the byte limit.
- `E-SYN-208` — unexpected content line in a genesis file.
- `E-SYN-209` — duplicate `body:`/`answer:` section.
- `E-SYN-210` — no complete parse of the reference body (holes
  reported).
- `E-SYN-211` — reference body is ambiguous (multiple structural
  parses).

### Types (`crates/emath-sema`, `crates/emath-ir`, `crates/emath-core`)

- `E-TYPE-001` — unknown type.
- `E-TYPE-002` — unknown variable.
- `E-TYPE-003` — unknown function.
- `E-TYPE-010` — unsupported type, or an implicit / mass-matrix
  left-hand side (`m * derivative(v) = rhs`) outside the explicit ODE
  subset.
- `E-TYPE-011` — non-finite constant refused under the strict-f64
  policy.
- `E-TYPE-012` — argument arity/type mismatch for the named function.
  Unit mismatches name the dimensions (`duration` vs `length`), never a
  Debug dump of `Infer::Unit`.
- `E-TYPE-101` — derivative target is not a state variable.
- `E-TYPE-102` — initial condition target is not a state variable.
- `E-TYPE-103` — state must not appear as a plain equation target.
- `E-TYPE-310` — numeric family code (documented in
  `crates/emath-ir/src/numeric.rs`; no construction site in HEAD yet).
- `E-TYPE-311` — cannot promote two numeric types without exact-width
  loss.
- `E-TYPE-313` — type variable bound to conflicting types.
- `E-TYPE-314` — occurs check: a type variable escapes into its own
  binding.

### Meaning lock (`crates/emath-portfolio/src/meaning_lock.rs`, `crates/emath-cli`)

Issued stories for `E-LOCK-001` through `E-LOCK-006` live in the
Meaning lock subsection of the issued-codes list above. The ADR-001
falsifier still holds: a drifted or tampered lock never silently falls
back to another world.

## Completeness annex: every issued code

Generated by `scripts/dump_error_codes.py` from the production crates;
regenerated as the registry changes. The workspace test
`crates/emath-hir/tests/registry_complete.rs` enforces that every emitted
code appears here (emitted ⊆ documented).

Emissions: **271 unique codes** from 244 rust files.
Not yet documented at generation time: **0**.

| Code | Emitting files | Context |
|---|---|---|
| `E-CODEGEN-002` | crates/emath-build/src/lib.rs<br>crates/emath-rust-backend/src/lib.rs<br>crates/emath-rust-ir/src/profiles.rs | `E-CODEGEN-002` |
| `E-CODEGEN-003` | crates/emath-rust-ir/src/profiles.rs | `library`<br>`E-CODEGEN-003` |
| `E-CODEGEN-004` | crates/emath-build/src/lib.rs<br>crates/emath-rust-backend/src/lib.rs<br>crates/emath-rust-ir/src/profiles.rs<br>crates/emath-rust-ir/src/render.rs | `E-CODEGEN-004` |
| `E-CODEGEN-005` | crates/emath-build/src/deps.rs | `git dependency `{}` denied by policy`<br>`registry dependency `{}` denied by policy` |
| `E-CODEGEN-006` | crates/emath-build/src/deps.rs | `undeclared dependency: {}` |
| `E-CODEGEN-007` | crates/emath-build/src/deps.rs | `E-CODEGEN-007` |
| `E-CODEGEN-008` | crates/emath-build/src/script.rs | `locked build script cannot satisfy: {error}`<br>`E-CODEGEN-008` |
| `E-CODEGEN-009` | crates/emath-build/src/deps.rs | `E-CODEGEN-009` |
| `E-CODEGEN-010` | crates/emath-build/src/script.rs | `E-CODEGEN-010` |
| `E-CODEGEN-011` | crates/emath-builder/src/lib.rs<br>crates/emath-macro/src/lib.rs | `E-CODEGEN-011`<br>`E-CODEGEN-011: {first}` |
| `E-CODEGEN-012` | crates/emath-builder/src/lib.rs | `compile spec `{}/{}` outside Phase 1 subset (E-CODEGEN-012)` |
| `E-CODEGEN-051` | crates/emath-sema/src/admit/sections.rs | `E-CODEGEN-051` |
| `E-CODEGEN-052` | crates/emath-sema/src/admit/sections.rs | `E-CODEGEN-052` |
| `E-CODEGEN-054` | crates/emath-sema/src/admit/sections.rs | `E-CODEGEN-054` |
| `E-CODEGEN-055` | crates/emath-sema/src/admit/sections.rs | `E-CODEGEN-055` |
| `E-CTOR-030` | crates/emath-builder/src/lib.rs<br>crates/emath-sema/src/admit/declaration.rs | `missing state assignment for `{}` (E-CTOR-030)`<br>`missing state assignment for `{}`` |
| `E-CTOR-031` | crates/emath-builder/src/lib.rs<br>crates/emath-sema/src/admit/declaration.rs | `E-CTOR-031` |
| `E-CTOR-032` | crates/emath-builder/src/lib.rs<br>crates/emath-sema/src/admit/lowering.rs | ``require` must be a Boolean expression (E-CTOR-032)`<br>``ensure` must be a Boolean expression (E-CTOR-032)` |
| `E-CTOR-033` | crates/emath-builder/src/lib.rs<br>crates/emath-sema/src/admit/sections.rs | `a default value cannot read `state.{target}` (E-CTOR-033)`<br>``{target}` is not a state field (E-CTOR-033)` |
| `E-CTOR-034` | crates/emath-builder/src/lib.rs<br>crates/emath-sema/src/admit/sections.rs | `multiple constructors named `new` (E-CTOR-034)`<br>`duplicate constructor parameter `{param}` (E-CTOR-034)` |
| `E-CTOR-035` | crates/emath-builder/src/lib.rs<br>crates/emath-sema/src/admit/sections.rs | `duplicate assignment for state field `{target}` (E-CTOR-035)`<br>`duplicate assignment for state field `{name}`` |
| `E-CTOR-036` | crates/emath-builder/src/lib.rs<br>crates/emath-sema/src/admit/declaration.rs | `the primary constructor must be named `new` (E-CTOR-036)`<br>`E-CTOR-036` |
| `E-CTOR-037` | crates/emath-builder/src/lib.rs | `constructor `{name}` delegates to unknown `{target}` (E-CTOR-037)` |
| `E-CTOR-038` | crates/emath-builder/src/lib.rs | `delegating constructor `{name}` cannot assign state directly (E-CTOR-038)` |
| `E-CTOR-039` | crates/emath-builder/src/lib.rs | `default for undeclared parameter `{target}` (E-CTOR-039)` |
| `E-DOM-001` | crates/emath-ir/src/domains.rs | `{name} value {value} outside domain {self}` |
| `E-DOM-002` | crates/emath-ir/src/domains.rs<br>crates/emath-sema/src/admit/lowering/helpers.rs<br>crates/emath-sema/src/admit/sections.rs | `ill-formed domain interval [{low}, {high}]`<br>`E-DOM-002` |
| `E-EVID-101` | crates/emath-checker/src/artifact_check.rs<br>crates/emath-checker/src/negative.rs<br>crates/emath-evidence/src/lib.rs | `content of {path} does not hash to its declared id`<br>`E-EVID-101` |
| `E-EVID-102` | crates/emath-artifact/src/lib.rs<br>crates/emath-checker/src/artifact_check.rs | `E-EVID-102` |
| `E-EVID-103` | crates/emath-build/src/lib.rs<br>crates/emath-checker/src/artifact_check.rs<br>crates/emath-checker/src/negative.rs | `E-EVID-103: goal requires {} but native build delivers only {}{}`<br>`E-EVID-103` |
| `E-EVID-104` | crates/emath-checker/src/artifact_check.rs<br>crates/emath-checker/src/negative.rs | `E-EVID-104` |
| `E-EVID-105` | crates/emath-checker/src/artifact_check.rs<br>crates/emath-checker/src/negative.rs<br>crates/emath-cli/src/lib.rs | `required artifact file missing: {path}`<br>`required artifact path missing: {path}` |
| `E-EVID-106` | crates/emath-checker/src/artifact_check.rs<br>crates/emath-checker/src/negative.rs | `E-EVID-106` |
| `E-EVID-107` | crates/emath-checker/src/artifact_check.rs | `resolved claim {} has no checker` |
| `E-EVID-108` | crates/emath-artifact/src/lib.rs<br>crates/emath-checker/src/artifact_check.rs | `E-EVID-108`<br>`emath/artifact-manifest.json does not conform to emath.artifact: {error}` |
| `E-EVID-109` | crates/emath-checker/src/artifact_check.rs | `E-EVID-109`<br>`manifest declares {path} but no such file exists` |
| `E-EVID-110` | crates/emath-checker/src/artifact_check.rs | `E-EVID-110` |
| `E-EVID-111` | crates/emath-checker/src/artifact_check.rs<br>crates/emath-evidence/src/lib.rs | `provider {} has no lock record`<br>`E-EVID-111` |
| `E-EVID-112` | crates/emath-checker/src/artifact_check.rs | `E-EVID-112` |
| `E-EVID-113` | crates/emath-checker/src/artifact_check.rs | `required artifact path is a symlink: {path}`<br>`declared artifact path is a symlink: {path}` |
| `E-EVID-114` | crates/emath-checker/src/artifact_check.rs<br>crates/emath-cli/src/tooling_cmd.rs | `artifact document is not valid UTF-8: {path}`<br>`declared artifact file is not valid UTF-8: {path}` |
| `E-EVID-201` | crates/emath-checker/src/claimlint.rs<br>crates/emath-evidence/src/lib.rs | `E-EVID-201` |
| `E-EVID-301` | crates/emath-checker/src/negative.rs<br>crates/emath-checker/src/translation.rs<br>crates/emath-evidence/src/lib.rs | `E-EVID-301` |
| `E-EVID-302` | crates/emath-checker/src/translation.rs<br>crates/emath-evidence/src/lib.rs | `E-EVID-302` |
| `E-EVID-401` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/registry.rs | `unknown certificate contract {} v{version}` |
| `E-EVID-402` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/registry.rs | `E-EVID-402` |
| `E-EVID-403` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/registry.rs | `E-EVID-403` |
| `E-EVID-404` | crates/emath-evidence/src/ir.rs<br>crates/emath-evidence/src/lib.rs | `E-EVID-404` |
| `E-EVID-405` | crates/emath-evidence/src/ledger.rs<br>crates/emath-evidence/src/lib.rs | `E-EVID-405` |
| `E-EVID-501` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/revalidation.rs<br>crates/emath-evidence/src/store.rs | `unknown evidence record {id}` |
| `E-EVID-502` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/store.rs | `record {id} is already revoked (append-only)` |
| `E-EVID-503` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/store.rs | `E-EVID-503` |
| `E-EVID-504` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/store.rs | `record {old} is already superseded (append-only)`<br>`record {old} cannot supersede itself` |
| `E-EVID-505` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/revalidation.rs | `record {id} is stale and requires revalidation` |
| `E-EVID-506` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/proof.rs | `E-EVID-506` |
| `E-EVID-507` | crates/emath-evidence/src/certify.rs<br>crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/proof.rs | `E-EVID-507` |
| `E-GEN-080` | crates/emath-cli/src/genesis_cmd.rs | `E-GEN-080: genesis parse refused: {detail}` |
| `E-GEN-081` | crates/emath-cli/src/genesis_cmd.rs | `E-GEN-081: genesis body expression is empty` |
| `E-GEN-082` | crates/emath-cli/src/genesis_cmd.rs | `E-GEN-082: reference body is not unique: ambiguity {}` |
| `E-GEN-083` | crates/emath-cli/src/genesis_cmd.rs | `E-GEN-083: signature inference refused: {detail}` |
| `E-GEN-084` | crates/emath-cli/src/genesis_cmd.rs | `E-GEN-084: inferred signature rejects term: {error:?}` |
| `E-GEN-090` | crates/emath-cli/src/genesis_cmd.rs | `E-GEN-090` |
| `E-GEN-091` | crates/emath-cli/src/genesis_cmd.rs | `E-GEN-091` |
| `E-GEN-092` | crates/emath-cli/src/eval_cmd.rs<br>crates/emath-cli/src/genesis_cmd.rs<br>crates/emath-cli/src/meaning_cmd.rs | `error: E-GEN-092: unknown world `{name}``<br>`error: E-GEN-092: unknown world `{label}`` |
| `E-GEN-093` | crates/emath-cli/src/genesis_cmd.rs<br>crates/emath-portfolio/src/meaning_lock.rs | `error: E-GEN-093: `keep: pareto 0` keeps no candidates` |
| `E-GEN-094` | crates/emath-cli/src/genesis_cmd.rs<br>crates/emath-world-codegen-rust/src/lib.rs | `error: E-GEN-094: CSA baseline evaluation failed on a total world`<br>`E-GEN-094` |
| `E-GEN-095` | crates/emath-cli/src/genesis_cmd.rs | `interpretation_portfolio`<br>`error: E-GEN-095: ambiguous portfolio: lock a world or request `answer: return interpretation_portfolio`` |
| `E-GOAL-011` | crates/emath-goal/src/schema.rs | `E-GOAL-011` |
| `E-GOAL-012` | crates/emath-goal/src/schema.rs | `E-GOAL-012` |
| `E-GOAL-013` | crates/emath-goal/src/schema.rs | `E-GOAL-013` |
| `E-GOAL-041` | crates/emath-sema/src/session.rs | `E-GOAL-041` |
| `E-GOAL-042` | crates/emath-sema/src/session.rs | `E-GOAL-042` |
| `E-GOAL-043` | crates/emath-sema/src/session.rs | `E-GOAL-043` |
| `E-GOAL-044` | crates/emath-sema/src/session.rs | `E-GOAL-044` |
| `E-GOAL-045` | crates/emath-sema/src/session.rs | `E-GOAL-045` |
| `E-GOAL-201` | crates/emath-plan/src/planner.rs | `E-GOAL-201`<br>`E-GOAL-201: no eligible plan` |
| `E-HOST-001` | crates/emath-rust-ir/src/host.rs | `host trait version `{version}` is not major.minor.patch`<br>`E-HOST-001` |
| `E-HOST-002` | crates/emath-rust-ir/src/host.rs |  |
| `E-HOST-003` | crates/emath-lab-core/src/error.rs<br>crates/emath-lab-core/src/manifest.rs<br>crates/emath-lab-core/src/stats.rs | `E-HOST-003`<br>`manifest JSON is invalid: {error}` |
| `E-HOST-004` | crates/emath-lab-core/src/manifest.rs | `E-HOST-004` |
| `E-HOST-005` | crates/emath-lab-core/src/candidate.rs<br>crates/emath-lab-core/src/gate.rs<br>crates/emath-lab-core/src/promotion.rs<br>crates/emath-lab-core/src/selector.rs | `E-HOST-005`<br>`E-HOST-005: evidence missing for {metric}` |
| `E-HOST-006` | crates/emath-lab-core/src/measure.rs<br>crates/emath-lab-core/src/promotion.rs<br>crates/emath-lab-core/src/stats.rs | `no raw samples for metric {}`<br>`E-HOST-006` |
| `E-HOST-007` | crates/emath-lab-core/src/promotion.rs | `E-HOST-007`<br>`E-HOST-007: median ratio {median_ratio} below floor` |
| `E-HOST-008` | crates/emath-lab-core/src/adversarial.rs<br>crates/emath-lab-core/src/promotion.rs<br>crates/emath-lab-core/src/stats.rs | `incomparable experiment: {} ({})`<br>`E-HOST-008` |
| `E-HOST-010` | crates/emath-lab-core/src/drift.rs<br>crates/emath-lab-core/src/lib.rs<br>crates/emath-lab-core/src/promotion.rs<br>crates/emath-lab-core/src/supervisor.rs | `E-HOST-010`<br>`E-HOST-010: {} drift in {}: observed {}, expected {} (tolerance {})` |
| `E-HOST-011` | crates/emath-lab-core/src/error.rs<br>crates/emath-lab-core/src/receipt.rs | `receipt manifest does not parse: {}`<br>`E-HOST-011` |
| `E-HOST-012` | crates/emath-lab-core/src/stats.rs | `E-HOST-012` |
| `E-HOST-013` | crates/emath-lab-core/src/selector.rs | `E-HOST-013` |
| `E-HOST-014` | crates/emath-lab-core/src/drift.rs | `E-HOST-014` |
| `E-HOST-015` | crates/emath-lab-core/src/promotion.rs | `E-HOST-015` |
| `E-HOST-016` | crates/emath-lab-core/src/identity.rs | `E-HOST-016` |
| `E-KIND-001` | crates/emath-sema/src/admit/sections_meta.rs<br>crates/emath-sema/src/recognition.rs | `declaration kind `{item_kind}` is not supported by this front-end`<br>`E-KIND-001` |
| `E-KIND-002` | crates/emath-sema/src/admit/sections_meta.rs | `E-KIND-002` |
| `E-KIND-003` | crates/emath-sema/src/recognition.rs | `E-KIND-003` |
| `E-KIND-010` | crates/emath-builder/src/lib.rs<br>crates/emath-hir/tests/registry_complete.rs<br>crates/emath-sema/src/admit/declaration.rs<br>crates/emath-sema/src/admit/equations.rs | `function declarations cannot have constructors in this subphase (E-KIND-010)`<br>`E-KIND-010` |
| `E-KIND-011` | crates/emath-hir/src/open.rs<br>crates/emath-sema/src/admit/declaration.rs | `kind `{}` requires section `{name}``<br>`E-KIND-011` |
| `E-KIND-012` | crates/emath-schema/src/lang.rs | `E-KIND-012`<br>`unknown repeat policy `{other}`` |
| `E-KIND-013` | crates/emath-schema/src/lang.rs | `duplicate section spec `{name}`` |
| `E-KIND-014` | crates/emath-schema/src/lang.rs | `duplicate default for section `{section}``<br>`E-KIND-014` |
| `E-KIND-015` | crates/emath-schema/src/lang.rs | `default for undeclared section `{section}``<br>`predicate references undeclared section `{section}`` |
| `E-KIND-016` | crates/emath-hir/src/open.rs<br>crates/emath-hir/tests/registry_complete.rs | `E-KIND-016`<br>`E-KIND-016 must be in the registry` |
| `E-KIND-020` | crates/emath-schema/src/lower.rs | `E-KIND-020` |
| `E-KIND-021` | crates/emath-schema/src/lower.rs | `E-KIND-021`<br>`rename source `{from}` is not a declared section` |
| `E-KIND-022` | crates/emath-schema/src/lower.rs | `lowering program exceeds {MAX_LOWER_OPS} ops`<br>`recursive hoist into `{into}`` |
| `E-KIND-030` | crates/emath-schema/src/load.rs | `E-KIND-030` |
| `E-KIND-031` | crates/emath-schema/src/load.rs | `E-KIND-031` |
| `E-KIND-032` | crates/emath-schema/src/load.rs | `E-KIND-032` |
| `E-KIND-100` | crates/emath-sema/src/admit/sections_meta.rs | `E-KIND-100` |
| `E-KIND-310` | crates/emath-adapter-rumoca/src/subset.rs | `E-KIND-310` |
| `E-KIND-311` | crates/emath-adapter-rumoca/src/subset.rs | `E-KIND-311` |
| `E-KIND-312` | crates/emath-adapter-rumoca/src/subset.rs | `E-KIND-312` |
| `E-LOCK-001` | crates/emath-cli/src/meaning_cmd.rs<br>crates/emath-portfolio/src/meaning_lock.rs | `error: E-LOCK-001: --cap must be an integer >= 1`<br>`error: E-LOCK-001: --declaration must be 16 hex digits` |
| `E-LOCK-002` | crates/emath-portfolio/src/meaning_lock.rs | `.emath`<br>`E-LOCK-002` |
| `E-LOCK-003` | crates/emath-portfolio/src/meaning_lock.rs | `E-LOCK-003` |
| `E-LOCK-004` | crates/emath-cli/src/eval_cmd.rs<br>crates/emath-cli/src/genesis_cmd.rs<br>crates/emath-portfolio/src/meaning_lock.rs | `error: E-LOCK-004: --world `{wanted}` disagrees with locked fingerprint {:016x}; re-open the portfolio with `e`<br>`error: E-LOCK-004: :world `{label}` disagrees with locked `{locked_name}` ({:016x}); re-open the portfolio wit` |
| `E-LOCK-005` | crates/emath-portfolio/src/meaning_lock.rs | `E-LOCK-005` |
| `E-LOCK-006` | crates/emath-cli/src/meaning_cmd.rs<br>crates/emath-portfolio/src/meaning_lock.rs | `error: E-LOCK-006: no lock entry for {} {}`<br>`E-LOCK-006` |
| `E-MIGR-001` | crates/emath-hir/src/migrate.rs | `E-MIGR-001` |
| `E-MIGR-002` | crates/emath-hir/src/migrate.rs | `E-MIGR-002` |
| `E-MIGR-003` | crates/emath-hir/src/migrate.rs | `E-MIGR-003` |
| `E-NAME-020` | crates/emath-adapter-rumoca/src/conformance.rs<br>crates/emath-adapter-rumoca/src/structural.rs<br>crates/emath-hir/src/notation.rs<br>crates/emath-sema/src/admit.rs | `E-NAME-020`<br>`duplicate variable `{}`` |
| `E-NAME-021` | crates/emath-hir/src/notation.rs<br>crates/emath-sema/src/admit/declaration.rs | `E-NAME-021`<br>``{}` is unary and cannot be infix` |
| `E-NAME-022` | crates/emath-hir/src/notation.rs<br>crates/emath-sema/src/admit/sections_meta.rs<br>crates/emath-wasm/src/lib.rs | `E-NAME-022`<br>`duplicate declaration name `{}`` |
| `E-NAME-023` | crates/emath-sema/src/admit/declaration.rs<br>crates/emath-sema/src/admit/sections_meta.rs<br>crates/emath-wasm/src/lib.rs | `output `{}` has no definition`<br>`_` |
| `E-NAME-024` | crates/emath-builder/src/lib.rs<br>crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/sections_meta.rs | `derived field `{name}` is not an output (E-NAME-024)`<br>`E-NAME-024` |
| `E-NAME-025` | crates/emath-sema/src/admit/declaration.rs | `state `{}` has no `derivative({})` equation` |
| `E-NAME-026` | crates/emath-sema/src/admit/declaration.rs | `E-NAME-026` |
| `E-NAME-027` | crates/emath-sema/src/admit/declaration.rs | `E-NAME-027` |
| `E-NUM-001` | crates/emath-ir/src/numeric.rs | `E-NUM-001` |
| `E-NUM-002` | crates/emath-ir/src/numeric.rs<br>crates/emath-sema/src/admit/sections.rs | `E-NUM-002`<br>`precision demand `{value_text}` is not a bit count` |
| `E-NUM-003` | crates/emath-ir/src/numeric.rs<br>crates/emath-sema/src/admit/sections.rs | `error-limit `{max_abs_error}` is not a finite non-negative bound`<br>`E-NUM-003` |
| `E-NUM-004` | crates/emath-sema/src/admit/sections.rs | `E-NUM-004` |
| `E-PKG-020` | crates/emath-schema/src/load.rs | `E-PKG-020` |
| `E-PKG-050` | crates/emath-sema/src/admit/sections_meta.rs<br>crates/emath-sema/src/recognition.rs | `E-PKG-050`<br>`custom` |
| `E-PKG-064` | crates/emath-sema/src/recognition.rs | `E-PKG-064` |
| `E-PKG-065` | crates/emath-sema/src/recognition.rs | `E-PKG-065` |
| `E-PKG-080` | crates/emath-cli/src/lib.rs<br>crates/emath-cli/src/genesis_cmd.rs<br>crates/emath-cli/src/simulate_cmd.rs<br>crates/emath-sema/src/session.rs | `E-PKG-080` |
| `E-PKG-081` | crates/emath-sema/src/admit/sections_meta.rs | `source has no declarations` |
| `E-PKG-081` | crates/emath-sema/src/admit/sections_meta.rs | `source has no declarations` |
| `E-PLG-001` | crates/emath-plugin-sdk/src/lib.rs | `E-PLG-001` |
| `E-PLG-002` | crates/emath-plugin-sdk/src/lib.rs | `E-PLG-002` |
| `E-PLG-003` | crates/emath-plugin-sdk/src/lib.rs | `plugin `{}` declares no capabilities`<br>`E-PLG-003` |
| `E-PLG-004` | crates/emath-plugin-sdk/src/lib.rs | `E-PLG-004` |
| `E-PLG-005` | crates/emath-plugin-sdk/src/lib.rs | `E-PLG-005` |
| `E-PROV-001` | crates/emath-adapter-dew/src/seam.rs | `E-PROV-001` |
| `E-PROV-002` | crates/emath-adapter-dew/src/seam.rs | `E-PROV-002` |
| `E-PROV-030` | crates/emath-adapter-dew/src/backends.rs<br>crates/emath-adapter-dew/src/dexpr.rs<br>crates/emath-adapter-dew/src/lib.rs<br>crates/emath-adapter-dew/src/mapping.rs | `E-PROV-030: generated Rust fragment fails the syntax sanity gate`<br>`E-PROV-030: integer literal `{text}` is not a finite f64` |
| `E-PROV-031` | crates/emath-adapter-dew/src/backends.rs<br>crates/emath-adapter-dew/src/capability.rs | `E-PROV-031: accelerator target `{}` has no admitted subset`<br>`E-PROV-031: backend `{}` is outside the Dew capability inventory` |
| `E-PROV-033` | crates/emath-adapter-dew/src/dexpr.rs | `E-PROV-033` |
| `E-PROV-210` | crates/emath-adapter-rumoca/src/structural.rs | `E-PROV-210` |
| `E-PROV-220` | crates/emath-adapter-rumoca/src/lower.rs | `underdetermined: no equation produces {expected}`<br>`underdetermined: no equation produces `{variable}`` |
| `E-PROV-221` | crates/emath-adapter-rumoca/src/lower.rs | `E-PROV-221` |
| `E-PROV-222` | crates/emath-adapter-rumoca/src/lower.rs | `multiple equations produce `{identifier}`` |
| `E-PROV-223` | crates/emath-adapter-rumoca/src/lower.rs | `E-PROV-223` |
| `E-PROV-230` | crates/emath-adapter-rumoca/src/provider.rs | `missing parameter value for `{parameter}``<br>`unknown variable `{name}` during evaluation` |
| `E-PROV-231` | crates/emath-adapter-rumoca/src/provider.rs | `E-PROV-231` |
| `E-PROV-232` | crates/emath-adapter-rumoca/src/provider.rs | `assignment to unknown derivative `der({state})``<br>`unknown derivative `der({name})` during evaluation` |
| `E-PROV-233` | crates/emath-adapter-rumoca/src/provider.rs | `E-PROV-233` |
| `E-PROV-234` | crates/emath-adapter-rumoca/src/provider.rs | `unresolvable initial value `{value}` for `{target}``<br>`initial condition targets unknown state `{target}`` |
| `E-PROV-235` | crates/emath-adapter-rumoca/src/provider.rs | `E-PROV-235`<br>`invalid time step `dt={}`` |
| `E-PROV-236` | crates/emath-adapter-rumoca/src/provider.rs | `E-PROV-236` |
| `E-PROV-237` | crates/emath-adapter-rumoca/src/provider.rs | `E-PROV-237` |
| `E-PROV-238` | crates/emath-adapter-rumoca/src/provider.rs | `E-PROV-238` |
| `E-PROV-240` | crates/emath-adapter-rumoca/src/import.rs | `E-PROV-240`<br>`model `{name}` has no `end` terminator` |
| `E-PROV-241` | crates/emath-adapter-rumoca/src/import.rs | `construct `{keyword}` is not in the mapping table`<br>`unsupported construct `{construct}` in model `{name}`` |
| `E-PROV-300` | crates/emath-adapter-rumoca/src/diagnostics.rs | `E-PROV-300` |
| `E-PROV-310` | crates/emath-adapter-rumoca/src/diagnostics.rs | `E-PROV-310` |
| `E-PROV-401` | crates/emath-adapter-rumoca/src/seam.rs | `E-PROV-401` |
| `E-PROV-402` | crates/emath-adapter-rumoca/src/seam.rs | `E-PROV-402` |
| `E-PROV-410` | crates/emath-ir/src/contracts.rs | `E-PROV-410` |
| `E-PROV-411` | crates/emath-ir/src/contracts.rs | `E-PROV-411` |
| `E-PROV-412` | crates/emath-ir/src/contracts.rs | `E-PROV-412` |
| `E-PROV-501` | crates/emath-provider-api/src/descriptor.rs<br>crates/emath-provider-api/src/registry.rs | `registration `{id}` rejected: descriptor invalid` |
| `E-PROV-502` | crates/emath-provider-api/src/descriptor.rs | `E-PROV-502` |
| `E-PROV-503` | crates/emath-provider-api/src/descriptor.rs | `E-PROV-503` |
| `E-PROV-510` | crates/emath-provider-api/src/registry.rs | `E-PROV-510` |
| `E-PROV-511` | crates/emath-provider-api/src/registry.rs | `E-PROV-511` |
| `E-PROV-512` | crates/emath-plan/src/algebra.rs<br>crates/emath-provider-api/src/filter.rs | `E-PROV-512` |
| `E-PROV-513` | crates/emath-plan/src/algebra.rs<br>crates/emath-provider-api/src/filter.rs | `E-PROV-513` |
| `E-PROV-514` | crates/emath-plan/src/algebra.rs<br>crates/emath-provider-api/src/filter.rs | `no capability serves target family `{family}`` |
| `E-PROV-515` | crates/emath-plan/src/algebra.rs<br>crates/emath-plan/src/representations.rs<br>crates/emath-provider-api/src/filter.rs | `lossy conversion path {from} -> {to} not authorized by exact goal`<br>`E-PROV-515` |
| `E-PROV-516` | crates/emath-plan/src/algebra.rs<br>crates/emath-provider-api/src/filter.rs | `E-PROV-516` |
| `E-PROV-517` | crates/emath-plan/src/representations.rs | `no conversion path {from} -> {to} (or cycle refused)` |
| `E-PROV-518` | crates/emath-provider-api/src/registry.rs | `registration `{id}` refused: provider id already registered` |
| `E-PROV-521` | crates/emath-provider-api/src/constellation.rs | `unknown provider `{id}`` |
| `E-PROV-522` | crates/emath-provider-api/src/constellation.rs | `E-PROV-522` |
| `E-PROV-523` | crates/emath-provider-api/src/constellation.rs | `E-PROV-523` |
| `E-PROV-524` | crates/emath-provider-api/src/constellation.rs | `E-PROV-524` |
| `E-PROV-525` | crates/emath-provider-api/src/constellation.rs | `E-PROV-525` |
| `E-REG-020` | crates/emath-registry/src/lib.rs | `unknown package `{package}`` |
| `E-REG-021` | crates/emath-registry/src/lib.rs | `E-REG-021`<br>`pin `{name}@{version}` does not resolve: {}` |
| `E-REG-022` | crates/emath-registry/src/lib.rs | `E-REG-022` |
| `E-REG-023` | crates/emath-registry/src/lib.rs | `E-REG-023` |
| `E-REG-024` | crates/emath-registry/src/lib.rs | `no usable version of `{package}` satisfies the constraint` |
| `E-REG-030` | crates/emath-registry/src/lib.rs | `E-REG-030` |
| `E-REG-031` | crates/emath-registry/src/lib.rs | `E-REG-031` |
| `E-RES-100` | crates/emath-plan/src/planner.rs | `{}:resume:nodes>{}`<br>`E-RES-100: {} plan nodes exceed the {} node budget` |
| `E-RES-110` | crates/emath-holes/src/synth.rs |  |
| `E-RES-111` | crates/emath-holes/src/synth.rs | `satisfy` |
| `E-RES-120` | crates/emath-build/src/lib.rs | `E-RES-120: cargo exceeded the {timeout:?} wall-clock budget` |
| `E-SCHEMA-001` | crates/emath-schema/src/registry.rs | `1.0.0`<br>`E-SCHEMA-001` |
| `E-SEC-101` | crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/declaration.rs | `inputs`<br>`E-SEC-101` |
| `E-SHAPE-001` | crates/emath-ir/src/shapes.rs | `E-SHAPE-001` |
| `E-SHAPE-002` | crates/emath-ir/src/shapes.rs<br>crates/emath-sema/src/admit/lowering.rs | `E-SHAPE-002`<br>`dimension mismatch in dot product: {ext1:?} vs {ext2:?}` |
| `E-SHAPE-003` | crates/emath-ir/src/shapes.rs | `E-SHAPE-003`<br>`slice end {rows_end} exceeds extent {size}` |
| `E-SHAPE-004` | crates/emath-ir/src/shapes.rs<br>crates/emath-sema/src/admit/expr_helpers.rs<br>crates/emath-sema/src/admit/lowering/helpers.rs<br>crates/emath-sema/src/admit/types.rs | `E-SHAPE-004`<br>`declared extent `{name}` is not a well-formed shape` |
| `E-SHAPE-005` | crates/emath-sema/src/admit/equations.rs<br>crates/emath-sema/src/admit/expr_helpers.rs<br>crates/emath-sema/src/admit/lowering.rs<br>crates/emath-sema/src/admit/lowering/helpers.rs | `dimension mismatch in residual subtraction: {le} vs {re}`<br>`E-SHAPE-005` |
| `E-SHAPE-006` | crates/emath-sema/src/admit/expr_helpers.rs<br>crates/emath-sema/src/admit/lowering/helpers.rs | `E-SHAPE-006`<br>`open slice on axis {axis} needs a fixed extent` |
| `E-SYN-100` | crates/emath-syntax/src/lexer.rs | `E-SYN-100` |
| `E-SYN-101` | crates/emath-sema/src/admit/declaration.rs<br>crates/emath-sema/src/admit/equations.rs<br>crates/emath-sema/src/admit/sections.rs<br>crates/emath-sema/src/admit/sections_meta.rs<br>crates/emath-sema/src/recognition.rs<br>crates/emath-sema/src/session.rs<br>crates/emath-syntax/src/lexer.rs<br>crates/emath-syntax/src/parser/decl.rs<br>crates/emath-syntax/src/parser/expr.rs<br>crates/emath-syntax/src/parser/stmt.rs<br>crates/emath-syntax/src/parser/stmt_binders.rs<br>crates/emath-syntax/src/parser/stmt_idents.rs<br>crates/emath-syntax/src/parser/stmt_suite.rs<br>crates/emath-syntax/src/parser/types.rs | `E-SYN-101`<br>`statement is not admitted inside `{head} {fn_name}`` |
| `E-SYN-102` | crates/emath-syntax/src/parser/expr.rs<br>crates/emath-syntax/src/parser/stmt_binders.rs<br>crates/emath-syntax/src/parser/stmt_idents.rs<br>crates/emath-syntax/src/parser/types.rs<br>crates/emath-wasm/src/lib.rs | `E-SYN-102` |
| `E-SYN-103` | crates/emath-hir/src/open.rs<br>crates/emath-sema/src/admit/declaration.rs | `E-SYN-103` |
| `E-SYN-105` | crates/emath-syntax/src/lexer.rs | `E-SYN-105` |
| `E-SYN-106` | crates/emath-syntax/src/lexer.rs<br>crates/emath-syntax/src/parser/expr.rs | `E-SYN-106` |
| `E-SYN-107` | crates/emath-syntax/src/parser/expr.rs | `E-SYN-107` |
| `E-SYN-108` | crates/emath-syntax/src/lexer.rs | `E-SYN-108` |
| `E-SYN-109` | crates/emath-syntax/src/lexer.rs | `E-SYN-109`<br>`invalid string escape `\\{}`` |
| `E-SYN-110` | crates/emath-syntax/src/parser/decl.rs<br>crates/emath-syntax/src/parser/expr.rs<br>crates/emath-syntax/src/parser/stmt.rs<br>crates/emath-syntax/src/parser/stmt_binders.rs<br>crates/emath-syntax/src/parser/stmt_idents.rs | `E-SYN-110`<br>`expected an expression, found {}` |
| `E-SYN-111` | crates/emath-syntax/src/parser/decl.rs<br>crates/emath-syntax/src/parser/expr.rs<br>crates/emath-syntax/src/parser/stmt.rs<br>crates/emath-syntax/src/parser/stmt_binders.rs<br>crates/emath-syntax/src/parser/stmt_idents.rs<br>crates/emath-syntax/src/parser/stmt_suite.rs | `E-SYN-111` |
| `E-SYN-112` | crates/emath-syntax/src/parser/stmt_suite.rs | `example`<br>`E-SYN-112` |
| `E-SYN-113` | crates/emath-syntax/src/lexer.rs | `E-SYN-113` |
| `E-SYN-114` | crates/emath-syntax/src/lexer.rs | `E-SYN-114` |
| `E-SYN-115` | crates/emath-syntax/src/lexer.rs | `E-SYN-115` |
| `E-SYN-116` | crates/emath-syntax/src/lexer.rs | `E-SYN-116` |
| `E-SYN-117` | crates/emath-sema/src/recognition.rs<br>crates/emath-syntax/src/parser/decl.rs | `E-SYN-117` |
| `E-SYN-118` | crates/emath-sema/src/recognition.rs | `E-SYN-118` |
| `E-SYN-120` | crates/emath-core/src/parse.rs<br>crates/emath-lsp/src/server.rs<br>crates/emath-sema/src/session.rs | `E-SYN-120` |
| `E-SYN-121` | crates/emath-syntax/src/parser/expr.rs | `E-SYN-121` |
| `E-SYN-122` | crates/emath-sema/src/admit/declaration.rs<br>crates/emath-syntax/src/parser/decl.rs | `E-SYN-122` |
| `E-SYN-123` | crates/emath-sema/src/admit/declaration.rs<br>crates/emath-syntax/src/parser/decl.rs | `E-SYN-123` |
| `E-SYN-201` | crates/emath-syntax/src/genesis.rs | `line {line}: malformed header, expected `emath custom Name:``<br>`E-SYN-201` |
| `E-SYN-202` | crates/emath-syntax/src/genesis.rs | `line {line}: `{content}` clause outside `construct meaning:``<br>`line {line}: unsupported `construct meaning:` clause `{other}`` |
| `E-SYN-203` | crates/emath-syntax/src/genesis.rs | `line {line}: malformed `keep:` clause, expected `pareto <u32>`` |
| `E-SYN-204` | crates/emath-syntax/src/genesis.rs | `E-SYN-204` |
| `E-SYN-205` | crates/emath-syntax/src/genesis.rs | `E-SYN-205` |
| `E-SYN-206` | crates/emath-syntax/src/genesis.rs | `E-SYN-206` |
| `E-SYN-207` | crates/emath-syntax/src/genesis.rs | `source is {} bytes; limit is {max} bytes` |
| `E-SYN-208` | crates/emath-syntax/src/genesis.rs | `line {line}: unexpected content `{content}`` |
| `E-SYN-209` | crates/emath-syntax/src/genesis.rs | `line {line}: duplicate `body:` section`<br>`line {line}: duplicate `answer:` section` |
| `E-SYN-210` | crates/emath-genesis/src/forest.rs | `E-SYN-210` |
| `E-SYN-211` | crates/emath-genesis/src/forest.rs | `E-SYN-211` |
| `E-TLT-004` | crates/emath-cli/src/catalog.rs<br>crates/emath-cli/src/tooling_cmd.rs | `bench`<br>`error: E-TLT-004: benchmarking `{file}` is not a Phase 1 CLI comparison; measure via `cargo bench --profile re` |
| `E-TLT-005` | crates/emath-cli/src/lib.rs<br>crates/emath-cli/src/tooling_cmd.rs | `error: E-TLT-005: cannot list artifact state directory {}`<br>`error: E-TLT-005: no `emath/` state directory under {}` |
| `E-TLT-006` | crates/emath-cli/src/catalog.rs<br>crates/emath-cli/src/tooling_cmd.rs | `fork`<br>`error: E-TLT-006: network/source sync is disabled in Phase 1 (offline-first); use --dry-run` |
| `E-TLT-007` | crates/emath-cli/src/tooling_cmd.rs | `error: E-TLT-007: upstream lock missing at {}`<br>`error: E-TLT-007: upstream lock is empty at {}` |
| `E-TLT-010` | crates/emath-cli/src/tooling_cmd.rs | `error: invalid package name `{name}` (E-TLT-010)` |
| `E-TLT-011` | crates/emath-cli/src/catalog.rs<br>crates/emath-cli/src/tooling_cmd.rs | `new`<br>`error: refusing to overwrite existing project at {} (E-TLT-011)` |
| `E-TLT-012` | crates/emath-build/src/lib.rs<br>crates/emath-cli/src/catalog.rs | `tests passed`<br>`E-TLT-012: generated crate has no `#[test]` tests; --verify refuses an empty test surface (add a `tests:` sect` |
| `E-TLT-013` | crates/emath-cli/src/tooling_cmd.rs | `error: E-TLT-013: provider `{id}` has no in-CLI negative-control battery; run `cargo test` against tests/emath` |
| `E-TLT-016` | crates/emath-cli/src/tooling_cmd.rs | `error: E-TLT-016: unknown provider `{id}`` |
| `E-TYPE-001` | crates/emath-sema/src/admit/types.rs | `unknown type `{other}`` |
| `E-TYPE-002` | crates/emath-core/src/diagnostic.rs<br>crates/emath-sema/src/admit.rs<br>crates/emath-wasm/src/lib.rs | `E-TYPE-002` |
| `E-TYPE-003` | crates/emath-sema/src/admit.rs | `E-TYPE-003` |
| `E-TYPE-010` | crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/equations.rs | `E-TYPE-010`<br>`state field `{name}` must use `derivative({name}) = rhs`, not `{name} = rhs`` |
| `E-TYPE-011` | crates/emath-sema/src/admit/lowering.rs | `non-finite constant `{text}` refused under strict-f64 policy`<br>`E-TYPE-011` |
| `E-TYPE-012` | crates/emath-sema/src/admit/declaration.rs<br>crates/emath-sema/src/admit/equations.rs<br>crates/emath-sema/src/admit/expr_helpers.rs<br>crates/emath-sema/src/admit/infer.rs<br>crates/emath-sema/src/admit/lowering.rs<br>crates/emath-sema/src/admit/lowering/helpers.rs<br>crates/emath-sema/src/admit/sections.rs<br>crates/emath-wasm/src/lib.rs | `constraint must be Bool, got {infer:?}`<br>`E-TYPE-012` |
| `E-TYPE-101` | crates/emath-adapter-rumoca/src/conformance.rs<br>crates/emath-adapter-rumoca/src/structural.rs | `E-TYPE-101` |
| `E-TYPE-102` | crates/emath-adapter-rumoca/src/structural.rs | `E-TYPE-102` |
| `E-TYPE-103` | crates/emath-adapter-rumoca/src/structural.rs | `E-TYPE-103` |
| `E-TYPE-110` | crates/emath-syntax/src/parser/types.rs | `fn`<br>`E-TYPE-110` |
| `E-TYPE-111` | crates/emath-syntax/src/parser/stmt_idents.rs | `E-TYPE-111` |
| `E-TYPE-112` | crates/emath-sema/src/recognition.rs<br>crates/emath-syntax/src/parser/stmt_binders.rs | `E-TYPE-112` |
| `E-TYPE-310` | crates/emath-ir/src/numeric.rs |  |
| `E-TYPE-311` | crates/emath-ir/src/numeric.rs | `E-TYPE-311` |
| `E-TYPE-312` | crates/emath-ir/src/type_system.rs | `E-TYPE-312`<br>`cannot unify {} with {}` |
| `E-TYPE-313` | crates/emath-ir/src/type_system.rs | `E-TYPE-313` |
| `E-TYPE-314` | crates/emath-ir/src/type_system.rs | `E-TYPE-314` |
| `E-UNIT-100` | crates/emath-adapter-rumoca/src/structural.rs<br>crates/emath-ir/src/units.rs | `unknown variable `{name}` in dimensional analysis` |
| `E-UNIT-101` | crates/emath-adapter-rumoca/src/structural.rs<br>crates/emath-ir/src/units.rs<br>crates/emath-sema/src/admit/equations.rs<br>crates/emath-sema/src/admit/infer.rs<br>crates/emath-sema/src/admit/lowering.rs<br>crates/emath-wasm/src/lib.rs | `dimension mismatch in sum: {left} vs {right}`<br>`event `{}` condition: {}` |
| `E-UNIT-102` | crates/emath-ir/src/units.rs | `E-UNIT-102` |
| `E-UNIT-103` | crates/emath-adapter-rumoca/src/structural.rs | `event `{}` condition: {}`<br>`event `{}` condition is not dimensionless` |
| `E-UNIT-104` | crates/emath-ir/src/units.rs<br>crates/emath-sema/src/admit/types.rs | `s`<br>`unknown unit `{other}`` |
| `E-UNIT-105` | crates/emath-ir/src/units.rs<br>crates/emath-sema/src/admit/lowering.rs<br>crates/emath-sema/src/admit/types.rs | ``Per<{inner}>` is invalid: affine units have no inverse`<br>``Per<{inner}>` is not a well-formed unit: {}` |
