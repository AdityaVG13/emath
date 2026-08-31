# Chapter 16: Language Acceptance Gates

These gates are the finish line for the *whole* language, not a claim
that we are there. Today the working subset is: parse and admit
`function` / `policy` / `model`, evaluate strict-f64 definitions,
simulate explicit ODEs, and generate Rust for `evaluate` goals. Most
official examples still illustrate later chapters.

The language is accepted only when:

1. grammar and parser handle every official example and invalid fixture deterministically;
2. formatter is idempotent and parse-preserving;
3. package/import/name resolution is locked and reproducible;
4. custom-kind expansion is bounded, source-mapped and versioned;
5. constructors enforce valid-state boundaries in generated Rust;
6. type/unit/shape/domain diagnostics name conflicting constraints;
7. definitions remain distinct from goals/plans;
8. unknown providers can be lifted into parametric artifacts;
9. canonical semantic identities ignore declared presentation differences and change for every semantic mutation;
10. migrations are explicit and golden-tested;
11. no official example depends on an undocumented parser exception;
12. all public language features have at least one producer, consumer, negative case and artifact consequence.
