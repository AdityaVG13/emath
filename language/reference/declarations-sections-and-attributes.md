# Declarations, Sections, and Attributes

A named declaration starts with `emath <kind> <Name>:` and an indented body.

```emath
emath function Square:
    inputs:
        x: Float64
    definitions:
        y = x * x
```

The built-in declaration kinds are:

| Kind | Use it for |
|---|---|
| `function` | Stateless formulas |
| `model` | State, derivatives, algebraic unknowns, and events |
| `policy` | Stateful values with constructors |
| `law` | A function with explicit assumptions, provenance, and evidence |
| `kind` | A user-defined declaration schema |
| `reaction_network` | Species, reactions, rates, and stoichiometry |

Additional kinds such as `capability`, `family`, `theory`, `morphism`, `method`, `experiment`, `migration`, and `field_pack` require their matching `use std.kinds.*` import. An unmounted kind is diagnosed, never guessed.

### Feature Capsules

`emath feature Name:` is the restricted, data-like shell for the stable
`emath.feature-capsule` schema. The schema has twenty primary classes:
`constitution`, `syntax`, `kind`, `section`, `surface`, `symbol`, `type`,
`binder`, `capability`, `theory`, `instance`, `goal`, `method`, `world`,
`provider`, `effect`, `artifact`, `diagnostic`, `migration`, and `field_pack`.
The class segment of `feature_id` must equal the primary class.

Every capsule records `feature_id`, `semantic_hash`, `class`, `maturity`,
summary/source, typed edges, surface, semantics, exactness, effects, worlds,
providers, artifacts, reference disposition, projections, conformance,
migration, authority target, presentation, and agent guidance. The agent row
names owner files, direct prerequisites, hazards, allowed edit categories, and
required checks. New classes and class-specific obligations are schema data;
they never create a feature-name parser branch or stable core operation variant.

Maturity is coverage, not authority:

```text
cataloged -> proposed -> accepted -> stable -> deprecated -> retired
```

Direct reversals exist only from `deprecated` to `stable` and from `retired` to
`deprecated`. Cataloged capsules cannot claim provided/generated/provider
projections. A capsule becomes candidate data only after validation and does
not become live language until the separate authority gate switches its exact
FeatureID.

`n/a(rule | reason)` is typed non-applicability. Both the applicable class rule
and concrete reason are mandatory; a missing or empty field is unfinished work,
not N/A. `hole(gate | reason)` is a first-class Spec Hole scoped to its feature
and publication gate. Any blocking hole prevents `accepted` or `stable`.

The canonical representation sorts typed edges, projections, and named slots;
two clean canonicalizations are byte-identical. Presentation and agent guidance
do not enter the semantic hash. Meaning changes retain the FeatureID, change the
semantic hash, and require an explicit migration and authority receipt. Legacy
capability-cell bytes remain separately tagged and addressable during migration;
there is never split authority.

Author from [`../templates/feature-capsule.emath`](../templates/feature-capsule.emath).
The two candidate examples in
[`../examples/intro/feature-capsules.emath`](../examples/intro/feature-capsules.emath)
show arithmetic and field-pack classes. Generated images, tables, and reference
views are projections and must never be edited by hand.

#### Authority and publication

Maturity never grants authority. The per-FeatureID authority lock uses exactly
`legacy-active`, `capsule-candidate`, `legacy-active-dual-run`,
`capsule-active`, `rollback-pending`, and `retired`. Exactly one source is the
winner at a repository state; dual-run keeps legacy active while comparing the
candidate.

Publication modes are cumulative. **framework** requires valid schemas and
reference vectors. **candidate-image** additionally requires a deterministic
image and explicit unrealized coverage. **stable-language** additionally
requires complete projections, live adapter evidence, unique authority, no
blocking Spec Hole, valid migrations, independent conformance, fresh generated
views/status, and authorized semantic changes.

Every transition emits a receipt containing FeatureID, old/new semantic hashes,
conformance, regenerated views, and the rollback image. Emergency rollback is
`capsule-active -> rollback-pending -> legacy-active`; it changes only that
FeatureID and never deletes prior capsules/images. A contributor distinguishes
cataloged, implemented, candidate, and active state by reading both capsule
maturity/coverage and the authority lock. `maturity: stable` without a
`capsule-active` lock row is not active language.

#### Generated runtime tables

Stage-0 consumers read compact sorted tables generated from capsules for
symbols/operators, binders, declaration kinds/sections, diagnostics, worlds,
providers, and capability handles. Every row carries its FeatureID, capsule
semantic hash, authored source, and generic dispatch handle. Operator rows also
carry aliases/precedence; collisions and confusables refuse generation.

Generated files begin `# @generated from Feature Capsules; DO NOT EDIT` and
carry a distribution-hash lock. Manual changes or stale locks refuse loading.
To add or change a feature, edit its authored capsule and conformance case, then
regenerate the Language Image/tables. Never add a handwritten table row or a
domain-specific match forest. During migration, legacy registries may dual-run,
but the authority lock—not a generated edit—selects the active FeatureID.

#### Active foundational slice

The first capsule-authority packet contains exactly eighteen FeatureIDs:
`std.syntax.source`, `std.syntax.declaration.generic`,
`std.syntax.section.generic`, `std.section.inputs`, `std.section.outputs`,
`std.section.definitions`, `std.section.tests`, `std.kind.function`,
`std.type.int`, `std.symbol.math.add`, `std.capability.math.add`,
`std.theory.additive_monoid`, `std.instance.int.additive_monoid`,
`std.world.exact.int`, `std.artifact.source`, `std.artifact.value`,
`std.artifact.diagnostic`, and `std.diagnostic.exactness_loss`.

The cutover is atomic, but rollback remains per FeatureID. Every active row uses
capsule source; legacy lookup is reserved only for the explicit
`rollback-pending -> legacy-active` transition. `AddExact` produces exact Int
`3` and source/value artifacts; `FloatIntoInt` produces the authorized
exactness-loss diagnosis. No sum, symbolic, or unrelated catalog FeatureID is
part of this packet.

For capsule-active capabilities, semantic admission resolves the authored
`presentation` aliases through the verified Language Image. The capsule's
`semantics` value supplies machine-readable `arity`, `inputs`, `output`, and
`diagnostic` fields. A matching call or operator lowers to a universal
`ExprNode::Apply` carrying the capability arena's FeatureID; semantic Rust does
not select it by a feature-specific match arm. An explicit `std::...` FeatureID
that is absent, catalog-only, blocked by a hole, or not capsule-active diagnoses
`E-LANG-FEATURE` rather than falling through to a guessed builtin.

## Named shorthand

A small function may place definitions and examples directly in its body:

```emath
emath function Square:
    y = x^2
    example x = 3
```

This expands to ordinary `inputs:`, `definitions:`, and `tests:` sections. Use `emath expand` to inspect the expanded form. A header without a body is `E-SYN-143`. L0 scratch files omit the header entirely.

## Sections

Sections are optional unless the declaration kind says otherwise. Order does not change meaning.

| Section | Payload | Meaning |
|---|---|---|
| `inputs:` | typed fields | Input parameters |
| `outputs:` | typed fields | Public result names; defaults to definitions |
| `state:` | typed fields | Model state |
| `algebraic:` | typed fields | Implicit unknowns solved with model state |
| `definitions:` | `name = expression` rows | Computed values |
| `equations:` | derivative, equality, or residual rows | Model equations |
| `constructors:` | constructor declarations | Valid policy state creation |
| `observations:` | `obs name[: type] = data` | Read-only measured data |
| `constraints:` | Boolean expressions | Solver constraints |
| `invariant:` | claims | Stated model invariants |
| `tests:` | `given` and `expect` cases | Worked examples |
| `goals:` | commands | Evaluation, solving, simulation, or generation requests |
| `exports:` | commands | Public declaration surface |
| `compile:` | commands | Target and numeric profile |
| `assumptions:` | law metadata | Named and executable preconditions |
| `domain:` | law metadata | Mathematical domain |
| `provenance:` | source records | Origin of a law or binding |
| `citations:` | citation rows | References for a law |
| `evidence:` | claims | Evidence level, checker, and requirements |
| `about:` | metadata | Human description |
| `host:` | mappings | Host-language bindings |

A section not allowed by its kind is `E-KIND-010` or `E-SYN-101`. A stateless `model` may admit with `N-KIND-001`, suggesting `function`.

### Contract mode

A declaration carrying `outputs:`, `definitions:`, or `goals:` is in
**contract mode**, and three rules keep the contract honest:

- `outputs:` or `goals:` without `inputs:` (and without a `Hole`
  placeholder) is diagnosed with `E-SEC-130`: outputs with no declared
  input have no source.
- A `definitions:` name that shadows an `inputs:` name is diagnosed with
  `E-NAME-020`: a definition may not overwrite a declared input. A name
  in both `inputs:` and `outputs:` is diagnosed by the same
  duplicate-field rule.
- Omitting `goals:` is allowed but warns with `E-SEC-133`: every
  definition defaults to `evaluate`, and the default must be visible.

`evidence:` is demanded only when a goal asserts truth without
computing it (`E-EV-140`). Phase 1 goal verbs (`evaluate`,
`differentiate`, `benchmark`, `fit`, `simulate`) are operational;
they compute, they do not claim; so they never require evidence.

## Laws

A law uses the normal function execution path but requires its mathematical context:

```emath
emath law NewtonSecond:
    assumptions:
        assume: "constant mass in an inertial frame"
        require mass >= 0 kg
    domain:
        name: "classical mechanics"
    provenance:
        source: "Newton, Principia (1687)"
    citations:
        cite: "SI Brochure"
    inputs:
        mass: Float64 in kg
        acceleration: Float64 in m/s^2
    definitions:
        force = mass * acceleration
    evidence:
        claim <dimensional_consistency>:
            statement: "kg*m/s^2 is force"
            require dimensional_analysis
            level E2
```

`assumptions:`, `domain:`, `provenance:`, `citations:`, and at least one evidence claim are required. A false executable `require` diagnoses evaluation. Evidence levels are `E0` through `E5`.

## Observations and provenance

Observations are read-only:

```emath
observations:
    obs plasma_conc: Float64 = 2.4
    obs time_points = [0.5, 1.0, 2.0, 4.0]
```

A definition cannot reuse an observation name (`E-OBS-WRITE`). Duplicate names are `E-NAME-022`; type mismatches are `E-TYPE-012`.

Binding provenance uses one of six closed kinds: `Exact`, `Citation`, `InstrumentRun`, `Fitted`, `Assumed`, or `Unstated`.

```emath
provenance:
    length:
        kind: "Citation"
        reference: "doi:10.1234/example"
        adjustment: "temperature corrected"
```

`emath check --verify-data` verifies an `InstrumentRun` SHA-256 against its source file. Drift or unreadable data is `E-OBS-HASH`.

## Time series

```emath
definitions:
    wind = [(0.0 s, 0.0 [unit m/s]), (0.1 s, 1.0 [unit m/s])] with interpolation: linear, extrapolation: diagnose
    at_midpoint = series_at(wind, 0.05 s)
```

Times must be strictly increasing. Interpolation is `previous`, `linear`, `nearest`, `pwc`, or `monotone_cubic`. Extrapolation is `diagnose`, `clamp`, or `extend`; the default is `diagnose`. The policy is part of semantic identity.

### Interpolation semantics

Let the support be the strictly increasing sample pairs `(t0, v0), ..., (tn, vn)` in SI units. `series_at(series, t)` evaluates the declared policy. At every support point every mode evaluates to that point's sample.

- `previous` and `pwc`: the left-continuous step. For `ti <= t < ti+1` the value is `vi`; on the last support point the value is `vn` (the endpoint is always evaluated, never the step below it). `pwc` is the canonical alias of `previous`; they agree at every time.
- `linear`: the time-proportional blend `vi + alpha * (vi+1 - vi)` with `alpha = (t - ti) / (ti+1 - ti)` on the bracketing interval.
- `nearest`: the closer sample; on an exact midpoint (`alpha == 0.5`) the tie resolves to the later sample (round-half-up). The choice is deterministic.
- `monotone_cubic`: the Fritsch–Carlson monotone cubic. Per-interval slopes are the sign-matched average of the adjacent secants (zero where adjacent secants disagree in sign), the outer slopes are the outer secant, and the segment is the cubic Hermite basis over that slope data. The interpolant is shape-preserving: it never overshoots the bracketing samples. With exactly two points (or equal adjacent secants) it reduces to the straight line.

### Extrapolation semantics

Outside `[t0, tn]` the declared policy decides:

- `diagnose` (default): a typed out-of-support fault, never a value. The fault names the requested time and the support bounds (`SeriesOutOfSupport` at execution).
- `clamp`: the nearest endpoint value (`v0` before `t0`, `vn` after `tn`), for every interpolation mode.
- `extend`: continue the OUTER interval's interpolation. `linear` and `monotone_cubic` extend their outer segment/slope; `nearest`, `previous`, and `pwc` hold the last sample on that side (`v0` before `t0`, `vn` after `tn`).

### Series diagnoses

- a non-increasing or equal time axis diagnoses; every mode orders the support by time (`E-SYN-101`);
- an empty series and non-`(time, value)` rows diagnose (`E-SYN-101`);
- missing `with interpolation:` diagnoses; the mode changes every downstream number and is never guessed (`E-SYN-101`);
- duplicate or unknown policy keys diagnose (`E-SYN-103`, `E-SYN-101`);
- CSV projection failures (missing/duplicated selected column, ragged row, non-finite cell, non-increasing selected time) diagnose with `E-SERIES-CSV`.

Pure CSV text can be mapped into a series by header name:

```emath
wind = series_from_csv(csv_text, "time", "wind", "linear", "diagnose")
```

Headers may carry units as `time (s)`. Unmapped columns are ignored by the series projection. A missing or duplicated selected column, ragged row, non-finite selected cell, or non-increasing selected time column diagnoses with `E-SERIES-CSV`. This operation consumes text already present in the program; it performs no filesystem or network I/O.

## Reaction networks

```emath
emath reaction_network HydrogenCombustion:
    species:
        H2
        O2
        H2O
    reactions:
        combustion: 2H2 + O2 -> 2H2O
    stoichiometry:
        nu = stoich(reactions)
```

Species must be declared before use (`E-CHEM-SPECIES`). Reaction arrows are `->`, `<->`, and `<=>`. Element imbalance is `E-CHEM-BALANCE`.

`stoichiometry:` accepts the derived form `stoich(reactions)`. `extents:` declares typed extents. `ice_table <reaction>:` contains `initial:`, `change:`, and optionally `equilibrium = initial + xi * change`. Re-entered or inconsistent coefficients are `E-CHEM-STOICH`.

## Imported declaration kinds

Imported kinds use the same declaration and section machinery. They do not add lexer keywords.

```emath
use std.kinds.capability

emath capability Softmax:
    inputs:
        x: Float64
    outputs:
        probability: Float64
    definitions:
        probability = x
```

- `capability` declares a schema-validated capability cell.

  A capability may use the biform surface: `class: biform` plus `version: "…"` and `migration:` rows, and `spec:` / `algorithm:` side sections each binding an independent quoted `evidence: "…"` with an optional `authority: authored | verified | provider` row (defaults: spec `authored`, algorithm `verified`). The sides reach the capability layer's closure planner at admission: a missing side diagnoses `E-CELL-009`, an authority that cannot attest a side diagnoses `E-CELL-010`, and one evidence object claimed for both sides diagnoses `E-CELL-011` (a green algorithm test never stamps the spec proved). The cell name is namespaced by the declared `package <path>`; a package-less biform declaration diagnoses `E-CELL-005`. Legacy capability declarations without a `class:` row keep the generic shape above.

- `family` expands a bounded list of instances into ordinary capabilities.
- `theory`, finite `model`, and `morphism` declarations are exhaustively checked on bounded carriers.
- `method` records one `algorithm:` and one `falsifier:`. It is proposal-only and cannot grant itself authority.
- `experiment` references problems, methods, providers, protection rules, and keep policy.
- `migration` classifies changes as `presentation`, `meaning`, `evidence`, or `provider`; a meaning change requires evidence.
- `field_pack` exports existing cells and metadata for installation.

Unknown sections, unsupported members, invalid authority claims, and unclassified migration changes diagnose with typed diagnostics.

## Constructor defaults

Defaults are admitted in constructor parameter lists:

```emath
constructors:
    public fn new(scale: Float64 = 1.0) -> Self:
        Self:
            scale = scale
```

A default cannot read state. Defaults in declaration-head arguments are not admitted.

## Attributes

Two item attributes are admitted:

```emath
@capabilities(experimental-syntax)
@experimental
emath function Candidate:
    y = 1
```

`@capabilities(experimental-syntax)` enables the file-scoped experimental lane. `@experimental` takes no arguments and requires that capability. Unknown attributes are `E-SYN-118`; unknown capability keys are `E-PKG-065`.

## Generics and extensions

Generic parameters may range over types, dimensions, shapes, units, constants, providers, and capabilities:

```emath
emath function Kernel<T: Real, N: Nat, U: Unit>:
```

Namespaced extension data belongs under `extensions:` and must declare whether it affects semantic identity, planning, presentation, or evidence.
