# `std.tensor.softmax`; capability cell contract (authoring draft)

Status: cell machinery implemented in `crates/emath-ir/src/capability.rs`.
This page is the stdlib-side
contract for authors. It documents data, not a parser keyword; nothing
here grows a core IR enum variant.

## Identity

| Field | Value |
|---|---|
| Schema | `emath.capability-cell.v1` |
| Canonical name | `std.tensor.softmax` |
| Class | `pure` (deterministic function of declared inputs) |
| Version | `1.0.0` |
| Migration | `frozen` |
| Arity | `1` (rank-1 vector) |

Canonical preimage (`canonical_cell`) carries exactly: schema, name, class,
version, migration, arity. `about` is presentation-only and never moves the
`CellId` (identity law, tested in `capability_cells.rs`).

## Reference semantics (strict-f64)

Stable-max form: `softmax(x) = exp(x - max(x)) / sum(exp(x - max(x)))`.
The shift is the law applied: `softmax(x) == softmax(x + c)` componentwise,
and it doubles as the overflow guard (exp(1000) would overflow f64 without
it).

Typed refusals; never silent:

- empty input (no numeric policy declared for the evaluation): `E-CELL-006`
- non-finite logit under the strict-f64 finite policy (NAN/INF): `E-CELL-006`
  (the policy declares what is refused; `f64::max` silently drops NAN, so
  the guard checks each element)

## Laws (all tested, `softmax_capability_cell.rs`)

1. **Shift invariance**; `softmax(x) == softmax(x + c)` componentwise.
2. **Nonnegativity**; every component `>= 0`.
3. **Normalization**; components sum to 1 within 1e-12; a single-element
   input normalizes to exactly 1.
4. **Overflow guard**; large finite logits (800, 799, 100) normalize with
   ordering preserved.

## Provider seam

The contract is one rank-1 vector evaluated whole. `softmax_axis_well_formed`
refuses rank-2-style axis requests (rank != 1) at the provider seam; a
wrong-axis failure is typed, never silently reinterpreted (negative seed
`tests/invalid/softmax_capability_cell.emath` expects `E-CELL-003`/`E-CELL-004`
at the admission seam).

## Surface spelling

```emath
use std.kinds.capability

emath capability Softmax:
    inputs:
        x: Float64
    outputs:
        probability: Float64
    definitions:
        probability = x
```

`capability` is not a lexer keyword and adds no stable-IR operation variant:
the schema (`use std.kinds.capability`) requires `inputs:`, exactly one
`outputs:`, and `definitions:` (`E-KIND-001` without the import,
`E-SYN-101` for outside sections). Run
`tests/fixtures/language/intro/imported-capabilities.emath`.

## Zero-core-delta claim (verify before extending)

Adding a cell appends to `SemanticPackage::capabilities` and is referenced
from `ExprNode::Apply` by `CapabilityId`. `ExprNode`, `UnaryOp`,
`BinaryOp` do not grow; the diff gate plus the negative tests
guard this. Never add a `Softmax` variant to a core enum.

## No-claim boundaries

- strict-f64 is the only phase-1 numeric model the reference accepts;
  an interval/certified profile is future work, not claimed here.
- The cell computes; it does not certify anything about the underlying
  distribution model (no probabilistic-semantics claim).
- Provider acceleration (vector backends) implements the same contract;
  a provider that cannot honor the rank-1 seam refuses by code.
