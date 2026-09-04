# Chapter 18: Using emath as a Mathematical Probe Lab

An agent (or human) assigned "use emath to attack conjecture X" needs a
repeatable workflow, not session folklore. This chapter is that
workflow: five steps from a hypothesis statement to a sweep whose
verdicts carry their own provenance. The running example throughout is
Euler's criterion against Tonelli-Shanks over small primes; the full
worked example lives at
[`language/examples/research/euler-criterion-p-sweep.emath`](../examples/research/euler-criterion-p-sweep.emath).

The standing rule from the language overview applies with full force
here: **the compiler returns numbers, trajectories, generated Rust, or
labeled artifacts and routed diagnoses; it never claims that submitted
mathematics is true.**
A probe that evaluated without error is *evidence about the inputs you
gave it*, nothing more.

## 18.1 Step 1 — State the hypothesis as checkable mathematics

Write the conjecture as ordinary declarations before touching the
compiler:

- the **objects**: the functions whose behavior the conjecture constrains
  (e.g. "Euler's criterion `a^((p-1)/2) mod p` distinguishes quadratic
  residues from non-residues mod an odd prime `p`");
- the **claim**: the identity or property (e.g. "the criterion returns 1
  exactly when `sqrt_mod(a, p)` computes a root, and `(p-1)/2` exactly
  when it returns the non-residue diagnosis");
- the **counterexample shape**: what input would falsify it (a pair
  `(a, p)` where the two engines disagree).

If the claim cannot be phrased with the builtins of chapter 5
(`types-units-shapes-and-domains.md`) and ordinary binder folds
(chapter 7), the probe is not ready to author — say so in the hypothesis
instead of inventing machinery.

## 18.2 Step 2 — Author the probe as a function spec

A probe is an ordinary `emath function` spec: declared `inputs:`,
declared `outputs:`, `definitions:` over admitted builtins. Prefer one
declaration per engine-under-test so the cross-check (step 4) can run
each side separately:

```emath
emath function euler_symbol:
    inputs:
        a: Int
        p: Int
    outputs:
        e: Int
    definitions:
        e = pow_mod(a, (p - 1) / 2, p)
```

Check it admits before evaluating:

```sh
emath check language/examples/research/euler-criterion-p-sweep.emath
```

Typed diagnoses here (unknown callee, arity, domain) are the compiler
telling you the probe does not yet say what you meant; fix the probe,
never the diagnostic.

## 18.3 Step 3 — Evaluate and read the receipt

```sh
emath eval language/examples/research/euler-criterion-p-sweep.emath \
    --function euler_symbol --set a=3 --set p=7
```

The receipt is the provenance record of the run:

- `meaning_id` — the identity of the admitted mathematics that ran
  (independent of presentation, local names, and evidence attachments;
  chapter 11). Two runs agree only if this agrees.
- `inputs_from set|example` — whether the bindings came from `--set` or
  from the spec's own worked example (the input oracle).
- `input` / `output` echo lines — the exact values, in the interpreter's
  display vocabulary.

Evaluation is deterministic: same spec bytes, same inputs, same bytes
out. Anything nondeterministic in a sweep is a defect in the probe, not
a property of the mathematics.

## 18.4 Step 4 — Independent cross-engine check

Never trust a single engine. The interpreter is the reference
semantics; `emath build --bin <entrypoint>` additionally emits a
compiled function-spec probe — a standalone native binary with the
**same `--set` CLI contract** (same strict parsing, same value display,
a receipt whose `engine` field names `compiled-probe`):

```sh
emath build language/examples/research/euler-criterion-p-sweep.emath \
    --bin euler_symbol --out target/emath
target/emath-cargo/probe-*-release/euler_symbol --set a=3 --set p=7
```

Compare the value lines byte-for-byte against the interpreter run for
the same inputs. On disagreement the probe result is quarantined: one
engine is wrong (or the two are executing different mathematics), and
nothing downstream may consume the verdict until that is resolved. The
parity battery in `tests/emath-build/tests/probe_parity.rs` pins this
contract for the shipped kernels; a probe that uses it inherits the
same guarantee only as far as its own definitions are parity-clean.

## 18.5 Step 5 — p-sweep discipline (never trust small-p)

A property checked at `p ≤ 29` says almost nothing about `p ≥ 2^31`.
Every claimed verdict must state its sweep range, and a counterexample
search is only as strong as the bound it actually swept — the
proximityprize corpus (G84/SYZ53) paid for this lesson; it generalizes
to every probe lab:

1. **Declare the sweep** as a loop over the probe, one eval per point,
   diagnoses recorded as data (a typed one-line diagnosis at one `p` is a
   *result about that `p`*, not a crashed run):

   ```sh
   for p in 7 11 13 19 29; do
     for a in $(seq 0 $((p - 1))); do
       emath eval language/examples/research/euler-criterion-p-sweep.emath \
           --function euler_symbol --set a=$a --set p=$p --json
     done
   done
   ```

2. **Report the range, not the impression**: "Euler criterion agrees
   with the Tonelli-Shanks gate for all `a` at `p ∈ {7, 11, 13, 19, 29}`"
   — never "verified for primes".
3. **Escalate the bound before believing the verdict**: the interesting
   regime is where the naive arithmetic overflows (chapter 5 documents
   the 2^63 stage-1 modular width); the compiled probe of step 4 exists
   precisely so sweeps at large `p` finish in seconds, not minutes.
4. **Keep the interpreter as the reference**: at every new sweep bound,
   spot-check compiled values against interpreted values (step 4) —
   the compiled path is a fast follower, not an authority.

A larger worked exemplar (an independent staircase verification with a
p-sweep to p = 29 from the 2026-08-31 session) is deliberately **not
checked in** yet — it waits for meaningful prize progress. This chapter
and the Euler-criterion example are the documented workflow until then;
do not cite the withheld exemplar as if it were part of the tree.

## 18.6 Where the pieces live

| Piece | Location |
|-------|----------|
| Builtins and their domain contracts | chapter 5 (`types-units-shapes-and-domains.md`) |
| Binder/fold surface for probes | chapter 7 (`expressions-equations-state-and-events.md`) |
| `meaning_id` identity definition | chapter 11 (`canonicalization-identity-and-serialization.md`) |
| `emath eval` / `emath build --bin` commands | the Commands list in `overview.md` |
| Worked example | [`../examples/research/euler-criterion-p-sweep.emath`](../examples/research/euler-criterion-p-sweep.emath) |
| Compiled-probe parity contract | `tests/emath-build/tests/probe_parity.rs` |
