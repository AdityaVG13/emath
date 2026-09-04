# Cognitive Control Surface

The agent-facing UI/API should answer immediately:

```text
What exactly is the task?
What source is authoritative?
Which FeatureIDs are involved?
What depends on them?
What must not change?
Which files may I edit?
Which goldens and tests prove completion?
Which choices remain unresolved?
What is the last passing state?
How do I roll back?
```

The control surface exposes operations:

```text
orient(feature|task)
expand(feature, depth)
impact(feature)
authority(feature)
holes(feature)
owners(feature)
gates(feature)
status(feature)
diff(old,new)
receipt(task)
resume(task)
```

All operations return structured data first and prose second.
