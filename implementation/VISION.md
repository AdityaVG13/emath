# Vision

The positioning document for the emath project. The constitution
(`implementation/CONSTITUTION.md`) and the core IR (`emath-ir`) outrank
this document where they disagree with it. Terminology follows
`language/reference/overview.md`.

## Governing idea: router, not gate

The language does not refuse; it routes. The compiler's job is not to
reject what it cannot handle. It routes every input to the best
available answer and labels that answer honestly. Documents that
describe the compiler as "rejecting" or "refusing" are being revised
to this frame; where they disagree with this document, this document
wins until the revision lands.

## The law of the language

Two invariants, in priority order:

1. **Nothing is refused at the door.** Everything a user writes enters
   the language. The only legitimate "no" is input that cannot be
   parsed into a tree at all, and even that boundary shrinks toward
   zero: unknown constructs parse as symbols (glyph identifiers,
   `emath custom`) rather than failing.

2. **Nothing crosses the exit unlabeled.** Every artifact the system
   emits carries an explicit meaning label:

| Label | Meaning |
| --- | --- |
| `exact` | Computed in an exact domain; the value is the value. |
| `approximate(±bound)` | Numeric with a stated error bound. |
| `symbolic-only` | Returned as form; no world in scope evaluated it. |
| `hole-open` | A deliberate unknown, with its constraints attached. |
| `fault` | Evaluated and failed at runtime; a value, not a crash. |

"Cannot compute" is never a refusal. It produces the truthful artifact
the system can produce, such as the symbolic form, a bound, or a hole
with its constraints, plus a route to whichever world can do better.
Never a naked number; never a silent default. The language never
pretends an interpretation the user did not choose.

## World model

A **world** is a compilation target with four parts:

- **Representations** it binds under-specified things to (`Real` as
  Float64, `Interval`, `Rat`, or symbol).
- A **capability manifest** declaring what it can execute. The manifest
  is testable; gaps are facts about the world, not surprises.
- An **evaluation strategy**: numeric, symbolic, or hybrid.
- An **evidence policy** stating what it may claim about its output.

Worlds are curated products we ship, not user-assembled configuration.
User-defined worlds are future work (see Open questions). The `world`
construct selects which worlds a compile runs through, and one compile
may return answers in several worlds at once. Cross-world disagreement
is a feature: agreement is evidence, and divergence is a discovery.

Ship-target defaults (three to five; several already exist in the code
in embryo):

| World | Status |
| --- | --- |
| **Interpreter** (strict-f64 VM) | Live. `eval` and the browser playground path; verified correct on functions and derivatives. |
| **Symbolic** (expressions as values) | Partial. Term IR and rewrite machinery exist; not yet the universal fallback. |
| **Numeric compiled** (Rust) | Partial. Phase-1 subset only; known gaps (multi-goal, `Field`/`Text`, capability applications, data series) and known bugs (incorrect values from autodiff codegen, stale embedded runtime). |
| **Exact** (`Rat`, `GF(p)`, intervals) | Partial. VM-side cells live; per-operation coverage remains. |
| **Verify** (proof obligations; future Lean adapters) | Planned. Rigor as an opt-in world. |

## Principles

1. **Nothing is lost when a representation is lossy.** √2 in a Float64
   world is the symbol √2 and the approximation `1.4142135…±ε`, both
   carried, both labeled. A projection narrows what a world can say; it
   never destroys what the language knows.

2. **Errors are diagnoses with routes.** Every former refusal becomes:
   what cannot be done here, which world can do it, and what it costs
   to go there. `check` reports the world-coverage map of a file rather
   than passing verdicts.

3. **Rigor is a world, not the law.** Proof mode is opt-in. Future Lean
   adapters will bridge verified mathematics to external proof engines
   for the subset provers can address. Mathematics that is genuinely
   new may have no verifier at all; it still gets answers, labeled as
   such. The system never certifies what nothing checked, and never
   refuses to answer merely because nothing can prove it.

4. **The machine is a first-class user.** `emath custom` is a gradient:
   an unknown symbol is legal everywhere (opaque, universally
   representable); adding semantics lets worlds that understand it
   compute with it; adding per-world implementations makes it native.
   Agents can explore notation space at machine scale, harvest labeled
   answers, and hand humans things worth interpreting. Mathematics is a
   medium for machine-driven discovery.

5. **The base syntax is small.** Inputs, definitions, questions, and a
   declaration keyword. The surface grows as data, not as parser
   features: syntax packs carry the sugar (see Open questions).

## Frame

Three commitments the system carries from its design lineage, restated
under the law:

1. **Mathematics as intent.** A `.emath` declaration states meaning
   (definitions, equations, constructors, laws); goals name work.
   Partial, unproven, or invented structure is admitted when it is
   structurally sound. Missing execution is a labeled disposition with
   a route, not a refusal and never a silent drop.

2. **Executable portfolios.** Underconstrained mathematics may have
   several coherent interpretations. The compiler keeps them as worlds
   and artifacts (native, parametric, exploration, continuation,
   diagnostic) rather than collapsing into one unlabeled guess.

3. **Protected optimization.** Candidates may vary meaning or
   implementation only inside an admission, measurement, Pareto, and
   promotion envelope. Rank never raises evidence authority.

Genesis is part of this frame: glyphs and partial constraints can
become checked worlds before ordinary compilation.

## State of the system

### Language

**Today.** The specification lives in `language/reference/`. Surface
crates: `emath-syntax` (lexer, lossless tree, parser, formatter,
scratch expansion, `emath custom`), `emath-schema` (the kind-schema
registry), `emath-hir`, `emath-term`, `emath-core`. `emath check` and
`emath fmt` cover the implemented subset; all 42 `language/examples/`
files parse and check, and the implemented subset remains smaller than
the spec. The interpreter (`eval`) executes functions and derivatives
correctly on its supported input types. `simulate` runs Euler, RK4,
and adaptive RK45 with event detection, and matches analytic solutions
on check problems. Meaning versus work is already in the language:
`definitions:` versus `goals:`.

**Known gaps (language path).** `eval` binds only `Float64` and vector
inputs; multi-function files need an explicit `--function`; a few
examples carry constructs the checker accepts but execution rejects.
These are world-capability gaps to close, not language law.

**Next.** The full section families from the spec (packages, units,
shapes, domains, events, goals); the planned crates on
`implementation/CRATE_MAP.md`; zero-math learnability. Surface
minimalism is doctrine: the language demands only what meaning
requires.

### Compiler

**Today.** `emath-sema` (`CompilerSession`) admits and plans.
`emath-ir` holds IR, plans, evidence, and the registries.
`emath-plan` is a deterministic planner with total dispositions; no
external providers are installed. The lowering path runs
`emath-exec-ir` through `emath-rust-backend` to `emath-artifact` and
`emath-build`. The host CLI (`emath-cli`) provides `check`, `plan`,
`build`, `run`, `test`, `eval`, `simulate`, `explain`, `web`, and the
genesis family. The playground is `emath-wasm` (C-ABI engine) plus
`emath web`; `emath-exec-ir` carries the interpreter, so `run` works
in the browser without cargo, labeled `interpreted-strict-f64`. The
compiled Rust tier remains the evidence pipeline's native lane, and
tier agreement is a differential gate. The genesis substrate is
`emath-genesis` (worlds, VM) with `emath-world-ir` (the codegen half
folded in). Working demos: affine-scorer and semantic-genesis.

**Known gaps (compiler path).** Phase-1 codegen covers one `evaluate`
goal per declaration and a narrow type set. Generated programs embed a
runtime snapshot that lags `crates/emath-rt` (the graph and
probability modules). The Rust codegen for derivatives currently
disagrees with the interpreter; the interpreter is correct, and the
generated-test failure is the detecting signal. Current direction: the
language and its worlds are the priority; codegen breadth is not the
near-term focus.

**Next.** The full goal set; live provider bridges; `emath bench` as a
comparison ruleset; `migrate`.

### Frontier engine

**Today.** Library scaffold, not a shipped product. `emath-lab-core`
provides experiment manifests, quality gates, promotion policy, and
the Pareto archive. `emath-evidence` and `emath-checker` provide the
independent check and the negative-control battery. Feature-gated
spikes exist elsewhere.

**Next.** A live protection envelope against real host metrics:
candidate construction, evaluation across worlds, promotion with
receipts, rollback.

## Success criteria

| Lane | Pass means |
| --- | --- |
| **Language** | Everything a user writes enters the language and comes back labeled. Unsupported work yields a routed diagnosis or a labeled partial artifact; never a refusal, never an unlabeled guess. |
| **Semantic** | The active world, plan, and disposition of every artifact are explicit. Portfolio rank never escalates authority (`Structural`, `Tested`, `Certified`, `Verified` stay honest). |
| **Execution** | A run's label tells the truth about how it was produced (interpreted, compiled, symbolic, bound). Unverified steps say `not-run`. |
| **Evidence** | Independent checker; claim language cannot exceed checks. Cross-world agreement is recorded as evidence; disagreement is surfaced, not hidden. |

## Open questions

1. **Growth mechanism: decided (Option C).** The core stays small and
   stable. The surface grows as data: **syntax packs** (verbs,
   sections, notation) that expand to the core, and users and agents
   can define their own packs. The governing rule: **semantics goes in
   the compiler; spelling goes in sugar.** The first pilot moves the
   intent verbs into a verbs-pack once the pack mechanism exists; they
   are already pure sugar over goals. Open sub-questions (packs as
   data versus self-hosted `.emath` macros; expand-form-as-contract
   before the LSP) live in the internal language-growth notes.

2. **User-defined worlds.** When, not whether, users compose their own
   worlds from parameter packs. Until then, the world set is curated
   and ship-owned.

## Decision log

- **Gate to router.** The framing "the compiler refuses what it cannot
  prove" is retired. Agreed: the two Law invariants; the label
  vocabulary; worlds as curated, shipped products (three to five
  defaults, selected via `world`); errors become diagnoses with
  routes; `check` becomes the world-coverage report; rigor is an
  opt-in world with future Lean adapters; nothing is lost on lossy
  projection; and `custom` as the
  symbol-to-semantics-to-implementation gradient, with the machine as
  a first-class user.

- **Growth mechanism: Option C.** A small core plus shipped syntax
  packs; users and agents can define their own. Governing rule:
  semantics in the compiler, spelling in sugar. Pilot: the verbs-pack.
  Open sub-questions live in the internal notes.
