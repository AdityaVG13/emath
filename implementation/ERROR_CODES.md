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
| `E-GOAL-*` | requests/planning | no eligible plan, weakened requirement |
| `E-PROV-*` | conversion contracts | bit-identical primitive (410), affine unit (411), sparse index (412) |
| `E-CODEGEN-*` | backend/artifact | unsupported lowering, manifest mismatch |
| `E-RES-*` | resources | budget, memory, cancellation |
| `E-HOST-*` | host/lab | ABI mismatch (001), invalid experiment manifest (003), insufficient evidence (006), incomparable experiment (008), self-comparison refused (016) |
| `E-TLT-*` | tooling/CLI | bench harness absent (E-TLT-004), manifest missing (E-TLT-005), network sync disabled (E-TLT-006), invalid package name (E-TLT-010), scaffold overwrite refused (E-TLT-011) |
| `E-CAPSULE-*` | feature capsules | malformed field/line (001), duplicate field (003), id/class mismatch (014), semantic-hash mismatch (022), reference-term refusals (023-026) |
| `E-TRANS-*` | transitions | undeclared event (001), bad action target (002/005), non-assignment body (003/004), unbound event parameter (006/007), runtime action fault (008) |

Codes are stable identifiers. Messages can improve without changing code meaning. A code is never repurposed.

## Issued codes (implementation evidence)

Tooling (`crates/emath-cli/src/tooling_cmd.rs`, `crates/emath-cli/src/catalog.rs`):

- `E-TLT-004`: `bench` requires the external `cargo bench` harness.
- `E-TLT-005`: `inspect`/`verify`: artifact state or manifest missing.
- `E-TLT-006`: `fork sync` refuses network/source sync (offline-first);
  `--dry-run` allowed.
- `E-TLT-010`: `new`: invalid package name.
- `E-TLT-011`: `new`: refuses to overwrite an existing scaffold.
- `E-TLT-012`: `test`: generated crate has no
  `#[test]` tests; refused so an empty test surface is never reported as
  passing (add a `tests:` section to the spec).

Resources (`crates/emath-build/src/lib.rs`, `crates/emath-tui/src/experiment/mod.rs`):

- `E-RES-120`: cargo (verify/test/run) exceeded the wall-clock timeout

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
  a one-error-one-OK result is a hard failure.
- `E-SYN-120`: `check`/`plan`/`parse_text` refuse when no source-parser
  backend is installed: a host must wire
  `emath_syntax::install_source_parser` once per process (CLI and LSP
  do this at startup). The refusal is typed; never a silent empty parse.
- `E-MEANING-001`: an admitted module could not assemble its complete
  scoped declaration graph (a local name clobbers an import, or a
  retained import module is missing), so no meaning tree is recorded.

Generated-crate profiles (`crates/emath-rust-backend/src/rust_ir/profiles.rs`,
checked on the build path via `CrateProfile::Library`):
`emath build` emits one NAMED entry per runnable function
(`pub fn <function_name>`), so sibling calls compile in one crate. The
old `E-CODEGEN-013` one-entry-per-crate spelling is retired and is not
emitted.

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
- `E-SYN-153`: hanging infix: a line ends with a binary operator, so the
  expression is incomplete, but NEWLINE fires outside brackets and the parse
  splits. The diagnostic teaches the bracket idiom: wrap the expression in
  `()` (or `[]`) to continue it across lines (F2).
- `E-SYN-154`: ambiguous brace: `{name: value}` in expression position
  without a path prefix. Inline records are
  path-prefixed (`Point:{x: 1.0}`); bare braces are set literals, so the
  record spelling without a path is refused instead of silently reading as
  a malformed one-element set.
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
- `E-NOTATION-AMBIG`: context-scoped brackets:
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
  token (`+ - * / ^ == != < <= > >= and or not = := -> => :: . ..
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

Kinds (`crates/emath-schema/src/load.rs`, `crates/emath-sema/src/admit.rs`):

- `E-KIND-011`: kind schema is missing a
  `RepeatPolicy::ExactlyOne` section (`SectionManifest::check` and
  semantic admission in `admit.rs`). `inputs:` and `outputs:` are
  `AtMostOne` on the core function schema, so omitting either is not
  this refusal.

Units (`crates/emath-ir/src/units.rs`):

- `E-UNIT-100`: unknown variable in dimensional analysis. A missing
  variable is refused, never treated as dimensionless.
- `E-UNIT-101`: dimension mismatch: two operands that must share
  dimensions do not (unit compatibility in `units.rs`).
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
Observations (`crates/emath-sema/src/admit/declaration.rs`,
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

Types (`crates/emath-ir/src/type_system.rs`):

- `E-TYPE-312`: `unify` refuses two refined types whose predicates differ,
  or whose discharge is different (e.g. a statically discharged refinement
  against a construct-time one); the discharge is never silently dropped.

Lab (`crates/emath-lab-core/src`, `crates/emath-rust-backend/src/rust_ir/host.rs`):

- `E-HOST-001`: rust-ir host gate: toolchain/ABI incompatible with what
  the crate declares (typed refusal, never a silent fallback).
- `E-HOST-003`: invalid experiment manifest: wrong schema, duplicate
  partitions/metrics/kill-rule ids, non-positive thresholds, malformed
  canonical JSON.
- `E-HOST-006`: insufficient evidence: too few samples after
  warmup/cull, or an empty sample set given to `percentile` /
  `percentile_f64` (typed refusal, never an index underflow).
- `E-HOST-008`: incomparable inputs (paired analysis under an unpaired
  protocol, zero-duration samples, no paired comparison).
- `E-HOST-011`: receipt/drift record does not verify against the
  recorded hash.
- `E-HOST-012`: invalid statistical protocol configuration (repetitions
  below the minimum, warmup below repetitions, MAD trim not positive and
  finite, field does not fit `usize`); distinct from manifest validation.
- `E-HOST-016`: refuse self-comparison: the subject and oracle of a
  comparison must be distinct engine identities (honest comparator
  separation; never a silent self-comparison).

LSP (`crates/emath-lsp/src`):

- LSP/JSON-RPC uses the standard `-32601` (method not found) and `-32700`
  (parse error) codes; message text is deterministic.

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

### Exact numbers (`crates/emath-rt`, `crates/emath-rust-backend`)

- `E-RAT-001`: `rat(n, 0)` has no exact-rational value: a zero
  denominator is refused with a typed diagnostic (check time for
  literal denominators, `EvalFault::Arithmetic` at run otherwise):
  never a panic, never a silent zero.
- `E-INT-002`: an exact integer does not fit the native i64 lane (a
  buffer integer, an index, or a ratio narrowed onto i64). The narrowing
  is checked, never truncated.
- `E-INT-003`: an exact-integer operand is a non-integral rational.

### Host/lab (`crates/emath-rust-backend/src/rust_ir/host.rs`)

- `E-HOST-002`: host-binding layer failure beyond ABI version mismatch
  (family code owned by the `rust_ir` host layer; no construction site in HEAD).

### Custom kinds (`crates/emath-schema`, `crates/emath-sema`)

- `E-KIND-001`: declaration kind is not supported by this front-end.
- `E-KIND-002`: declaration could not be admitted.

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

### Symbolic algebra (`crates/emath-ir`, `crates/emath-sema`)

- `E-SYM-001`: malformed symbolic expression or rewrite replacement.
- `E-SYM-002`: exact coefficient overflow or the native degree/resource
  bound was exceeded.
- `E-SYM-003`: expression or claim is outside the native exact univariate
  polynomial/simplification fragment.
- `E-SYM-004`: a structural rewrite requested `proved` authority without a
  checkable certificate.

### Names/visibility (`crates/emath-exec-ir/src/constructor_layer`)

- `E-NAME-025`: a function from a `use`d file is private: it is not on
  that file's `exports:` line (a file without one exports nothing). Add
  the name to its `exports:` line, or call a name it exports.
- `E-NAME-026`: malformed `exports:`: a file has more than one
  `exports:` line, or the line names something the file does not
  declare, or names it twice.

### Packages (`crates/emath-schema`, `crates/emath-sema`)

- `E-PKG-050`: external file import outside the front-end subset
  (library-path imports only).
- `E-PKG-064`: `@experimental` on an item while the source file does
  not declare the `experimental-syntax` capability; experimental syntax
  never compiles silently in a stable package (ELP experimental lane).
- `E-PKG-065`: unknown capability key in `@capabilities(...)`
  (declared: `experimental-syntax`); the key is refused instead of
  being silently dropped.

### Conversion contracts (`crates/emath-ir/src/contracts.rs`)

- `E-PROV-410`: bit-identical primitive conversion contract
  (float64/i64).
- `E-PROV-411`: value-conserving affine unit conversion contract (degC
  ↔ kelvin).
- `E-PROV-412`: index-conserving sparse matrix conversion (csc-matrix).

### Shapes (`crates/emath-ir`)

- `E-SHAPE-001`: matrix product requires rank-2 operands.
- `E-SHAPE-002`: inner extents differ.
- `E-SHAPE-003`: slice bounds invalid (start exceeds end).
- `E-SHAPE-004`: declared shape is not well-formed (empty tensor rank,
  zero extent, or `Zero` symbolic extent).

### Syntax/layout (`crates/emath-syntax`, including `src/genesis.rs`)

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

### Types (`crates/emath-sema`, `crates/emath-ir`, `crates/emath-core`)

- `E-TYPE-001`: unknown type.
- `E-TYPE-002`: unknown variable.
- `E-TYPE-003`: unknown function.
- `E-TYPE-010`: unsupported type, or an implicit / mass-matrix
  left-hand side (`m * derivative(v) = rhs`) outside the explicit ODE
  subset.
- `E-TYPE-012`: argument arity/type mismatch for the named function.
- `E-TYPE-310`: numeric family code (documented in
  `crates/emath-ir/src/numeric.rs`; no construction site in HEAD yet).
- `E-TYPE-311`: cannot promote two numeric types without exact-width
  loss.
- `E-TYPE-313`: type variable bound to conflicting types.
- `E-TYPE-314`: occurs check: a type variable escapes into its own
  binding.

Semantic image (`crates/emath-exec-ir/src/image.rs`):

- `E-IMAGE-001`: a semantic-image partition's body no longer matches its stamped content id: a corrupt page.
- `E-IMAGE-002`: a semantic-image partition is malformed.

Diagnostics schema (`crates/emath-cli/src/diagnostics.rs`):

- `E-LAW-001`: the `emath.diagnostic.explanation v1` explanation schema identity (law-explanation surface).

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

Transitions (`crates/emath-ir/src/package.rs`):

- `E-TRANS-006`: an event parameter name matches no declared model
  variable.

## Retired codes

These codes are no longer emitted. Their crates or lanes were removed, or
the refusal was folded into another code. A retired code is never reissued.

- `E-CAT`: 001, 002, 003, 004, 005, 006, 007
- `E-CODEGEN`: 005, 006, 007, 008, 009, 010, 012, 013, 053
- `E-CSV`: 001, 002, 003, 004, 005, 006, 007, 008, 009
- `E-CTOR`: 030, 031, 032, 033, 034, 035, 036, 037, 038, 039
- `E-EINSUM`: 001, 002
- `E-EV`: 140
- `E-EVAL`: 001, 002, 003, 004, 005, 006, 007, 008
- `E-EVID`: 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 201, 301, 302, 401, 402, 403, 404, 405, 501, 502, 503, 504, 505, 506, 507, 601, 602, 603, 604, 605, 606
- `E-FIT`: 000
- `E-GEN`: 080, 081, 082, 083, 084, 090, 091, 092, 093, 094, 095, 096
- `E-GOAL`: 201
- `E-GROWTH`: 001
- `E-HOST`: 004, 005, 007, 010, 013, 014, 015
- `E-KIND`: 010, 012, 013, 014, 015, 016, 020, 021, 022, 030, 031, 032, 310, 311, 312
- `E-LAZY`: 001, 002
- `E-LINALG`: 004
- `E-LOCK`: 001, 002, 003, 004, 005, 006
- `E-MEAS`: 001, 002, 003
- `E-MIGR`: 001, 002, 003
- `E-MODEL`: 001
- `E-NAME`: 021
- `E-ODE`: 001, 002, 003
- `E-PACK`: 001, 002, 003, 004, 005
- `E-PKG`: 020
- `E-PLG`: 001, 002, 003, 004, 005
- `E-PROV`: 001, 002, 030, 031, 033, 210, 220, 221, 222, 223, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 300, 310, 401, 402, 501, 502, 503, 510, 511, 512, 513, 514, 515, 516, 517, 518, 521, 522, 523, 524, 525
- `E-PROVIDER`: 001, 002, 003
- `E-RAT`: 002
- `E-REG`: 020, 021, 022, 023, 024, 030, 031
- `E-RES`: 100, 110, 111
- `E-SCHEMA`: 001
- `E-SHAKE`: 001, 002
- `E-SHAPE`: 005, 006
- `E-STD`: 001, 002, 003
- `E-SYN`: 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 155, 210, 211
- `E-SYNTH`: 001, 002
- `E-TLT`: 007, 013, 016
- `E-TYPE`: 011, 101, 102, 103
- `E-UNIT`: 001, 103
- `E-WORLD`: 001, 002, 003, 004, 005, 006, 007, 008

## Completeness annex: every issued code

Generated by `scripts/dump_error_codes.py` from the production crates;
regenerated as the registry changes. The workspace test
`tests/emath-cli/tests/registry_complete.rs` enforces that every emitted
code appears here (emitted ⊆ documented).

Emissions: **154 unique codes** from 285 rust files.
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
| `E-CODEGEN-002` | crates/emath-rust-backend/src/rust_ir/profiles.rs | `E-CODEGEN-002` |
| `E-CODEGEN-003` | crates/emath-rust-backend/src/rust_ir/profiles.rs | `library`<br>`E-CODEGEN-003` |
| `E-CODEGEN-004` | crates/emath-rust-backend/src/rust_ir/profiles.rs<br>crates/emath-rust-backend/src/rust_ir/render.rs | `E-CODEGEN-004` |
| `E-DOM-001` | crates/emath-ir/src/domains.rs | `{name} value {value} outside domain {self}` |
| `E-DOM-002` | crates/emath-ir/src/domains.rs | `ill-formed domain interval [{low}, {high}]` |
| `E-GOAL-041` | crates/emath-sema/src/session/requests.rs | `E-GOAL-041` |
| `E-GOAL-042` | crates/emath-sema/src/session/requests.rs | `E-GOAL-042` |
| `E-GOAL-043` | crates/emath-sema/src/session/requests.rs | `E-GOAL-043` |
| `E-GOAL-044` | crates/emath-sema/src/session/requests.rs | `E-GOAL-044` |
| `E-GOAL-045` | crates/emath-sema/src/session/requests.rs | `E-GOAL-045` |
| `E-HOST-001` | crates/emath-rust-backend/src/rust_ir/host.rs | `host trait version `{version}` is not major.minor.patch`<br>`E-HOST-001` |
| `E-HOST-002` | crates/emath-rust-backend/src/rust_ir/host.rs |  |
| `E-HOST-003` | crates/emath-lab-core/src/error.rs<br>crates/emath-lab-core/src/stats.rs |  |
| `E-HOST-006` | crates/emath-lab-core/src/measure.rs<br>crates/emath-lab-core/src/stats.rs | `no raw samples for metric {}`<br>`E-HOST-006` |
| `E-HOST-008` | crates/emath-lab-core/src/stats.rs | `E-HOST-008` |
| `E-HOST-011` | crates/emath-lab-core/src/error.rs |  |
| `E-HOST-012` | crates/emath-lab-core/src/stats.rs | `E-HOST-012` |
| `E-HOST-016` | crates/emath-lab-core/src/identity.rs | `E-HOST-016` |
| `E-IMAGE-001` | crates/emath-exec-ir/src/image.rs | `E-IMAGE-001` |
| `E-IMAGE-002` | crates/emath-exec-ir/src/image.rs | `E-IMAGE-002` |
| `E-INT-002` | crates/emath-rt/src/body/buffer.rs<br>crates/emath-rust-backend/src/codegen_render.rs<br>crates/emath-rust-backend/src/codegen_render/kinds.rs<br>crates/emath-rust-backend/src/codegen_render/op_collections.rs | `E-INT-002: buffer integer does not fit the native i64 carrier`<br>`emath_rt::code::CodeValue::Int({})` |
| `E-INT-003` | crates/emath-rust-backend/src/codegen_render/kinds.rs<br>crates/emath-rust-backend/src/codegen_render/rtcalls.rs | `{{ let __ratio = {}; let (__quotient, __remainder) = __ratio.0.div_rem(&__ratio.1).map_err(|e| e.to_string())?`<br>`{{ let __ratio = {ratio}; __ratio.1.is_one().then(|| __ratio.0.clone()).ok_or(\"E-INT-003: exact integer opera` |
| `E-KIND-001` | crates/emath-sema/src/admit/sections_meta.rs | `E-KIND-001` |
| `E-KIND-002` | crates/emath-sema/src/admit/sections_meta.rs | `E-KIND-002` |
| `E-KIND-011` | crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-sema/src/admit/declaration/setup.rs | `E-KIND-011`<br>`kind `{}` requires section `{name}`` |
| `E-LAW-001` | crates/emath-cli/src/cli_parse.rs<br>crates/emath-cli/src/diagnostics.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs | `positional argument 1 (expected `.emath` file or diagnostic code such as `E-LAW-001`)`<br>`emath explain <file.emath> [<symbol>] or emath explain E-LAW-001 [--json]` |
| `E-MEANING-001` | crates/emath-sema/src/admit/sections_meta.rs | `E-MEANING-001` |
| `E-NAME-020` | crates/emath-exec-ir/src/constructor_layer/call.rs<br>crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/declaration/setup.rs | `E-NAME-020` |
| `E-NAME-022` | crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-exec-ir/src/constructor_layer/scope.rs<br>crates/emath-exec-ir/src/constructor_layer/setup.rs<br>crates/emath-sema/src/admit/sections_meta.rs<br>crates/emath-sema/src/session.rs | `E-NAME-022`<br>`duplicate declaration name `{}`` |
| `E-NAME-023` | crates/emath-sema/src/admit/sections_meta.rs | `E-NAME-023` |
| `E-NAME-024` | crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/sections_meta.rs | `E-NAME-024` |
| `E-NAME-025` | crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-exec-ir/src/constructor_layer/scope.rs | `E-NAME-025` |
| `E-NAME-026` | crates/emath-exec-ir/src/constructor_layer/admit.rs<br>crates/emath-exec-ir/src/constructor_layer/mod.rs | `E-NAME-026`<br>``exports:` lists `{name}`, but this file declares no `{name}`` |
| `E-NUM-001` | crates/emath-ir/src/numeric.rs | `unknown numeric model `{other}` (known: strict-f64, interval-f64)` |
| `E-NUM-002` | crates/emath-ir/src/numeric.rs | `E-NUM-002` |
| `E-NUM-003` | crates/emath-ir/src/numeric.rs | `error-limit `{max_abs_error}` is not a finite non-negative bound`<br>`E-NUM-003` |
| `E-PKG-050` | crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-exec-ir/src/constructor_layer/setup.rs<br>crates/emath-sema/src/recognition.rs<br>crates/emath-sema/src/session.rs | `E-PKG-050`<br>`unresolved file import `{}`` |
| `E-PKG-064` | crates/emath-sema/src/admit/attributes.rs | `E-PKG-064` |
| `E-PKG-065` | crates/emath-sema/src/admit/attributes.rs | `E-PKG-065` |
| `E-PKG-080` | crates/emath-cli/src/cli_build.rs<br>crates/emath-cli/src/cli_check.rs<br>crates/emath-cli/src/execution/run.rs<br>crates/emath-cli/src/export_cmd.rs<br>crates/emath-cli/src/provenance_cmd.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-cli/src/tooling_cmd/migrate.rs<br>crates/emath-sema/src/session.rs | `cannot read source file ({})`<br>`cannot read spec: {}` |
| `E-PKG-081` | crates/emath-cli/src/cli_json.rs<br>crates/emath-cli/src/compare_cmd.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-sema/src/admit/sections_meta.rs | `E-PKG-081`<br>`E-PKG-081 checked source has no constructor module` |
| `E-PROB-001` | crates/emath-rt/src/probability.rs | `E-PROB-001` |
| `E-PROB-002` | crates/emath-rt/src/probability.rs | `E-PROB-002` |
| `E-PROV-410` | crates/emath-ir/src/contracts.rs | `E-PROV-410` |
| `E-PROV-411` | crates/emath-ir/src/contracts.rs | `E-PROV-411` |
| `E-PROV-412` | crates/emath-ir/src/contracts.rs | `E-PROV-412` |
| `E-RAT-001` | crates/emath-rt/src/body/exact.rs | `E-RAT-001: denominator is zero`<br>`E-RAT-001: noncanonical rational` |
| `E-RES-120` | crates/emath-build/src/lib.rs<br>crates/emath-tui/src/experiment/mod.rs | `E-RES-120: cargo exceeded the {timeout:?} wall-clock budget`<br>`E-RES-120` |
| `E-SEC-101` | crates/emath-exec-ir/src/constructor_layer/admit.rs<br>crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-sema/src/admit.rs<br>crates/emath-sema/src/admit/declaration/setup.rs | `E-SEC-101`<br>`parameters` |
| `E-SEC-130` | crates/emath-sema/src/admit/declaration/setup.rs | `inputs`<br>`E-SEC-130` |
| `E-SEC-133` | crates/emath-cli/src/cli_dispatch.rs<br>crates/emath-sema/src/session/requests.rs |  |
| `E-SHAPE-001` | crates/emath-ir/src/shapes.rs | `E-SHAPE-001` |
| `E-SHAPE-002` | crates/emath-ir/src/shapes.rs | `E-SHAPE-002` |
| `E-SHAPE-003` | crates/emath-ir/src/shapes.rs | `E-SHAPE-003`<br>`slice end {rows_end} exceeds extent {size}` |
| `E-SHAPE-004` | crates/emath-ir/src/shapes.rs | `E-SHAPE-004`<br>`declared extent `{name}` is not a well-formed shape` |
| `E-SYM-001` | crates/emath-ir/src/symbolic.rs | `E-SYM-001`<br>`replacement references uncaptured variable `{name}`` |
| `E-SYM-002` | crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-ir/src/symbolic.rs | `E-SYM-002`<br>`polynomial degree exceeds {MAX_POLYNOMIAL_DEGREE}` |
| `E-SYM-003` | crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-ir/src/symbolic.rs | `E-SYM-003`<br>`polynomial decision expected variable `{variable}`, found `{name}`` |
| `E-SYM-004` | crates/emath-ir/src/symbolic.rs | `E-SYM-004` |
| `E-SYN-100` | crates/emath-syntax/src/lexer/engine.rs | `E-SYN-100` |
| `E-SYN-101` | crates/emath-sema/src/session/requests.rs<br>crates/emath-syntax/src/lexer/engine.rs<br>crates/emath-syntax/src/parser.rs<br>crates/emath-syntax/src/parser/decl.rs<br>crates/emath-syntax/src/parser/expr/braket.rs<br>crates/emath-syntax/src/parser/expr/forms.rs<br>crates/emath-syntax/src/parser/expr/infix.rs<br>crates/emath-syntax/src/parser/expr/literals.rs<br>crates/emath-syntax/src/parser/expr/postfix.rs<br>crates/emath-syntax/src/parser/expr/primary.rs<br>crates/emath-syntax/src/parser/expr/units.rs<br>crates/emath-syntax/src/parser/stmt.rs<br>crates/emath-syntax/src/parser/stmt_binders.rs<br>crates/emath-syntax/src/parser/stmt_idents.rs<br>crates/emath-syntax/src/parser/stmt_idents/default.rs<br>crates/emath-syntax/src/parser/stmt_idents/series.rs<br>crates/emath-syntax/src/parser/stmt_suite.rs<br>crates/emath-syntax/src/parser/types.rs | `E-SYN-101`<br>`unexpected character `{}`` |
| `E-SYN-102` | crates/emath-syntax/src/parser/expr/literals.rs<br>crates/emath-syntax/src/parser/expr/postfix.rs<br>crates/emath-syntax/src/parser/expr/primary.rs<br>crates/emath-syntax/src/parser/expr/units.rs<br>crates/emath-syntax/src/parser/stmt_binders.rs<br>crates/emath-syntax/src/parser/stmt_idents/default.rs<br>crates/emath-syntax/src/parser/stmt_idents/look.rs<br>crates/emath-syntax/src/parser/types.rs | `E-SYN-102` |
| `E-SYN-103` | crates/emath-sema/src/admit/declaration/setup.rs<br>crates/emath-syntax/src/parser/stmt_idents/series.rs | `E-SYN-103` |
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
| `E-SYN-120` | crates/emath-core/src/parse.rs<br>crates/emath-sema/src/session/requests.rs | `E-SYN-120` |
| `E-SYN-121` | crates/emath-syntax/src/parser/expr/postfix.rs | `E-SYN-121` |
| `E-SYN-122` | crates/emath-syntax/src/parser/decl.rs | `E-SYN-122` |
| `E-SYN-123` | crates/emath-syntax/src/parser/decl.rs | `E-SYN-123` |
| `E-SYN-153` | crates/emath-syntax/src/layout.rs | `E-SYN-153` |
| `E-SYN-154` | crates/emath-core/src/tree/expr.rs<br>crates/emath-syntax/src/parser/expr/primary.rs | `E-SYN-154` |
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
| `E-TLT-004` | crates/emath-cli/src/catalog.rs |  |
| `E-TLT-005` | crates/emath-cli/src/tooling_cmd/explain.rs | `E-TLT-005` |
| `E-TLT-006` | crates/emath-cli/src/catalog.rs |  |
| `E-TLT-010` | crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-cli/src/tooling_cmd/new.rs | `E-TLT-010`<br>`error: invalid package name `{name}` (E-TLT-010)` |
| `E-TLT-011` | crates/emath-cli/src/catalog.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-cli/src/tooling_cmd/new.rs | `new`<br>`E-TLT-011` |
| `E-TLT-012` | crates/emath-cli/src/catalog.rs |  |
| `E-TRANS-006` | crates/emath-ir/src/package.rs |  |
| `E-TYPE-001` | crates/emath-sema/src/admit/declaration/setup.rs | ``{name}` needs an explicit constructor type, not an inferred default` |
| `E-TYPE-002` | crates/emath-cli/src/catalog.rs<br>crates/emath-cli/src/tooling_cmd/explain.rs<br>crates/emath-core/src/diagnostic.rs<br>crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-sema/src/admit.rs | `diagnostic-code lookup (`emath explain E-TYPE-002`). File/plan explanation refuses: there is no goals planner`<br>`emath explain E-TYPE-002` |
| `E-TYPE-003` | crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-sema/src/admit.rs | `E-TYPE-003`<br>`stale_dependency` |
| `E-TYPE-010` | crates/emath-exec-ir/src/constructor_layer/admit.rs<br>crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-sema/src/admit.rs | `type`<br>`E-TYPE-010` |
| `E-TYPE-012` | crates/emath-exec-ir/src/constructor_layer/mod.rs<br>crates/emath-exec-ir/src/interp/eval_calls.rs<br>crates/emath-exec-ir/src/native_kernel.rs<br>crates/emath-rt/src/body/numeric.rs | `E-TYPE-012: checked-add arguments must be Int`<br>`E-TYPE-012: program result must be a real scalar` |
| `E-TYPE-110` | crates/emath-syntax/src/parser/types.rs | `fn`<br>`E-TYPE-110` |
| `E-TYPE-111` | crates/emath-syntax/src/parser/stmt_idents.rs | `E-TYPE-111` |
| `E-TYPE-112` | crates/emath-syntax/src/parser/stmt_binders.rs | `E-TYPE-112` |
| `E-TYPE-310` | crates/emath-ir/src/numeric.rs |  |
| `E-TYPE-311` | crates/emath-ir/src/numeric.rs | `E-TYPE-311` |
| `E-TYPE-312` | crates/emath-ir/src/type_system.rs | `E-TYPE-312`<br>`cannot unify {} with {}` |
| `E-TYPE-313` | crates/emath-ir/src/type_system.rs | `E-TYPE-313` |
| `E-TYPE-314` | crates/emath-ir/src/type_system.rs | `E-TYPE-314` |
| `E-UNIT-100` | crates/emath-ir/src/units.rs |  |
| `E-UNIT-101` | crates/emath-ir/src/units.rs | `E-UNIT-101` |
| `E-UNIT-102` | crates/emath-ir/src/units.rs | `E-UNIT-102` |
| `E-UNIT-104` | crates/emath-core/src/units.rs<br>crates/emath-ir/src/units.rs | `E-UNIT-104`<br>`unknown unit `{name}`` |
| `E-UNIT-105` | crates/emath-ir/src/units.rs | ``Per<{inner}>` is invalid: affine units have no inverse`<br>`E-UNIT-105` |
