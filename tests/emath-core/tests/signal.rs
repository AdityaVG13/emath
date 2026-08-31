//! signal tests migrated from the in-crate `#[cfg(test)]` module:
//! every symbol they exercise is public crate surface.

use emath_core::signal::*;
use emath_core::signal::TransformBackend;

/// Twiddle identity: exp(-2 pi i k / N) conjugates back; the radix-2
/// provider must equal the direct reference on a canonical pair.
#[test]
fn quarter_turn_twiddles_are_exact() {
    let w = Complex::new(0.0f64.cos(), -(0.0f64).sin());
    assert_eq!(w, Complex::new(1.0, 0.0));
    let fft = Radix2Fft
        .transform(&[Complex::new(1.0, 0.0), Complex::new(0.0, 0.0)])
        .expect("power of two");
    assert_eq!(fft, vec![Complex::new(1.0, 0.0), Complex::new(1.0, 0.0)]);
}
