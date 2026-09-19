# Error and Refusal Code Registry

| Prefix | Area | Examples |
|---|---|---|
| `E-SYN-*` | syntax/layout | bad indentation, unclosed delimiter, BOM rejection (E-SYN-113), confusable identifier lint (E-SYN-114), NFC violation refused (E-SYN-115), attribute-arg subset (E-SYN-117), unknown attribute refused (E-SYN-118), named call argument refused (E-SYN-121), head-args mixed with `inputs:`/`outputs:` (E-SYN-122), head-args on a stateful or non-function declaration (E-SYN-123) |
| `E-PKG-*` | package/lock | missing dependency, checksum mismatch, experimental without capability (E-PKG-064), unknown capability key (E-PKG-065) |
| `E-NAME-*` | names/visibility | ambiguous import, private access |
| `E-KIND-*` | custom kind | missing section, recursive expansion |
| `E-CELL-*` | capability cells | unknown class token (E-CELL-001), missing schema version (E-CELL-002), policy-refused identity mutation (E-CELL-003), arity over bound 64 (E-CELL-004), namespace-less cell name (E-CELL-005), pure cell without explicit numeric policy / non-finite logit (E-CELL-006), missing required closure projection (E-CELL-007), docs drifted from cell ID (E-CELL-008) |
| `E-SEC-*` | sections | section outside the current subset |
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
| `E-EVAL-*` | `emath eval` function lane | unsupported entrypoint (001), unknown named entrypoint (002), ambiguous entrypoint (003), missing input (004), malformed/unknown/duplicate `--set` (005), unsupported input type (006), lowering/evaluation fault or failing example (007), `--world` misuse (008) |
| `E-EV-*` | language evidence policy | claim-verb goal without `evidence:` (E-EV-140) |
| `E-CAPSULE-*` | feature capsules | malformed field/line (001), duplicate field (003), id/class mismatch (014), semantic-hash mismatch (022), reference-term refusals (023-026) |
| `E-CAT-*` | category kernels | non-finite carrier (001), shape (002), index (003), composition/identity/associativity laws (004-006), uncertifiable carrier (007) |
| `E-CSV-*` | csv ingest | missing/ambiguous time or value column (001-004), ragged row (005), unclosed quote (006), no data rows (007), non-numeric cell (008), nonincreasing time (009) |
| `E-EINSUM-*` | einsum kernel | arithmetic/subscript refusal (001), index out of range (002) |
| `E-EVENT-*` | event actions | malformed payload (001), non-Bool condition (002/006), `else` arm (003), bad action slot/value (004/005), unbound parameter (007), runtime refusal/fault (008/009) |
| `E-FIT-*` | fit lane | untagged refusal fallback (000); the lane is a single E-KIND-GONE refusal — the fitting math is authored `.emath` (`numerics.levenberg`) |
| `E-MODEL-*` | simulate model selection | named model has no `emath model` declaration (001) |
| `E-STD-*` | stdlib store | malformed envelope (001), forged object id (002), id collision (003) |
| `E-TRANS-*` | transitions | undeclared event (001), bad action target (002/005), non-assignment body (003/004), unbound event parameter (006/007), runtime action fault (008) |

Codes are stable identifiers. Messages can improve without changing code meaning. A code is never repurposed.

## Issued codes (implementation evidence)

Tooling (`crates/emath-cli/src/tooling_cmd.rs`):

- `E-TLT-004`: `bench` requires the external `cargo bench` harness.
- `E-TLT-005`: `inspect`/`verify`: artifact state or manifest missing.
- `E-TLT-006`: `fork sync` refuses network/source sync (offline-first);
  `--dry-run` allowed.
- `E-TLT-007`: `vendor`/`fork`: upstream lock file unreadable.
- `E-TLT-010`: `new`: invalid package name.
- `E-TLT-011`: `new`: refuses to overwrite an existing scaffold.
- `E-TLT-012`: `test`: generated crate has no
  `#[test]` tests; refused so an empty test surface is never reported as
  passing (add a `tests:` section to the spec).
- `E-TLT-013`: `provider test`: no in-CLI negative-control battery
  exists, so no "suite: ok" is printed; the command refuses and points at
  the workspace test suites.
- `E-TLT-016`: unknown provider id (`provider inspect|test`); distinct
  from `E-PROV-001` (adapter seam version drift).

Genesis (`crates/emath-cli-lab/src/genesis_cmd/`):

- `E-GEN-090`: a `matrix`/`graph` world is deferred in the genesis
  admission lane (not silently admitted).
- `E-GEN-091`: another admitted world label is deferred in the genesis
  admission lane.
- `E-GEN-092`: `semantic genesis --world <label>` named a world outside
  the admitted set; refused.
- `E-GEN-093`: `keep: pareto 0` keeps no candidates; the genesis run is
  refused instead of emitting an empty or winner-less portfolio receipt.
- `E-GEN-094`: parametric codegen: a world declares operator
  semantics the label-based emission cannot honor, so the unused WorldIr
  is refused (SURF-0008) instead of silently dropped.
- `E-GEN-095`: genesis would have collapsed several kept worlds to a
  single answer without `answer: return interpretation_portfolio` or a
  user lock; refused instead of `kept.first()`.

`emath eval` function-spec lane (`crates/emath-cli/src/eval_cmd.rs`):

- `E-EVAL-001`: unsupported entrypoint: the file has no `emath
  function` declaration, or `--function` names a non-function
  declaration (stateful models refuse here), or the selected function is
  stateful.
- `E-EVAL-002`: `--function <name>` does not match any declared
  function; the named entrypoint does not exist.
- `E-EVAL-003`: ambiguous entrypoint: several function declarations
  share the file with no `--function` to select (or the file carries
  several `tests:` examples and a plain eval cannot choose inputs);
  nothing is silently picked.
- `E-EVAL-004`: missing input: a declared input has no `--set`
  binding and no worked example supplied it; never a partial eval.
- `E-EVAL-005`: malformed/unknown/duplicate `--set`: a duplicate
  name, a value that is not a finite decimal scalar or `[vector]`, or a
  name that is not a declared input.
- `E-EVAL-006`: unsupported input type: a declared input is not
  `Float64` or `Vector[Float64]`, or a bound value does not match the
  declared slot shape.
- `E-EVAL-007`: lowering/evaluation fault, meaning-identity refusal,
  or a spec example whose `expect` failed (the oracle did not pass).
- `E-EVAL-008`: `--world` misuse: combined with `--function`/`--set`
  (or function flags on a genesis-format file); `--world` selects a
  genesis world only.

Meaning lock (`crates/emath-portfolio/src/meaning_lock.rs`,
`crates/emath-cli/src/meaning_cmd.rs`, genesis/eval/compile resolution):

- `E-LOCK-001`: malformed meaning lock (truncated JSON, unknown or
  missing fields, unreadable file); never best-effort parsing.
- `E-LOCK-002`: unknown `schema_version`; refuse rather than adapt.
- `E-LOCK-003`: stored `lock_id` does not match the identity body
  (tampered fingerprint or other identity field).
- `E-LOCK-004`: locked world drifted or inadmissible; hint to re-open
  the portfolio with `emath meaning unset` (never a silent fallback).
- `E-LOCK-005`: `meaning set` targeted a checker/guard-disqualified
  world; diagnostic includes the disqualification ledger entry.
- `E-LOCK-006`: `meaning set` named a fingerprint that is not in the
  current portfolio.

HIR migration (`crates/emath-hir/src/migrate.rs`):

- `E-MIGR-001`: declaration carried a legacy bootstrap schema tag; the
  stable edition must regenerate it (V3/Phase1 examples gate).
- `E-MIGR-002`: section renamed by migration (`request:` →
  `goals:` with a nested block; `input:` → `inputs:`; `output:` →
  `outputs:`).
- `E-MIGR-003`: inline constructor lifted into a `constructors:`
  section by migration.

Resources (`crates/emath-plan/src/planner.rs`, `crates/emath-build/src/lib.rs`,
`crates/emath-cli/src/tooling_cmd.rs`, `crates/emath-holes/src/synth.rs`):

- `E-RES-100`: planner: plan DAG exceeds the `max_nodes` budget;
exhausted outcome with a resume continuation instead of a silently
oversized artifact.
- `E-RES-110`: hole synthesis: carrier exceeds `MAX_CARRIER_SIZE`;
table space (`carrier^(carrier²)`) would be unbounded.
- `E-RES-111`: hole synthesis with an empty law set refused: every
  table would vacuously "satisfy" it, so the outcome is a typed refusal,
  never an invented `Contradictory`.
- `E-RES-120`: cargo (verify/test/run) exceeded the wall-clock timeout
and was killed; a session never blocks forever on a generated crate.

Registry (`crates/emath-registry/src/lib.rs`):

- `E-REG-020`: unknown package in index snapshot.
- `E-REG-021`: lock snapshot fingerprint or pin does not verify.
- `E-REG-022`: pinned version is yanked.
- `E-REG-023`: pinned version is revoked.
- `E-REG-024`: no usable version satisfies the constraint.
- `E-REG-030`: package does not serve the required kind schema.
- `E-REG-031`: package does not serve the required provider capability.

Plugin SDK (`crates/emath-plugin-sdk/src/lib.rs`):

- `E-PLG-001`: component runtime absent (execution
  contract; typed refusal).
- `E-PLG-002`: sandbox violation: network without permission, or
  wrong/missing fuel (untrusted plugin without positive fuel, and
  `execute` requiring positive fuel under every trust class: claiming
  `Trust::Local` can never skip the fuel gate).
- `E-PLG-003`: capability outside the sandbox's allowed set, or no
  capabilities declared.
- `E-PLG-004`: plugin interface core does not match the SDK's.
- `E-PLG-005`: plugin id is empty or contains ASCII control characters.

Requests (`crates/emath-sema/src/session/requests.rs`,
`crates/emath-cli/src/execution/run.rs`):

- `E-GOAL-041`: `evaluate` request missing a target in `<...>`.
- `E-GOAL-042`: `evaluate` request without `produce rust.library`, or
  with a produce target outside the current subset (the request is
  refused, not silently accepted).
- `E-GOAL-043`: request kind outside the current subset
  (supported: `evaluate`, `differentiate`, `benchmark`).
- `E-GOAL-044`: `differentiate` request missing `wrt [names]`.
- `E-GOAL-045`: `benchmark` request missing `against <path>`.

Admission (`crates/emath-sema/src/admit.rs`):

- `E-SEC-101`: section outside the current subset (`inputs`,
  `outputs`, `state`, `definitions`, `equations`, `constructors`,
  `goals`, `exports`, `tests`, `compile`, `about`, `evidence`,
  `host`); refused instead of silently dropped.
  The pre-`goals:` spellings `request:` and `requests:` refuse with a
  migration hint to `goals:`.
- `E-SEC-130`: contract-mode declaration has `outputs:`/`goals:` but no
  `inputs:` section (and no `Hole` placeholder): outputs with no declared
  input have no source (`emath-sema/src/admit/declaration.rs`).
- `E-SEC-133`: warning: no `goals:` section, so every definition
  defaults to `evaluate`; the default is surfaced, never silent
  (`emath-sema/src/admit/declaration.rs`).

Session (`crates/emath-sema/src/session.rs`):

- `E-PKG-080`: `check`/`plan` on a source file that was never loaded;
  refused with a typed diagnostic instead of returning an empty-source
  plan that passes admission. `eval`/`simulate`
  use the same code when the source path cannot be read, instead of an
  uncoded `cannot read` line.

- `E-PKG-081`: `check`/`plan` on a file that parses to zero items
  (empty, comment-only, or whitespace-only). Refused instead of a
  vacuous admit: `eval`/`simulate` already refuse empty source, and
  K-8 treats one-error-one-OK as a hard failure.

- `E-SYN-120`: `check`/`plan`/`parse_text` refuse when no source-parser
  backend is installed: a host must wire
  `emath_syntax::install_source_parser` once per process (CLI and LSP
  do this at startup). The refusal is typed; never a silent empty parse.

Codegen (`crates/emath-build/src/builder/build.rs`):

- `E-CODEGEN-012`: builder `compile:` spec outside the current
  rust/library subset; the model is refused, never adapted.

Generated-crate profiles (`crates/emath-rust-ir/src/profiles.rs`,
checked on the build path via `CrateProfile::Library`):

- `E-CODEGEN-013`: `emath build` emits one runnable entry per crate: a second runnable function in the same file refuses (split the file into one `emath function` per file).

- `E-CODEGEN-002`: generated module contains `unsafe` while the profile
  is safe (every profile bans unsafe); the build fails before any
  artifact is staged.
- `E-CODEGEN-003`: profile name not recognized
  (`parse_profile("...")`); unknown profiles are typed refusals, never
  a silent default.
- `E-CODEGEN-004`: a public item in the generated module lacks a source
  anchor; source-map round-trips are incomplete, so the build fails.

Syntax (`crates/emath-syntax/src/lexer.rs`, `crates/emath-syntax/src/parser.rs`):

- `E-SYN-101`: malformed argument list in a section head; the whole
  statement is refused instead of recording `args: None`.
- `E-SYN-121`: a named call argument (`f(name = value)` in call position)
  is outside the current subset; the call is refused instead of stripping
  the name and silently losing the binding.
- `E-SYN-122`: declaration head arguments (`name(args)`) mixed with an
  `inputs:` section, or a head `-> T` mixed with an `outputs:` section;
  the two spellings are identity-equivalent and cannot both appear.
- `E-SYN-123`: declaration head arguments on a stateful declaration
  (`state:` / `constructors:`) or on a non-`function` kind (policy,
  world, builder, method). Head-args are only admitted on stateless
  `emath function`.
- `E-SYN-149`: L2 named shorthand with an explicit head-arg signature
  whose names do not cover the body's free names (typed refusal, not a
  guessed coercion of `n` onto `x`).
- `E-SYN-150`: L2 named shorthand whose body calls an unknown name
  (`mystery(x)`) with no hole `mystery = ?`; the callee is not inferred
  as a Float64 input.
- `E-SYN-151`: `solve` presented with an unlabeled unique numeric root
  (`uniquely 1.414…`); the candidate menu must be labeled, never a naked
  float as intended meaning.
- `E-SYN-153`: hanging infix: a line ends with a binary operator, so the
  expression is incomplete, but NEWLINE fires outside brackets and the parse
  splits. The diagnostic teaches the bracket idiom: wrap the expression in
  `()` (or `[]`) to continue it across lines (F2).
- `E-SYN-154`: ambiguous brace: `{name: value}` in expression position
  without a path prefix. Inline records are
  path-prefixed (`Point:{x: 1.0}`); bare braces are set literals, so the
  record spelling without a path is refused instead of silently reading as
  a malformed one-element set.
- `E-SYN-155`: `emath exactness --raise <dimension>` on a file that carries
  a freeze lock (`<file>.freeze.lock.json`). A frozen meaning does
  not raise: edit the source and refreeze. Display without `--raise` stays
  allowed: the budget is a view, not an authority change.
- `E-SYN-156`: a malformed `reactions:` line:
  a line is `name: coefficient species (+ …) arrow (coefficient species …)`;
  admitted arrows are `->` (irreversible; the token-equivalent `=>` shares
  the lexer token and denotes the same arrow: no lambda position exists in
  this T3 grammar), `<->` (reversible), `<=>` (equilibrium). Unknown arrow
  spellings, trailing tokens, or non-(coefficient species) terms refuse.
- `E-CHEM-SPECIES`: `species:` closes the world:
  every species named in a reaction line must be declared; no implicit
  species, no guessed formula.
- `E-CHEM-BALANCE`: element balance is checked statically at admission
  per-element atom counts must match
  across the arrow (`2H2 + O2 -> 2H2O` balances; `2H2 + O2 -> H2O` refuses).
  A species that is not an element formula (`A`, `B` in generic networks) is
  an abstract label: balance cannot be checked statically, so that reaction
  is skipped rather than refused.
- `E-CHEM-KA-EXACT`: an equilibrium constant is a MEASURED value
  the uncertainty form is the point
  (`1.75(3)e-5` or `1.75 ± 0.03e-5`). A bare exact literal for a constant
  in a `reaction_network` refuses: it is the dishonest spelling for a
  measured quantity.
- `E-CHEM-THERMO`: the honesty triangle: a
  network declaring BOTH a reversible kinetic pair (`<->` with `kf`/`kr`
  rate entries) AND an equilibrium (`<=>` with a measured constant) must
  satisfy K == kf/kr within combined uncertainty. Refuses when the
  constant is missing or numerically inconsistent with kf/kr.
- `E-NOTATION-AMBIG`: §3.4 context-scoped brackets:
  inside a `rate:` entry, `[X]` reads as concentration-of-X only when X is
  a declared species; an undeclared bracket or a bare list literal in a
  rate-law argument has no resolvable reading and refuses instead of
  guessing. Outside rate contexts `[x]` keeps the list/index reading.
- `W-CHEM-RATELAW`: warning receipt: a named
  rate-law form (`michaelis_menten(Vmax, Km, [S])`) is non-mass-action;
  without a declared `assumptions:` section (e.g. `quasi_steady_state`)
  the approximation would be ambient, so admission warns. Declared
  assumptions silence the receipt.
- `E-SYN-115`: an identifier contains a combining mark (U+0300-U+036F);
  such a spelling is canonically non-NFC by construction and cannot be
  verified without a Unicode table, so the identifier is refused instead
  of entering the pipeline with an identity the toolchain cannot check.
- `E-SYN-117`: an item attribute argument is outside the
  identifier/string-literal/bracket-list subset (`@experimental(deep)`,
  `@capabilities(x = 1)`); the attribute is refused instead of dropping
  the argument and silently changing meaning.
- `E-SYN-118`: unknown item attribute (`@bogus`): the front-end does
  not understand it, so it is refused instead of silently discarded.
- `E-TYPE-110`: function types (`fn(params) -> T`) are outside the current
  subset; refused instead of recording a lossy `Path(["fn"])`.
- `E-TYPE-111`: type aliases (`type X = T`) are outside the current subset;
  refused instead of dropping the right-hand side.
- `E-TYPE-112`: generic `extern operator` declarations are outside the
  current subset; refused instead of discarding the generic parameters.

Notation (`crates/emath-syntax/src/parser.rs`):

- `E-NOTATION-RESERVED`: a notation glyph or alias shadows a core
  token (N3: `+ - * / ^ == != < <= > >= and or not = := -> => :: . ..
  ..= ?`); the core vocabulary cannot be rebound, so the declaration is
  refused instead of silently overloading an operator.
- `E-NOTATION-GLYPH`: a glyph or alias does not lex as a single
  identifier (e.g. `!`, `++`); such a spelling could never appear in an
  expression without re-lexing as other syntax, so it is refused at the
  declaration instead of registering a dead operator.
- `E-NOTATION-AMBIG`: the same glyph is bound to two different target
  paths in one scope; the glyph must map to exactly one canonical
  operator, so the later binding is refused instead of resolved by
  precedence or fixity.
- `E-NOTATION-PRECEDENCE`: the declared integer precedence is below
  the custom-operator floor `CUSTOM_OP_MIN_PRECEDENCE` (11); tiers
  1-10 belong to the core lexical ladder and a lower declaration would
  parse without ever binding, so it is refused up front.

Schema registry (`crates/emath-schema/src/registry.rs`):

- `E-SCHEMA-001`: unknown schema name: the thirteen canonical
  `emath.<name>` registry entries are fixed; any other name is a
  typed refusal instead of a guessed document.

Providers (`crates/emath-adapter-rumoca/src/provider.rs`,
`crates/emath-adapter-rumoca/src/import.rs`):

- `E-PROV-230`: missing parameter value, or unknown variable during
  evaluation.
- `E-PROV-234`: unresolvable or mis-targeted initial condition.
- `E-PROV-235`: simulation plan shape refused: fewer than two states,
  non-finite or non-positive `dt`, or a state without a derivative row.
- `E-PROV-236`: assignment to a non-state variable, or an equation whose
  left-hand side is not a variable or `der` reference; the simulation fails
  instead of silently dropping the assignment.
- `E-PROV-237`: provider model refused by the validation gate before any lowering or simulation runs; untrusted provider output never becomes `Resolved`.
- `E-PROV-238`: simulation plan with more than two states refused: the
  fixture-time recorder represents at most two states and never silently
  drops extra states.
- `E-PROV-240`: malformed Modelica subset import: missing model name or
  missing `end` terminator.
- `E-PROV-241`: Modelica subset import refuses a construct that is not in
  the semantic mapping table (or is classified unsupported); the import is
  rejected instead of retained with the construct dropped.

- `E-PROV-030`: provider-supplied IR refused before splicing/generation:
  integer literal not finite in strict-f64, a variable name that is not a
  safe Rust identifier, or a non-scalar linear-algebra node under the
  scalar backends; the adapter emits a typed refusal instead of invalid
  code or a numeric placeholder stub.
- `E-PROV-031`: requested capability outside the advertised inventory:
  a backend or accelerator target with no admitted subset
  (`capability::select_backend`, `backends::admit_target`).
- `E-PROV-033`: linear-algebra shape mismatch in `map_linear` (e.g.
  `dot` on non-vectors, `matvec` with incompatible columns/rows).
- `E-PROV-501`: provider registration rejected because the descriptor failed
  self-validation (duplicate capability names, empty representations, empty
  semantic subsets); untrusted descriptors never enter the registry.
- `E-PROV-515`: provider excluded because its declared exactness cannot serve
  the goal's exactness policy (bounded/checked-numeric/any-explicit goals never
  fall back to estimate-only or undeclared exactness).


Rumoca dynamic-model subset (`crates/emath-adapter-rumoca/src/subset.rs`):

- `E-KIND-310`: variable role outside the dynamic-model subset (only
  `Parameter`/`State` are in scope).
- `E-KIND-311`: component construct outside the dynamic-model subset.
- `E-KIND-312`: discrete event outside the current subset; only basic
  continuous events are accepted (distinct from `E-KIND-310`, which is
  about variable roles).

Adapter seams (`crates/emath-adapter-dew/src/seam.rs`):

- `E-PROV-001`: upstream fork version drift between the packed lock and
  the adapter's expected baseline; the seam refuses to map a mismatched
  world.
- `E-PROV-002`: uncategorized upstream patch set blocks the seam; the
  patch table must be curated before mapping.

Evidence (`crates/emath-evidence/src`, `crates/emath-checker/src`):

- `E-EVID-101`: artifact file content does not hash to its declared
  content id.
- `E-EVID-102`: artifact identity does not recompute from the manifest
  body under the independent checker.
- `E-EVID-103`: evidence bundle is not scoped to the frozen
  goal/source package.
- `E-EVID-110`: source-map schema is not `emath.source-map`.
- `E-EVID-111`: provider lock record missing or not matching the
  manifest's provider dependency.
- `E-EVID-112`: source map does not reference the manifest's source
  package (distinct from `E-EVID-110`, which is about the schema).
- `E-EVID-403`: checker contract registers with an empty `admits` list;
  a contract that admits nothing is refused.
- `E-EVID-507`: certifier output refused by the corpus gate:
  known-unsound pattern or non-UTF-8 payload.

Kinds (`crates/emath-schema/src/load.rs`, `crates/emath-sema/src/admit.rs`,
`crates/emath-hir/src/open.rs`):

- `E-KIND-010`: function declarations cannot have state or constructors in
  the current subset (semantic admission in `admit.rs`; the builder embeds the same
  predicate verbatim). One predicate, one code; the HIR manifest schema
  violations use their own codes (`E-KIND-011`, `E-KIND-016`).
  Coaching refusals: when the mismatch has a canonical repair, the message
  names the right kind: `state:`/`equations:`/`algebraic:` on a non-model
  points at `emath model` (or `emath policy` for a stateful object),
  `constructors:` on a non-policy points at `emath policy`. The code and
  predicate are stable; only the message coaches (see the decision tree in
  `language/reference/declarations-sections-and-attributes.md`).
- `E-KIND-011`: kind schema is missing a
  `RepeatPolicy::ExactlyOne` section (`SectionManifest::check` and
  semantic admission in `admit.rs`). `inputs:` and `outputs:` are
  `AtMostOne` on the core function schema, so omitting either is not
  this refusal.
- `E-KIND-016`: HIR section manifest declares a section that is not part
  of the kind schema (`SectionViolationReason::UnknownSection`). Split from
  `E-KIND-010` so unknown-section (schema conformance) and
  constructors/state (current subset) are never the same code. Live
  `request:` / `requests:` use this code with a `goals:` migration hint.
- `E-KIND-032`: schema load refuses a kind whose expansion would recurse.

Units (`crates/emath-ir/src/units.rs`,
`crates/emath-adapter-rumoca/src/structural.rs`):

- `E-UNIT-100`: unknown variable in dimensional analysis. A missing
  variable is refused, never treated as dimensionless.
- `E-UNIT-101`: dimension mismatch: two operands that must share
  dimensions do not (unit compatibility in `units.rs`; sums and equations
  in the rumoca structural check). The predicate is identical at every
  site, so one code serves all of them.
- `E-UNIT-102`: affine unit misuse: multiplying or dividing by a unit
  with a non-zero offset.

Names/constructors/units (`crates/emath-sema/src/admit.rs`):

- `E-NAME-020`: duplicate symbol/field in a package or structural model
  (also emitted by `emath-hir` notation and the rumoca structural check).
  In section terms: a name declared in two sections (e.g. both `inputs:`
  and `outputs:`), or a `definitions:` name shadowing an `inputs:` name
  in contract mode.
- `E-NAME-022`: duplicate declaration name in one admission: two
  declarations with the same name would collide in generated Rust, so the
  second is refused instead of silently overwriting the first. Also fires
  for two `example <name>:` blocks in one function's `tests:` section
  with the same resolved name (each becomes a generated test fn
  `<function>_<name>`); the second block is refused at admission instead
  of surfacing as a raw rustc E0428 inside `emath test`.
- `E-NAME-023`: declaration named `_`: `_` cannot be escaped into a Rust
  type name, so the declaration is refused up front. Also reused for a
  declared output that has no definition.
- `E-NAME-024`: declaration name differs from an already-seen name only
  by confusable glyphs (e.g. Latin `o` vs Cyrillic `о`, per the
  `confusable_fold` seed map); the second public declaration is refused
  so the API never presents two visually indistinguishable names.
Dimensional mismatch (e.g. adding `Length` to `Duration`) is `E-UNIT-101`.
The old `E-UNIT-001` spelling is retired and is not emitted.

Observations (04 §5.2, `crates/emath-sema/src/admit/declaration.rs`,
`crates/emath-cli/src/lib.rs`):

- `E-OBS-WRITE`: a `definitions:` binding targets an observation name:
  observations are read-only measured evidence and are never written by
  the model (the model/observation line). Bind a different name for the
  model quantity.
- `E-OBS-HASH`: `emath check --verify-data` re-hashes a `sha256`
  declared in InstrumentRun provenance and the digest does not match the
  data file on disk (drift), or the data file cannot be read: the
  evidence cannot be confirmed. Changed data under an unchanged model is
  a different artifact identity.
- `E-UNIT-104`: unknown unit name in a quantity literal (`1 furlong`)
  or catalog lookup.
- `E-UNIT-105`: ill-formed unit constructor (`Per<>` arity, affine
  inverse).
- `E-CTOR-030`: missing state assignment in a constructor (also emitted
  by the builder for generated-code invariants).

Types (`crates/emath-ir/src/type_system.rs`):

- `E-TYPE-312`: `unify` refuses two refined types whose predicates differ,
  or whose discharge is different (e.g. a statically discharged refinement
  against a construct-time one); the discharge is never silently dropped.

Provider API (`crates/emath-provider-api/src`):

- `E-PROV-510`: registration refused: claim contradicts advertised table isolation, or advertised isolation not allowed by the registry policy.
- `E-PROV-518`: provider registry refuses a duplicate provider id (silent overwrite would replace isolation/capabilities under the same lookup key).
- `E-PROV-524`: constellation register attempts a maturity above P0 without proofs; census entries start at P0 and climb via promote.
- `E-PROV-525`: constellation register refuses a duplicate provider id (a second P0 register must not silently demote a promoted entry).

Lab (`crates/emath-lab-core/src`, `crates/emath-rust-ir/src/host.rs`):

- `E-HOST-001`: rust-ir host gate: toolchain/ABI incompatible with what
  the crate declares (typed refusal, never a silent fallback).
- `E-HOST-003`: invalid experiment manifest: wrong schema, duplicate
  partitions/metrics/kill-rule ids, non-positive thresholds, malformed
  canonical JSON.
- `E-HOST-004`: manifest not frozen before measurement, or baseline and
  candidate are not distinct artifacts.
- `E-HOST-005`: promotion quality gate blocked the candidate (default
  gate failure code; `E-EVID-*` check codes take precedence).
- `E-HOST-006`: insufficient evidence: too few samples after
  warmup/cull, or an empty sample set given to `percentile` /
  `percentile_f64` (typed refusal, never an index underflow).
- `E-HOST-007`: metric ratio regressed beyond the policy floor/ceiling
  (median, p99, memory, energy).
- `E-HOST-008`: incomparable inputs (paired analysis under an unpaired
  protocol, zero-duration samples, no paired comparison).
- `E-HOST-010`: runtime drift detected; the candidate was demoted
  instead of promoted.
- `E-HOST-011`: receipt/drift record does not verify against the
  recorded hash.
- `E-HOST-012`: invalid statistical protocol configuration (repetitions
  below the minimum, warmup below repetitions, MAD trim not positive and
  finite, field does not fit `usize`); distinct from manifest validation.
- `E-HOST-013`: invalid canary routing configuration (canary outcome
  without a positive canary interval).
- `E-HOST-014`: invalid drift band tolerance (non-finite or
  non-positive).
- `E-HOST-015`: invalid engine policy (median ratio bounds, regression
  and memory ceilings outside `[1.0, inf)`).
- `E-HOST-016`: refuse self-comparison: the subject and oracle of a
  comparison must be distinct engine identities (honest comparator
  separation; never a silent self-comparison).

LSP (`crates/emath-lsp/src`):

- LSP/JSON-RPC uses the standard `-32601` (method not found) and `-32700`
  (parse error) codes; message text is deterministic.

Units/shapes/domains (`crates/emath-ir`, `crates/emath-adapter-rumoca`):

- `E-UNIT-103`: event condition is not dimensionless (Rumoca structural
  validation); distinct from `E-UNIT-102` (affine multiplication/division
  misuse in `units.rs`).

### Codegen (`crates/emath-build`)

- `E-CODEGEN-005`: git/registry dependency denied by dependency policy
  (forbidden source).
- `E-CODEGEN-006`: dependency used but not declared.
- `E-CODEGEN-007`: version conflict among mapped dependencies.
- `E-CODEGEN-008`: locked build script cannot satisfy a dependency that
  needs network access while locked (offline-first honored).
- `E-CODEGEN-009`: absolute-path dependency refused (absolute-path
  leak).
- `E-CODEGEN-010`: generated build script wrote outside `$OUT_DIR`
  (isolation refusal).
Unknown numeric profile names are `E-NUM-001`. The old `E-CODEGEN-053`
spelling is retired and is not emitted.

### Constructors (`crates/emath-builder`, `crates/emath-sema/src/admit.rs`)

- `E-CTOR-031`: policy declarations require a `constructors:` section
  with a public `new`.
- `E-CTOR-032`: `require`/`ensure` must be a Boolean expression.
- `E-CTOR-033`: default value reads a target that is not a state field.
- `E-CTOR-034`: multiple constructors named `new`, or duplicate
  constructor parameter.
- `E-CTOR-035`: duplicate assignment for a state field.
- `E-CTOR-036`: The current subset admits exactly one public `new` constructor;
  the primary constructor must be named `new`.
- `E-CTOR-037`: delegating constructor targets an unknown constructor.
- `E-CTOR-038`: delegating constructor assigns state directly
  (refused).
- `E-CTOR-039`: default supplied for an undeclared parameter.

### Domains (`crates/emath-ir`)

- `E-DOM-001`: a value falls outside its declared domain.
- `E-DOM-002`: ill-formed domain declaration (inverted or non-finite
  interval; compile `domain lo..hi`).

### Numeric models (`crates/emath-ir/src/numeric.rs`, `crates/emath-sema/src/admit.rs`)

- `E-NUM-001`: unknown numeric model name. Known models: `strict-f64`
  (the default when `numeric:` is omitted) and explicit
  `interval-f64`. Other names are refused, never silently coerced.
- `E-NUM-002`: precision demand no selected model can honor (zero bits,
  or more significand bits than the model provides).
- `E-NUM-003`: error-limit demand no selected model can honor (strict-f64
  cannot certify a bound tighter than machine epsilon; interval-f64
  cannot honor a zero/exact bound; non-finite limits refused).

### Runtime kernels (`crates/emath-rt`, surfaced through `crates/emath-exec-ir`)

- `E-PROB-001`: the unit-interval draw count is non-integer / over
  budget, or the stream path is invalid.
- `E-PROB-002`: a non-finite seed; refused (never a silently
  corrupted stream).

### Exact rationals (`crates/emath-sema`, `crates/emath-exec-ir`)

- `E-RAT-001`: `rat(n, 0)` has no exact-rational value: a zero
  denominator is refused with a typed diagnostic (check time for
  literal denominators, `EvalFault::Arithmetic` at run otherwise):
  never a panic, never a silent zero.
- `E-RAT-002`: an exact-rational intermediate exceeds the `i128`
  carrier; refused instead of silently overflowing.

### Evidence and artifacts (`crates/emath-checker`, `crates/emath-evidence`)

- `E-EVID-104`: certificate for a claim is stale (fresh-until passed).
- `E-EVID-105`: required artifact file is missing.
- `E-EVID-106`: claim class is not supported by any checker.
- `E-EVID-107`: resolved claim has no checker bound.
- `E-EVID-108`: manifest schema is not `emath.artifact`.
- `E-EVID-109`: manifest declares no files, or declares a path with no
  file present.
- `E-EVID-113`: required/declared artifact path is a symlink (refused).
- `E-EVID-114`: artifact document or declared file is not valid UTF-8.
- `E-EVID-201`: claim language stronger than the available evidence
  (downgrade suggested).
- `E-EVID-301`: translation mismatch (no equivalence witness).
- `E-EVID-302`: witness cannot be independently verified.
- `E-EVID-401`: unknown certificate contract kind/version.
- `E-EVID-402`: duplicate versioned checker contract refused.
- `E-EVID-404`: incomplete computation cannot become resolved evidence.
- `E-EVID-405`: assumption already registered under a different class.
- `E-EVID-501`: unknown evidence record id.
- `E-EVID-502`: duplicate append-only revocation marker.
- `E-EVID-503`: content-identity mismatch (bootstrap identity).
- `E-EVID-504`: double supersession (append-only conflict).
- `E-EVID-505`: stale record refused for promotion (revalidation
  required).
- `E-EVID-506`: proof provider unavailable (optional-path refusal).

### Genesis (`crates/emath-cli-lab/src/genesis_cmd/`)

- `E-GEN-080`: genesis parse refused.
- `E-GEN-081`: genesis body expression is empty.
- `E-GEN-082`: reference body is not unique (ambiguity reported).
- `E-GEN-083`: signature inference refused.
- `E-GEN-084`: inferred signature rejects the term.

### Goals (`crates/emath-plan/src/planner.rs`)

- `E-GOAL-201`: no eligible plan; an exhausted budget yields a
  continuation/diagnostic per the goal's fallback policy.

### Host/lab (`crates/emath-lab-core`, `crates/emath-rust-ir`)

- `E-HOST-002`: host-binding layer failure beyond ABI version mismatch
  (family code owned by `emath-rust-ir`; no construction site in HEAD).

### Custom kinds (`crates/emath-schema`, `crates/emath-sema`)

- `E-KIND-001`: declaration kind is not supported by this front-end.
- `E-KIND-002`: declaration could not be admitted.
- `E-KIND-012`: malformed directive or unknown token (repeat policy).
- `E-KIND-013`: duplicate section spec.
- `E-KIND-014`: duplicate default for a section.
- `E-KIND-015`: default or predicate references an undeclared section.
- `E-KIND-020`: invalid lowering operation.
- `E-KIND-021`: rename source is not a declared (core) section.
- `E-KIND-022`: lowering bound exceeded (op limit or recursive hoist).
- `E-KIND-030`: unknown kind at schema load.
- `E-KIND-031`: incompatible schema version refused at load.

Capability cells (`crates/emath-ir/src/capability.rs`, schema
`emath.capability-cell.v1`):

- `E-CELL-001`: class token outside the closed ten-class taxonomy.
- `E-CELL-002`: capability cell declares no schema version.
- `E-CELL-003`: identity-affecting cell change refused by its migration
  policy (frozen cell, or `bump-and-note` with no note or no version bump).
- `E-CELL-004`: declared cell arity exceeds the bounded maximum (64).
- `E-CELL-005`: cell name is empty or has no namespace path.
- `E-CELL-006`: pure-cell evaluation without the required explicit
  numeric policy, or a non-finite logit under the strict-f64 finite
  policy (`std.tensor.softmax` reference semantics).
- `E-CELL-007`: a required closure projection is missing for a
  capability cell; missing required projections block stable
  (projection planner, fjxh.4).
- `E-CELL-008`: docs are not bound to the cell's current identity
  (`CellId`); docs cannot drift from the cell identity.
- `E-CELL-009`: a biform cell is missing one required side's evidence
  object; a missing side is never treated as proved by the algorithm
  side.
- `E-CELL-010`: a biform authority escalation: the supplying authority
  cannot attest the claimed side (algorithm tests, benchmarks, or
  provider receipts never raise spec authority).
- `E-CELL-011`: one evidence object is claimed for both sides of a
  biform cell (spec and algorithm evidence must be distinct).

### Executable laws (`crates/emath-sema`)

### Symbolic algebra (`crates/emath-ir`, `crates/emath-sema`)

- `E-SYM-001`: malformed symbolic expression or rewrite replacement.
- `E-SYM-002`: exact coefficient overflow or the native degree/resource
  bound was exceeded.
- `E-SYM-003`: expression or claim is outside the native exact univariate
  polynomial/simplification fragment.
- `E-SYM-004`: a structural rewrite requested `proved` authority without a
  checkable certificate.

### Names/visibility (`crates/emath-sema`, `crates/emath-hir`)

- `E-NAME-021`: Exports must be `public` (also: empty operator
  symbol refused in notation mount).

### Packages (`crates/emath-schema`, `crates/emath-sema`)

- `E-PKG-020`: schema lock checksum mismatch.
- `E-PKG-050`: external file import outside the front-end subset
  (library-path imports only).
- `E-PKG-064`: `@experimental` on an item while the source file does
  not declare the `experimental-syntax` capability; experimental syntax
  never compiles silently in a stable package (ELP experimental lane).
- `E-PKG-065`: unknown capability key in `@capabilities(...)`
  (declared: `experimental-syntax`); the key is refused instead of
  being silently dropped.

### Providers (`crates/emath-adapter-rumoca`, `crates/emath-provider-api`, `crates/emath-ir`, `crates/emath-plan`)

- `E-PROV-210`: connection cardinality violation or unknown port
  (structural census).
- `E-PROV-220`: underdetermined: no equation produces the expected
  variable.
- `E-PROV-221`: algebraic cycle refused (tearing candidates reported).
- `E-PROV-222`: multiple equations produce the same identifier.
- `E-PROV-223`: equation has no single unknown on its left-hand side.
- `E-PROV-231`: non-finite value during evaluation.
- `E-PROV-232`: assignment to an unknown derivative.
- `E-PROV-233`: division by zero during evaluation.
- `E-PROV-300`: provider diagnostic mapped with provider code and
  component preserved.
- `E-PROV-310`: source-map loss: no emath span for a component (never
  silently dropped).
- `E-PROV-401`: provider seam version drift refused.
- `E-PROV-402`: provider seam identity mismatch refused.
- `E-PROV-410`: bit-identical primitive conversion contract
  (float64/i64).
- `E-PROV-411`: value-conserving affine unit conversion contract (degC
  ↔ kelvin).
- `E-PROV-412`: index-conserving sparse matrix conversion (csc-matrix).
- `E-PROV-502`: duplicate capability name in a descriptor.
- `E-PROV-503`: capability declares no representations or no semantic
  subset.
- `E-PROV-511`: lock-mismatched provider registration refused.
- `E-PROV-512`: capability filtered: goal kind/subset exclusion.
- `E-PROV-513`: capability filtered: evidence/checker exclusion.
- `E-PROV-514`: capability filtered: no capability serves the target
  family.
- `E-PROV-516`: capability filtered: determinism exclusion.
- `E-PROV-517`: no conversion path between representations (or cycle
  refused).
- `E-PROV-521`: unknown provider id in the provider constellation.
- `E-PROV-522`: promotion target is not above the provider's current
  level.
- `E-PROV-523`: promotion of a provider blocked: selection criteria
  unmet.

### Shapes (`crates/emath-ir`)

- `E-SHAPE-001`: matrix product requires rank-2 operands.
- `E-SHAPE-002`: inner extents differ.
- `E-SHAPE-003`: slice bounds invalid (start exceeds end).
- `E-SHAPE-004`: declared shape is not well-formed (empty tensor rank,
  zero extent, or `Zero` symbolic extent).
- `E-SHAPE-005`: elementwise shape mismatch: vector/matrix add or sub
  extents differ, or a matrix literal has ragged rows.
- `E-SHAPE-006`: index rank or index type is wrong (`v[i, j]` on a
  vector, non-numeric subscript). Out-of-range evaluation is an
  interpreter fault, not this code.

### Syntax/layout (`crates/emath-syntax`, `crates/emath-sema`, `crates/emath-hir`, `crates/emath-genesis`)

- `E-SYN-100`: inconsistent indentation: dedent does not match an
  enclosing block.
- `E-SYN-102`: expected `)` to close a parameter list.
- `E-SYN-103`: duplicate section refused.
- `E-SYN-105`: unterminated string literal.
- `E-SYN-106`: nesting limit exceeded.
- `E-SYN-108`: token limit exceeded (source too complex).
- `E-SYN-109`: invalid string escape.
- `E-SYN-110`: expected a package path after `package`.
- `E-SYN-111`: expected `:` after the declaration head.
- `E-SYN-112`: expected an indented block.
- `E-SYN-113`: UTF-8 BOM rejected at the start of a source file.
- `E-SYN-114`: non-ASCII identifier refused (confusable lookalike
  hazard).
- `E-SYN-116`: source exceeds `Limits::max_source_bytes`; the lexer
  refuses before scanning so huge inputs cannot burn O(n) work.
- `E-SYN-201`: malformed genesis header (expected `emath custom
  Name:`).
- `E-SYN-202`: clause outside `construct meaning:` (or unsupported
  clause).
- `E-SYN-203`: malformed `keep:` clause (expected `pareto <u32>`).
- `E-SYN-204`: malformed `answer:` clause (expected `return <name>`).
- `E-SYN-205`: genesis file has no `body:` section.
- `E-SYN-206`: genesis file has no `answer:` section.
- `E-SYN-207`: genesis source exceeds the byte limit.
- `E-SYN-208`: unexpected content line in a genesis file.
- `E-SYN-209`: duplicate `body:`/`answer:` section.
- `E-SYN-210`: no complete parse of the reference body (holes
  reported).
- `E-SYN-211`: reference body is ambiguous (multiple structural
  parses).

### Types (`crates/emath-sema`, `crates/emath-ir`, `crates/emath-core`)

- `E-TYPE-001`: unknown type.
- `E-TYPE-002`: unknown variable.
- `E-TYPE-003`: unknown function.
- `E-TYPE-010`: unsupported type, or an implicit / mass-matrix
  left-hand side (`m * derivative(v) = rhs`) outside the explicit ODE
  subset.
- `E-TYPE-011`: non-finite constant refused under the strict-f64
  policy.
- `E-TYPE-012`: argument arity/type mismatch for the named function.
- `E-TYPE-101`: derivative target is not a state variable.
- `E-TYPE-102`: initial condition target is not a state variable.
- `E-TYPE-103`: state must not appear as a plain equation target.
- `E-TYPE-310`: numeric family code (documented in
  `crates/emath-ir/src/numeric.rs`; no construction site in HEAD yet).
- `E-TYPE-311`: cannot promote two numeric types without exact-width
  loss.
- `E-TYPE-313`: type variable bound to conflicting types.
- `E-TYPE-314`: occurs check: a type variable escapes into its own
  binding.

### Meaning lock (`crates/emath-portfolio/src/meaning_lock.rs`, `crates/emath-cli`)

Issued stories for `E-LOCK-001` through `E-LOCK-006` live in the
Meaning lock subsection of the issued-codes list above. The ADR-001
falsifier still holds: a drifted or tampered lock never silently falls
back to another world.

- `E-LINALG-004`: linear-algebra operand dimensions do not compose (typed refusal, never a silently wrong result).

- `E-PROV-239`: the runnable component profile requires exactly two scalar states and no algebraic outputs (simulation-artifact construction refuses any other shape).

- `E-EVID-601`: a re-materialization derived a different artifact id under a recorded recipe identity (delete + rehydrate is the only path; the recorded binding is never overwritten).
- `E-EVID-602`: a pack read hit a bad magic header (whole-magic check, never a partial read).
- `E-EVID-603`: a pack is truncated (declared lengths exceed the file).
- `E-EVID-604`: a pack is oversized or would blow its size/ref-count budget (corruption refuses instead of being read).
- `E-EVID-605`: a thin pack without its parent closure (never a partial silent read).
- `E-EVID-606`: duplicate entry ids in a pack write (canonical export requires an id set).

- `E-ODE-001`: an implicit ODE solve (Newton on the residual) did not converge to machine tolerance, or the solve went non-finite (typed refusal; never a silently wrong trajectory).
- `E-ODE-002`: velocity-Verlet simulation requires the separable carrier `der_q = v, der_v = a(q)` exactly; any other structure refuses at the STRUCTURE gate (never a silently misintegrated model).
- `E-ODE-003`: an ODE step size is non-advancing (zero/negative/absurd h).

Fork-pack install/lazy/shake/specialize/image tooling (`crates/emath-exec-ir/src/{install,image,growth,lazy,shake,specialize}.rs`):

- `E-GROWTH-001`: the growth gate refuses an operation-name branch in a gated file: new math enters as data, not as a per-op VM branch.
- `E-IMAGE-001`: a semantic-image partition's body no longer matches its stamped content id: a corrupt page.
- `E-IMAGE-002`: a semantic-image partition is malformed.
- `E-LAZY-001`: a page was requested from a pack the session never loaded (unused-pack access is detected, never served eagerly).
- `E-LAZY-002`: an unknown pack/page identity in the lazy-loading seam.
- `E-PACK-001`: a layout directory outside the closed field-pack set.
- `E-PACK-002`: an export the cell registry does not provide (install never fabricates a cell).
- `E-PACK-003`: a `use` path with no installed pack.
- `E-PACK-004`: an export name matching several registry cells (not uniquely resolvable; packs must name cells precisely).
- `E-PACK-005`: the pack exports no cells (a metadata-only pack has nothing installable; the image law requires non-empty pages).
- `E-SHAKE-001`: a shake entry the installed image does not contain (never a silent no-op that pretends to shake).
- `E-SHAKE-002`: shaking an image that still has a required dependency as an entry (dependencies must be shaken in dependency order).

Provider adapter gate (`crates/emath-provider-api/src/adapter.rs`):

- `E-PROVIDER-001`: a provider-native type token in the public IR-facing signature (allowlist gate; provider types cannot leak into the public adapter surface).
- `E-PROVIDER-002`: the provider reported a reduction axis other than the one the binding declares (wrong-axis must FAIL).
- `E-PROVIDER-003`: an oracle comparison was demanded for a cell without local reference semantics (handwritten kernels only).

World declarations and results (`crates/emath-genesis/src/{world_decl,world_result}.rs`):

- `E-WORLD-001`: the result does not name the world that produced it: a naked answer.
- `E-WORLD-002`: the result does not name the method that produced it.
- `E-WORLD-003`: a declared world domain is empty.
- `E-WORLD-004`: a carrier element outside the declared domain.
- `E-WORLD-005`: a world table is incomplete over its declared carrier.
- `E-WORLD-006`: strict-Genesis firewall: a strict declaration cannot carry Genesis/custom world attachments.
- `E-WORLD-007`: a false-model claim in a world declaration (a model claiming what the declared world does not establish).
- `E-WORLD-008`: a declared world exceeds its size bound.
- `E-SYNTH-001`: law-synthesis carrier size exceeds the declared bound.
- `E-SYNTH-002`: a law-synthesis identity element is outside the declared carrier.

Genesis CLI (`crates/emath-cli-lab/src/genesis_cmd/`):

- `E-GEN-096`: a world/portfolio id is not a single path component (path safety for generated artifact trees).

Semantic admission (`crates/emath-sema/src/{admit/lowering,recognition}.rs`):

- `E-MEAS-001`: a measurement literal value is not a valid number.
- `E-MEAS-002`: an unknown distribution tag in a measurement literal (admitted tags: normal | uniform | lognormal).
- `E-MEAS-003`: a measurement literal with uncertainty is used as a strict value (uncertainty never silently collapses).

Scratch surface (`crates/emath-syntax/src/scratch.rs`, `crates/emath-cli/src/lib.rs`):

- `E-SYN-141`: scratch lines cannot mix with an explicit `emath` declaration; wrap the scratch block explicitly.
- `E-SYN-142`: conflicting or duplicated example bindings (conflicting examples refuse rather than pick a silent default).
- `E-SYN-143`: an L2 named declaration needs a body or L3 sections; a name alone is nothing.
- `E-SYN-144`: hidden desugaring is refused; every shorthand must expand through `emath expand`.
- `E-SYN-145`: a scratch line is not an expression, assignment, example, or intent verb.
- `E-SYN-146`: unlabeled defaults that hide solve/plot candidates are refused; name the domain.
- `E-SYN-147`: claiming exactness while holes remain open is refused; freeze does not upgrade an open hole.
- `E-SYN-148`: an unknown intent verb (known verbs: plot, solve, simulate, compile, differentiate, ...).

Diagnostics schema (`crates/emath-diagnostics/src/lib.rs`):

- `E-LAW-001`: the `emath.diagnostic.explanation v1` explanation schema identity (law-explanation surface).

Placeholders and probes:

- `E-FOO-001`: parser-teaching example code in cli docs (splitting `error: E-FOO-001: rest`); never emitted by production code.

Feature capsules (`crates/emath-schema/src/feature_capsule.rs`,
`crates/emath-sema/src/recognition/feature_capsule.rs`):

- `E-CAPSULE-001`: a capsule line is not a `field: value` pair (or a
  labeled requirement line is malformed).
- `E-CAPSULE-002`: a forbidden revision field is present.
- `E-CAPSULE-003`: a field is declared more than once (duplicate
  capsule row).
- `E-CAPSULE-004`: a required field is missing.
- `E-CAPSULE-005`: `schema` is not the feature-capsule schema identity.
- `E-CAPSULE-006`: `feature_id` does not parse as a feature id.
- `E-CAPSULE-007`: `class` does not parse as a feature class.
- `E-CAPSULE-008`: `maturity` does not parse as a maturity level.
- `E-CAPSULE-009`: `semantic_hash` does not parse as a semantic hash.
- `E-CAPSULE-010`: a slot value does not parse as a valid capsule slot.
- `E-CAPSULE-011`: an edge kind is not in the capsule edge-kind set.
- `E-CAPSULE-012`: a projection name is declared more than once.
- `E-CAPSULE-013`: a projection disposition is invalid.
- `E-CAPSULE-014`: `feature_id` and `class` disagree (id/class
  mismatch).
- `E-CAPSULE-015`: a field the capsule's class requires is missing.
- `E-CAPSULE-016`: a `cataloged` capsule carries a live projection
  (provided/generated/provider).
- `E-CAPSULE-017`: a blocking Spec Hole prevents accepted/stable
  publication.
- `E-CAPSULE-018`: a `reference` mode is illegal for the capsule's
  class.
- `E-CAPSULE-019`: a conformance declaration is missing or empty.
- `E-CAPSULE-020`: an illegal direct maturity transition.
- `E-CAPSULE-021`: a capsule field cannot be canonicalized for hashing
  (semantic-hash computation failed).
- `E-CAPSULE-022`: the declared `semantic_hash` does not match the
  computed hash of the capsule meaning. `reference_body` hashes as its
  canonical term, not the pretty spelling.
- `E-CAPSULE-023`: an executable reference is malformed (body is not an
  emath-term in canonical `apply(...)` or user expression syntax,
  `reference_params` missing, or an invalid reference parameter).
  `reference_signature` is inferred from the body when omitted.
- `E-CAPSULE-024`: a reference symbol is declared with conflicting
  arities.
- `E-CAPSULE-025`: a reference term uses a variable outside
  `reference_params`.
- `E-CAPSULE-026`: an executable reference body requires
  `reference: authored`.

Category kernels (`crates/emath-rt/src/body/control.rs`):

- `E-CAT-001`: a non-finite entry anywhere in the category carrier.
- `E-CAT-002`: a shape refusal (dimension mismatch, malformed face
  record, path geometry).
- `E-CAT-003`: an out-of-range or non-integral index.
- `E-CAT-004`: the composition law is violated.
- `E-CAT-005`: the identity law is violated.
- `E-CAT-006`: the associativity law is violated (or definedness
  disagreement).
- `E-CAT-007`: the carrier exceeds the certifiable bound (commutativity
  is never answered over an uncertifiable carrier).

CSV ingest (`crates/emath-sema/src/admit/lowering/csv.rs`):

- `E-CSV-001`: the time column is missing from the CSV header.
- `E-CSV-002`: the time column is ambiguous (several columns match).
- `E-CSV-003`: the value column is missing from the CSV header.
- `E-CSV-004`: the value column is ambiguous (several columns match).
- `E-CSV-005`: a CSV row's cell count disagrees with the header.
- `E-CSV-006`: a CSV header or row has an unclosed double-quote.
- `E-CSV-007`: the CSV series has no data rows.
- `E-CSV-008`: a CSV row's selected columns are not finite numbers.
- `E-CSV-009`: the CSV time column is nonincreasing (times must be
  strictly increasing).

Einsum kernel (`crates/emath-exec-ir/src/native_kernels/einsum.rs`):

- `E-EINSUM-001`: an einsum subscript/precondition refusal from the
  checked kernel (arithmetic fault).
- `E-EINSUM-002`: an einsum index is outside the operand's range.

Event actions (`crates/emath-sema/src/admit/declaration/events.rs`,
`crates/emath-exec-ir/src/runner/simulate/events.rs`):


Fit lane (`crates/emath-cli-lab/src/fit_cmd.rs`):

- `E-FIT-000`: a fit refusal whose message carries no explicit code
  (the fallback diagnostic code for the fit lane). The lane is a
  single E-KIND-GONE refusal; least-squares fitting is authored
  `.emath` (`language/modules/numerics/levenberg.emath`).

Simulate model selection (`crates/emath-cli/src/simulate_cmd.rs`):

- `E-MODEL-001`: the named model has no `emath model <name>`
  declaration.

Stdlib store (`crates/emath-store/src/stdlib.rs`):

- `E-STD-001`: a malformed stdlib envelope structure.
- `E-STD-002`: a forged object: the entry id does not re-derive from
  the content.
- `E-STD-003`: an id collision while inserting an object into the
  store graph.

Transitions (`crates/emath-sema/src/admit/declaration/transitions.rs`,
`crates/emath-ir/src/package.rs`,
`crates/emath-exec-ir/src/runner/simulate/events.rs`):

- `E-TRANS-006`: an event parameter name matches no declared model
  variable.

## Completeness annex: every issued code

Generated by `scripts/dump_error_codes.py` from the production crates;
regenerated as the registry changes. The workspace test
`crates/emath-hir/tests/registry_complete.rs` enforces that every emitted
code appears here (emitted ⊆ documented).

Emissions: **394 unique codes** from 516 rust files.
Not yet documented at generation time: **0**.

| Code | Emitting files | Context |
|---|---|---|
| `E-CAPSULE-001` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-001`<br>`{label} requires `name -> value`` |
| `E-CAPSULE-002` | crates/emath-schema/src/feature_capsule.rs | `forbidden revision field `{key}``<br>`forbidden revision field `{name}`` |
| `E-CAPSULE-003` | crates/emath-schema/src/feature_capsule.rs | `duplicate field `{key}``<br>`E-CAPSULE-003` |
| `E-CAPSULE-004` | crates/emath-schema/src/feature_capsule.rs | `missing required field `{name}`` |
| `E-CAPSULE-005` | crates/emath-schema/src/feature_capsule.rs | `schema must be `{FEATURE_CAPSULE_SCHEMA}`` |
| `E-CAPSULE-006` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-006` |
| `E-CAPSULE-007` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-007` |
| `E-CAPSULE-008` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-008` |
| `E-CAPSULE-009` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-009` |
| `E-CAPSULE-010` | crates/emath-schema/src/feature_capsule.rs | `{name}: {detail}` |
| `E-CAPSULE-011` | crates/emath-schema/src/feature_capsule.rs | `unknown edge kind `{kind}``<br>`unknown edge kind `{}`` |
| `E-CAPSULE-012` | crates/emath-schema/src/feature_capsule.rs | `duplicate projection `{name}``<br>`duplicate projection `{}`` |
| `E-CAPSULE-013` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-013` |
| `E-CAPSULE-014` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-014` |
| `E-CAPSULE-015` | crates/emath-schema/src/feature_capsule.rs | `class `{}` requires `{name}`` |
| `E-CAPSULE-016` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-016` |
| `E-CAPSULE-017` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-017` |
| `E-CAPSULE-018` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-018` |
| `E-CAPSULE-019` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-019` |
| `E-CAPSULE-020` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-020` |
| `E-CAPSULE-021` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-021` |
| `E-CAPSULE-022` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-022` |
| `E-CAPSULE-023` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-023`<br>`invalid reference parameter `{name}`` |
| `E-CAPSULE-024` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-024`<br>`reference symbol `{symbol}` declared with conflicting arities` |
| `E-CAPSULE-025` | crates/emath-schema/src/feature_capsule.rs | `reference term uses variable `{name}` outside `reference_params`` |
| `E-CAPSULE-026` | crates/emath-schema/src/feature_capsule.rs | `E-CAPSULE-026` |
| `E-CAT-001` | crates/emath-rt/src/body/control.rs |  |
| `E-CAT-002` | crates/emath-rt/src/body/control.rs |  |
| `E-CAT-003` | crates/emath-rt/src/body/control.rs |  |
| `E-CAT-004` | crates/emath-rt/src/body/control.rs |  |
| `E-CAT-005` | crates/emath-rt/src/body/control.rs |  |
| `E-CAT-006` | crates/emath-rt/src/body/control.rs |  |
| `E-CAT-007` | crates/emath-rt/src/body/control.rs |  |
| `E-CELL-001` | crates/emath-ir/src/capability/model.rs | `pure`<br>`E-CELL-001` |
| `E-CELL-002` | crates/emath-ir/src/capability/model.rs | `E-CELL-002`<br>`capability cell `{name}` declares no schema version (E-CELL-002)` |
| `E-CELL-003` | crates/emath-ir/src/capability/model.rs | `E-CELL-003` |
| `E-CELL-004` | crates/emath-ir/src/capability/model.rs | `E-CELL-004` |
| `E-CELL-005` | crates/emath-ir/src/capability/model.rs | `E-CELL-005`<br>`capability cell name `{name}` is empty or has no namespace path (E-CELL-005)` |
| `E-CELL-006` | crates/emath-exec-ir/src/interp/value.rs<br>crates/emath-exec-ir/src/term_compile/types.rs<br>crates/emath-ir/src/capability/model.rs | `E-CELL-006` |
| `E-CELL-007` | crates/emath-ir/src/capability/biform.rs<br>crates/emath-ir/src/capability/projection.rs | `E-CELL-007` |
| `E-CELL-008` | crates/emath-ir/src/capability/projection.rs | `E-CELL-008` |
| `E-CELL-009` | crates/emath-ir/src/capability/biform.rs | `E-CELL-009` |
| `E-CELL-010` | crates/emath-ir/src/capability/biform.rs | `E-CELL-010` |
| `E-CELL-011` | crates/emath-ir/src/capability/biform.rs | `E-CELL-011` |
| `E-CODEGEN-002` | crates/emath-build/src/package.rs<br>crates/emath-rust-backend/src/lib.rs<br>crates/emath-rust-backend/src/rust_ir/profiles.rs | `E-CODEGEN-002` |
| `E-CODEGEN-003` | crates/emath-rust-backend/src/rust_ir/profiles.rs | `library`<br>`E-CODEGEN-003` |
| `E-CODEGEN-004` | crates/emath-build/src/package.rs<br>crates/emath-rust-backend/src/lib.rs<br>crates/emath-rust-backend/src/rust_ir/profiles.rs<br>crates/emath-rust-backend/src/rust_ir/render.rs | `E-CODEGEN-004` |
| `E-CODEGEN-005` | crates/emath-build/src/deps.rs | `git dependency `{}` denied by policy`<br>`registry dependency `{}` denied by policy` |
| `E-CODEGEN-006` | crates/emath-build/src/deps.rs | `undeclared dependency: {}` |
| `E-CODEGEN-007` | crates/emath-build/src/deps.rs | `E-CODEGEN-007` |
| `E-CODEGEN-008` | crates/emath-build/src/script.rs | `locked build script cannot satisfy: {error}`<br>`E-CODEGEN-008` |
| `E-CODEGEN-009` | crates/emath-build/src/deps.rs | `E-CODEGEN-009` |
| `E-CODEGEN-010` | crates/emath-build/src/script.rs | `E-CODEGEN-010` |
| `E-CODEGEN-012` | crates/emath-build/src/builder/build.rs | `compile spec `{}/{}` outside the current subset (E-CODEGEN-012)` |
| `E-CODEGEN-013` | crates/emath-cli/src/cli_build.rs | `E-CODEGEN-013` |
| `E-CSV-001` | crates/emath-sema/src/admit/lowering/csv.rs | `E-CSV-001` |
| `E-CSV-002` | crates/emath-sema/src/admit/lowering/csv.rs | `E-CSV-002` |
| `E-CSV-003` | crates/emath-sema/src/admit/lowering/csv.rs | `E-CSV-003` |
| `E-CSV-004` | crates/emath-sema/src/admit/lowering/csv.rs | `E-CSV-004` |
| `E-CSV-005` | crates/emath-sema/src/admit/lowering/csv.rs | `E-CSV-005` |
| `E-CSV-006` | crates/emath-sema/src/admit/lowering/csv.rs | `E-CSV-006` |
| `E-CSV-007` | crates/emath-sema/src/admit/lowering/csv.rs | `E-CSV-007` |
| `E-CSV-008` | crates/emath-sema/src/admit/lowering/csv.rs | `E-CSV-008` |
| `E-CSV-009` | crates/emath-sema/src/admit/lowering/csv.rs | `E-CSV-009` |
| `E-CTOR-030` | crates/emath-build/src/builder/policy.rs | `missing state assignment for `{}` (E-CTOR-030)` |
| `E-CTOR-031` | crates/emath-build/src/builder/build.rs |  |
| `E-CTOR-032` | crates/emath-build/src/builder/policy.rs | ``require` must be a Boolean expression (E-CTOR-032)`<br>``ensure` must be a Boolean expression (E-CTOR-032)` |
| `E-CTOR-033` | crates/emath-build/src/builder/policy.rs | `a default value cannot read `state.{target}` (E-CTOR-033)`<br>``{target}` is not a state field (E-CTOR-033)` |
| `E-CTOR-034` | crates/emath-build/src/builder/build.rs<br>crates/emath-build/src/builder/policy.rs | `multiple constructors named `new` (E-CTOR-034)`<br>`duplicate constructor parameter `{param}` (E-CTOR-034)` |
| `E-CTOR-035` | crates/emath-build/src/builder/policy.rs | `duplicate assignment for state field `{target}` (E-CTOR-035)` |
| `E-CTOR-036` | crates/emath-build/src/builder/build.rs | `the primary constructor must be named `new` (E-CTOR-036)` |
| `E-CTOR-037` | crates/emath-build/src/builder/policy.rs | `constructor `{name}` delegates to unknown `{target}` (E-CTOR-037)` |
| `E-CTOR-038` | crates/emath-build/src/builder/policy.rs | `delegating constructor `{name}` cannot assign state directly (E-CTOR-038)` |
| `E-CTOR-039` | crates/emath-build/src/builder/policy.rs | `default for undeclared parameter `{target}` (E-CTOR-039)` |
| `E-DOM-001` | crates/emath-ir/src/domains.rs | `{name} value {value} outside domain {self}` |
| `E-DOM-002` | crates/emath-ir/src/domains.rs<br>crates/emath-sema/src/admit/lowering/exprs.rs<br>crates/emath-sema/src/admit/lowering/helpers.rs | `ill-formed domain interval [{low}, {high}]`<br>`E-DOM-002` |
| `E-EINSUM-001` | crates/emath-exec-ir/src/native_kernels/einsum.rs<br>crates/emath-rust-backend/src/codegen_render/kernels.rs | `E-EINSUM-001: {detail}`<br>`{{ let __e = emath_rt::einsum_checked({spec:?}, &[{}]).map_err(|error| match error {{ emath_rt::EinsumError::A` |
| `E-EINSUM-002` | crates/emath-exec-ir/src/native_kernels/einsum.rs<br>crates/emath-rust-backend/src/codegen_render/kernels.rs | `E-EINSUM-002: einsum index {index} is outside 0..{len}`<br>`{{ let __e = emath_rt::einsum_checked({spec:?}, &[{}]).map_err(|error| match error {{ emath_rt::EinsumError::A` |
| `E-EVAL-001` | crates/emath-cli-lab/src/eval_cmd/spec.rs<br>crates/emath-cli-lab/src/eval_cmd/sweep.rs | `E-EVAL-001` |
| `E-EVAL-002` | crates/emath-cli-lab/src/eval_cmd/spec.rs<br>crates/emath-cli/src/execution/advance.rs | `E-EVAL-002` |
| `E-EVAL-003` | crates/emath-cli-lab/src/eval_cmd/spec.rs | `E-EVAL-003` |
| `E-EVAL-004` | crates/emath-cli-lab/src/eval_cmd/spec.rs<br>crates/emath-cli-lab/src/eval_cmd/sweep.rs | `E-EVAL-004` |
| `E-EVAL-005` | crates/emath-build/src/probe.rs<br>crates/emath-cli-lab/src/eval_cmd/args.rs<br>crates/emath-cli-lab/src/eval_cmd/spec.rs<br>crates/emath-cli-lab/src/eval_cmd/sweep.rs<br>crates/emath-cli/src/execution/advance.rs | `E-EVAL-005`<br>`duplicate `--set` binding for input `{name}`` |
| `E-EVAL-006` | crates/emath-cli-lab/src/eval_cmd/spec.rs<br>crates/emath-cli-lab/src/eval_cmd/sweep.rs | `E-EVAL-006` |
| `E-EVAL-007` | crates/emath-cli-lab/src/eval_cmd/spec.rs<br>crates/emath-cli-lab/src/eval_cmd/sweep.rs<br>crates/emath-sema/src/admit.rs | `meaning identity refused: {error:?}`<br>`E-EVAL-007` |
| `E-EVAL-008` | crates/emath-build/src/probe.rs<br>crates/emath-cli-lab/src/eval_cmd/args.rs | `E-EVAL-008` |
| `E-EVID-101` | crates/emath-evidence/src/checker/artifact_check.rs<br>crates/emath-evidence/src/checker/negative.rs<br>crates/emath-evidence/src/lib.rs | `content of {path} does not hash to its declared id`<br>`E-EVID-101` |
| `E-EVID-102` | crates/emath-artifact/src/identity.rs<br>crates/emath-evidence/src/checker/artifact_check.rs | `E-EVID-102` |
| `E-EVID-103` | crates/emath-build/src/package.rs<br>crates/emath-evidence/src/checker/artifact_check.rs<br>crates/emath-evidence/src/checker/negative.rs | `E-EVID-103: goal requires {} but native build delivers only {}{}`<br>`E-EVID-103` |
| `E-EVID-104` | crates/emath-evidence/src/checker/artifact_check.rs<br>crates/emath-evidence/src/checker/negative.rs | `E-EVID-104` |
| `E-EVID-105` | crates/emath-cli/src/cli_artifacts.rs<br>crates/emath-evidence/src/checker/artifact_check.rs<br>crates/emath-evidence/src/checker/negative.rs | `error: E-EVID-105: no `emath/` state directory under {}`<br>`error: E-EVID-105: no published artifacts under {}` |
| `E-EVID-106` | crates/emath-evidence/src/checker/artifact_check.rs<br>crates/emath-evidence/src/checker/negative.rs | `E-EVID-106` |
| `E-EVID-107` | crates/emath-evidence/src/checker/artifact_check.rs | `resolved claim {} has no checker` |
| `E-EVID-108` | crates/emath-artifact/src/emit.rs<br>crates/emath-evidence/src/checker/artifact_check.rs | `E-EVID-108`<br>`emath/artifact-manifest.json does not conform to emath.artifact: {error}` |
| `E-EVID-109` | crates/emath-evidence/src/checker/artifact_check.rs | `E-EVID-109`<br>`manifest declares {path} but no such file exists` |
| `E-EVID-110` | crates/emath-evidence/src/checker/artifact_check.rs | `E-EVID-110` |
| `E-EVID-111` | crates/emath-evidence/src/checker/artifact_check.rs<br>crates/emath-evidence/src/lib.rs | `provider {} has no lock record`<br>`E-EVID-111` |
| `E-EVID-112` | crates/emath-evidence/src/checker/artifact_check.rs | `E-EVID-112` |
| `E-EVID-113` | crates/emath-evidence/src/checker/artifact_check.rs | `required artifact path is a symlink: {path}`<br>`declared artifact path is a symlink: {path}` |
| `E-EVID-114` | crates/emath-cli/src/tooling_cmd/inspect.rs<br>crates/emath-evidence/src/checker/artifact_check.rs | `error: E-EVID-114: manifest is not valid UTF-8 at {}`<br>`artifact document is not valid UTF-8: {path}` |
| `E-EVID-201` | crates/emath-evidence/src/checker/claimlint.rs<br>crates/emath-evidence/src/lib.rs | `E-EVID-201` |
| `E-EVID-301` | crates/emath-evidence/src/checker/negative.rs<br>crates/emath-evidence/src/checker/translation.rs<br>crates/emath-evidence/src/lib.rs | `E-EVID-301` |
| `E-EVID-302` | crates/emath-evidence/src/checker/translation.rs<br>crates/emath-evidence/src/lib.rs | `E-EVID-302` |
| `E-EVID-401` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/registry.rs | `unknown certificate contract {} v{version}` |
| `E-EVID-402` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/registry.rs | `E-EVID-402` |
| `E-EVID-403` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/registry.rs | `E-EVID-403` |
| `E-EVID-404` | crates/emath-evidence/src/ir.rs<br>crates/emath-evidence/src/lib.rs | `E-EVID-404` |
| `E-EVID-405` | crates/emath-evidence/src/ledger.rs<br>crates/emath-evidence/src/lib.rs | `E-EVID-405` |
| `E-EVID-501` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/revalidation.rs<br>crates/emath-evidence/src/store.rs | `unknown evidence record {id}` |
| `E-EVID-502` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/store.rs | `record {id} is already revoked (append-only)` |
| `E-EVID-503` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/store.rs<br>crates/emath-store/src/evidence_plane.rs<br>crates/emath-store/src/stdlib.rs | `E-EVID-503` |
| `E-EVID-504` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/store.rs | `record {old} is already superseded (append-only)`<br>`record {old} cannot supersede itself` |
| `E-EVID-505` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/revalidation.rs | `record {id} is stale and requires revalidation` |
| `E-EVID-506` | crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/proof.rs | `E-EVID-506` |
| `E-EVID-507` | crates/emath-evidence/src/certify.rs<br>crates/emath-evidence/src/lib.rs<br>crates/emath-evidence/src/proof.rs | `E-EVID-507` |
| `E-EVID-601` | crates/emath-store/src/materialization.rs | `E-EVID-601` |
| `E-EVID-602` | crates/emath-store/src/pack.rs | `E-EVID-602` |
| `E-EVID-603` | crates/emath-store/src/pack.rs | `E-EVID-603` |
| `E-EVID-604` | crates/emath-store/src/pack.rs | `E-EVID-604` |
| `E-EVID-605` | crates/emath-store/src/pack.rs | `E-EVID-605` |
| `E-EVID-606` | crates/emath-store/src/pack.rs | `E-EVID-606` |
| `E-FIT-000` | crates/emath-cli-lab/src/fit_cmd.rs | `E-FIT-000` |
| `E-FOO-001` | crates/emath-cli/src/cli_json.rs | `error: ` |
| `E-GEN-080` | crates/emath-cli-lab/src/eval_cmd/args.rs<br>crates/emath-cli-lab/src/genesis_cmd/analysis.rs | `E-GEN-080: genesis parse refused: {detail}`<br>`E-GEN-080` |
| `E-GEN-081` | crates/emath-cli-lab/src/genesis_cmd/analysis.rs | `E-GEN-081: genesis body expression is empty` |
| `E-GEN-082` | crates/emath-cli-lab/src/genesis_cmd/analysis.rs | `E-GEN-082: reference body is not unique: ambiguity {}` |
| `E-GEN-083` | crates/emath-cli-lab/src/genesis_cmd/analysis.rs | `E-GEN-083: signature inference refused: {detail}` |
| `E-GEN-084` | crates/emath-cli-lab/src/genesis_cmd/analysis.rs | `E-GEN-084: inferred signature rejects term: {error:?}` |
| `E-GEN-090` | crates/emath-cli-lab/src/genesis_cmd/genesis.rs | `E-GEN-090` |
| `E-GEN-091` | crates/emath-cli-lab/src/genesis_cmd/genesis.rs | `E-GEN-091` |
| `E-GEN-092` | crates/emath-cli-lab/src/eval_cmd/repl.rs<br>crates/emath-cli-lab/src/genesis_cmd/compile.rs<br>crates/emath-cli-lab/src/meaning_cmd.rs | `error: E-GEN-092: unknown world `{token}``<br>`error: E-GEN-092: unknown world `{label}`` |
| `E-GEN-093` | crates/emath-cli-lab/src/genesis_cmd/genesis.rs<br>crates/emath-cli/src/portfolio/meaning_lock/lock.rs | `error: E-GEN-093: `keep: pareto 0` keeps no candidates` |
| `E-GEN-094` | crates/emath-cli-lab/src/genesis_cmd/compile.rs<br>crates/emath-cli-lab/src/genesis_cmd/genesis.rs<br>crates/emath-world-ir/src/world_codegen_rust/model.rs | `error: E-GEN-094: CSA baseline evaluation failed on a total world`<br>`E-GEN-094` |
| `E-GEN-095` | crates/emath-cli-lab/src/genesis_cmd/genesis.rs | `interpretation_portfolio`<br>`error: E-GEN-095: ambiguous portfolio: lock a world or request `answer: return interpretation_portfolio`` |
| `E-GEN-096` | crates/emath-cli-lab/src/genesis_cmd/compile.rs | `{id}.json`<br>`error: E-GEN-096: portfolio id is not a single path component` |
| `E-GOAL-041` | crates/emath-sema/src/session/requests.rs | `E-GOAL-041` |
| `E-GOAL-042` | crates/emath-sema/src/session/requests.rs | `E-GOAL-042` |
| `E-GOAL-043` | crates/emath-cli/src/execution/run.rs<br>crates/emath-sema/src/session/requests.rs | `E-GOAL-043` |
| `E-GOAL-044` | crates/emath-sema/src/session/requests.rs | `E-GOAL-044` |
| `E-GOAL-045` | crates/emath-sema/src/session/requests.rs | `E-GOAL-045` |
| `E-GOAL-201` | crates/emath-plan/src/planner.rs | `E-GOAL-201`<br>`E-GOAL-201: no eligible plan` |
| `E-GROWTH-001` | crates/emath-exec-ir/src/growth.rs | `E-GROWTH-001` |
| `E-HOST-001` | crates/emath-rust-backend/src/rust_ir/host.rs | `host trait version `{version}` is not major.minor.patch`<br>`E-HOST-001` |
| `E-HOST-002` | crates/emath-rust-backend/src/rust_ir/host.rs |  |
| `E-HOST-003` | crates/emath-lab-core/src/error.rs<br>crates/emath-lab-core/src/manifest.rs<br>crates/emath-lab-core/src/manifest/fields.rs<br>crates/emath-lab-core/src/manifest/model.rs<br>crates/emath-lab-core/src/manifest/parse.rs<br>crates/emath-lab-core/src/stats.rs | `manifest JSON is missing field {name}`<br>`{at} must be a JSON object` |
| `E-HOST-004` | crates/emath-lab-core/src/manifest.rs<br>crates/emath-lab-core/src/manifest/model.rs<br>crates/emath-lab-core/src/manifest/parse.rs | `E-HOST-004` |
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
| `E-IMAGE-001` | crates/emath-exec-ir/src/image.rs | `E-IMAGE-001` |
| `E-IMAGE-002` | crates/emath-exec-ir/src/image.rs<br>crates/emath-exec-ir/src/shake.rs | `E-IMAGE-002` |
| `E-KIND-001` | crates/emath-sema/src/admit/sections_meta.rs | `E-KIND-001` |
| `E-KIND-002` | crates/emath-sema/src/admit/sections_meta.rs | `E-KIND-002` |
| `E-KIND-010` | crates/emath-build/src/builder/build.rs<br>crates/emath-hir/tests/registry_complete.rs | `function declarations cannot have constructors in this subphase (E-KIND-010)`<br>`E-KIND-010` |
| `E-KIND-011` | crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-hir/src/open.rs<br>crates/emath-sema/src/admit/declaration/setup.rs | `E-KIND-011`<br>`kind `{}` requires section `{name}`` |
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
| `E-KIND-310` | crates/emath-adapter-rumoca/src/subset.rs | `E-KIND-310` |
| `E-KIND-311` | crates/emath-adapter-rumoca/src/subset.rs | `E-KIND-311` |
| `E-KIND-312` | crates/emath-adapter-rumoca/src/subset.rs | `E-KIND-312` |
| `E-LAW-001` | crates/emath-cli/src/cli_parse.rs<br>crates/emath-cli/src/diagnostics.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs | `positional argument 1 (expected `.emath` file or diagnostic code such as `E-LAW-001`)`<br>`emath explain <file.emath> [<symbol>] or emath explain E-LAW-001 [--json]` |
| `E-LAZY-001` | crates/emath-exec-ir/src/lazy.rs | `E-LAZY-001`<br>`{pack}/{page}` |
| `E-LAZY-002` | crates/emath-exec-ir/src/lazy.rs | `E-LAZY-002` |
| `E-LINALG-004` | crates/emath-exec-ir/src/native_kernels/linear.rs<br>crates/emath-rust-backend/src/codegen_render/op_collections.rs | `E-LINALG-004: dense carrier extent overflow`<br>`E-LINALG-004: dense carrier data length does not match its shape` |
| `E-LOCK-001` | crates/emath-cli-lab/src/meaning_cmd.rs<br>crates/emath-cli/src/portfolio/meaning_lock/model.rs | `E-LOCK-001`<br>`error: E-LOCK-001: --cap must be an integer >= 1` |
| `E-LOCK-002` | crates/emath-cli/src/portfolio/meaning_lock/model.rs | `.emath`<br>`E-LOCK-002` |
| `E-LOCK-003` | crates/emath-cli/src/portfolio/meaning_lock/model.rs | `E-LOCK-003` |
| `E-LOCK-004` | crates/emath-cli-lab/src/eval_cmd/repl.rs<br>crates/emath-cli-lab/src/genesis_cmd/compile.rs<br>crates/emath-cli/src/portfolio/meaning_lock/model.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs | `E-LOCK-004`<br>`error: E-LOCK-004: --world `{label}` disagrees with locked fingerprint {:016x}; re-open the portfolio with `em` |
| `E-LOCK-005` | crates/emath-cli/src/portfolio/meaning_lock/model.rs | `E-LOCK-005` |
| `E-LOCK-006` | crates/emath-cli-lab/src/meaning_cmd.rs<br>crates/emath-cli/src/portfolio/meaning_lock/model.rs | `E-LOCK-006`<br>`error: E-LOCK-006: no lock entry for {} {}` |
| `E-MEAS-001` | crates/emath-sema/src/admit/lowering/terms.rs | `measurement literal value `{value}` is not a valid number`<br>`E-MEAS-001` |
| `E-MEAS-002` | crates/emath-sema/src/admit/lowering/terms.rs | `unknown distribution tag `~ {other}` (normal | uniform | lognormal)` |
| `E-MEAS-003` | crates/emath-sema/src/admit/lowering/terms.rs | `E-MEAS-003` |
| `E-MIGR-001` | crates/emath-hir/src/migrate.rs | `E-MIGR-001` |
| `E-MIGR-002` | crates/emath-hir/src/migrate.rs | `E-MIGR-002` |
| `E-MIGR-003` | crates/emath-hir/src/migrate.rs | `E-MIGR-003` |
| `E-MODEL-001` | crates/emath-cli/src/simulate_cmd.rs | `{} has no `emath model` declaration` |
| `E-NAME-020` | crates/emath-adapter-rumoca/src/conformance.rs<br>crates/emath-adapter-rumoca/src/structural.rs<br>crates/emath-exec-ir/src/constructor_layer/call.rs<br>crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-hir/src/notation.rs<br>crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/declaration/setup.rs | `E-NAME-020`<br>`duplicate variable `{}`` |
| `E-NAME-021` | crates/emath-hir/src/notation.rs | `E-NAME-021`<br>``{}` is unary and cannot be infix` |
| `E-NAME-022` | crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-exec-ir/src/constructor_layer/setup.rs<br>crates/emath-hir/src/notation.rs<br>crates/emath-sema/src/admit/sections_meta.rs<br>crates/emath-sema/src/session.rs | `E-NAME-022`<br>`duplicate declaration name `{}`` |
| `E-NAME-023` | crates/emath-sema/src/admit/lowering/series.rs<br>crates/emath-sema/src/admit/lowering/terms.rs<br>crates/emath-sema/src/admit/sections_meta.rs | `E-NAME-023`<br>`unknown variable` |
| `E-NAME-024` | crates/emath-build/src/builder/build.rs<br>crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/sections_meta.rs | `derived field `{name}` is not an output (E-NAME-024)`<br>`E-NAME-024` |
| `E-NUM-001` | crates/emath-ir/src/numeric.rs | `unknown numeric model `{other}` (known: strict-f64, interval-f64)` |
| `E-NUM-002` | crates/emath-ir/src/numeric.rs | `E-NUM-002` |
| `E-NUM-003` | crates/emath-ir/src/numeric.rs | `error-limit `{max_abs_error}` is not a finite non-negative bound`<br>`E-NUM-003` |
| `E-ODE-001` | crates/emath-exec-ir/src/runner/simulate/types.rs |  |
| `E-ODE-002` | crates/emath-exec-ir/src/runner/simulate/types.rs |  |
| `E-ODE-003` | crates/emath-exec-ir/src/runner/simulate/types.rs |  |
| `E-PACK-001` | crates/emath-exec-ir/src/install.rs | `E-PACK-001` |
| `E-PACK-002` | crates/emath-exec-ir/src/install.rs | `E-PACK-002` |
| `E-PACK-003` | crates/emath-exec-ir/src/install.rs | `E-PACK-003` |
| `E-PACK-004` | crates/emath-exec-ir/src/install.rs | `E-PACK-004` |
| `E-PACK-005` | crates/emath-exec-ir/src/install.rs | `E-PACK-005` |
| `E-PKG-020` | crates/emath-schema/src/load.rs | `E-PKG-020` |
| `E-PKG-050` | crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-exec-ir/src/constructor_layer/setup.rs<br>crates/emath-sema/src/recognition.rs<br>crates/emath-sema/src/session.rs | `E-PKG-050`<br>`unresolved file import `{}`` |
| `E-PKG-064` | crates/emath-sema/src/admit/attributes.rs | `E-PKG-064` |
| `E-PKG-065` | crates/emath-sema/src/admit/attributes.rs | `E-PKG-065` |
| `E-PKG-080` | crates/emath-cli-lab/src/agent_cmd.rs<br>crates/emath-cli-lab/src/eval_cmd/spec.rs<br>crates/emath-cli-lab/src/eval_cmd/sweep.rs<br>crates/emath-cli-lab/src/genesis_cmd/analysis.rs<br>crates/emath-cli/src/cli_build.rs<br>crates/emath-cli/src/cli_check.rs<br>crates/emath-cli/src/execution/package.rs<br>crates/emath-cli/src/execution/run.rs<br>crates/emath-cli/src/provenance_cmd.rs<br>crates/emath-cli/src/simulate_cmd.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-cli/src/tooling_cmd/migrate.rs<br>crates/emath-cli/src/tooling_cmd/run.rs<br>crates/emath-sema/src/session.rs | `cannot read source file ({})`<br>`cannot read spec: {}` |
| `E-PKG-081` | crates/emath-cli-lab/src/eval_cmd/args.rs<br>crates/emath-cli-lab/src/eval_cmd/spec.rs<br>crates/emath-cli-lab/src/eval_cmd/sweep.rs<br>crates/emath-cli/src/cli_check.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-sema/src/admit/sections_meta.rs | `source has no declarations ({})`<br>`E-PKG-081` |
| `E-PLG-001` | crates/emath-provider-api/src/plugin_sdk.rs | `E-PLG-001` |
| `E-PLG-002` | crates/emath-provider-api/src/plugin_sdk.rs | `E-PLG-002` |
| `E-PLG-003` | crates/emath-provider-api/src/plugin_sdk.rs | `plugin `{}` declares no capabilities`<br>`E-PLG-003` |
| `E-PLG-004` | crates/emath-provider-api/src/plugin_sdk.rs | `E-PLG-004` |
| `E-PLG-005` | crates/emath-provider-api/src/plugin_sdk.rs | `E-PLG-005` |
| `E-PROB-001` | crates/emath-rt/src/probability.rs | `E-PROB-001` |
| `E-PROB-002` | crates/emath-rt/src/probability.rs | `E-PROB-002` |
| `E-PROV-001` | crates/emath-adapter-dew/src/seam.rs | `E-PROV-001` |
| `E-PROV-002` | crates/emath-adapter-dew/src/seam.rs | `E-PROV-002` |
| `E-PROV-030` | crates/emath-adapter-dew/src/backends.rs<br>crates/emath-adapter-dew/src/dexpr.rs<br>crates/emath-adapter-dew/src/lib.rs<br>crates/emath-adapter-dew/src/mapping.rs | `E-PROV-030: generated Rust fragment fails the syntax sanity gate`<br>`E-PROV-030: integer literal `{text}` is not a finite f64` |
| `E-PROV-031` | crates/emath-adapter-dew/src/backends.rs<br>crates/emath-adapter-dew/src/capability.rs | `E-PROV-031: accelerator target `{}` has no admitted subset`<br>`E-PROV-031: backend `{}` is outside the Dew capability inventory` |
| `E-PROV-033` | crates/emath-adapter-dew/src/dexpr.rs | `E-PROV-033` |
| `E-PROV-210` | crates/emath-adapter-rumoca/src/structural.rs | `E-PROV-210` |
| `E-PROV-220` | crates/emath-adapter-rumoca/src/lower.rs<br>crates/emath-adapter-rumoca/src/provider/sim.rs | `underdetermined: no equation produces {expected}`<br>`underdetermined: no equation produces `{variable}`` |
| `E-PROV-221` | crates/emath-adapter-rumoca/src/lower.rs | `E-PROV-221` |
| `E-PROV-222` | crates/emath-adapter-rumoca/src/lower.rs | `multiple equations produce `{identifier}`` |
| `E-PROV-223` | crates/emath-adapter-rumoca/src/lower.rs | `E-PROV-223` |
| `E-PROV-230` | crates/emath-adapter-rumoca/src/provider/exec.rs<br>crates/emath-adapter-rumoca/src/provider/sim.rs | `missing parameter value for `{parameter}``<br>`unknown variable `{name}` during evaluation` |
| `E-PROV-231` | crates/emath-adapter-rumoca/src/provider/exec.rs | `E-PROV-231` |
| `E-PROV-232` | crates/emath-adapter-rumoca/src/provider/exec.rs | `assignment to unknown derivative `der({state})``<br>`unknown derivative `der({name})` during evaluation` |
| `E-PROV-233` | crates/emath-adapter-rumoca/src/provider/exec.rs | `E-PROV-233` |
| `E-PROV-234` | crates/emath-adapter-rumoca/src/provider/exec.rs | `unresolvable initial value `{value}` for `{target}``<br>`initial condition targets unknown state `{target}`` |
| `E-PROV-235` | crates/emath-adapter-rumoca/src/provider/exec.rs<br>crates/emath-adapter-rumoca/src/provider/sim.rs | `E-PROV-235`<br>`invalid time step `dt={}`` |
| `E-PROV-236` | crates/emath-adapter-rumoca/src/provider/exec.rs | `E-PROV-236` |
| `E-PROV-237` | crates/emath-adapter-rumoca/src/provider/artifact.rs<br>crates/emath-adapter-rumoca/src/provider/exec.rs<br>crates/emath-adapter-rumoca/src/provider/sim.rs | `E-PROV-237` |
| `E-PROV-238` | crates/emath-adapter-rumoca/src/provider/exec.rs | `E-PROV-238` |
| `E-PROV-239` | crates/emath-adapter-rumoca/src/provider/artifact.rs<br>crates/emath-adapter-rumoca/src/provider/sim.rs | `E-PROV-239`<br>`no flattened equation for `{}`` |
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
| `E-PROVIDER-001` | crates/emath-provider-api/src/adapter.rs | `E-PROVIDER-001` |
| `E-PROVIDER-002` | crates/emath-provider-api/src/adapter.rs | `E-PROVIDER-002` |
| `E-PROVIDER-003` | crates/emath-provider-api/src/adapter.rs | `E-PROVIDER-003` |
| `E-RAT-001` | crates/emath-rt/src/body/exact.rs | `E-RAT-001: denominator is zero`<br>`E-RAT-001: noncanonical rational` |
| `E-RAT-002` | crates/emath-rt/src/body/exact.rs | `E-RAT-002: exact rational intermediate exceeds i128` |
| `E-REG-020` | crates/emath-registry/src/lib.rs | `unknown package `{package}`` |
| `E-REG-021` | crates/emath-registry/src/lib.rs | `E-REG-021`<br>`pin `{name}@{version}` does not resolve: {}` |
| `E-REG-022` | crates/emath-registry/src/lib.rs | `E-REG-022` |
| `E-REG-023` | crates/emath-registry/src/lib.rs | `E-REG-023` |
| `E-REG-024` | crates/emath-registry/src/lib.rs | `no usable version of `{package}` satisfies the constraint` |
| `E-REG-030` | crates/emath-registry/src/lib.rs | `E-REG-030` |
| `E-REG-031` | crates/emath-registry/src/lib.rs | `E-REG-031` |
| `E-RES-100` | crates/emath-plan/src/planner.rs | `{}:resume:nodes>{}`<br>`E-RES-100: {} plan nodes exceed the {} node budget` |
| `E-RES-110` | crates/emath-lab-core/src/holes/synth.rs |  |
| `E-RES-111` | crates/emath-lab-core/src/holes/synth.rs | `satisfy` |
| `E-RES-120` | crates/emath-build/src/package.rs | `E-RES-120: cargo exceeded the {timeout:?} wall-clock budget` |
| `E-SCHEMA-001` | crates/emath-schema/src/registry.rs | `1.0.0`<br>`E-SCHEMA-001` |
| `E-SEC-101` | crates/emath-exec-ir/src/constructor_layer/admit.rs<br>crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/declaration/setup.rs | `E-SEC-101`<br>`parameters` |
| `E-SEC-130` | crates/emath-sema/src/admit/declaration/setup.rs<br>crates/emath-syntax/src/scratch/render.rs | `inputs`<br>`E-SEC-130` |
| `E-SEC-133` | crates/emath-cli/src/cli_dispatch.rs<br>crates/emath-rust-backend/src/generate.rs<br>crates/emath-sema/src/session/requests.rs |  |
| `E-SHAKE-001` | crates/emath-exec-ir/src/shake.rs | `E-SHAKE-001` |
| `E-SHAKE-002` | crates/emath-exec-ir/src/shake.rs | `E-SHAKE-002` |
| `E-SHAPE-001` | crates/emath-exec-ir/src/interp/helpers.rs<br>crates/emath-exec-ir/src/native_kernels/calculus.rs<br>crates/emath-ir/src/shapes.rs<br>crates/emath-rust-backend/src/codegen_render/kernels.rs | `E-SHAPE-001: std.capability.geometry.inner-product requires equal vector lengths`<br>`E-SHAPE-001` |
| `E-SHAPE-002` | crates/emath-ir/src/shapes.rs<br>crates/emath-sema/src/admit/lowering/exprs.rs | `E-SHAPE-002`<br>`dimension mismatch in matrix-vector multiplication: matrix columns {c_e:?} != vector length {v_e:?}` |
| `E-SHAPE-003` | crates/emath-ir/src/shapes.rs | `E-SHAPE-003`<br>`slice end {rows_end} exceeds extent {size}` |
| `E-SHAPE-004` | crates/emath-ir/src/shapes.rs<br>crates/emath-sema/src/admit/expr_helpers.rs<br>crates/emath-sema/src/admit/lowering/helpers.rs | `E-SHAPE-004`<br>`declared extent `{name}` is not a well-formed shape` |
| `E-SHAPE-005` | crates/emath-sema/src/admit/expr_helpers.rs<br>crates/emath-sema/src/admit/lowering/exprs.rs<br>crates/emath-sema/src/admit/lowering/helpers.rs | `E-SHAPE-005`<br>`tensor broadcast mismatch: {lhs} vs {rhs}` |
| `E-SHAPE-006` | crates/emath-exec-ir/src/native_kernels/linear.rs<br>crates/emath-rust-backend/src/codegen_render/op_collections.rs<br>crates/emath-sema/src/admit/expr_helpers.rs<br>crates/emath-sema/src/admit/lowering/helpers.rs | `E-SHAPE-006: index requires {} subscript(s), found {}`<br>`E-SHAPE-006: dense index offset overflow` |
| `E-STD-001` | crates/emath-store/src/stdlib.rs | `E-STD-001` |
| `E-STD-002` | crates/emath-store/src/stdlib.rs | `E-STD-002` |
| `E-STD-003` | crates/emath-store/src/stdlib.rs | `E-STD-003` |
| `E-SYM-001` | crates/emath-ir/src/symbolic.rs | `E-SYM-001`<br>`replacement references uncaptured variable `{name}`` |
| `E-SYM-002` | crates/emath-cli/src/cli_artifacts.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-ir/src/symbolic.rs | `E-SYM-002`<br>`polynomial degree exceeds {MAX_POLYNOMIAL_DEGREE}` |
| `E-SYM-003` | crates/emath-cli/src/cli_artifacts.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-ir/src/symbolic.rs<br>crates/emath-sema/src/session.rs | `E-SYM-002`<br>`E-SYM-003` |
| `E-SYM-004` | crates/emath-ir/src/symbolic.rs | `E-SYM-004` |
| `E-SYN-100` | crates/emath-syntax/src/lexer/engine.rs | `E-SYN-100` |
| `E-SYN-101` | crates/emath-exec-ir/src/install.rs<br>crates/emath-sema/src/admit/lowering/series.rs<br>crates/emath-sema/src/admit/lowering/terms.rs<br>crates/emath-sema/src/session/requests.rs<br>crates/emath-syntax/src/lexer/engine.rs<br>crates/emath-syntax/src/parser.rs<br>crates/emath-syntax/src/parser/decl.rs<br>crates/emath-syntax/src/parser/expr/braket.rs<br>crates/emath-syntax/src/parser/expr/forms.rs<br>crates/emath-syntax/src/parser/expr/infix.rs<br>crates/emath-syntax/src/parser/expr/literals.rs<br>crates/emath-syntax/src/parser/expr/postfix.rs<br>crates/emath-syntax/src/parser/expr/primary.rs<br>crates/emath-syntax/src/parser/expr/units.rs<br>crates/emath-syntax/src/parser/stmt.rs<br>crates/emath-syntax/src/parser/stmt_binders.rs<br>crates/emath-syntax/src/parser/stmt_idents.rs<br>crates/emath-syntax/src/parser/stmt_idents/default.rs<br>crates/emath-syntax/src/parser/stmt_idents/series.rs<br>crates/emath-syntax/src/parser/stmt_suite.rs<br>crates/emath-syntax/src/parser/types.rs | `E-SYN-101`<br>`unexpected character `{}`` |
| `E-SYN-102` | crates/emath-syntax/src/parser/expr/literals.rs<br>crates/emath-syntax/src/parser/expr/postfix.rs<br>crates/emath-syntax/src/parser/expr/primary.rs<br>crates/emath-syntax/src/parser/expr/units.rs<br>crates/emath-syntax/src/parser/stmt_binders.rs<br>crates/emath-syntax/src/parser/stmt_idents/default.rs<br>crates/emath-syntax/src/parser/stmt_idents/look.rs<br>crates/emath-syntax/src/parser/types.rs | `E-SYN-102` |
| `E-SYN-103` | crates/emath-hir/src/open.rs<br>crates/emath-sema/src/admit/declaration/setup.rs<br>crates/emath-syntax/src/parser/stmt_idents/series.rs | `E-SYN-103` |
| `E-SYN-105` | crates/emath-syntax/src/lexer/engine.rs | `E-SYN-105` |
| `E-SYN-106` | crates/emath-syntax/src/lexer/engine.rs<br>crates/emath-syntax/src/parser/expr.rs | `E-SYN-106` |
| `E-SYN-108` | crates/emath-syntax/src/lexer/engine.rs | `E-SYN-108` |
| `E-SYN-109` | crates/emath-syntax/src/lexer/engine.rs | `E-SYN-109`<br>`invalid string escape `\\{}`` |
| `E-SYN-110` | crates/emath-syntax/src/layout.rs<br>crates/emath-syntax/src/parser.rs<br>crates/emath-syntax/src/parser/decl.rs<br>crates/emath-syntax/src/parser/expr/forms.rs<br>crates/emath-syntax/src/parser/expr/literals.rs<br>crates/emath-syntax/src/parser/expr/primary.rs<br>crates/emath-syntax/src/parser/stmt.rs<br>crates/emath-syntax/src/parser/stmt_binders.rs<br>crates/emath-syntax/src/parser/stmt_idents.rs<br>crates/emath-syntax/src/parser/stmt_idents/default.rs | `E-SYN-110`<br>`expected an expression, found {}` |
| `E-SYN-111` | crates/emath-syntax/src/parser/decl.rs<br>crates/emath-syntax/src/parser/expr/infix.rs<br>crates/emath-syntax/src/parser/expr/postfix.rs<br>crates/emath-syntax/src/parser/expr/primary.rs<br>crates/emath-syntax/src/parser/stmt.rs<br>crates/emath-syntax/src/parser/stmt_binders.rs<br>crates/emath-syntax/src/parser/stmt_idents.rs<br>crates/emath-syntax/src/parser/stmt_idents/default.rs<br>crates/emath-syntax/src/parser/stmt_idents/series.rs<br>crates/emath-syntax/src/parser/stmt_suite.rs | `E-SYN-111` |
| `E-SYN-112` | crates/emath-syntax/src/parser/stmt_suite.rs | `E-SYN-112` |
| `E-SYN-113` | crates/emath-syntax/src/lexer/engine.rs | `E-SYN-113` |
| `E-SYN-114` | crates/emath-syntax/src/lexer/engine.rs | `E-SYN-114` |
| `E-SYN-115` | crates/emath-syntax/src/lexer/engine.rs | `E-SYN-115` |
| `E-SYN-116` | crates/emath-syntax/src/lexer.rs | `source is {} bytes; limit is {max} bytes` |
| `E-SYN-117` | crates/emath-sema/src/admit/attributes.rs<br>crates/emath-syntax/src/parser/decl.rs | `E-SYN-117` |
| `E-SYN-118` | crates/emath-sema/src/admit/attributes.rs | `E-SYN-118` |
| `E-SYN-120` | crates/emath-cli/src/lsp/server.rs<br>crates/emath-core/src/parse.rs<br>crates/emath-sema/src/session/requests.rs | `E-SYN-120` |
| `E-SYN-121` | crates/emath-syntax/src/parser/expr/postfix.rs | `E-SYN-121` |
| `E-SYN-122` | crates/emath-syntax/src/parser/decl.rs | `E-SYN-122` |
| `E-SYN-123` | crates/emath-syntax/src/parser/decl.rs | `E-SYN-123` |
| `E-SYN-141` | crates/emath-syntax/src/scratch/lower.rs | `E-SYN-141` |
| `E-SYN-142` | crates/emath-syntax/src/scratch/lower.rs | `E-SYN-142` |
| `E-SYN-143` | crates/emath-syntax/src/scratch/intent.rs | `E-SYN-143` |
| `E-SYN-144` | crates/emath-syntax/src/scratch/lower.rs | `E-SYN-144` |
| `E-SYN-145` | crates/emath-syntax/src/scratch/lower.rs | `E-SYN-145` |
| `E-SYN-146` | crates/emath-syntax/src/scratch/lower.rs | `E-SYN-146` |
| `E-SYN-147` | crates/emath-cli-lab/src/cli_freeze.rs<br>crates/emath-syntax/src/scratch/lower.rs | `E-SYN-147`<br>`E-SYN-147 claiming exactness while holes remain open is refused; freeze does not upgrade authority` |
| `E-SYN-148` | crates/emath-syntax/src/scratch/lower.rs | `E-SYN-148` |
| `E-SYN-149` | crates/emath-syntax/src/scratch/intent.rs | `E-SYN-149` |
| `E-SYN-150` | crates/emath-syntax/src/scratch/intent.rs | `E-SYN-150` |
| `E-SYN-151` | crates/emath-syntax/src/scratch/lower.rs | `E-SYN-151` |
| `E-SYN-153` | crates/emath-syntax/src/layout.rs | `E-SYN-153` |
| `E-SYN-154` | crates/emath-core/src/tree/expr.rs<br>crates/emath-syntax/src/parser/expr/primary.rs | `E-SYN-154` |
| `E-SYN-155` | crates/emath-cli-lab/src/cli_scratch.rs |  |
| `E-SYN-156` | crates/emath-syntax/src/parser/stmt_suite.rs | `E-SYN-156`<br>`coefficient `{text}` is not a non-negative integer` |
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
| `E-SYNTH-001` | crates/emath-genesis/src/world_decl.rs |  |
| `E-SYNTH-002` | crates/emath-genesis/src/world_decl.rs |  |
| `E-TLT-004` | crates/emath-cli-lab/src/host_cmd.rs<br>crates/emath-cli-lab/src/provider_cmd.rs<br>crates/emath-cli/src/catalog.rs | `error: E-TLT-004: benchmarking `{}` is not a CLI comparison; measure via `cargo bench --profile release-perf -`<br>`schema` |
| `E-TLT-005` | crates/emath-cli/src/cli_artifacts.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-cli/src/tooling_cmd/inspect.rs | `error: E-TLT-005: cannot list artifact state directory {}`<br>`E-TLT-005` |
| `E-TLT-006` | crates/emath-cli-lab/src/provider_cmd.rs<br>crates/emath-cli/src/catalog.rs | `code`<br>`error: E-TLT-006: network/source sync is disabled (offline-first); use --dry-run` |
| `E-TLT-007` | crates/emath-cli-lab/src/provider_cmd.rs<br>crates/emath-cli-lab/src/vendor_cmd.rs | `error: E-TLT-007: upstream lock missing at {}`<br>`error: E-TLT-007: upstream lock is empty at {}` |
| `E-TLT-010` | crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-cli/src/tooling_cmd/new.rs | `E-TLT-010`<br>`error: invalid package name `{name}` (E-TLT-010)` |
| `E-TLT-011` | crates/emath-cli/src/catalog.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-cli/src/tooling_cmd/new.rs | `new`<br>`E-TLT-011` |
| `E-TLT-012` | crates/emath-build/src/package.rs<br>crates/emath-cli/src/catalog.rs | `tests passed`<br>`E-TLT-012: generated crate has no `#[test]` tests; --verify refuses an empty test surface (add a `tests:` sect` |
| `E-TLT-013` | crates/emath-cli-lab/src/provider_cmd.rs | `code`<br>`error: E-TLT-013: provider `{id}` has no in-CLI negative-control battery; run `cargo test` against tests/emath` |
| `E-TLT-016` | crates/emath-cli-lab/src/provider_cmd.rs | `error: E-TLT-016: unknown provider `{id}`` |
| `E-TRANS-006` | crates/emath-ir/src/package.rs |  |
| `E-TYPE-001` | crates/emath-sema/src/admit/declaration/setup.rs | ``{name}` needs an explicit constructor type, not an inferred default` |
| `E-TYPE-002` | crates/emath-cli/src/catalog.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-core/src/diagnostic.rs<br>crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/lowering/series.rs | `diagnostic-code lookup (`emath explain E-TYPE-002`). File/plan explanation refuses: there is no goals planner`<br>`emath explain E-TYPE-002` |
| `E-TYPE-003` | crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-sema/src/admit.rs | `E-TYPE-003`<br>`transformation_rule_unavailable` |
| `E-TYPE-010` | crates/emath-exec-ir/src/constructor_layer/admit.rs<br>crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/lowering.rs | `type`<br>`E-TYPE-010` |
| `E-TYPE-011` | crates/emath-sema/src/admit/lowering/series.rs<br>crates/emath-sema/src/admit/lowering/terms.rs | `E-TYPE-011`<br>`non-finite constant `{text}` refused under strict-f64 policy` |
| `E-TYPE-012` | crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-exec-ir/src/interp/eval_calls.rs<br>crates/emath-exec-ir/src/native_kernel.rs<br>crates/emath-exec-ir/src/native_kernels/calculus.rs<br>crates/emath-exec-ir/src/native_kernels/checked.rs<br>crates/emath-exec-ir/src/native_kernels/einsum.rs<br>crates/emath-exec-ir/src/native_kernels/linear.rs<br>crates/emath-exec-ir/src/native_kernels/probability.rs<br>crates/emath-exec-ir/src/native_kernels/program_solve.rs<br>crates/emath-rt/src/body/numeric.rs<br>crates/emath-rust-backend/src/codegen_render/kernels.rs<br>crates/emath-sema/src/admit/expr_helpers.rs<br>crates/emath-sema/src/admit/infer.rs<br>crates/emath-sema/src/admit/lowering.rs<br>crates/emath-sema/src/admit/lowering/call.rs<br>crates/emath-sema/src/admit/lowering/call/carriers.rs<br>crates/emath-sema/src/admit/lowering/csv.rs<br>crates/emath-sema/src/admit/lowering/exprs.rs<br>crates/emath-sema/src/admit/lowering/goals.rs<br>crates/emath-sema/src/admit/lowering/helpers.rs<br>crates/emath-sema/src/admit/lowering/series.rs<br>crates/emath-sema/src/admit/lowering/terms.rs | `E-TYPE-012: checked-add arguments must be Int`<br>`E-TYPE-012: euclidean-gcd arguments must be Int` |
| `E-TYPE-101` | crates/emath-adapter-rumoca/src/conformance.rs<br>crates/emath-adapter-rumoca/src/structural.rs | `E-TYPE-101` |
| `E-TYPE-102` | crates/emath-adapter-rumoca/src/structural.rs | `E-TYPE-102` |
| `E-TYPE-103` | crates/emath-adapter-rumoca/src/structural.rs | `E-TYPE-103` |
| `E-TYPE-110` | crates/emath-syntax/src/parser/types.rs | `fn`<br>`E-TYPE-110` |
| `E-TYPE-111` | crates/emath-syntax/src/parser/stmt_idents.rs | `E-TYPE-111` |
| `E-TYPE-112` | crates/emath-syntax/src/parser/stmt_binders.rs | `E-TYPE-112` |
| `E-TYPE-310` | crates/emath-ir/src/numeric.rs |  |
| `E-TYPE-311` | crates/emath-ir/src/numeric.rs | `E-TYPE-311` |
| `E-TYPE-312` | crates/emath-ir/src/type_system.rs | `E-TYPE-312`<br>`cannot unify {} with {}` |
| `E-TYPE-313` | crates/emath-ir/src/type_system.rs | `E-TYPE-313` |
| `E-TYPE-314` | crates/emath-ir/src/type_system.rs | `E-TYPE-314` |
| `E-UNIT-100` | crates/emath-adapter-rumoca/src/structural.rs<br>crates/emath-ir/src/units.rs | `unknown variable `{name}` in dimensional analysis` |
| `E-UNIT-101` | crates/emath-adapter-rumoca/src/structural.rs<br>crates/emath-ir/src/units.rs<br>crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/infer.rs<br>crates/emath-sema/src/admit/lowering.rs<br>crates/emath-sema/src/admit/lowering/exprs.rs<br>crates/emath-sema/src/admit/lowering/terms.rs | `dimension mismatch in sum: {left} vs {right}`<br>`event `{}` condition: {}` |
| `E-UNIT-102` | crates/emath-ir/src/units.rs<br>crates/emath-sema/src/admit/infer.rs<br>crates/emath-sema/src/admit/lowering/series.rs<br>crates/emath-sema/src/admit/lowering/terms.rs | `E-UNIT-102` |
| `E-UNIT-103` | crates/emath-adapter-rumoca/src/structural.rs | `event `{}` condition: {}`<br>`event `{}` condition is not dimensionless` |
| `E-UNIT-104` | crates/emath-cli/src/tooling_cmd/fmt.rs<br>crates/emath-core/src/units.rs<br>crates/emath-ir/src/units.rs<br>crates/emath-sema/src/admit/lowering.rs | `E-UNIT-104`<br>`unknown unit `{name}`` |
| `E-UNIT-105` | crates/emath-ir/src/units.rs<br>crates/emath-sema/src/admit/lowering/series.rs<br>crates/emath-sema/src/admit/lowering/terms.rs | ``Per<{inner}>` is invalid: affine units have no inverse`<br>`E-UNIT-105` |
| `E-WORLD-001` | crates/emath-genesis/src/world_result.rs | `E-WORLD-001` |
| `E-WORLD-002` | crates/emath-genesis/src/world_result.rs | `E-WORLD-002`<br>`result carries no producer method label (E-WORLD-002)` |
| `E-WORLD-003` | crates/emath-genesis/src/world_decl.rs | `E-WORLD-003` |
| `E-WORLD-004` | crates/emath-genesis/src/world_decl.rs | `E-WORLD-004` |
| `E-WORLD-005` | crates/emath-genesis/src/world_decl.rs | `E-WORLD-005` |
| `E-WORLD-006` | crates/emath-genesis/src/world_decl.rs | `E-WORLD-006` |
| `E-WORLD-007` | crates/emath-genesis/src/world_decl.rs | `E-WORLD-007` |
| `E-WORLD-008` | crates/emath-genesis/src/world_decl.rs | `E-WORLD-008` |
