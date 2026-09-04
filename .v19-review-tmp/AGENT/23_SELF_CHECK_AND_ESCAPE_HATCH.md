# Self-Check and Escape Hatch

Before acting, the agent checks:

```text
baseline fresh?
authority unique?
task scope valid?
dependencies complete?
blocking hole?
golden authority?
required provider available?
```

The agent may request more context or a Task Capsule revision. It may not silently widen scope.

When the generated context is wrong, the defect is filed against the Meaning Spine/index so later
agents inherit the correction.
