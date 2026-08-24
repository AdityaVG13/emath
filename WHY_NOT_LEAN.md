# Why Not Just Lean?

> A reference for the common question: "Why does emath exist if Lean already does math?"

## Summary

Lean and emath serve different purposes:

- **Lean** is a theorem prover. It certifies that mathematical statements are true by checking proofs through a trusted kernel. It rejects anything it cannot prove.
- **emath** is a mathematical-intent compiler. It turns declarative math specifications (including incomplete, ambiguous, or unproven ones) into deterministic, verified Rust programs that execute and produce results.

If the question is "is this theorem true?" — use Lean.
If the question is "turn this math into a running program that gives an answer" — use emath.

These are complementary, not competitive. emath does not attempt to replace Lean.

## Core Distinction

| Axis | Lean | emath |
|---|---|---|
| **Purpose** | Certify mathematical truth | Compile math into executable software |
| **Verifies** | Proofs, via kernel type-checking | Programs and artifacts, via an evidence pipeline (claims, checks, hashes, negative controls) |
| **Trust model** | Small trusted kernel as sole authority | No single anchor: independent artifact checker, byte-determinism, format/test/lint gates |
| **Determinism** | Not applicable (proofs are logical objects) | Central: byte-identical reruns, pinned builds, sequential state ordering |
| **Execution** | Secondary to proving | Primary: execution is the deliverable (evaluate, differentiate, solve, optimize, simulate, compile) |
| **Admission policy** | Near-closed: any imprecision causes kernel rejection | Open: many legitimate interpretations are admitted; ambiguity is resolved explicitly, not silently |

> "It is not a theorem prover pretending every theorem is executable."
> — emath README

## What emath Does That Lean Cannot

### Ambiguous or incomplete specifications

A single `.emath` specification can admit multiple valid interpretations ("worlds"), each producing a different answer. This is a real, tested behavior — the same symbols compile to different results depending on which world is selected:

- `free_symbolic` → symbolic application
- `Boolean_algebra` → `false`
- `modular_numeric` → `6`

Lean requires a fully specified meaning before it can accept anything. emath admits the ambiguity, picks an interpretation, evaluates it, and labels the result honestly.

### Open problems and unproven conjectures

An open conjecture cannot be stated in Lean until it is proven. emath accepts it anyway: "search up to n = 2^68, terminate if a counterexample is found, return trajectories." The output is a working, deterministic program plus a finite verdict ("no counterexample in range", with certified bounds and negative controls).

This produces a reproducible artifact — the exact substrate a future proof or counterexample would build against.

**The boundary emath will not cross:** compiling is not proving. The pipeline guarantees "this artifact is exactly what you asked for" — not "your idea is true." Feed it a false conjecture and you get a faithful, verifiably-executing program containing the false idea. The guarantee is on the construction, not the truth of the content.

### Exploration and prototyping

emath enables iterating on mathematical ideas before they are proven:

- Write a conjecture, pick an interpretation, evaluate examples, observe what it computes
- Test ideas for problems that may not be settled or even well-posed
- Prototype, find finite evidence or candidate counterexamples, refine, repeat

Lean cannot participate until a proof exists. emath does this work while the proof does not.

## The Lean Integration Plan

Lean is planned as an optional evidence provider (FrankenLean adapter), not a core dependency. If Lean verifies a claim, its kernel verdict becomes one input to emath's evidence pipeline — packaged as evidence that the independent checker still hashes and gates like everything else.

The relationship: emath's pipeline always runs its own checks. Lean can provide stronger evidence for specific claims, but it never overrides the pipeline. Stronger proof, same door policy.

## Current Status

The deterministic, verify-always core is real today: source → semantic IR → execution IR → interpreter or Rust codegen → verified Cargo artifact. The browser playground (`emath web`) runs check, plan, generate, and execute in-page through a WASM engine with no install beyond the repository checkout.

Provider adapters (Wrenfold for symbolic codegen, FrankenJAX/SciPy/Sim for numerical backends, FrankenLean for proof evidence) are planned for later phases. The mathematical language surface is the current focus — building the foundation that makes everything else possible.
