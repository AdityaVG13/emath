# Total Compilation Protocol

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

## Refusal versus diagnostic

Syntactically malformed or semantically contradictory input is refused. Well-formed meaning with unavailable execution capability may produce a diagnostic/parametric artifact if policy allows.

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
