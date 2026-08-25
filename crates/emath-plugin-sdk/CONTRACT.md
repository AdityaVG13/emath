# CONTRACT.md - emath-plugin-sdk

## Purpose and layer

Tier 7 (governance and operations) plugin SDK slice: descriptors, sandbox
policy decisions, and a deterministic test-harness contract. Std-only, no
network, no component host. Depends on `emath-core` (FNV-1a64 content id).

## Public types and semantics

- `PluginDescriptor` (schema `emath.plugin`): id, kind, interface core,
  declared capabilities, sandbox policy. Canonical JSON rendering
  (`canonical_json`) and FNV-1a64 content id (`content_id`).
- `SandboxPolicy`: fuel (`None` = unmetered), granted permissions, network
  flag, allowed capabilities.
- `Trust`: `Local` (locally audited) vs `Untrusted` (third-party).
- `PluginOutput` (`Vec<u8>`): the runtime result contract.
- `PluginError`: typed error with stable `E-PLG-0xx` code.
- Free fns: `admit` (sandbox/fuel/permission gate), `execute` (harness
  entry), `compatible` (interface-core compatibility), `descriptor_for`.
- Constants `PLUGIN_SCHEMA`, `INTERFACE_CORE`.

## Invariants

- Plugin ids must be non-empty and free of ASCII control characters
  (breaks log/diagnostic framing and content-id ambiguity), refused with
  `E-PLG-005` before any sandbox check.
- Every declared capability must be inside `allowed_capabilities`
  (`E-PLG-003`); an empty declared capability set is refused
  (`E-PLG-003`).
- A capability touching a resource class requires the matching granted
  permission; `network` requires the `network` permission (`E-PLG-002`).
- Untrusted descriptors must declare positive fuel (`E-PLG-002`).
- `execute` re-enforces positive fuel under every trust class before
  `E-PLG-001`, so `Trust::Local` can never admit an unmetered plugin onto an
  execution path.
- Phase 1 has no component runtime; `execute` is always a typed refusal
  (`E-PLG-001`).
- `canonical_json` is byte-stable; `content_id` is the shared FNV-1a64
  convention.

## Error model

`PluginError` with stable codes: `E-PLG-001` (component runtime absent),
`E-PLG-002` (sandbox/fuel/permission violation), `E-PLG-003` (capability
outside the allowed set or none declared), `E-PLG-004` (interface-core
mismatch), `E-PLG-005` (empty or ASCII-control-bearing plugin id).

## Determinism class

Admission/refusal decisions, `canonical_json`, and `content_id` are
deterministic; `execute` deterministically refuses every Phase 1 call.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. The crate sets `#![forbid(unsafe_code)]`.

## Feature flags

None (`Cargo.toml` has no `[features]`).

## Conformance tests

None present. No `tests/` directory on disk and no inline `#[cfg(test)]`
module in `src/lib.rs`.

## No-claim boundaries

Plugin execution is not implemented in Phase 1 (component runtime absent);
the `execute` call shape (`descriptor, input -> output`) is the stable
surface the Phase 2+ runtime must fill. A declared permission is only as
good as the gate that enforces it; no runtime verifies a plugin actually
holds the resources it declares.
