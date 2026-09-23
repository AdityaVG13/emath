// ── Counter-stream unit-interval kernel ──────────────────────────────
//
// ONE generator, one place: SplitMix64 (the compute-layer nucleus the
// stream contract composes above — no second RNG namespace).
// The seed is an f64 scalar whose to_bits() initializes the state
// (PROVISIONAL mapping; re-mappable without
// touching the generators). Uniform01 via the high 53 bits.
// Deterministic strict-f64 throughout: same seed ⟹ bit-identical
// draws.
//
// The former polynomial and probability bodies (poly_mul / poly_eval /
// sequence_generate / sequence_convolve / prob_sample / prob_density)
// had no callers after the authored probability cutover and were
// removed; the polynomial math now lives in the language as
// language/modules/algebra/float64_polynomials.emath, the distribution
// transforms and densities as language/modules/probability/*.emath.
// This file retains only the unit-interval stream primitive — the
// remaining native leaf (bead emath-nwmm6): a machine seam, never
// named mathematics.

/// One SplitMix64 step: stateful, deterministic, high-quality output
/// for seeding and streams alike.
pub fn splitmix64_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Uniform f64 in [0, 1) from one u64 (high 53 bits).
fn prob_uniform01(state: &mut u64) -> f64 {
    let bits = splitmix64_next(state) >> 11;
    (bits as f64) * (1.0 / (1u64 << 53) as f64)
}

/// Counter-stream unit-interval uniforms in [0, 1). `seed` is already the
/// local stream seed as an f64 bit pattern. Empty on `draws == 0`.
pub fn prob_unit_interval(seed: f64, draws: usize) -> Vec<f64> {
    if draws == 0 {
        return Vec::new();
    }
    let mut state = seed.to_bits();
    (0..draws).map(|_| prob_uniform01(&mut state)).collect()
}
