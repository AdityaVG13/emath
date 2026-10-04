# Why not just Lean?

A short answer to a common question: if Lean already does mathematics,
why does emath exist?

## Summary

Lean and emath answer different questions.

- **Lean** is a proof assistant. It certifies that statements are true by
  checking proofs against a small trusted kernel. It accepts nothing it
  cannot check.
- **emath** is a language for mathematics that computes. It turns
  definitions (including incomplete, exploratory, or unproven ones) into
  programs that run and return labeled answers.

If your question is "is this theorem true?", use Lean. If your question
is "what does this mathematics compute, and where does it break?", use
emath. The two are complementary.

## Side by side

| | Lean | emath |
| --- | --- | --- |
| **Purpose** | Certify mathematical truth | Compute with mathematics and compile it to software |
| **Checks** | Proofs, by kernel type checking | Programs, by admission, authored tests, and deterministic replay |
| **Trust anchor** | A small trusted kernel | Determinism, exact arithmetic, and explicit labels on every answer |
| **Execution** | Possible, secondary to proof | The primary deliverable: values, trajectories, and Rust crates |
| **Unfinished ideas** | Must be fully stated and proved | Run as written; open parts come back `partial` or as named refusals |

## What emath adds

### Computing before a proof exists

A conjecture can be explored in emath long before anyone proves it:
evaluate it on cases, search a bounded range for counterexamples, and
record the result as a reproducible run. The answer is a finite verdict
("no counterexample up to N"), labeled as finite.

### Seeing where arithmetic matters

`emath compare` runs one function with exact rationals, several float
formats, 64-bit integers, intervals, and affine forms, and reports where
the answers differ. That tells you whether a result depends on rounding,
cancellation, or a branch that flips, which is a question a proof
assistant does not ask.

### Inventing new definitions

Code is a value in emath. A discovery loop (`emath dream`) can propose
programs, score them with an authored judge, and keep the winners as
ordinary definitions in your own library (`emath keep`). See
[DISCOVERY.md](language/DISCOVERY.md).

## The line emath does not cross

**Compiling is not proving.** emath guarantees that the program is what
you wrote, not that your idea is true. A false conjecture compiles into a
faithful program that contains the false idea. Each answer from
`emath compare` carries a claim marked `unchecked`.

## Working together

Lean is an optional second step. `emath export lean --check` translates
your functions, their examples, and the laws you state with
`search.certify` into a Lean 4 project, and the Lean kernel proves them
from the definitions. emath's own checks always run first, and Lean
never claims more than emath: an example that faults in emath is not
exported, and a law counts as proved only when emath certifies it too.
Nothing in emath depends on Lean; only this step needs it. See
[Certifying results in Lean](docs/guide/lean.md).
