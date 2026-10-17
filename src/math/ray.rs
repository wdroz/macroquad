//! Rays in 3D.
//!
//! A ray is an origin and a direction in world space. It does not assume a
//! particular up axis, matching [`Aabb`](crate::math::Aabb) and
//! [`Sphere`](crate::math::Sphere).

use glam::{vec3, Vec3};

use super::{Aabb, RayHit, Sphere};

/// Closest intersection of a [`Ray`] with one shape in a slice.
///
/// `hit.t` is never negative. Equal distances keep the earlier index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClosestHit {
    /// Index of the shape in the slice passed to the query.
    pub index: usize,
    /// The intersection. Compare hits from different queries by `hit.t` only
    /// when those queries used the same direction.
    pub hit: RayHit,
}

/// A ray in world space: `point = origin + direction * t`.
///
/// `direction` does not need to be a unit vector. [`Ray::normalized`] returns
/// a copy that is, after which `t` is a distance. A zero direction never
/// advances, so [`Ray::point_at`] stays at the origin for every `t`, and the
/// closest-hit queries only succeed when a shape already contains the origin
/// (those report `t == 0`).
///
/// Hits with `t < 0` are behind the origin and are rejected. A single shape
/// still uses [`Aabb::ray_intersect`] / [`Sphere::ray_intersect`] with
/// `origin` and `direction`; the methods here scan a slice and keep the
/// smallest non-negative `t`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ray {
    /// Start of the ray.
    pub origin: Vec3,
    /// Direction. Not necessarily normalized. See the struct docs.
    pub direction: Vec3,
}

impl Ray {
    /// Creates a ray from its origin and direction.
    pub const fn new(origin: Vec3, direction: Vec3) -> Ray {
        Ray { origin, direction }
    }

    /// Position `origin + direction * t`.
    ///
    /// Negative `t` is behind the origin. This helper does not reject it;
    /// the closest-hit queries do.
    pub const fn point_at(self, t: f32) -> Vec3 {
        vec3(
            self.origin.x + self.direction.x * t,
            self.origin.y + self.direction.y * t,
            self.origin.z + self.direction.z * t,
        )
    }

    /// Copy with a unit direction.
    ///
    /// A zero or non-finite direction becomes [`Vec3::ZERO`] rather than NaN.
    /// The origin is unchanged.
    pub fn normalized(self) -> Ray {
        Ray {
            origin: self.origin,
            direction: self.direction.normalize_or_zero(),
        }
    }

    /// Closest hit against `boxes`, or [`None`] when every box is missed.
    ///
    /// Hits with `t < 0` are rejected. Ties keep the earlier index. Assumes
    /// each box is normalized (`min` <= `max` on every axis).
    pub fn closest_aabb(&self, boxes: &[Aabb]) -> Option<ClosestHit> {
        closest_hit(boxes, |aabb| {
            aabb.ray_intersect(self.origin, self.direction)
        })
    }

    /// Closest hit against `spheres`, or [`None`] when every sphere is missed.
    ///
    /// Hits with `t < 0` are rejected. Ties keep the earlier index.
    pub fn closest_sphere(&self, spheres: &[Sphere]) -> Option<ClosestHit> {
        closest_hit(spheres, |sphere| {
            sphere.ray_intersect(self.origin, self.direction)
        })
    }
}

fn closest_hit<T>(
    shapes: &[T],
    mut intersect: impl FnMut(&T) -> Option<RayHit>,
) -> Option<ClosestHit> {
    let mut best: Option<ClosestHit> = None;
    for (index, shape) in shapes.iter().enumerate() {
        let Some(hit) = intersect(shape) else {
            continue;
        };
        // A non-finite `t` must not win a comparison, and points behind the
        // origin are not hits.
        if !hit.t.is_finite() || hit.t < 0.0 {
            continue;
        }
        let closer = match best {
            Some(current) => hit.t < current.hit.t,
            None => true,
        };
        if closer {
            best = Some(ClosestHit { index, hit });
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec3_close(left: Vec3, right: Vec3) {
        let delta = left - right;
        assert!(
            delta.x.abs() < 1e-5 && delta.y.abs() < 1e-5 && delta.z.abs() < 1e-5,
            "left {left:?} right {right:?}"
        );
    }

    #[test]
    fn point_at_scales_direction_including_behind_the_origin() {
        let ray = Ray::new(vec3(1.0, 2.0, 3.0), vec3(0.0, 0.0, 2.0));
        assert_vec3_close(ray.point_at(0.0), ray.origin);
        assert_vec3_close(ray.point_at(0.5), vec3(1.0, 2.0, 4.0));
        assert_vec3_close(ray.point_at(-1.0), vec3(1.0, 2.0, 1.0));
    }

    #[test]
    fn normalized_is_unit_length_and_keeps_a_zero_direction() {
        let ray = Ray::new(vec3(1.0, 2.0, 3.0), vec3(0.0, 3.0, 4.0)).normalized();
        assert_vec3_close(ray.origin, vec3(1.0, 2.0, 3.0));
        assert_vec3_close(ray.direction, vec3(0.0, 0.6, 0.8));
        assert!((ray.direction.length() - 1.0).abs() < 1e-5);

        let already = Ray::new(Vec3::ZERO, Vec3::X).normalized();
        assert_eq!(already.direction, Vec3::X);

        let zero = Ray::new(Vec3::X, Vec3::ZERO).normalized();
        assert_eq!(zero.origin, Vec3::X);
        assert_eq!(zero.direction, Vec3::ZERO);
    }

    #[test]
    fn closest_aabb_picks_the_nearer_box_and_skips_what_is_behind() {
        let ray = Ray::new(vec3(-4.0, 1.0, 1.0), Vec3::X);
        let boxes = [
            Aabb::from_min_max(vec3(4.0, 0.0, 0.0), vec3(6.0, 2.0, 2.0)),
            Aabb::from_min_max(Vec3::ZERO, vec3(2.0, 2.0, 2.0)),
            Aabb::from_min_max(vec3(-8.0, 0.0, 0.0), vec3(-6.0, 2.0, 2.0)),
        ];
        let closest = ray.closest_aabb(&boxes).unwrap();
        assert_eq!(closest.index, 1);
        assert!((closest.hit.t - 4.0).abs() < 1e-5);
        assert_vec3_close(closest.hit.point, ray.point_at(closest.hit.t));
        assert_vec3_close(closest.hit.normal, vec3(-1.0, 0.0, 0.0));

        let only_behind = [boxes[2]];
        assert!(ray.closest_aabb(&only_behind).is_none());
        assert!(ray.closest_aabb(&[]).is_none());
    }

    #[test]
    fn closest_aabb_tie_keeps_the_earlier_index_and_inside_is_t_zero() {
        let ray = Ray::new(vec3(0.0, 0.0, -5.0), Vec3::Z);
        let tied = [
            Aabb::from_center_size(Vec3::ZERO, Vec3::ONE),
            Aabb::from_center_size(Vec3::ZERO, Vec3::ONE),
        ];
        assert_eq!(ray.closest_aabb(&tied).unwrap().index, 0);

        let from_inside = Ray::new(Vec3::ZERO, Vec3::Z);
        let boxes = [
            Aabb::from_center_size(Vec3::ZERO, vec3(2.0, 2.0, 2.0)),
            Aabb::from_center_size(vec3(0.0, 0.0, 5.0), Vec3::ONE),
        ];
        let inside = from_inside.closest_aabb(&boxes).unwrap();
        assert_eq!(inside.index, 0);
        assert_eq!(inside.hit.t, 0.0);
        assert_eq!(inside.hit.point, Vec3::ZERO);
    }

    #[test]
    fn closest_sphere_matches_a_scaled_direction_and_rejects_a_miss() {
        let spheres = [
            Sphere::new(vec3(0.0, 4.0, 0.0), 0.5),
            Sphere::new(Vec3::ZERO, 1.0),
            Sphere::new(vec3(0.0, 0.0, 3.0), -1.0),
        ];
        let unit = Ray::new(vec3(0.0, 0.0, 5.0), vec3(0.0, 0.0, -1.0));
        let scaled = Ray::new(vec3(0.0, 0.0, 5.0), vec3(0.0, 0.0, -2.0));
        let unit_hit = unit.closest_sphere(&spheres).unwrap();
        let scaled_hit = scaled.closest_sphere(&spheres).unwrap();
        assert_eq!(unit_hit.index, 1);
        assert_eq!(scaled_hit.index, 1);
        assert_vec3_close(unit_hit.hit.point, scaled_hit.hit.point);
        assert_vec3_close(unit_hit.hit.point, vec3(0.0, 0.0, 1.0));
        assert!((unit_hit.hit.t - 4.0).abs() < 1e-4);
        assert!((scaled_hit.hit.t - 2.0).abs() < 1e-4);
        assert_vec3_close(unit_hit.hit.point, unit.point_at(unit_hit.hit.t));

        assert!(Ray::new(vec3(0.0, 0.0, 5.0), Vec3::X)
            .closest_sphere(&spheres)
            .is_none());
        assert!(unit.closest_sphere(&[]).is_none());
    }

    #[test]
    fn non_finite_or_negative_t_is_not_a_hit() {
        let shapes = [()];
        let nan = closest_hit(&shapes, |_| {
            Some(RayHit {
                t: f32::NAN,
                point: Vec3::ZERO,
                normal: Vec3::ZERO,
            })
        });
        assert!(nan.is_none());
        let behind = closest_hit(&shapes, |_| {
            Some(RayHit {
                t: -0.25,
                point: Vec3::ZERO,
                normal: Vec3::ZERO,
            })
        });
        assert!(behind.is_none());
    }

    #[test]
    fn zero_direction_reports_a_shape_that_contains_the_origin() {
        let ray = Ray::new(Vec3::ZERO, Vec3::ZERO);
        let spheres = [
            Sphere::new(vec3(3.0, 0.0, 0.0), 1.0),
            Sphere::new(Vec3::ZERO, 1.0),
        ];
        let hit = ray.closest_sphere(&spheres).unwrap();
        assert_eq!(hit.index, 1);
        assert_eq!(hit.hit.t, 0.0);

        let boxes = [Aabb::from_center_size(vec3(4.0, 0.0, 0.0), Vec3::ONE)];
        assert!(ray.closest_aabb(&boxes).is_none());
    }
}
