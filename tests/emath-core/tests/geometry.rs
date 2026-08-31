//! geometry tests migrated from the in-crate `#[cfg(test)]` module:
//! every symbol they exercise is public crate surface.

use emath_core::geometry::*;
use emath_core::geometry::Field;

#[test]
fn quarter_turn_matrices_are_exact_signatures() {
    // 90°: (x, y) -> (-y, x). The matrix entries are -1/0/1 by
    // construction; verified through a point application.
    let rotate = Transform::rotation_quarter_turns(1);
    let p: Point<Rational> = Point::new(Rational::new(2, 1).expect("fits"), Field::zero());
    assert_eq!(
        rotate.apply_point(&p).expect("exact"),
        Point::new(Field::zero(), Rational::new(2, 1).expect("fits"))
    );
}

#[test]
fn rational_canonicalization_is_normalizing() {
    assert_eq!(
        Rational::new(6, -8).expect("normalizes"),
        Rational::new(-3, 4).expect("normalizes")
    );
    assert_eq!(Rational::new(0, 5).expect("zero"), Field::zero());
}
