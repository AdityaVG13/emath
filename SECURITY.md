# Security Model and Reporting

## Security boundaries

The principal attack surfaces are:

- untrusted `.emath` source and package metadata;
- hostile custom-kind schemas and lowering rules;
- dependency/registry substitution;
- malicious provider plugins;
- solver output and proof/certificate forgery;
- generated Rust and build-script execution;
- JIT/GPU/FFI backends;
- resource exhaustion and adversarial symbolic growth;
- evidence or source-map tampering;
- host promotion based on poisoned measurements.

## Default posture

- Parsing and semantic admission are bounded and fail closed.
- Package resolution uses content locks.
- Compiler plugins run in a sandboxed component boundary unless statically trusted.
- Native providers are explicit build dependencies and appear in the trust manifest.
- Build scripts and procedural macros are treated as code execution.
- Generated Rust defaults to `#![forbid(unsafe_code)]`.
- Solvers have proposal authority; independent checkers grant evidence authority.
- All expensive phases consume explicit budgets and cancellation tokens.
- Artifact manifests and evidence bundles are content-addressed.
- Promotion requires frozen workloads, provenance and rollback.

## Reporting

A production repository should add a private reporting address and coordinated disclosure policy before public release. Do not open public issues containing exploitable inputs, tokens, private model artifacts or unpublished proof failures.
