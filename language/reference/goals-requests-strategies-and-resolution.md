# Chapter 9: Goals, Requests, Strategies and Resolution

## Goal structure

The `goals:` section names the work the compiler must perform. It is
optional: when a declaration omits it, every definition is an
`evaluate` goal producing `rust.library` (the compiler asks for the
whole surface). Declaring `goals:` selects just the work you want:

```emath
goals:
    evaluate <score>:
        produce rust.library
```

Advanced goals carry policy; a goal names the work and policy, not a
hard-coded algorithm:

```emath
goals:
    differentiate <score>:
        wrt [state.alpha, state.gamma]
        order 1
        require evidence >= E2
        produce rust.library
```

## Core goals

```text
evaluate, transform, simplify, differentiate
integrate, solve, optimize, simulate
search, synthesize, prove, verify
compile, benchmark
```

## Native symbolic slice

`simplify <target>` computes during planning for exact integer scalar
expressions. It recursively folds exact integer constants and applies neutral
element rewrites such as `x + 0`, `x - 0`, and `x * 1`; the resulting goal
expression is carried by a `native-symbolic` plan. Rewrite rules use
`RewritePattern` captures and are labeled `structural-checked`, never `proved`
without a certificate.

The native algebraic decision procedure compares univariate polynomial
coefficient vectors exactly for `+`, `-`, `*`, and non-negative integer powers,
up to degree 64. Overflow/resource failures are `E-SYM-002`; expressions
outside that fragment, including transcendental and general first-order
claims, are `E-SYM-003`. Gröbner bases, ideal membership, CAD, and arbitrary
quantified claims are explicit no-claims pending a compatible provider and
checker.

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
