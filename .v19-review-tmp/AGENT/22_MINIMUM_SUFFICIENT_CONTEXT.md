# Minimum Sufficient Context

A context is sufficient when it contains:

```text
all authority required to make the change
all dependencies whose contracts constrain the change
all reverse impacts requiring validation
all known hazards and blocking holes
all acceptance and rollback information
```

It is minimal when removing any item makes one of those properties false.

The Context Compiler should optimize this condition, not an arbitrary token number.
