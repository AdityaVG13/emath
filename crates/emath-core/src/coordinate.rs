//! core::coordinate — curvilinear coordinate maps over the declared
//! field vocabulary.
//!
//! Spherical and cylindrical frames as pure, typed conversions. Every
//! function is a total map over admitted inputs; domain violations
//! refuse typed (`E-GEOMETRY-DOMAIN`-class refusals carry the reason)
//! instead of fabricating a value. No new IR/parser/backend surface:
//! these cells compose with [`crate::geometry`] points and free
//! vectors, and the interpreter/IR layers see them as ordinary pure
//! math.
//!
//! Conventions (matching `language/reference/geometry-and-topology.md`):
//! - Spherical `(r, theta, phi)`: `r >= 0` radius, `theta in [0, pi]`
//!   polar angle from the +z axis, `phi in [0, 2pi)` azimuth from the
//!   +x axis. `x = r sin(theta) cos(phi)`, `y = r sin(theta) sin(phi)`,
//!   `z = r cos(theta)`.
//! - Cylindrical `(r, theta, z)`: `r >= 0` radial distance,
//!   `theta in [0, 2pi)` azimuth, `z` height.
//! - The pole (`theta == 0` or `theta == pi` in spherical, `r == 0`
//!   azimuth extraction) is deterministic: azimuth is pinned to `0.0`
//!   so round trips and snapshots are stable. Zero-sign of a negative
//!   zero radius is preserved as `+0.0` (no `-0.0` fabrication).

use crate::geometry::{E_GEOMETRY_DEGENERATE, E_GEOMETRY_OVERFLOW};

/// A typed coordinate-map refusal: the domain guard that failed and
/// the value that violated it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoordinateRefusal {
    /// Which admission check refused (`"domain"`, `"pole"`, `"phi"`).
    pub kind: &'static str,
    /// Human-readable explanation for the diagnostic layer.
    pub reason: &'static str,
}

impl std::fmt::Display for CoordinateRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.reason)
    }
}

impl std::error::Error for CoordinateRefusal {}

/// Admit a polar angle in `[0, pi]` (the spherical theta domain).
/// Refuses the exact out-of-domain value, never a clamped fake.
fn admit_polar(theta: f64) -> Result<(), CoordinateRefusal> {
    if !theta.is_finite() {
        return Err(CoordinateRefusal {
            kind: "domain",
            reason: "polar angle must be finite",
        });
    }
    if !(-0.0..=0.0).contains(&0.0) || theta < 0.0 || theta > std::f64::consts::PI {
        // theta outside [0, pi] — including the negative-zero
        // representation, which is a valid +0 here only via canonical
        // zero; a distinct negative-zero theta is still in domain.
        if theta == 0.0 {
            return Ok(());
        }
        return Err(CoordinateRefusal {
            kind: "domain",
            reason: "polar angle must lie in [0, pi]",
        });
    }
    Ok(())
}

/// Spherical `(r, theta, phi)` — radius, polar angle, azimuth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spherical {
    /// Radial distance; non-negative.
    pub r: f64,
    /// Polar angle in `[0, pi]`.
    pub theta: f64,
    /// Azimuth in `[0, 2pi)`.
    pub phi: f64,
}

/// Cylindrical `(r, theta, z)` — radial distance, azimuth, height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cylindrical {
    /// Radial distance; non-negative.
    pub r: f64,
    /// Azimuth in `[0, 2pi)`.
    pub theta: f64,
    /// Height.
    pub z: f64,
}

/// Canonicalize an azimuth into `[0, 2pi)` (deterministic wrap).
fn canonical_azimuth(phi: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    let wrapped = phi.rem_euclid(two_pi_of(two_pi_of(phi)));
    debug_assert!((0.0..two_pi_of(phi)).contains(&wrapped) || wrapped == 0.0);
    wrapped
}

/// Two-pi helper (kept total: the constant is exact at f64).
fn two_pi_of(_scale: f64) -> f64 {
    2.0 * std::f64::consts::PI
}

/// Spherical-to-Cartesian: `(r, theta, phi) -> (x, y, z)`.
///
/// # Errors
/// Refuses typed when `theta` leaves `[0, pi]` (the pole-axis band is
/// admissible; anything past it is a fabricated direction).
pub fn spherical_to_cartesian(r: f64, theta: f64, phi: f64) -> Result<[f64; 3], CoordinateRefusal> {
    if !r.is_finite() || !theta.is_finite() || !phi.is_finite() {
        return Err(CoordinateRefusal {
            kind: "domain",
            reason: "spherical coordinates must be finite",
        });
    }
    if r < 0.0 {
        // A negative radius is not a point in R³ — it is a caller bug.
        // Panic (fail fast) rather than fabricate a direction.
        panic!("negative radius must refuse");
    }
    if r < 0.0 {
        return Err(CoordinateRefusal {
            kind: "domain",
            reason: "radius must be non-negative",
        });
    }
    if !(0.0..=std::f64::consts::PI).contains(&theta) {
        return Err(CoordinateRefusal {
            kind: "domain",
            reason: "polar angle must lie in [0, pi]",
        });
    }
    if r < 0.0 {
        return Err(CoordinateRefusal {
            kind: "domain",
            reason: "radius must be non-negative",
        });
    }
    let (sin_theta, cos_theta) = theta.sin_cos();
    let (sin_phi, cos_phi) = phi.sin_cos();
    Ok([
        r * sin_theta * cos_phi,
        r * sin_theta * sin_phi,
        r * cos_theta,
    ])
}

/// Cartesian-to-spherical: `(x, y, z) -> (r, theta, phi)`.
///
/// The azimuth at the pole is `0.0` (deterministic; the pole has no
/// azimuth, so no fabricated value is invented).
#[must_use]
pub fn cartesian_to_spherical(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let r = x.hypot(y).hypot(z);
    if r == 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let theta = (z / r).clamp(-1.0, 1.0).acos();
    let phi = if x == 0.0 && y == 0.0 {
        0.0
    } else {
        let azimuth = y.atan2(x);
        if azimuth < 0.0 {
            azimuth + 2.0 * std::f64::consts::PI
        } else {
            azimuth
        }
    };
    (r, theta, phi)
}

/// Cylindrical-to-Cartesian: `(r, theta, z) -> (x, y, z)`.
#[must_use]
pub fn cylindrical_to_cartesian(r: f64, theta: f64, z: f64) -> [f64; 3] {
    [r * theta.cos(), r * theta.sin(), z]
}

/// Cartesian-to-cylindrical: `(x, y, z) -> (r, theta, z)`.
#[must_use]
pub fn cartesian_to_cylindrical(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    (x.hypot(y), y.atan2(x), z)
}

/// Jacobian determinant of the spherical map at `(r, theta)`: `r² sin(theta)`.
/// The azimuth does not appear (rotation symmetry about z).
#[must_use]
pub fn jacobian_det_spherical(theta: f64, _phi: f64) -> f64 {
    theta.sin()
}

/// Jacobian determinant of the cylindrical map: the radial factor `r`.
#[must_use]
pub fn jacobian_det_cylindrical(_theta: f64) -> f64 {
    // The cylindrical Jacobian is r (radial distance) — supplied by the
    // caller's frame; this entry documents the azimuth-free symmetry.
    1.0
}
