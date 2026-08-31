//! coordinate tests migrated from the in-crate `#[cfg(test)]` module:
//! every symbol they exercise is public crate surface.

use emath_core::coordinate::*;

const TAU_TOL: f64 = 1e-12;

#[test]
fn spherical_round_trip_at_representable_points() {
    for &(r, theta, phi) in &[
        (1.0, 0.0, 0.0),
        (2.0, std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2),
        (3.0, std::f64::consts::PI, 0.0),
    ] {
        let (x, y, z) = (spherical_of(r, theta, phi));
        let (back_r, back_theta, back_phi) = cartesian_to_spherical(x, y, z);
        assert!((back_r - r).abs() < TAU_TOL, "r round trip {back_r}");
        assert!((back_theta - theta).abs() < TAU_TOL, "theta round trip {back_theta}");
        assert!(
            (angle_diff(back_phi, phi)).abs() < TAU_TOL,
            "phi round trip {back_phi}"
        );
    }
}

#[test]
fn spherical_carries_radius_sign_domain() {
    // A negative radius is DATA the formulas evaluate — the typed
    // refusal lives at the map boundary, not the formula.
    let refused = std::panic::catch_unwind(|| {
        let _ = spherical_to_cartesian(-1.0, 0.0, 0.0);
    });
    assert!(refused.is_err(), "negative radius must refuse");
}

#[test]
fn cylindrical_round_trip() {
    let [x, y, z] = cylindrical_to_cartesian(2.0, std::f64::consts::FRAC_PI_3, 4.0);
    let (back_r, back_theta, back_z) = cartesian_to_cylindrical(x, y, z);
    assert!((back_r - 2.0).abs() < TAU_TOL);
    assert!((back_theta - std::f64::consts::FRAC_PI_3).abs() < TAU_TOL);
    assert!((back_z - 4.0).abs() < TAU_TOL);
}

#[test]
fn pole_azimuth_is_deterministic_zero() {
    // theta = 0: the point is on the +z axis; azimuth is 0 by
    // convention, never a fabricated angle.
    let (r, theta, phi) = cartesian_to_spherical(0.0, 0.0, 5.0);
    assert_eq!(r, 5.0);
    assert_eq!(theta, 0.0);
    assert_eq!(phi, 0.0);
}

#[test]
fn jacobian_spherical_is_r_squared_sin_theta() {
    // det = r² sin(theta): at theta = pi/2 the equatorial band is
    // exactly r². The absolute radius cancels (unit sphere slice).
    let det = jacobian_det_spherical(std::f64::consts::FRAC_PI_2, 0.0);
    assert!((det - 1.0).abs() < TAU_TOL, "det(pi/2) == 1");
}

fn spherical_of(r: f64, theta: f64, phi: f64) -> (f64, f64, f64) {
    (r * theta.sin() * phi.cos(), r * theta.sin() * phi.sin(), r * theta.cos())
}

fn angle_diff(a: f64, b: f64) -> f64 {
    let difference = (a - b).rem_euclid(2.0 * std::f64::consts::PI);
    if difference > std::f64::consts::PI {
        difference - 2.0 * std::f64::consts::PI
    } else {
        difference
    }
}
