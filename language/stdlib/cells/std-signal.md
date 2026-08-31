# `std.signal` — discrete signals over declared sampling

Status: std-layer package (bead `emath-r3-signal-z2yt`, Phase 13),
implemented in `crates/emath-core/src/signal.rs`. The reference
transform is the DIRECT O(n^2) DFT; FFT is a provider behind a
contract, never core.

## Declared sampling, never ambient

`Sampling { rate, phase }` is the only source of time semantics:
sample `n` is taken at `t(n) = phase + n/rate` (rate in Hz, phase in
seconds). There is deliberately no ambient or inferred form —
`Sampling::new` is the sole constructor, so an undeclared rate cannot
compile into a signal. A non-positive or non-finite rate, or a
non-finite phase, refuses typed (`E-SIGNAL-1`). Samples are validated
at construction; a non-finite sample refuses with its index named
(`E-SIGNAL-2`).

## Rate worlds do not mix silently

Signals sampled at different rates are different time worlds.
`convolve` REFUSES when the declared rates differ (`E-SIGNAL-4`)
instead of assuming a rate or resampling implicitly; resampling into
one declared time world is the user's explicit operation (fence: not
yet provided). Same-rate convolution is the direct definition:
length `n+m-1`, output phase = sum of the declared phases, because the
convolution support starts at `t_a0 + t_b0`. Mutant-tested: a mutant
that assumes equal rates is killed by the mismatched-rate refusal test.

## Reference transform: direct DFT, exact semantics

`DirectDft` computes `X[k] = Σ_n x[n]·e^{-2πikn/N}` by direct
double loop with a deterministic summation order. Empty input refuses
(`E-SIGNAL-3`) — never a fabricated spectrum. Contract tests pin known
transform pairs exactly to 1e-9 (e.g. `[1,2,3,4] ↦ [10, −2+2i, −2,
−2−2i]`) and Parseval's identity. The DFT's sign convention was caught
by the exact pair test during development (a double-negation computed
e^{+i}; the test defined the contract, the code was fixed).

## FFT is a provider, not core

`TransformBackend { name, transform }` is the provider contract. Core
ships the direct DFT as the reference backend. `Radix2Fft` (iterative
radix-2 Cooley-Tukey, bit-reversal permutation) plugs in as a provider
for nonzero power-of-two lengths; any other length refuses typed
(`E-SIGNAL-5`). Provider-vs-reference agreement is contract-tested
through `&dyn TransformBackend` objects, proving the seam is real.

## Windows

`Window::hann/hamming/rectangular` use the SYMMETRIC convention:
palindromic, hann vanishes at both endpoints (the last endpoint to
1e-12 — `cos(2π)` roundoff), hamming's endpoints are `2a−1 = −0.08`
by definition. Spectral-estimation (periodic) normalization is a named
fence, not silently mixed in.

## Named fences (follow-up slices, deliberately open)

- **No `.emath` surface**: a `signal` binder/declaration kind would
  extend the tree/binder surface; that design belongs to the
  measures/transforms lane (bead r2mt, which also gates this bead's
  claim edge) — BLOCKED there.
- **No resampling operator** yet (rate-world conversion is refused,
  not performed).
- **No provider crates outside core**: the FFT provider ships in-core
  behind the contract until the provider-crate layout decision lands.
- No wavelet or other transforms; no streaming/infinite signals; no
  multi-channel signals beyond vectors of samples.
