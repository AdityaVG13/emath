# Executable Feature Specifications

A Feature Capsule uses a restricted data-like `.emath` subset:

```emath
emath spec AddCapability:
    identity: "std.capability.math.add@1"
    class: "capability"
    status: "proposed"
    edition: 1
    summary: "Typed addition selected through a mathematical instance."
    dependencies: ["std.syntax.expression.core@1"]
    surface: {"spellings":["+","add"],"role":"infix/call"}
    canonical: {"lower_to":"CapabilityApplication","identity_fields":["capability","operands"]}
    semantics: {"signature":"forall T: Additive<T>. (T,T)->T"}
    exactness: {"policy":"inherited","implicit_demotion":false}
    effects: {"allowed":[]}
    worlds: {"applicability":"manifest-declared"}
    reference: {"mode":"reference-term","target":"std.capability.math.add@1"}
    artifact: {"classes":["value","symbolic","diagnostic"]}
    projections: ["identity", "..."]
    diagnostics: ["E-CAPABILITY-DOMAIN"]
    conformance: {"positive":["..."],"negative":["..."],"mutation":["..."]}
    evolution: {"meaning_change":"new major or migration"}
    agent: {"owner":"emath-term","read":["..."],"edit":["..."],"hazards":["..."]}
```

The format is deliberately not a general macro language. It declares language meaning; it does not
perform arbitrary I/O or mutate the repository.
