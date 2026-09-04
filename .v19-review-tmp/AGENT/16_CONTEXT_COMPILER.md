# Context Compiler

Inputs:

```text
Task Capsule or root FeatureID
Language Image
Meaning Spine
authority lock
current coverage
Spec Holes
conformance index
recent Change Receipts
```

Algorithm:

1. validate freshness;
2. compute dependency and reverse-impact closure;
3. retain normative sources;
4. retain owner contracts and touched implementation anchors;
5. retain required tests/mutations/migrations;
6. summarize large ancestors by stable fields;
7. order from law → feature → implementation → evidence;
8. fit the requested context budget without deleting authority;
9. report anything omitted and how to expand it.
