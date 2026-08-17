# Goals, Requests, Strategies and Resolution

## Goal structure

The `goals:` section names the work the compiler must perform:

```emath
goals:
    differentiate <score>:
        wrt [state.alpha, state.gamma]
        order 1
        require evidence >= E2
        produce rust.library
```

A goal names the work and policy, not a hard-coded algorithm.

## Core goals

```text
evaluate, transform, simplify, differentiate
integrate, solve, optimize, simulate
search, synthesize, prove, verify
compile, benchmark
```

## Goal contract

Each goal includes:

- semantic target;
- bound inputs/unknowns;
- desired outputs;
- exactness/error tolerance;
- evidence requirement;
- budget;
- target/deployment;
- determinism;
- provider allow/deny/preference;
- fallback and unresolved policy.

## Strategies

```emath
strategies:
    prefer [native, symbolic, interval]
    allow providers matching NumericSolver
    deny remote
    fallback parametric
```

Strategy statements constrain planner policy. They cannot weaken a goal's correctness/evidence requirements implicitly.

## Resolution outcomes

- selected native/provider composition;
- multiple Pareto plans retained for benchmarking;
- parametric provider requirement;
- exploration/search artifact;
- continuation;
- diagnostic unresolved artifact.

## Plan introspection

The CLI exposes eligible providers, excluded providers and reasons, decomposed subgoals, budgets, checks, estimated cost and fallback graph before execution.

## Custom goals

A package may define a custom goal schema. It must either lower to core goals or provide a versioned provider/checker ecosystem. Artifact consumers preserve unknown custom goals but cannot claim them satisfied without compatible checkers.
