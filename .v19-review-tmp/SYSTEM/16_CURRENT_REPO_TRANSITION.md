# Current Repository Transition

First objective: no behavior change.

```text
assign provisional FeatureID
→ point authority lock to legacy anchor
→ write capsule matching observed behavior
→ generate current goldens
→ dual-run legacy and constitution paths
→ resolve discrepancies explicitly
→ switch feature authority
→ generate legacy view
```

Never migrate all features simultaneously.
