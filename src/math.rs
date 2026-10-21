//! Math types and helpers.
//!
//! Consists of re-exported `glam` types with some additions, including 2D
//! [`Rect`] / [`Circle`] and the 3D [`Aabb`], [`Obb`], [`Sphere`], [`Capsule`],
//! [`Ray`], [`Plane`], [`Triangle`], and [`Frustum`]. [`Contact`] is the overlap of two of
//! those shapes. [`SweepHit`] is the time of impact of a moving [`Aabb`]
//! against a static [`Aabb`], [`Obb`], [`Sphere`] or [`Capsule`], or a moving [`Sphere`]
//! against either box. [`slide_sphere`] moves a sphere along
//! [`Aabb`] and [`Obb`] faces. [`slide_sphere_solids`] does the same and also
//! slides against [`Sphere`] and [`Capsule`] solids. [`slide_spheres_solids`]
//! returns one shared translation for a rigid group of equal-radius spheres.
//! [`Heightfield`] samples regular terrain in a caller-defined up frame.
//! [`TriangleMesh`] caches triangle bounds for indexed ray hits and surface overlaps.
//! [`UniformGrid`] indexes caller-owned world bounds for overlap and ray candidates.
//! [`RigidBody`] integrates caller-owned velocity and position with gravity and
//! damping, with local [`RigidBodyShape`] geometry. Its explicit
//! [`RigidBody::resolve_collision`] and [`RigidBody::resolve_static_obb`] methods
//! add one checked discrete separation/impulse step.
//! The 3D types are unitless
//! and do not assume an up axis.
//! [`CharacterController`] moves a caller-owned AABB with sliding, ground
//! snapping and slope limits, using an explicit caller-supplied up direction.
//! [`CharacterController::move_body_with_step`] adds optional stair stepping
//! and diagnostics without requiring changes to ordinary movement calls.
//! `docs/3d-guide.md` walks through them.
//!
//! Choose a query by what the caller needs:
//!
//! - Overlap and [`Contact`] queries describe the current pose, including a touch.
//! - [`Aabb::sweep`], [`Aabb::sweep_obb`], [`Aabb::sweep_sphere`], [`Aabb::sweep_capsule`],
//!   [`Sphere::sweep_aabb`] and [`Sphere::sweep_obb`] find the first impact along
//!   a full movement delta; [`SweepHit::t`] is a fraction of that delta.
//! - [`slide_sphere_solids`] returns a resolved center against static solids.
//!   The caller owns velocity, gravity, ground state, and trigger detection.
//! - [`slide_spheres_solids`] returns a shared translation for several sphere
//!   centers, keeping their separation fixed while sliding the entire group.
//! - [`CharacterController::move_body`] returns the moved AABB, clipped velocity
//!   and final ground state against explicitly selected [`CharacterSolids`].
//! - [`Frustum`] tests visibility bounds; its intersection tests are conservative
//!   culling queries and do not replace collision tests.
//! - [`UniformGrid`] returns unique caller indices whose stored bounds overlap
//!   a box or ray segment; run shape queries on those broadphase candidates.
//!
//! ```
//! use macroquad::math::{Aabb, Ray, vec3, Vec3};
//!
//! let wall = Aabb::from_center_size(Vec3::ZERO, vec3(2.0, 2.0, 2.0));
//! let shot = Ray::new(vec3(0.0, 0.0, 5.0), vec3(0.0, 0.0, -1.0));
//! assert!(shot.closest_aabb(&[wall]).is_some());
//! ```
//!
//! # Translating a complete box
//!
//! All four AABB sweeps use a world-space displacement, for example
//! `velocity * dt`, rather than velocity or a ray direction. They keep the
//! moving box axis-aligned and leave both arguments unchanged. Choose the
//! method by the **static** solid's shape:
//!
//! | Static solid | Moving [`Aabb`] query |
//! | --- | --- |
//! | Axis-aligned box | [`Aabb::sweep`] |
//! | Oriented box | [`Aabb::sweep_obb`] |
//! | Sphere | [`Aabb::sweep_sphere`] |
//! | Capsule | [`Aabb::sweep_capsule`] |
//!
//! These are single-pair queries, not a character controller. Test solid
//! candidates with the same starting body and delta, select the earliest
//! hit, and apply `delta * hit.t`. [`SweepHit::point`] is a contact position,
//! not the moving body's new center. Keep trigger overlap checks separate.
//! [`CharacterController`] adds repeated sliding, bounded overlap recovery,
//! grounding and slope limits over these queries, with opt-in stair stepping
//! through [`CharacterController::move_body_with_step`]. Gravity remains the
//! caller's responsibility.
//!
//! ```
//! use macroquad::math::{Aabb, Capsule, Obb, Quat, Sphere, Vec3, vec3};
//!
//! let body = Aabb::from_center_size(vec3(-3.0, 0.0, 0.0), Vec3::ONE);
//! let delta = vec3(6.0, 0.0, 0.0);
//! let wall = Obb::new(vec3(2.0, 0.0, 0.0), Vec3::ONE, Quat::IDENTITY);
//! let rock = Sphere::new(Vec3::ZERO, 1.0);
//! let post = Capsule::new(vec3(0.0, -2.0, 4.0), vec3(0.0, 2.0, 4.0), 0.5);
//! let first = [
//!     body.sweep_obb(delta, &wall),
//!     body.sweep_sphere(delta, &rock),
//!     body.sweep_capsule(delta, &post),
//! ].into_iter().flatten().min_by(|a, b| a.t.total_cmp(&b.t));
//! let displacement = delta * first.map_or(1.0, |hit| hit.t);
//! let stopped = Aabb::from_min_max(body.min + displacement, body.max + displacement);
//! assert_eq!(stopped.center(), vec3(-1.5, 0.0, 0.0));
//! ```

pub use glam::*;

mod aabb;
mod capsule;
mod character;
mod circle;
mod contact;
mod frustum;
mod heightfield;
mod obb;
mod plane;
mod ray;
mod rect;
mod rigid_body;
mod slide;
mod sphere;
mod sweep;
mod sweep_capsule;
mod sweep_obb;
mod triangle;
mod triangle_mesh;
mod uniform_grid;

pub use aabb::{Aabb, RayHit};
pub use capsule::Capsule;
pub use character::{
    CharacterController, CharacterMotion, CharacterSolids, CharacterStepDiagnostics,
    CharacterStepMotion, CharacterStepStatus,
};
pub use circle::Circle;
pub use contact::Contact;
pub use frustum::Frustum;
pub use heightfield::Heightfield;
pub use obb::Obb;
pub use plane::{Plane, PlaneSide};
pub use ray::{ClosestHit, Ray};
pub use rect::{Rect, RectOffset};
pub use rigid_body::{
    CollisionResponseError, RigidBody, RigidBodyError, RigidBodyMode, RigidBodyShape,
};
pub use slide::{slide_sphere, slide_sphere_solids, slide_spheres_solids};
pub use sphere::Sphere;
pub use sweep::SweepHit;
pub use triangle::Triangle;
pub use triangle_mesh::TriangleMesh;
pub use uniform_grid::UniformGrid;

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
