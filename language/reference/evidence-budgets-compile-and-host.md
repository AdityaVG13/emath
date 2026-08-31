# Chapter 10: Evidence, Budgets, Compilation and Host Sections

## Evidence declarations

```emath
evidence:
    claim <score_finite>:
        statement is_finite(score)
        require runtime-check

    claim <monotone_reuse>:
        statement derivative(score, reuse_probability) >= 0
        require certificate
```

A claim separates statement, scope, assumptions and required evidence.

## Budgets

```emath
budgets:
    compile:
        wall_time: 30 s
        memory: 1 GiB
    runtime:
        evaluations: 1_000_000
        iterations: 10_000
```

Budgets are semantic policy and enter planning/artifact identity when they affect results.

## Compile section

```emath
compile:
    target rust
    profile library
    numeric strict-f64
    unresolved parametric
    safety forbid-unsafe
```

This sets output requirements, not the mathematical meaning of definitions unless numeric semantics are intentionally part of the declaration.

## Exports

Exports select public values, types, constructors, evidence and provider interfaces. Non-exported internals may still appear in source maps/evidence according to privacy policy.

## Host section

```emath
host:
    rust:
        crate_name "generated_policy"
        implement cache_core::Policy for Self:
            method score = score
```

Host mappings declare conversions, ownership, errors and fallback.

## Proof outlines

Proofs are obligations as DATA, admitted as a `proofs:` section inside
existing kinds (expansiveness via sections, not new kinds — ch. 8
compass rule). They are additive authority, never admission tickets: an
unproved declaration compiles to its full artifact, and outline claims
are never lowered as definitions or constraints — justification stays
structurally separate from meaning (`evidence:` carries what was
observed, `proofs:` carries what must be discharged).

```emath
emath function bounded:
    inputs:
        a: Float64

    outputs:
        y: Float64

    definitions:
        y = a * a

    proofs:
        outline NonNegativity:
            assumption finite_a: is_finite(a)
            lemma square_nonneg: y >= 0.0
            check square_nonneg
            qed square_nonneg
```

Obligation kinds are data, exactly four: `assumption <name>: <claim>`
(context taken on faith, recorded), `lemma <name>: <claim>` (an
obligation to discharge), `check <name>` (assert a previously declared
obligation holds under the outline's assumptions), `qed <name>`
(the concluding obligation). Completeness is checked: an outline must
contain at least one step and end with its `qed`, `check`/`qed` must
name obligations declared earlier in the same outline, and an unknown
kind refuses (`E-SYN-101` naming the four). A complete outline admits
as data; an incomplete outline refuses.

Each outline lowers to `emath.proof-obligation v1` records — the
stable machine target providers code against:

```json
{"schema": "emath.proof-obligation v1",
 "outline": "NonNegativity",
 "kind": "lemma", "name": "square_nonneg",
 "claim": "y >= 0.0", "hypotheses": ["finite_a"], "target": null}
```

Proof outlines lower to data with completeness checks and trace records.
`check` steps do not run a prover. Case splitting, per-step evidence
levels, and proof-provider adapters are unsupported; ordinary
computation never depends on an unavailable prover.

## Tests and benchmarks

Tests are executable specifications. Benchmarks define comparison methodology and never substitute for semantic evidence.
