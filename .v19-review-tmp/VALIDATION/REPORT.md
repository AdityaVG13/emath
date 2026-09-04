# Validation Report

## Executed checks

- Prototype constitution/index compiler: **PASS**
- Full `validate_all.py`: **PASS**
- Deterministic repeated Language Image build: **PASS**
- Feature Capsules parsed: **156**
- Meaning Spine edges: **163**
- Unique FeatureIDs: **PASS**
- Dependency closure/cycle validation: **PASS**
- Surface role/spelling collision validation: **PASS**
- Authority uniqueness: **PASS**
- JSON files parsed: **55**
- TOML files parsed: **4**
- Python tools byte-compiled: **9**
- Rust files structurally checked: **3**
- Conformance cases resolved to known FeatureIDs: **8**
- V16 promotion ledger rows: **1224**
- Task IDs/dependencies: **420 / PASS**
- Current repository gap records: **45**
- Explicit Spec Holes: **5**

## Agent-orientation sample

- Feature: `std.kind.cipher@1`
- Dependency closure: **4 features**
- Reverse impact listed: **1 features**
- Read order: **7 files**
- Serialized context: **1740 bytes / ~435 rough tokens**

## Honest boundary

- The package implements a working prototype capsule parser, Language Image/index builder, gap auditor, orientation tool, impact closure, authority check, and receipt lint.
- It does not modify the GitHub repository or transfer authority from current reference/code.
- All 156 features remain prototype specifications until implemented, conformed, dual-run, and authority-switched in the repository.
- `projection-closures.json` deliberately reports missing realization projections.
- Repeated prototype image determinism is not the future Stage1/Stage2 self-hosting proof.
- The Rust reference workspace was structurally checked; no Rust toolchain was present, so it was not compiled.
