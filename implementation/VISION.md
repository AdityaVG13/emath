# Vision (positioning)

Positioning umbrella. Constitution and Neutral IR (`emath-ir`) outrank
this copy. Terminology follows `language/reference/overview.md`.

> **Revised 2026-08-31.** The governing metaphor moves from **gate** to
> **router**. The language does not refuse; it routes. Older documents
> that say "rejects / refuses / the compiler rejects" are being revised
> to this frame (tracked as a docs-work item); where they disagree with
> this file, this file wins until the revision lands.

## The law of the language

Two invariants, in priority order:

1. **Nothing is refused at the door.** Everything a user writes enters
   the language. The only legitimate "no" is input that cannot be parsed
   into a tree at all — and even that boundary shrinks toward zero:
   unknown constructs parse as symbols (glyph identifiers, `emath
   custom`) rather than failing.
2. **Nothing crosses the exit unlabeled.** Every artifact the system
   emits carries an explicit meaning label. The label vocabulary:

   | Label | Meaning |
   | --- | --- |
   | `exact` | computed in an exact domain; the value is the value |
   | `approximate(±bound)` | numeric with a stated error bound |
   | `symbolic-only` | returned as form; no world in scope evaluated it |
   | `hole-open` | a deliberate unknown, with its constraints attached |
   | `fault` | evaluated and failed at runtime — a value, not a crash |

"Cannot compute" is therefore never a refusal: it produces the truthful
artifact the system *can* produce — the symbolic form, a bound, a hole
with its constraints — plus a route to whichever world can do better.
Never a naked number; never a silent default. This preserves the
founding invariant unchanged: the language never pretends an
interpretation the user did not choose.

## World model

A **world** is a compilation target with four parts:

- **representations** it binds under-specified things to
  (`Real` → Float64? Interval? `Rat`? symbol?);
- a **capability manifest** declaring what it can execute (the manifest
  is testable — gaps are facts about the world, not surprises);
- an **evaluation strategy** (numeric / symbolic / hybrid);
- an **evidence policy** stating what it may claim about its output.

Worlds are **curated products we ship**, not user-assembled
configuration (user-defined worlds are future work; see Open questions).
The `world` construct selects which worlds a compile runs through, and
one compile may return answers in several worlds at once. Cross-world
disagreement is a feature: agreement is evidence, divergence is a
discovery.

Ship-target defaults (3–5; several already exist in the code in embryo,
statuses verified 2026-08-31 unless noted):

| World | Status |
| --- | --- |
| **Interpreter** (strict-f64 VM) | live — `eval`, the browser playground path; verified correct on functions and derivatives |
| **Symbolic** (expressions as values) | partial — term IR and rewrite machinery exist; not yet the universal fallback |
| **Numeric compiled** (Rust) | partial — Phase-1 subset only; known gaps (multi-goal, `Field`/`Text`, capability applications, data series) and known bugs (autodiff codegen wrong-values, stale embedded runtime) |
| **Exact** (`Rat`, `GF(p)`, intervals) | partial — VM-side cells live; per-operation coverage |
| **Verify** (proof obligations; future Lean adapters) | planned — rigor as an opt-in world |

## Principles

1. **Nothing is lost when a representation is lossy.** √2 in a Float64
   world is the symbol √2 *and* the approximation `1.4142135…±ε`, both
   carried, both labeled. A projection narrows what a world can say; it
   never destroys what the language knows.
2. **Errors are diagnoses with routes.** Every former refusal becomes:
   what cannot be done *here*, which world *can*, and what it costs to
   go there. `check` reports the world-coverage map of a file instead of
   passing verdicts.
3. **Rigor is a world, not the law.** Proof mode is opt-in. Lean
   adapters (future) bridge verified mathematics to external proof
   engines for the subset provers can address. Mathematics that is
   genuinely new may have no verifier at all — it still gets answers,
   labeled as such. The system never certifies what nothing checked,
   and never refuses to *answer* merely because nothing can prove it.
4. **The machine is a first-class user.** `emath custom` is a gradient:
   an unknown symbol is legal everywhere (opaque, universally
   representable); adding semantics lets worlds that understand it
   compute with it; adding per-world implementations makes it native.
   Agents can explore notation space at machine scale, harvest labeled
   answers, and hand humans things worth interpreting. That is the
   point: math as a medium for machine-driven discovery.
5. **The base syntax is small.** Inputs, definitions, questions, and a
   declaration keyword. How much of today's surface (sections, intent
   verbs, notation) collapses into user-extensible syntax over a frozen
   core is deliberately unresolved — see Open questions.

## Frame (V6/V7 heritage, restated under the new law)

1. **Any mathematics as intent.** A `.emath` declaration states meaning
   (definitions, equations, constructors, laws). Goals name work.
   Partial, unproven, or invented structure is admitted when it is
   structurally well-formed; missing execution is a labeled disposition
   with a route, not a refusal and never a silent drop.
2. **Executable portfolios.** Underconstrained math may have several
   coherent interpretations. The compiler keeps them as worlds and
   artifacts (native, parametric, exploration, continuation, diagnostic)
   rather than collapsing to one unlabeled guess.
3. **Protected optimization.** Candidates may vary meaning or
   implementation only inside an admission / measurement / Pareto /
   promotion envelope. Rank never raises evidence authority.

V6 named this a mathematical-intent compiler. V7 added genesis: glyphs
and partial constraints can become checked worlds before ordinary
compilation.

## Language

**Today.** Spec is `language/reference/`. Surface crates: `emath-syntax`
(lexer, lossless tree, parser, formatter, scratch expansion, `emath
custom`), `emath-schema` (kind-schema registry), `emath-hir`,
`emath-term`, `emath-core`. `emath check` / `emath fmt` cover the
implemented subset (all 42 `language/examples/` files parse and check;
the implemented subset remains smaller than the spec). The interpreter
(`eval`) executes functions and derivatives correctly on its supported
input types; `simulate` runs Euler/RK4/adaptive-RK45 with event
detection and matches analytic solutions on check problems. Meaning
versus work is already in the language: `definitions:` vs `goals:`.

**Known gaps (language path).** `eval` binds only `Float64` and vector
inputs; multi-function files need explicit `--function`; a few examples
carry constructs the checker accepts but execution rejects. These are
world-capability gaps to close, not language law.

**Aspirational.** Full section families from the spec (packages, units,
shapes, domains, events, goals); the planned crates on
`implementation/CRATE_MAP.md`; zero-math learnability. Surface
minimalism is doctrine: the language demands only what meaning requires.

## Compiler

**Today.** `emath-sema` (`CompilerSession`) admits and plans. `emath-ir`
holds IR/plan/evidence and the registries. `emath-plan` is a
deterministic planner with total dispositions; no external providers are
installed. Lowering: `emath-exec-ir` → `emath-rust-backend` →
`emath-artifact` → `emath-build`. Host: `emath-cli` (`check`, `plan`,
`build`, `run`, `test`, `eval`, `simulate`, `explain`, `web`, genesis
family). Playground: `emath-wasm` (C-ABI engine) + `emath web`;
`emath-exec-ir` carries the interpreter so `run` works in-browser
without cargo, labeled `interpreted-strict-f64`; the compiled Rust tier
stays the evidence pipeline's native lane, and tier agreement is a
differential gate. Genesis substrate: `emath-genesis` (worlds, VM),
`emath-world-ir`, `emath-world-codegen` (folded into `world-ir`).
Working demos: affine-scorer and semantic-genesis.

**Known gaps (compiler path).** Phase-1 codegen covers one `evaluate`
goal per declaration and a narrow type set; generated programs embed a
runtime snapshot that lags `crates/emath-rt` (graph/probability
modules); the Rust codegen for derivatives currently disagrees with the
interpreter (interpreter is correct; the generated-test failure is the
detecting signal). Per the 2026-08-31 direction, the language and its
worlds are the priority; codegen breadth is not the near-term front.

**Aspirational.** Full goal set; live provider bridges; `emath bench`
as a comparison ruleset; `migrate`.

## Frontier engine

**Today.** Library scaffold, not a shipped product. `emath-lab-core`:
experiment manifests, quality gates, promotion policy, Pareto archive.
`emath-evidence`, `emath-checker` (independent check + negative-control
battery). Feature-gated spikes elsewhere.

**Aspirational.** A live protection envelope against real host metrics:
candidate construction, evaluation across worlds, promotion with
receipts, rollback.

## Success criteria

| Lane | Pass means |
| --- | --- |
| **Language** | Everything a user writes enters the language and comes back labeled. Unsupported work yields a routed diagnosis or a labeled partial artifact — never a refusal, never an unlabeled guess. |
| **Semantic** | The active world, plan, and disposition of every artifact are explicit. Portfolio rank never escalates authority (`Structural` / `Tested` / `Certified` / `Verified` stay honest). |
| **Execution** | A run's label tells the truth about how it was produced (interpreted / compiled / symbolic / bound). Unverified steps say `not-run`. |
| **Evidence** | Independent checker; claim language cannot exceed checks. Cross-world agreement is recorded as evidence; disagreement is surfaced, not hidden. |

## Open questions

1. **Growth mechanism (OPEN — under discussion 2026-08-31).** Frozen
   core with all surface sugar as user-extensible syntax (macros and
   `expand` as the backbone) vs. a growing surface over a stable core
   vs. curated sugar packs (a stdlib of notations, still
   user-overridable). Decision gates the next build phase: expansion
   infrastructure vs parser features.
2. **User-defined worlds.** When — not whether — users compose their own
   worlds from parameter packs. Until then the world set is curated and
   ship-owned.

## Decision log

- **2026-08-31.** "The compiler refuses what it cannot prove" framing is
  retired. Gate → router. Agreed: the two Law invariants; the label
  vocabulary; worlds as curated, shipped products (3–5 defaults,
  selected via `world`); errors become diagnoses with routes; `check`
  becomes the world-coverage report; rigor is an opt-in world with
  future Lean adapters; nothing is lost on lossy projection; `custom`
  as the symbol → semantics → implementation gradient with the machine
  as a first-class user. Open: growth mechanism (Open question 1),
  timing of user-defined worlds (Open question 2).
