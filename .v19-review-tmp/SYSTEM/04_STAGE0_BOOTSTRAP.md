# Stage-0 Bootstrap

Stage 0 is a bounded, lossless, domain-neutral parser and schema interpreter.

It must be:

```text
panic-free on arbitrary UTF-8
span-preserving
registry-driven
unknown-symbol-preserving
deterministic
resource-bounded
```

The first capsule schema is seeded in Rust/JSON because a language cannot parse its own definition
from nothing. Stage 1 then replaces as much seed behavior as possible.
