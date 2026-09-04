# emath V19 — Executable Language Constitution

## Problem

A design ZIP cannot be the implementation authority. Two agents can reasonably disagree about:

```text
surface syntax
precedence and aliases
section multiplicity
definition versus equality
types and coercions
world applicability
exactness and evidence
lowering
artifact class
diagnostics
migration
```

V19 removes that discretion from implementation.

## Canonical workflow

```text
idea or wave proposal
→ Feature classification
→ Spec Hole resolution where needed
→ ELP + Feature Capsule
→ conformance authored before completion
→ generated skeleton and impact closure
→ irreducible implementation/provider work
→ mutation and cross-world checks
→ authority transition
→ Language Image and generated views
→ append-only Change Receipt
```

## Authority after migration

```text
implementation/CONSTITUTION.md:
    project laws and authority firewall

language/authority.lock:
    exactly one authority source per FeatureID

language/spec/:
    accepted feature meaning

language/generated/language-image:
    compiled accepted language

implementation code:
    one realization of the language

generated docs/capability/agent views:
    projections

wave archives:
    design inputs and proposed candidates
```

## Transition

Current behavior is preserved feature by feature. Nothing becomes V19-authoritative merely because
this package describes it. Each feature moves through:

```text
legacy-authoritative
→ dual-run
→ constitution-authoritative
→ legacy-view-generated
```
