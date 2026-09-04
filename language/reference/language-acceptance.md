# Chapter 16: Language Acceptance

These checks are the finish line for the *whole* language, not a claim
that we are there. They apply the router law (see
`implementation/VISION.md`): nothing is refused at the door, and
nothing crosses the exit unlabeled. Today the working subset is: parse and admit
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

## Structural authority boundary

For every named mathematical feature, authority is read in this order:

1. the authored Feature Capsule in `language/spec/` defines meaning and the
   requested authority target;
2. `language.lock` selects the admitted authority state;
3. the verified Language Image and generated reference/runtime views reproduce
   that state without adding facts;
4. Rust IR, VM, kernels, providers, and emitters implement only the selected
   mechanism.

The Rust nucleus may contain universal representation and execution machinery:
opaque constants and loads, neutral carriers and graphs, closed VM control,
budget/fault handling, image verification, FeatureID-to-kernel/provider
bindings, and artifact rendering. Reusable arithmetic in a kernel is also only
a mechanism. It is forbidden for parser, admission, stable IR, registry,
backend, runtime, or a public semantic module to recover feature meaning from a
feature spelling or to select applicability, world, exactness, evidence,
authority, or result/claim labels.

A computed value proves only that the selected mechanism returned that value
under its declared inputs, world, numeric policy, and budget. It does not prove
a proposition, certify unrequested equivalence, enlarge the feature's carrier
or domain, or raise an evidence/exactness label. Such claims require the capsule
contract and its declared checkable evidence. Generated material is a locked
projection, reference prose is normative explanation, and crate contracts are
mechanism boundaries; none is a second source of mathematical truth.

The current structural gate's exact forbidden residue is 58 feature-name
dispatch sites, 79 stable mathematical IR variants, one active handwritten
registry entry, zero kernel claim-authority sites, and 26 public semantic
modules. Only the kernel claim-authority category is zero. The other counts are
not accepted architecture: they are an actionable migration inventory tied to
the currently authored FeatureIDs and may only decrease.

## Checked-in distribution acceptance

The source-first capstone is
`tests/emath-exec-ir/tests/portable_language_capstone.rs`. It loads the checked-in
`LanguageDistribution` from `language/` (including `language.image`,
`language.lock`, and `source-map.lock`), verifies it, installs its capsule-active
kernel bindings, and invokes exact, linear, special/probability,
graph/optimization/game, control/PDE, and geometry/units/chemistry representatives
through `ApplyCapability`. A caller-supplied image-ID string is never authority.
Two loads must have equal semantic and distribution identities and equal decoded
distributions.

Before installation, acceptance requires all of the following:

1. every partition stamp and the image/lock identities verify;
2. every capsule has an entry in the source-map partition;
3. every capsule-active kernel declaration resolves the exact authored kernel
   ID, carrier signature, and arity; and
4. a refused replacement leaves the last valid installed bindings unchanged.

The focused negative cases mutate in-memory clones: stale lock identity,
tampered source-map bytes, a missing source-map partition, and a stale kernel
signature must all refuse. The production APIs do not yet compose this complete
boundary themselves: `install_language_distribution` does not call
`LanguageDistribution::verify`, and `LanguageDistribution::verify` does not
require `language.sources`. The capstone therefore performs those two checks
explicitly. Removing that adapter is blocked until those API contracts are
strengthened.

`tests/emath-exec-ir/tests/independent_language_reader.rs` is the
cross-implementation check. It reads the actual canonical checked-in bytes,
decodes length-delimited partitions without the production image decoder,
recomputes each FNV partition stamp and the framed distribution SHA-256,
reconciles external and embedded locks/source maps, recovers authority and the
exact-add runtime row, and independently obtains exact `2 + 1 = 3` with overflow
refusal. The image does not contain full capsule canonical bytes, so an
image-only reader can reproduce the semantic identity token and lock agreement
but cannot recompute that semantic hash from first principles; doing so requires
the authored capsule sources.

Unavailable candidates remain explicit and non-executable: rank-polymorphic
`std.capability.tensor.index`, variadic `std.capability.tensor.einsum` and
`std.capability.reduction.finite`, `std.capability.special.elliptic-pi`,
`std.capability.statistics.median`,
`std.capability.dynamics.simulation-world`, and
`std.capability.pde.tensor-and-divergence`. Their authority is
`capsule-candidate`; generic native-kernel lookup must return no binding.
