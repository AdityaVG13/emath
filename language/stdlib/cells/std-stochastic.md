# `std.stochastic`; seed identity, named streams, replayable randomness

Status: thin contract slice of `emath-gap-stochastic-vnqo`, implemented
in `crates/emath-core/src/stochastic.rs`. This is the CONTRACT layer
(what any stochastic construct must satisfy to be admissible); the
distribution semantics (world meanings for `Normal(mu, sigma)` and
friends) are a separate lane and consume this seam.

## Seed is identity, never ambient

`Seed` is an explicit newtype with exactly one constructor. There is
no `Default`, no global RNG, no ambient entropy anywhere in core: a
seed exists only when a run declares one, and entropy access in
generated code is a DECLARED capability (C10); undeclared access
refuses with `E-STOCH-3`, never silently pseudo-random.

## Named algorithm, closed gate

The generator of record is `philox4x32-10` (Philox4x32, 10 rounds,
Random123 construction; counter-based, so streams split without
state carry). `stream_value` refuses any other algorithm name
(`E-STOCH-1`): receipts cannot bind an unnamed generator, and a
provider cannot silently swap generators behind the same receipt.
Algorithm identity is recorded in every receipt.

## Declared splits, parallel-safe by construction

`StreamPath` is the declared split topology (the empty path is the
ROOT stream; labels are non-empty and ORDERED; `a.b` ≠ `b.a`;
`E-STOCH-2` on an empty label). The stream primitive
`stream_value(seed, algorithm, path, counter) -> u64` is PURE in all
arguments. Declared mapping: Philox key words = seed halves; counter
words 0-1 = FNV-1a64 of the canonical path; counter words 2-3 = the
query counter. Consequences, pinned by tests:

- topology changes the stream (splits are real; root ≠ `a` ≠ `a.b`,
  and `a.b` ≠ `b.a`);
- call order changes nothing (batch == single queries; reordered
  queries give identical per-counter values); parallelism level
  cannot change results;
- the seed is identity (different seed ⇒ different stream).

## Receipts bind the triple

`StochasticReceipt` binds (seed, algorithm, stream path) with a
canonical one-line form and an `fnv1a64:` content id. Same binding ⇒
same id; changing ANY component changes the id. A receipt that cannot
reproduce its run is not a receipt.

## Named fences (follow-up slices, deliberately open)

- **Distributions are another lane** (per orchestration split):
  sampling procedures and parameterization conventions as world
  meanings with declared laws; two providers sampling the same world
  + seed must produce the same stream via `stream_value`. Not claimed
  here.
- **No goal-request surface yet**: how a simulate-style goal declares
  and receives its seed, and the byte-identical e2e replay under two
  parallelism levels, land with the goal/simulate lanes.
- **No cross-platform stream equality proof** (rides the
  numeric-truth/platform-matrix beads); the u32 arithmetic here is
  integer-exact, so the stream itself is platform-independent by
  construction.
- **No `language/spec/` split-out yet**; the contract is documented
  in the reference (types chapter) and this cell; a dedicated spec
  chapter follows the epic's full landing.
- Campaign-level seeding policy remains downstream.

## ONE seed story: stateful local generators

SplitMix64-class stepping generators (the probability nucleus keeps a
`u64` state) do NOT own a seed namespace. `local_stream_seed(seed,
path)` derives their initial state as **counter 0 of the declared
stream**; the same `(Seed, StreamPath)` identity every other stream
consumer uses. The runtime probability nucleus now calls this function
for both root and named split paths, so no provisional raw-seed mapping
or second RNG namespace remains. Mutant-tested: returning raw seed bits
instead fails the bridge test.

## Consumers

- `emath-xx0x.5` (probability nucleus) is the first distribution lane
  built on this seam: Normal/Uniform/Bernoulli sampling derives its
  local generator seed from the declared counter-based stream, with
  bit-identical replay and path splitting; runnable example
  [../../examples/probability/seeded_sampling.emath](../../examples/probability/seeded_sampling.emath),
  contract `language/stdlib/cells/std-probability.md`. The seed/stream
  vocabulary here (explicit `Seed`, named algorithm, declared splits)
  is exercised by the language-level reproducibility test.
