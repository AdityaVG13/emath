# Task Graph

Implementation tasks are derived from missing projection closures and dependencies.

```text
feature specification
→ conformance
→ reference/provider
→ generated projections
→ dual-run
→ authority transition
→ stable
```

Independent projection tasks may run in parallel. Authority transition waits for their join.
