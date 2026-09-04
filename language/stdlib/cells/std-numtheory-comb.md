# `core::number_theory` + `core::combinatorics`; package contracts (05 section 3.3 #3 + #4)

Status: **contracts + exact reference implementations landed**
. `factorial` and `congruence`
already admit end-to-end (sema call table → EMIR `Factorial` /
`Congruence` ops); the remaining names below are contract-first;
`.emath` models calling them refuse with the standard unknown-function
diagnostic until the admission-table follow-up lands (the
special-functions seam pattern).

## B16; number theory (u64 carrier)

| Function | Arguments | Domain | Notes |
|---|---|---|---|
| `is_prime(n)` | `u64` | full `u64` range | **deterministic** Miller–Rabin; witness set = first twelve primes (a proven certificate for all `n < 2^64`). Exact answer, no probabilistic mode. |
| `factorize(n)` | `u64` | `n ≥ 1` | trial division (the named reference); rows `(prime, exponent)` ascending; product reconstructs `n`. `factorize(0)` refuses (no primary decomposition). |
| `gcd(a, b)` | `u64` |; | Euclid; `gcd(0, 0) = 0` (divisibility-lattice convention). |
| `lcm(a, b)` | `u64` |; | `lcm(0, x) = 0`; a result past `u64::MAX` **refuses**; never wraps. |
| `congruence(value, residue, m)` | `i128, i128, u64` | `m ≥ 1` | `value ≡ residue (mod m)` with Euclidean normalization: negative operands compare honestly (`congruence(24, -1, 5)` is true; the C9 Wilson spelling). `m = 0` refuses. |

## B17; combinatorics (i128 exact carrier)

| Function / type | Signature | Exactness boundary |
|---|---|---|
| `factorial(n)` | `u32 → i128` | `33!` is the last value on the carrier; `34!` **refuses**. Reference contract for the admitted EMIR `Factorial` op (i64 compute path); aligning that op's overflow behavior with the typed refusal is a documented follow-up. |
| `binomial(n, k)` | `u64 × u64 → i128` | multiplicative identity with symmetry reduction `k → min(k, n−k)`; stepwise division is exact at every index (no rounding ever). `k > n` is the empty choice `0`; a step past the carrier refuses (`C(200, 100)` does). |
| `Permutation` | finite carrier of `0..n` | C10: the const-generic `Permutation<8>` is underivable; the constructor is the runtime value form `Permutation::new(n)`; the const-generic surface is deferred until value generics land. `from_order` validates the bijection (duplicates / out-of-range refuse); `apply(i)` is the source index feeding position `i`; `successor()` is the lexicographic continuation and returns `None` (never a wrap) at the last ordering. |
| `enumerate_from(start, budget)` | budgeted walk | lexicographic enumeration starting AT `start`, up to `budget` items; the continuation is the next unvisited permutation or `None` at exhaustion. Batches partition the walk; resuming never repeats or skips. |

## Hamming distance (capsule-active exact binding)

`hamming_distance(left, right)` — `Vector × Vector → i64`. Counts the
positions where the two vectors differ, comparing elements bit-exactly
(f64 `to_bits`, so `-0.0` and `+0.0` differ). Equal lengths are
required; unequal lengths refuse with the typed `unequal-length`
diagnostic (kernel error `hamming_distance: vectors must have equal
length`).

Authoritative binding: capsule `std.capability.exact.hamming-distance`
in `language/spec/capabilities/exact/number-theory.emath` is
capsule-active and binds the domain-neutral `hamming-distance` native
kernel (arity 2, `(Vector,Vector)->Int`). Execution runs through the
`ApplyCapability` seam on the installed Language Distribution; the
retired `EmirOp::HammingDistance` variant no longer exists. Conformance:
`rs_code_pipeline_evaluates` (self-distance 0 and the Singleton bound)
plus the signed-zero bit-exactness mutation probe.

## Provider seam

`emath-core::{numtheory, combinatorics}` are the std-only reference
implementations; the EMIR ops (`Factorial`, `Congruence`, …) are the
compute path for admitted names. Both paths claim exactness; where the
carrier differs (i64 EMIR vs i128 reference), the reference defines
the refusal contract and the EMIR alignment is tracked, not assumed.

## No-claim boundaries

- `is_prime` is deterministic on `u64` only; a wider carrier is a
  different contract, not an extension of this one.
- `factorize` is trial division: exact and simple, not fastest. Pollard
  rho / sieve acceleration are performance follow-ups with identical
  outputs, not semantic changes.
- Combinations as a first-class type (combinatorial-number-system
  ranking/unranking), `Permutation<8>` const generics, and sema
  admission for `is_prime`/`factorize`/`gcd`/`lcm`/`binomial` are the
  declared follow-ups (the discrete-math epic `x750` builds on them).
- Wilson's theorem is served as a MODEL example
  (`congruence(factorial(p - 1), -1, p)` per C9), not as a proof
  claim: emath verifies the congruence for given `p`, it does not
  prove the theorem.
