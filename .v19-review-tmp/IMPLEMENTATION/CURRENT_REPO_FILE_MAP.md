# Current Repository File Map

Extend:

```text
implementation/CONSTITUTION.md
implementation/VISION.md
language/README.md
language/MAINTAINING.md
language/reference/
language/grammar/
language/CAPABILITY.md
language/stdlib/
elps/
scripts/
crates/emath-syntax
crates/emath-schema
crates/emath-sema
crates/emath-ir
crates/emath-world-ir
crates/emath-term
crates/emath-plan
crates/emath-artifact
crates/emath-evidence
crates/emath-checker
crates/emath-lsp
crates/emath-wasm
tests/
```

Add target paths:

```text
language/spec/
language/authority.lock
language/spec-holes.json
language/generated/
language/conformance/
agent/
```

Follow the current crate map; create at most one new orchestration crate before ownership pressure is
measured.
