//! Math types and helpers.
//!
//! Consists of re-exported `glam` types with some additions, including 2D
//! [`Rect`] / [`Circle`] and the 3D [`Aabb`], [`Obb`], [`Sphere`], [`Capsule`],
//! [`Ray`], [`Plane`], and [`Triangle`]. [`Contact`] is the overlap of two of
//! those shapes. [`slide_sphere`] moves a sphere along [`Aabb`] and [`Obb`]
//! faces. The 3D types are unitless and do not assume an up axis.
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
mod obb;
mod plane;
mod ray;
mod rect;
mod slide;
mod sphere;
mod triangle;

pub use aabb::{Aabb, RayHit};
pub use capsule::Capsule;
pub use circle::Circle;
pub use contact::Contact;
pub use obb::Obb;
pub use plane::{Plane, PlaneSide};
pub use ray::{ClosestHit, Ray};
pub use rect::{Rect, RectOffset};
pub use slide::slide_sphere;
pub use sphere::Sphere;
pub use triangle::Triangle;

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
