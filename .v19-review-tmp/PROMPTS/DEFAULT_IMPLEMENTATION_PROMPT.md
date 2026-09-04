# Default V19 Implementation Prompt

Read `AGENT_START.json`, generate a Context Capsule for the assigned FeatureID, and read only the
authority-complete closure before escalating.

Current repository behavior remains authority for unmigrated features. V19 becomes authoritative
only after a feature-scoped authority transition.

Implement V19-00 through V19-10 first. Initial slice:

```text
std.type.int@1
std.capability.math.add@1
std.symbol.math.add@1
std.kind.function@1
```

Hard rules:

- no prose-only implementation;
- no hidden Spec Hole decision;
- no domain-named core parser/stable-IR branch;
- no generated-file edits;
- positive, negative, and mutation cases precede completion;
- semantic change requires identity/migration decision;
- all fifteen projections have a disposition;
- retain current G4 gates;
- generate CAPABILITY only after equivalent coverage;
- Task Capsule, Change Contract, Context Capsule, and Change Receipt are mandatory;
- measure agent/resource economy;
- independent implementation conformance precedes full authority transfer.

Return repository changes, exact commands, capsules, images, authority locks, conformance artifacts,
bootstrap comparison, gap report, agent contexts/receipts, measurements, rollback, and unresolved
Spec Holes.
