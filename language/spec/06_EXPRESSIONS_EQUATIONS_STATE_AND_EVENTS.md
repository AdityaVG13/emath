# Expressions, Equations, State and Events

## Expressions

Expressions support literals, variables, calls, records, tuples, collections, conditionals, pattern matching, binders and scoped notation.

Mathematical binders include:

```emath
sum i in 0..N: x[i]
integral x in Ω: f(x)
forall x in domain: property(x)
exists witness in candidates: valid(witness)
derivative f wrt x
jacobian f wrt x
hessian f wrt x
```

A binder produces semantic structure; it is not immediately expanded into loops.

## Definitions

A definition is directed and names a value/function:

```emath
definitions:
    energy = 0.5 * mass * velocity^2
```

Recursive definitions require explicit recursion/termination policy.

## Equations and relations

```emath
equations:
    derivative(position) == velocity
    mass * derivative(velocity) == force
```

Equations retain equality/relational meaning. Solver planning may causalize or discretize them with a trace.

## Constraints and objectives

```emath
constraints:
    capacity <= limit

objectives:
    minimize cost
    maximize quality
```

Multiple objectives declare lexicographic, weighted or Pareto semantics.

## State

State fields have ownership, initialization, mutability and clock semantics. State is not inferred merely because a Rust variable is mutable.

## Transitions and events

```emath
transitions:
    on observe(value):
        state.count += 1

events:
    event ThresholdCrossed(value: Real)
```

Events define ordering, clock/domain and delivery policy. Continuous zero crossings and discrete events are distinct.

## Effects

Effects include state mutation, randomness, IO, provider call, allocation, network and nondeterministic scheduling. Pure expressions cannot invoke effects through hidden provider behavior.
