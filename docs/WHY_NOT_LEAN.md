# "Why wouldn't I just use Lean?"

A copy-paste answer for when someone asks why emath exists if Lean does math.

Last updated: 2026-08-18. Sources: README.md, architecture/, language/spec/,
forks/UPSTREAM_LOCK.json, validate.sh gates.

---

## The 30-second answer

Lean is a theorem-prover: it proves math statements are true, and refuses
everything it cannot prove.

emath is a math-spec-to-program compiler: you throw often-messy, half-formed
mathematical intent at it, and it compiles *some* executable interpretation
of it, runs it, and hands you a verified result (while being explicit about
what it could and could not certify).

If your question is "is this true?": Lean.
If your question is "turn this math into something that runs and gives me an
answer right now": emath.

Lean answers questions. emath builds things and checks they behave.

And to be clear about the project's intention: **emath is not trying to take
over Lean.** Lean is very, very good at what it does and exists for a reason.
That's not what emath is. Ours is about all the *other* stuff: making math
something that can be used.

---

## The everyday analogy

**Lean** is a math teacher who never makes mistakes. You hand them a proof;
they check every logical step; one bad step and it's rejected. Their job is
to be sure, not to do stuff.

**emath** is a chef who turns recipes into working kitchens. You hand in a
recipe ("score = rate × weight, construct only via `new`, require precision
1e-10, prove on these inputs"), and emath:

1. reads the recipe,
2. builds the kitchen from scratch (generates Rust code),
3. tastes everything (runs tests, checks outputs equal expected numbers,
   regenerates twice and demands byte-identical output),
4. only then serves it (a runnable, inspectable program).

The job is *making runnable, trustworthy software from math ideas*.

---

## Companions, not competitors

Two different lanes, both legitimate:

- **Lean's lane: certify.** The last word on whether a statement is proven.
  If a proof is checked by the kernel, it's proven, full stop. No one needs
  to do that better than Lean does.
- **emath's lane: use.** The first word: turn an idea (even a wrong, half-
  formed, or unproven one) into something compilable, runnable, inspectable.
  Make the math *usable*.

Practically that means, with emath:

- **Testing random theorems.** Write the conjecture anyway, let emath pick an
  interpretation, evaluate examples, see what the idea actually computes.
- **Testing new ideas for problems that may not exist yet.** emath doesn't
  need the problem to be settled or even well-posed: it needs an
  interpretation and some inputs. You get compilable answers you can use to
  explore, simulate, and search before anyone has decided the question.
- **Prototyping the math.** Try the idea, see the consequences, find finite
  evidence or candidate counterexamples, refine, repeat. Lean can't do any of
  that until the proof exists; emath does it *while* the proof doesn't.

No turf war: a Lean-verified proof is the highest form of "true"; an emath
artifact is the highest form of "usable now". Lean certifies existing truth;
emath makes new math usable.

---

## What emath is for: the bigger picture

Two ways to read emath: the tool and the mission.

**The tool.** A compiler for math-as-usable-software: any interpretation you
can express gets compiled, run, and verified on its own terms. Jumbled
symbols, half-formed ideas, random formulas, physics notions, unproven
conjectures: all of it goes through the same deterministic pipeline and
comes out as something real you can run.

**The mission.** Make math something people can *do*:

- **Test anything.** Random theorems, random formulas, wild physics ideas (the
  adapters make it expansive by design): simulation (FrankenSim),
  tensors/AD (FrankenJAX), arrays (FrankenNumPy), solvers (FrankenSciPy),
  expression/JIT/GPU (Dew), symbolic (Wrenfold), graph/storage/search
  infrastructure (the adopted franken infra crates), proof evidence
  (FrankenLean, planned). Pick a world, get an answer.
- **Graph the jumble.** A big mish-mash polynomial compiles to a program
  that samples it: the parabola, the sine wave, whatever it is. Wiggle the
  coefficients, rerun, see what changed. Byte-deterministic, so what you see
  is what the math does.
- **Learn by writing.** Write emath code, get an answer, change it, see what
  changed. The compilation *is* the lesson: instant feedback, no textbook
  middle-man. This loop is live in a browser today: `emath web` opens a
  local playground pane where check / plan / generate / format / **run** all
  execute in-page through a WASM engine (no cargo, no install beyond the
  checkout). Change `given x = 3` to `given x = 4`, hit Run, watch `y` move.
  An example without an `expect` is a *worked example*: it computes and
  shows you the values, claiming nothing (because the point is the output,
  not the assertion).
- **Learn with an AI (later, on the same WASM pane).** An AI rides the same
  pipeline: explains why the same spec reads `free_symbolic → apply` but
  `modular_numeric → 6`, lets the learner author `.emath` and watch worlds
  choose themselves. Deterministic and replayable; no "trust me" layer.
- **Play.** Fun is a feature, not a distraction. Learning math by messing
  around with it is how most people actually learn it.

**Honest roadmap note.** The expansive part is partly planned, not all
shipped. The deterministic, verify-always core is real today, and so is the
browser playground (`emath web`, in-page runs via a strict-f64 interpreter,
honestly labeled `interpreted-strict-f64` and differentially gated against
the compiled tier). The adapters, the graphing surfaces, the AI-assisted
learning layer come down the line (built up step by step, and built well).
Lean can wait; the workshop is being built.

---

## The core difference: what gets verified, and how

| Axis | Lean | emath |
|---|---|---|
| Verifies | Proofs, by kernel type-checking | Programs + artifacts, by an evidence pipeline (claims → checks → hashes → negative controls) |
| Trust anchor | A small trusted kernel | No single anchor: independent artifact checker (E-EVID family), byte-determinism, fmt/test/clippy gates |
| Object of trust | Type-correctness of a theorem | A `.emath` goal admits, generates, compiles, and the output behaves as expected |
| Determinism | Not the point (proofs are logical) | The point: byte-identical reruns, pinned nightlies + git revs, seq-ordered state, bit-exact scores |
| Executes | Via VM/compilation, secondary to proving | Everything: execution is the deliverable (evaluate, differentiate, solve, search, compile) |
| Door policy | Near-closed: any imprecision = kernel rejection | Wide open: many legitimate meanings, not no meaning |

emath's README line 23 puts the boundary in writing:

> "It is not a theorem prover pretending every theorem is executable."

---

## The points that people actually mean when they ask

### 1. "You can put ANY math in: jumbled symbols, random formulas, ideas"

Yes. Same spec, three worlds, three answers: this is a real, running proof
in emath's own validation suite (validate.sh):

- `free_symbolic` → `apply`
- `Boolean_algebra` → `false`
- `modular_numeric` → `6`

Same glyphs, three legitimate readings, all compiled and executed. Lean
forces you to fix a meaning and prove it rigorously first. emath says "give
me the jumble, I'll pick a world and evaluate it."

### 2. "It may not be the exact answer you want, but it'll give you something"

Correct, with one honest refinement: emath always produces *an* answer, but
it refuses to fake *quality*. Run the same spec in the wrong world (swap
modular for Boolean) and the output comes out `5`, not the expected `6`:
emath's checker detects that and refuses to certify, with a structured
error, not silence.

So: jumbled-in, answer-out, always. And when the answer isn't the one you
claimed, it tells you loudly.

### 3. "What about problems that don't have an answer yet?"

This is where emath shines and Lean has nothing to offer.

An open conjecture cannot even be *stated* in Lean until it is proven. emath
still takes it: "search up to n = 2^68, terminate if counterexample found,
return trajectories." Output: a working, deterministic program plus a finite
verdict ("no counterexample in range", with certified bounds and negative
controls). You get a reproducible artifact: the exact thing a future proof
or counterexample needs to build against.

**The line emath will not cross:** compiling is not proving. The pipeline
guarantees *"this artifact is exactly what you asked for"* (not *"your idea
is true."*) Feed it a false conjecture and you get a beautiful, faithful,
verifiably-executing program *containing* the false idea. The guarantee is
on the building, not the truth of the recipe.

### 4. "So it's a search/exploration tool for open problems"

Yes: evidence-producing machinery for open questions (simulations with
certified bounds, counterexample hunts, finite verdicts, all re-runnable
byte-identically). The artifact *is* the progress. It ships you the search; it
will not fake the proof.

### 5. "Is Lean ever part of this?"

Eventually, as **hired help, not the boss** (planned FrankenLean provider,
lock row `franken-lean` @ 9e469e9; core crates never depend on providers).

Think of a bank (emath) hiring a top-tier forensic accountant (Lean):

- the bank still runs every normal check on every deposit,
- the accountant can verify a specific claim extra-thoroughly,
- nothing gets accepted *because* the accountant said so; it still passes
  the bank's pipeline.

If Lean arrives it is an optional oracle: its kernel verdict becomes one
input to emath's evidence pipeline, packaged as evidence the checker still
hashes and gates like everything else. Stronger proof, same door policy.

---

## The one-liners (steal freely)

- Lean: "Is this proof right?" → yes/no. emath: "Turn this math into a
  working, verified program" → `score(3.0) == 7`.
- Lean exists to be sure. emath exists to *make*, and makes sure while
  making.
- Lean is the judge. emath is the workshop.
- Lean certifies existing truth; emath makes new math usable (jumbled,
  unproven, or not even well-posed yet).
- Lean is for when the answer has to be right. emath is for when the math
  has to be used.
- Lean's door: near-closed, any imprecision = rejection. emath's door: wide
  open, imprecision = "I'll make a decision for you and show you the result."
- For unsolved problems: Lean gives nothing; emath gives a verified account
  of what was computed, re-runnable forever.
- "It is not a theorem prover pretending every theorem is executable." (README, emath)
