# Chapter 15: Total Compilation Protocol

## Principle

For every semantically admitted declaration and request, the compiler produces a typed disposition rather than a generic dead end.

## Dispositions

### Native

All work lowers to code available in the target artifact.

### Hybrid

Generated code binds explicit runtime providers.

### Parametric

Generated code exposes provider traits or generic parameters that a consumer must satisfy.

### Exploration

The artifact searches, simulates, synthesizes or enumerates within budgets and returns candidates/evidence.

### Continuation

The artifact executes bounded work and returns resumable state.

### Diagnostic

The semantic declaration and API are emitted, but execution returns a typed unresolved reason with the missing capabilities/assumptions.

## Diagnosis versus refusal

Under the router law (see `implementation/VISION.md`), well-formed
meaning is never dead-ended: unavailable execution capability produces a
labeled diagnostic, parametric, or continuation artifact per policy, with
the exit label stating exactly how the answer was produced. The
remaining true refusal is the shrink-toward-zero parse boundary: input
that cannot be read into a tree at all. Even that edge admits unknown
constructs as symbols (glyph identifiers, `emath custom`) rather than
failing.

### Explicit free-symbolic route

Unknown glyphs retain exact bytes/spans at Stage-0. Strict source never silently
acquires custom meaning. When the caller explicitly requests
`std.world.free_symbolic`, an unknown symbol becomes an open structural result
containing the symbol, binders, assumptions, holes, world label, and
`structural-only` authority—not a fabricated numeric answer. The capsule family
is authored in [`../spec/worlds/free-symbolic.emath`](../spec/worlds/free-symbolic.emath).

To supply real semantics later, author a new capability/provider capsule and
typed edges; do not edit the parser or add glyph-name dispatch. Missing glyph,
hole, world/authority label, numeric-world admission, or authority escalation
invalidates the artifact.


## Exactness ladder

A request may explicitly permit a fallback ladder:

```text
exact symbolic
→ exact finite algorithm
→ certified bounded approximation
→ checked numerical result
→ empirical estimate
→ unresolved
```

The compiler never descends the ladder implicitly. Each step changes result/evidence type and artifact identity.

## Conjectures and open problems

A conjecture declaration can compile into:

- statement and checker interface;
- bounded instance verifier;
- counterexample/witness search;
- proof-provider interface;
- distributed continuation;
- diagnostic artifact.

It is not mislabeled as proved.
