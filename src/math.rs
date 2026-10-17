//! Math types and helpers.
//!
//! Consists of re-exported `glam` types with some additions, including 2D
//! [`Rect`] / [`Circle`] and the 3D [`Aabb`], [`Sphere`], and [`Ray`].

pub use glam::*;

mod aabb;
mod circle;
mod ray;
mod rect;
mod sphere;

pub use aabb::{Aabb, RayHit};
pub use circle::Circle;
pub use ray::{ClosestHit, Ray};
pub use rect::{Rect, RectOffset};
pub use sphere::Sphere;

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
