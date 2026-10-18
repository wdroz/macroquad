//! Spheres in 3D.
//!
//! A sphere is a center and a radius in world space. It does not assume a
//! particular up axis, matching [`Aabb`](crate::math::Aabb). [`draw_sphere`](crate::models::draw_sphere)
//! takes the same center and radius.

use glam::{vec3, Vec3};

use super::{Aabb, RayHit};

/// A sphere, defined by its center and radius.
///
/// `radius` should be non-negative and finite. A negative or non-finite radius
/// is empty: it contains nothing, overlaps nothing, and the ray query misses.
/// A radius of zero is a single point at `center`.
///
/// Boundaries are inclusive, matching [`Aabb`] and [`Rect`](crate::math::Rect):
/// a point on the surface is inside, and two spheres that only touch overlap.
/// This differs from [`Circle`](crate::math::Circle), whose tests are strict.
///
/// # Example
///
/// ```
/// use macroquad::math::{Sphere, vec3};
///
/// let ball = Sphere::new(vec3(0.0, 1.0, 0.0), 0.5);
/// assert!(ball.contains(vec3(0.0, 1.0, 0.0)));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sphere {
    /// Center of the sphere.
    pub center: Vec3,
    /// Radius. See the struct docs for negative, non-finite, and zero values.
    pub radius: f32,
}

impl Sphere {
    /// Creates a sphere from its center and radius.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Sphere, Vec3};
    ///
    /// let ball = Sphere::new(Vec3::ZERO, 1.0);
    /// assert_eq!(ball.radius, 1.0);
    /// ```
    pub const fn new(center: Vec3, radius: f32) -> Sphere {
        Sphere { center, radius }
    }

    /// Translates the center by `offset`. The radius is unchanged.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Sphere, Vec3, vec3};
    ///
    /// let moved = Sphere::new(Vec3::ZERO, 1.0).offset(vec3(0.0, 2.0, 0.0));
    /// assert_eq!(moved.center, vec3(0.0, 2.0, 0.0));
    /// assert_eq!(moved.radius, 1.0);
    /// ```
    pub const fn offset(self, offset: Vec3) -> Sphere {
        Sphere {
            center: vec3(
                self.center.x + offset.x,
                self.center.y + offset.y,
                self.center.z + offset.z,
            ),
            radius: self.radius,
        }
    }

    /// Returns true when `point` lies inside the sphere, including the surface.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Sphere, Vec3, vec3};
    ///
    /// let ball = Sphere::new(Vec3::ZERO, 1.0);
    /// assert!(ball.contains(Vec3::X));
    /// assert!(!ball.contains(vec3(1.1, 0.0, 0.0)));
    /// ```
    pub const fn contains(&self, point: Vec3) -> bool {
        if !self.radius.is_finite() || self.radius < 0.0 {
            return false;
        }
        distance_squared(self.center, point) <= self.radius * self.radius
    }

    /// Returns true when the two spheres overlap, including a shared point.
    ///
    /// A negative or non-finite radius is empty and overlaps nothing, even when
    /// the other radius would cover this center.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Sphere, Vec3, vec3};
    ///
    /// let a = Sphere::new(Vec3::ZERO, 1.0);
    /// let b = Sphere::new(vec3(2.0, 0.0, 0.0), 1.0);
    /// assert!(a.overlaps(&b));
    /// ```
    pub const fn overlaps(&self, other: &Sphere) -> bool {
        if !self.radius.is_finite()
            || self.radius < 0.0
            || !other.radius.is_finite()
            || other.radius < 0.0
        {
            return false;
        }
        let reach = self.radius + other.radius;
        distance_squared(self.center, other.center) <= reach * reach
    }

    /// Returns true when this sphere and the box overlap, including a touch.
    ///
    /// Assumes the box is normalized (`min` <= `max` on every axis).
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, Sphere, vec3};
    ///
    /// let ball = Sphere::new(vec3(2.0, 0.5, 0.5), 1.0);
    /// let crate_ = Aabb::from_min_max(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
    /// assert!(ball.overlaps_aabb(&crate_));
    /// ```
    pub fn overlaps_aabb(&self, aabb: &Aabb) -> bool {
        if !self.radius.is_finite() || self.radius < 0.0 {
            return false;
        }
        let closest = aabb.closest_point(self.center);
        distance_squared(self.center, closest) <= self.radius * self.radius
    }

    /// Closest point on or inside the solid sphere to `point`.
    ///
    /// A point already inside is returned unchanged. An empty sphere (negative
    /// or non-finite radius) returns `center`. A non-finite `point` does too.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Sphere, Vec3, vec3};
    ///
    /// let ball = Sphere::new(Vec3::ZERO, 1.0);
    /// assert_eq!(ball.closest_point(vec3(3.0, 0.0, 0.0)), Vec3::X);
    /// ```
    pub fn closest_point(&self, point: Vec3) -> Vec3 {
        if !self.radius.is_finite() || self.radius <= 0.0 || !point.is_finite() {
            return self.center;
        }
        let delta = point - self.center;
        let dist_sq = delta.length_squared();
        if dist_sq <= self.radius * self.radius {
            return point;
        }
        self.center + delta * (self.radius / dist_sq.sqrt())
    }

    /// Ray test against this sphere.
    ///
    /// `direction` does not need to be normalized. Hits with `t < 0` (the
    /// sphere is behind the origin) are rejected. Touching the surface counts
    /// as a hit.
    ///
    /// When the origin is inside the closed sphere, the hit is at `t == 0`
    /// with a zero normal. The same `t == 0` / zero-normal result is returned
    /// when the origin is on the surface and `direction` points outward (or is
    /// zero). An origin on the surface with `direction` pointing inward or
    /// tangent reports `t == 0` and the outward normal.
    ///
    /// A non-finite origin, direction, center, or radius misses. An infinite
    /// radius misses: it is not a usable sphere.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Sphere, Vec3, vec3};
    ///
    /// let ball = Sphere::new(Vec3::ZERO, 1.0);
    /// let hit = ball.ray_intersect(vec3(-2.0, 0.0, 0.0), Vec3::X).unwrap();
    /// assert_eq!(hit.t, 1.0);
    /// assert_eq!(hit.point, -Vec3::X);
    /// ```
    pub fn ray_intersect(&self, origin: Vec3, direction: Vec3) -> Option<RayHit> {
        if !self.radius.is_finite() || self.radius < 0.0 {
            return None;
        }
        if !self.center.is_finite() || !origin.is_finite() || !direction.is_finite() {
            return None;
        }

        let offset = origin - self.center;
        let dist_sq = offset.length_squared();
        let radius_sq = self.radius * self.radius;
        let direction_zero = direction == Vec3::ZERO;

        if dist_sq <= radius_sq {
            let outward = offset.dot(direction) > 0.0;
            let normal = if direction_zero || dist_sq < radius_sq || outward || self.radius == 0.0 {
                Vec3::ZERO
            } else {
                offset.normalize_or_zero()
            };
            return Some(RayHit {
                t: 0.0,
                point: origin,
                normal,
            });
        }

        if direction_zero {
            return None;
        }

        // `a t^2 + b t + c = 0` for the line against the sphere. The origin is
        // outside, so the smaller non-negative root is the entry point.
        let a = direction.length_squared();
        let b = 2.0 * offset.dot(direction);
        let c = dist_sq - radius_sq;
        let discriminant = b * b - 4.0 * a * c;
        if discriminant < 0.0 {
            return None;
        }
        let sqrt_d = discriminant.sqrt();
        let sign_b = if b >= 0.0 { 1.0 } else { -1.0 };
        let q = -0.5 * (b + sign_b * sqrt_d);
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

        let t = if t_near >= 0.0 {
            t_near
        } else if t_far >= 0.0 {
            // Float error put an inside origin on the outside of the quadratic.
            0.0
        } else {
            return None;
        };

        let point = vec3(
            origin.x + direction.x * t,
            origin.y + direction.y * t,
            origin.z + direction.z * t,
        );
        let normal = if t == 0.0 {
            Vec3::ZERO
        } else {
            (point - self.center).normalize_or_zero()
        };
        if !t.is_finite() || !point.is_finite() || !normal.is_finite() {
            return None;
        }
        Some(RayHit { t, point, normal })
    }
}

const fn distance_squared(left: Vec3, right: Vec3) -> f32 {
    let dx = left.x - right.x;
    let dy = left.y - right.y;
    let dz = left.z - right.z;
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::vec3;

    fn assert_vec3_close(left: Vec3, right: Vec3) {
        let delta = left - right;
        assert!(
            delta.x.abs() < 1e-5 && delta.y.abs() < 1e-5 && delta.z.abs() < 1e-5,
            "left {left:?} right {right:?}"
        );
    }

    #[test]
    fn contains_is_inclusive_on_the_surface() {
        let sphere = Sphere::new(vec3(1.0, 2.0, 3.0), 2.0);
        assert!(sphere.contains(vec3(1.0, 2.0, 3.0)));
        assert!(sphere.contains(vec3(3.0, 2.0, 3.0)));
        assert!(sphere.contains(vec3(1.0, 2.0, 5.0)));
        assert!(!sphere.contains(vec3(3.01, 2.0, 3.0)));
        assert!(!Sphere::new(Vec3::ZERO, -1.0).contains(Vec3::ZERO));
        assert!(Sphere::new(Vec3::ZERO, 0.0).contains(Vec3::ZERO));
        assert!(!Sphere::new(Vec3::ZERO, 0.0).contains(vec3(0.01, 0.0, 0.0)));
    }

    #[test]
    fn overlaps_includes_touching_and_containment() {
        let sphere = Sphere::new(Vec3::ZERO, 1.0);
        assert!(sphere.overlaps(&sphere));
        let touching = Sphere::new(vec3(2.0, 0.0, 0.0), 1.0);
        assert!(sphere.overlaps(&touching));
        assert!(touching.overlaps(&sphere));
        let separated = Sphere::new(vec3(2.01, 0.0, 0.0), 1.0);
        assert!(!sphere.overlaps(&separated));
        let inside = Sphere::new(vec3(0.25, 0.0, 0.0), 0.25);
        assert!(sphere.overlaps(&inside));
        assert!(inside.overlaps(&sphere));
        assert!(!sphere.overlaps(&Sphere::new(vec3(0.5, 0.0, 0.0), -2.0)));
        // A negative radius is empty even when the other sphere covers its center.
        assert!(!Sphere::new(Vec3::ZERO, -0.25).overlaps(&Sphere::new(Vec3::ZERO, 10.0)));
        assert!(!Sphere::new(Vec3::ZERO, 10.0).overlaps(&Sphere::new(Vec3::ZERO, -0.25)));

        let moved = sphere.offset(vec3(0.0, 3.0, 0.0));
        assert_vec3_close(moved.center, vec3(0.0, 3.0, 0.0));
        assert_eq!(moved.radius, 1.0);
    }

    #[test]
    fn overlaps_aabb_on_a_face_and_a_corner() {
        let aabb = Aabb::from_min_max(Vec3::ZERO, vec3(2.0, 2.0, 2.0));
        let face = Sphere::new(vec3(3.0, 1.0, 1.0), 1.0);
        assert!(face.overlaps_aabb(&aabb));
        assert!(!Sphere::new(vec3(3.0, 1.0, 1.0), 0.99).overlaps_aabb(&aabb));

        // Closest point is (2, 2, 1); distance squared is 8.
        assert!(Sphere::new(vec3(4.0, 4.0, 1.0), 3.0).overlaps_aabb(&aabb));
        assert!(!Sphere::new(vec3(4.0, 4.0, 1.0), 2.0).overlaps_aabb(&aabb));

        assert!(Sphere::new(vec3(1.0, 1.0, 1.0), 0.1).overlaps_aabb(&aabb));
        assert!(!Sphere::new(vec3(5.0, 1.0, 1.0), -1.0).overlaps_aabb(&aabb));
        assert!(!Sphere::new(Vec3::ZERO, f32::INFINITY).overlaps_aabb(&aabb));
        assert!(!Sphere::new(Vec3::ZERO, f32::NAN).contains(Vec3::ZERO));
        assert!(!Sphere::new(Vec3::ZERO, f32::INFINITY).contains(vec3(100.0, 0.0, 0.0)));
        assert!(!Sphere::new(Vec3::ZERO, f32::INFINITY).overlaps(&Sphere::new(Vec3::X, 1.0)));
        assert!(!Sphere::new(Vec3::ZERO, 1.0).overlaps(&Sphere::new(Vec3::X, f32::NAN)));
    }

    #[test]
    fn closest_point_stays_inside_and_projects_outside() {
        let sphere = Sphere::new(Vec3::ZERO, 2.0);
        assert_vec3_close(
            sphere.closest_point(vec3(1.0, 0.0, 0.0)),
            vec3(1.0, 0.0, 0.0),
        );
        assert_vec3_close(
            sphere.closest_point(vec3(4.0, 0.0, 0.0)),
            vec3(2.0, 0.0, 0.0),
        );
        assert_vec3_close(
            sphere.closest_point(vec3(0.0, 0.0, -4.0)),
            vec3(0.0, 0.0, -2.0),
        );
        assert_eq!(
            Sphere::new(Vec3::X, -1.0).closest_point(Vec3::ZERO),
            Vec3::X
        );
        assert_eq!(
            Sphere::new(Vec3::X, f32::NAN).closest_point(Vec3::ZERO),
            Vec3::X
        );
        assert_eq!(
            Sphere::new(Vec3::X, f32::INFINITY).closest_point(vec3(4.0, 0.0, 0.0)),
            Vec3::X
        );
        assert_eq!(sphere.closest_point(vec3(f32::NAN, 0.0, 0.0)), Vec3::ZERO);
    }

    #[test]
    fn ray_hits_the_surface_with_an_outward_normal() {
        let sphere = Sphere::new(Vec3::ZERO, 1.0);
        let hit = sphere
            .ray_intersect(vec3(-2.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0))
            .unwrap();
        assert!((hit.t - 1.0).abs() < 1e-5);
        assert_vec3_close(hit.point, vec3(-1.0, 0.0, 0.0));
        assert_vec3_close(hit.normal, vec3(-1.0, 0.0, 0.0));

        let scaled = sphere
            .ray_intersect(vec3(-2.0, 0.0, 0.0), vec3(2.0, 0.0, 0.0))
            .unwrap();
        assert!((scaled.t - 0.5).abs() < 1e-5);
        assert_vec3_close(scaled.point, vec3(-1.0, 0.0, 0.0));
        assert_vec3_close(scaled.normal, vec3(-1.0, 0.0, 0.0));
    }

    #[test]
    fn ray_misses_when_separated_or_pointing_away() {
        let sphere = Sphere::new(Vec3::ZERO, 1.0);
        assert!(sphere
            .ray_intersect(vec3(0.0, 2.0, -5.0), vec3(0.0, 0.0, 1.0))
            .is_none());
        assert!(sphere
            .ray_intersect(vec3(2.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0))
            .is_none());
        assert!(sphere
            .ray_intersect(vec3(2.0, 0.0, 0.0), Vec3::ZERO)
            .is_none());
        assert!(Sphere::new(Vec3::ZERO, -1.0)
            .ray_intersect(vec3(-2.0, 0.0, 0.0), Vec3::X)
            .is_none());
        assert!(sphere
            .ray_intersect(vec3(f32::NAN, 0.0, 0.0), Vec3::X)
            .is_none());
        assert!(sphere
            .ray_intersect(Vec3::ZERO, vec3(f32::NAN, 0.0, 0.0))
            .is_none());
        assert!(sphere
            .ray_intersect(vec3(-2.0, 0.0, 0.0), vec3(f32::INFINITY, 0.0, 0.0))
            .is_none());
        assert!(Sphere::new(vec3(f32::NAN, 0.0, 0.0), 1.0)
            .ray_intersect(vec3(-2.0, 0.0, 0.0), Vec3::X)
            .is_none());
        assert!(Sphere::new(Vec3::ZERO, f32::INFINITY)
            .ray_intersect(vec3(-2.0, 0.0, 0.0), Vec3::X)
            .is_none());
    }

    #[test]
    fn ray_inside_and_on_the_surface() {
        let sphere = Sphere::new(Vec3::ZERO, 1.0);
        let inside = sphere.ray_intersect(Vec3::ZERO, Vec3::X).unwrap();
        assert_eq!(inside.t, 0.0);
        assert_eq!(inside.point, Vec3::ZERO);
        assert_eq!(inside.normal, Vec3::ZERO);

        let entering = sphere.ray_intersect(-Vec3::X, Vec3::X).unwrap();
        assert_eq!(entering.t, 0.0);
        assert_vec3_close(entering.point, -Vec3::X);
        assert_vec3_close(entering.normal, -Vec3::X);

        let leaving = sphere.ray_intersect(-Vec3::X, -Vec3::X).unwrap();
        assert_eq!(leaving.t, 0.0);
        assert_eq!(leaving.normal, Vec3::ZERO);

        let tangent_from_surface = sphere.ray_intersect(Vec3::Y, Vec3::X).unwrap();
        assert_eq!(tangent_from_surface.t, 0.0);
        assert_vec3_close(tangent_from_surface.normal, Vec3::Y);

        let stationary = sphere.ray_intersect(Vec3::ZERO, Vec3::ZERO).unwrap();
        assert_eq!(stationary.t, 0.0);
        assert_eq!(stationary.normal, Vec3::ZERO);
    }

    #[test]
    fn ray_grazes_and_hits_a_point_sphere() {
        let sphere = Sphere::new(Vec3::ZERO, 1.0);
        let grazing = sphere
            .ray_intersect(vec3(0.0, 1.0, -5.0), vec3(0.0, 0.0, 1.0))
            .unwrap();
        assert!((grazing.t - 5.0).abs() < 1e-4);
        assert_vec3_close(grazing.point, Vec3::Y);
        assert_vec3_close(grazing.normal, Vec3::Y);

        let point = Sphere::new(Vec3::ZERO, 0.0)
            .ray_intersect(vec3(-2.0, 0.0, 0.0), Vec3::X)
            .unwrap();
        assert!((point.t - 2.0).abs() < 1e-5);
        assert_vec3_close(point.point, Vec3::ZERO);
        assert_eq!(point.normal, Vec3::ZERO);
    }
}
