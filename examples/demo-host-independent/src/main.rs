//! Independent host: consumes the *committed* generated crate through a
//! normal Cargo path dependency, with no build-time pipeline. The committed
//! bytes must equal regeneration output (validated by `scripts/validate.sh`).

use affine_scorer::AffineScorer;

fn main() {
    let scorer = AffineScorer::new(0.5, -2.0).expect("preconditions hold");
    let score = scorer.score(10.0);
    println!("AffineScorer::new(0.5, -2.0).score(10.0) = {score}");
    assert!(
        (score - 3.0).abs() < 1e-9,
        "0.5 * 10 + (-2) must equal 3.0, got {score}"
    );
    assert!(AffineScorer::new(f64::NAN, 0.0).is_err());
    println!("independent host ok");
}
