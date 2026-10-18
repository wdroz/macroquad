//! Math types and helpers.
//!
//! Consists of re-exported `glam` types with some additions, including 2D
//! [`Rect`] / [`Circle`] and the 3D [`Aabb`], [`Obb`], [`Sphere`], [`Capsule`],
//! [`Ray`], [`Plane`], [`Triangle`], and [`Frustum`]. [`Contact`] is the overlap of two of
//! those shapes. [`SweepHit`] is the time of impact of a moving [`Aabb`]
//! against a static [`Aabb`], or a moving [`Sphere`] against a static [`Aabb`]
//! or [`Obb`]. [`slide_sphere`] moves a sphere along
//! [`Aabb`] and [`Obb`] faces. [`slide_sphere_solids`] does the same and also
//! slides against [`Sphere`] and [`Capsule`] solids. The 3D types are unitless
//! and do not assume an up axis.
//! `docs/3d-guide.md` walks through them.
//!
//! ```
//! use macroquad::math::{Aabb, Ray, vec3, Vec3};
//!
//! let wall = Aabb::from_center_size(Vec3::ZERO, vec3(2.0, 2.0, 2.0));
//! let shot = Ray::new(vec3(0.0, 0.0, 5.0), vec3(0.0, 0.0, -1.0));
//! assert!(shot.closest_aabb(&[wall]).is_some());
//! ```

pub use glam::*;

mod aabb;
mod capsule;
mod circle;
mod contact;
mod frustum;
mod obb;
mod plane;
mod ray;
mod rect;
mod slide;
mod sphere;
mod sweep;
mod triangle;

pub use aabb::{Aabb, RayHit};
pub use capsule::Capsule;
pub use circle::Circle;
pub use contact::Contact;
pub use frustum::Frustum;
pub use obb::Obb;
pub use plane::{Plane, PlaneSide};
pub use ray::{ClosestHit, Ray};
pub use rect::{Rect, RectOffset};
pub use slide::{slide_sphere, slide_sphere_solids};
pub use sphere::Sphere;
pub use sweep::SweepHit;
pub use triangle::Triangle;

/// Roots of `|offset + delta * t| = radius`, in increasing order.
///
/// The dots run in `f64`. In `f32`, `b² - 4ac` collapses to zero once the
/// sphere and the feature are far apart, and the hit lands a radius early.
pub(super) fn sphere_quadratic_roots(offset: Vec3, delta: Vec3, radius: f32) -> Option<(f32, f32)> {
    let ox = offset.x as f64;
    let oy = offset.y as f64;
    let oz = offset.z as f64;
    let dx = delta.x as f64;
    let dy = delta.y as f64;
    let dz = delta.z as f64;
    let a = dx * dx + dy * dy + dz * dz;
    if !a.is_finite() || a <= 1e-18 {
        return None;
    }
    let b = 2.0 * (ox * dx + oy * dy + oz * dz);
    let radius = radius as f64;
    let c = ox * ox + oy * oy + oz * oz - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if !disc.is_finite() || disc < -1e-8 {
        return None;
    }
    let sqrt_disc = disc.max(0.0).sqrt();
    let sign_b = if b >= 0.0 { 1.0 } else { -1.0 };
    let q = -0.5 * (b + sign_b * sqrt_disc);
    let (t_near, t_far) = if q == 0.0 {
        (0.0, -b / a)
    } else {
        let t_q = q / a;
        let t_c = c / q;
        if t_q <= t_c {
            (t_q, t_c)
        } else {
            (t_c, t_q)
        }
    };
    if t_near.is_finite() && t_far.is_finite() {
        Some((t_near as f32, t_far as f32))
    } else {
        None
    }
}

/// Converts 2d polar coordinates to 2d cartesian coordinates.
pub fn polar_to_cartesian(rho: f32, theta: f32) -> Vec2 {
    vec2(rho * theta.cos(), rho * theta.sin())
}

/// Converts 2d cartesian coordinates to 2d polar coordinates.
pub fn cartesian_to_polar(cartesian: Vec2) -> Vec2 {
    vec2(
        (cartesian.x.powi(2) + cartesian.y.powi(2)).sqrt(),
        cartesian.y.atan2(cartesian.x),
    )
}

/// Returns value, bounded in range [min, max].
pub fn clamp<T: PartialOrd>(value: T, min: T, max: T) -> T {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}
