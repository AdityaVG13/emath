# std.stochastic — deterministic SDE execution cells

Capability cells (data, not core-kind growth): the kernels live in
`emath-rt::stochastic`, exposed through the immutable native-kernel
registry keyed by capability name (no SDE EmirOp, no parser branch, no
backend domain switch). The full dispatch chain is generic end to end:

1. The cell is DECLARED with the standard capability surface
   (`class: pure`, `version:`, `migration:`, `inputs:`, `outputs:`)
   and admitted into the package's capability arena.
2. A call by name (bare, or qualified `std::stochastic::name`) lowers
   through the generic declared-capability call path to
   `ExprNode::Apply` — the shared builtin-miss seam: builtins first,
   then declared/mounted capability cells, then the typed
   unknown-function refusal. Any domain (geometry today) reuses the
   same seam without a new keyword.
3. The emitter lowers `ExprNode::Apply` to `EmirOp::ApplyCapability`
   using the admitted record (name + class) as data.
4. The interpreter resolves compiled-cell reference data FIRST; on a
   miss it consults the native-kernel registry with the identical
   arity/refusal discipline. Unknown names keep the exact pre-existing
   refusal; kernel refusals surface verbatim as typed
   `CapabilityRefused` values naming the capability and the stable
   code.

## Cells

| Capability | Arity | Returns | Refusals |
|---|---|---|---|
| `std.stochastic.euler_maruyama` | 7 | trajectory `Vector` (length n+1) | `E-SIM-SEED`, `E-SIM-001`, `E-SIM-002`, `E-SIM-003` |
| `std.stochastic.stratonovich` | 7 | trajectory `Vector` (length n+1) | same |

Arguments in order: `mu` (Vector), `sigma` (Vector), `x0` (scalar),
`h` (scalar), `steps` (scalar, integer-like), `seed` (scalar in
[0, 2⁶⁴)), `stream` (Vector/label carrier; root today).

## Determinism contract

One seed ⟹ one stream ⟹ bit-identical draws; the Z draws are the SAME
Normal(0,1) draws the `std.prob` sampler yields (the vnqo counter-based
stream → local seed → SplitMix64 → Box–Muller composition). No ambient
entropy, no hidden seed; omission of a legal seed refuses
`E-SIM-SEED`.

## Generic mechanism

The native-kernel registry is an immutable static table (no runtime
mutation), keyed by capability data, with identical arity/guard/refusal
discipline to the compiled-cell path. A toy entry
(`std.stochastic.toy_double`) proves the mechanism is domain-neutral:
any future kernel-backed pure cell registers here without touching the
VM/parser/backend.

## No-claim boundaries

- Interpreted local reference semantics only; Rust-backend codegen for
  kernel-backed cells is an explicit refusal.
- Scalar SDEs with polynomial drift/diffusion only.
- The `stream` argument accepts the label carrier; only the root
  stream executes today (declared split topology for those labels is a
  named deferral).
