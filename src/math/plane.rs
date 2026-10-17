//! Planes in 3D.
//!
//! A plane is a unit normal and a signed distance from the origin. It does not
//! assume a particular up axis, matching [`Aabb`](crate::math::Aabb) and
//! [`Sphere`](crate::math::Sphere).

use glam::{vec3, Vec3};

use super::RayHit;

/// Which side of a [`Plane`] a point is on.
///
/// [`PlaneSide::Front`] is the half-space the normal points into
/// (`signed_distance > 0`). [`PlaneSide::Coplanar`] is the plane itself.
///
/// # Example
///
/// ```
/// use macroquad::math::{Plane, PlaneSide, Vec3, vec3};
///
/// let ground = Plane::new(Vec3::Y, 0.0);
/// assert_eq!(ground.side(vec3(0.0, 1.0, 0.0)), PlaneSide::Front);
/// assert_eq!(ground.side(Vec3::ZERO), PlaneSide::Coplanar);
/// assert_eq!(ground.side(vec3(0.0, -1.0, 0.0)), PlaneSide::Back);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneSide {
    /// `signed_distance > 0`. The normal points into this half-space.
    Front,
    /// `signed_distance < 0`.
    Back,
    /// `signed_distance == 0`. The point lies on the plane.
    Coplanar,
}

/// An infinite plane, `normal · x = distance`.
///
/// `normal` is a unit vector. `distance` is the signed distance from the
/// origin to the plane along `normal`, so `normal * distance` is the point on
/// the plane closest to the origin. A positive [`Plane::signed_distance`] is
/// on the side `normal` points toward.
///
/// [`Plane::new`] stores the values as given and does not normalize. The other
/// constructors produce a unit normal. Distance, side, closest-point, ray, and
/// segment queries assume that unit length: a non-unit normal scales those
/// distances and leaves the ray parameter `t` wrong.
///
/// # Example
///
/// ```
/// use macroquad::math::{Plane, Vec3, vec3};
///
/// let ground = Plane::new(Vec3::Y, 0.0);
/// assert_eq!(ground.signed_distance(vec3(3.0, 2.0, -4.0)), 2.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    /// Unit normal. Query methods assume length 1. See the struct docs.
    pub normal: Vec3,
    /// Signed distance from the origin along `normal`.
    pub distance: f32,
}

impl Plane {
    /// Creates a plane from a unit normal and the signed distance from the origin.
    ///
    /// The values are stored as given. Pass a unit `normal`; this constructor
    /// does not normalize it.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Plane, Vec3};
    ///
    /// let ground = Plane::new(Vec3::Y, 1.5);
    /// assert_eq!(ground.distance, 1.5);
    /// ```
    pub const fn new(normal: Vec3, distance: f32) -> Plane {
        Plane { normal, distance }
    }

    /// Plane through `point` with the given normal.
    ///
    /// `normal` is normalized. Returns [`None`] when `normal` is zero-length or
    /// non-finite, or when `point` is non-finite. The origin is a valid point.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Plane, Vec3, vec3};
    ///
    /// let plane = Plane::from_normal_point(Vec3::Y, vec3(0.0, 3.0, 0.0)).unwrap();
    /// assert_eq!(plane.distance, 3.0);
    /// assert!(Plane::from_normal_point(Vec3::ZERO, Vec3::ZERO).is_none());
    /// ```
    pub fn from_normal_point(normal: Vec3, point: Vec3) -> Option<Plane> {
        let normal = unit_normal(normal)?;
        let distance = dot(normal, point);
        if !distance.is_finite() {
            return None;
        }
        Some(Plane { normal, distance })
    }

    /// Plane through three points.
    ///
    /// The normal is `(b - a) × (c - a)`, normalized, so it follows the
    /// right-hand rule from `a → b` toward `a → c`. Returns [`None`] when the
    /// points are collinear, coincident, or non-finite.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Plane, Vec3};
    ///
    /// let plane = Plane::from_points(Vec3::ZERO, Vec3::X, Vec3::Z).unwrap();
    /// assert_eq!(plane.normal, -Vec3::Y);
    /// ```
    pub fn from_points(a: Vec3, b: Vec3, c: Vec3) -> Option<Plane> {
        let normal = unit_normal(cross(b - a, c - a))?;
        let distance = dot(normal, a);
        if !distance.is_finite() {
            return None;
        }
        Some(Plane { normal, distance })
    }

    /// Signed distance from this plane to `point`.
    ///
    /// Positive on the side `normal` points into, zero on the plane, negative
    /// on the other side. Assumes `normal` is a unit vector.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Plane, Vec3, vec3};
    ///
    /// let ground = Plane::new(Vec3::Y, 0.0);
    /// assert_eq!(ground.signed_distance(vec3(1.0, -2.0, 0.0)), -2.0);
    /// ```
    pub const fn signed_distance(&self, point: Vec3) -> f32 {
        dot(self.normal, point) - self.distance
    }

    /// Which side of the plane `point` is on.
    ///
    /// A point with signed distance zero is [`PlaneSide::Coplanar`], including
    /// the boundary, matching the inclusive tests on [`Aabb`](crate::math::Aabb)
    /// and [`Sphere`](crate::math::Sphere). Assumes `normal` is a unit vector.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Plane, PlaneSide, Vec3};
    ///
    /// let ground = Plane::new(Vec3::Y, 0.0);
    /// assert_eq!(ground.side(Vec3::Y), PlaneSide::Front);
    /// ```
    pub fn side(&self, point: Vec3) -> PlaneSide {
        let distance = self.signed_distance(point);
        if distance > 0.0 {
            PlaneSide::Front
        } else if distance < 0.0 {
            PlaneSide::Back
        } else {
            PlaneSide::Coplanar
        }
    }

    /// Closest point on the plane to `point`.
    ///
    /// A point already on the plane is returned unchanged. Assumes `normal` is
    /// a unit vector.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Plane, Vec3, vec3};
    ///
    /// let ground = Plane::new(Vec3::Y, 0.0);
    /// assert_eq!(ground.closest_point(vec3(2.0, 5.0, -1.0)), vec3(2.0, 0.0, -1.0));
    /// ```
    pub fn closest_point(&self, point: Vec3) -> Vec3 {
        let distance = self.signed_distance(point);
        vec3(
            point.x - self.normal.x * distance,
            point.y - self.normal.y * distance,
            point.z - self.normal.z * distance,
        )
    }

    /// Ray test against this plane.
    ///
    /// `direction` does not need to be normalized. Hits with `t < 0` (the
    /// intersection is behind the origin) are rejected. The reported normal is
    /// this plane's normal and is not flipped to face the ray.
    ///
    /// When the origin already lies on the plane, the hit is at `t == 0` with
    /// that same normal, including a zero or parallel direction. A ray parallel
    /// to the plane misses when the origin is off the plane. Assumes `normal`
    /// is a unit vector. A non-finite origin, direction, normal, or distance
    /// misses, and so does a zero normal: `Plane::new(Vec3::ZERO, 0.0)` would
    /// otherwise report every origin as a hit.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Plane, Vec3, vec3};
    ///
    /// let ground = Plane::new(Vec3::Y, 0.0);
    /// let hit = ground
    ///     .ray_intersect(vec3(1.0, 4.0, 2.0), -Vec3::Y)
    ///     .unwrap();
    /// assert_eq!(hit.t, 4.0);
    /// assert_eq!(hit.point, vec3(1.0, 0.0, 2.0));
    /// assert_eq!(hit.normal, Vec3::Y);
    /// ```
    pub fn ray_intersect(&self, origin: Vec3, direction: Vec3) -> Option<RayHit> {
        if !origin.is_finite()
            || !direction.is_finite()
            || !self.normal.is_finite()
            || !self.distance.is_finite()
            || dot(self.normal, self.normal) == 0.0
        {
            return None;
        }

        let origin_distance = self.signed_distance(origin);
        if !origin_distance.is_finite() {
            return None;
        }
        if origin_distance == 0.0 {
            return Some(RayHit {
                t: 0.0,
                point: origin,
                normal: self.normal,
            });
        }

        let denom = dot(self.normal, direction);
        if denom == 0.0 {
            return None;
        }

        let t = -origin_distance / denom;
        if !t.is_finite() || t < 0.0 {
            return None;
        }

        let point = vec3(
            origin.x + direction.x * t,
            origin.y + direction.y * t,
            origin.z + direction.z * t,
        );
        if !point.is_finite() {
            return None;
        }
        Some(RayHit {
            t,
            point,
            normal: self.normal,
        })
    }

    /// Intersection of the closed segment from `start` to `end`.
    ///
    /// Returns the point where the segment crosses or touches the plane.
    /// Endpoints on opposite sides count, and so does an endpoint that lies on
    /// the plane. When the whole segment lies on the plane, the result is
    /// `start`. A segment that stays strictly on one side returns [`None`].
    /// Assumes `normal` is a unit vector. A zero or non-finite normal misses,
    /// matching [`Plane::ray_intersect`].
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Plane, Vec3, vec3};
    ///
    /// let ground = Plane::new(Vec3::Y, 0.0);
    /// let point = ground
    ///     .segment_intersect(vec3(0.0, 2.0, 0.0), vec3(0.0, -2.0, 0.0))
    ///     .unwrap();
    /// assert_eq!(point, Vec3::ZERO);
    /// assert!(ground
    ///     .segment_intersect(vec3(0.0, 1.0, 0.0), vec3(0.0, 2.0, 0.0))
    ///     .is_none());
    /// ```
    pub fn segment_intersect(&self, start: Vec3, end: Vec3) -> Option<Vec3> {
        if !start.is_finite()
            || !end.is_finite()
            || !self.normal.is_finite()
            || !self.distance.is_finite()
            || dot(self.normal, self.normal) == 0.0
        {
            return None;
        }

        let start_distance = self.signed_distance(start);
        let end_distance = self.signed_distance(end);
        if !start_distance.is_finite() || !end_distance.is_finite() {
            return None;
        }

        // Strictly the same side, neither endpoint on the plane.
        if start_distance * end_distance > 0.0 {
            return None;
        }

        // Both endpoints on the plane, so the whole segment lies in it.
        if start_distance == end_distance {
            return Some(start);
        }

        let t = start_distance / (start_distance - end_distance);
        if !t.is_finite() {
            return None;
        }
        let t = t.clamp(0.0, 1.0);
        Some(vec3(
            start.x + (end.x - start.x) * t,
            start.y + (end.y - start.y) * t,
            start.z + (end.z - start.z) * t,
        ))
    }
}

const fn dot(left: Vec3, right: Vec3) -> f32 {
    left.x * right.x + left.y * right.y + left.z * right.z
}

const fn cross(left: Vec3, right: Vec3) -> Vec3 {
    vec3(
        left.y * right.z - left.z * right.y,
        left.z * right.x - left.x * right.z,
        left.x * right.y - left.y * right.x,
    )
}

fn unit_normal(normal: Vec3) -> Option<Vec3> {
    if !normal.is_finite() {
        return None;
    }
    let len_sq = dot(normal, normal);
    if !len_sq.is_finite() || len_sq == 0.0 {
        return None;
    }
    let len = len_sq.sqrt();
    let unit = vec3(normal.x / len, normal.y / len, normal.z / len);
    if unit.is_finite() {
        Some(unit)
    } else {
        None
    }
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

    fn xy_plane() -> Plane {
        Plane::new(Vec3::Z, 0.0)
    }

    #[test]
    fn new_stores_the_unit_normal_and_signed_distance() {
        let plane = Plane::new(Vec3::Y, 3.0);
        assert_eq!(plane.normal, Vec3::Y);
        assert_eq!(plane.distance, 3.0);
        assert_eq!(plane.signed_distance(Vec3::ZERO), -3.0);
        assert_eq!(plane.signed_distance(vec3(4.0, 3.0, -2.0)), 0.0);
        assert_eq!(plane.signed_distance(vec3(0.0, 5.0, 0.0)), 2.0);
    }

    #[test]
    fn side_matches_the_sign_of_the_distance_and_includes_the_plane() {
        let plane = Plane::new(Vec3::Y, 1.0);
        assert_eq!(plane.side(vec3(2.0, 2.0, -4.0)), PlaneSide::Front);
        assert_eq!(plane.side(vec3(2.0, 0.0, -4.0)), PlaneSide::Back);
        assert_eq!(plane.side(vec3(2.0, 1.0, -4.0)), PlaneSide::Coplanar);
        assert_eq!(
            plane.side(plane.normal * plane.distance),
            PlaneSide::Coplanar
        );
    }

    #[test]
    fn closest_point_projects_onto_the_plane() {
        let plane = Plane::new(Vec3::Z, 2.0);
        assert_vec3_close(
            plane.closest_point(vec3(3.0, -4.0, 9.0)),
            vec3(3.0, -4.0, 2.0),
        );
        assert_vec3_close(
            plane.closest_point(vec3(3.0, -4.0, -1.0)),
            vec3(3.0, -4.0, 2.0),
        );
        let on_plane = vec3(3.0, -4.0, 2.0);
        assert_eq!(plane.closest_point(on_plane), on_plane);
        assert_eq!(
            plane.signed_distance(plane.closest_point(vec3(1.0, 2.0, 8.0))),
            0.0
        );
    }

    #[test]
    fn from_normal_point_normalizes_and_rejects_a_zero_normal() {
        let plane = Plane::from_normal_point(vec3(0.0, 2.0, 0.0), vec3(1.0, 3.0, 4.0)).unwrap();
        assert_vec3_close(plane.normal, Vec3::Y);
        assert!((plane.distance - 3.0).abs() < 1e-5);
        assert_eq!(plane.side(vec3(1.0, 3.0, 4.0)), PlaneSide::Coplanar);
        assert_eq!(plane.side(vec3(0.0, 4.0, 0.0)), PlaneSide::Front);

        assert!(Plane::from_normal_point(Vec3::ZERO, Vec3::X).is_none());
        assert!(Plane::from_normal_point(vec3(f32::NAN, 0.0, 0.0), Vec3::ZERO).is_none());
        assert!(Plane::from_normal_point(Vec3::X, vec3(f32::INFINITY, 0.0, 0.0)).is_none());

        let through_origin = Plane::from_normal_point(Vec3::Y, Vec3::ZERO).unwrap();
        assert_vec3_close(through_origin.normal, Vec3::Y);
        assert!(through_origin.distance.abs() < 1e-5);
        assert_eq!(through_origin.side(Vec3::ZERO), PlaneSide::Coplanar);
    }

    #[test]
    fn from_points_follows_the_right_hand_rule_and_rejects_collinear_input() {
        let xy = Plane::from_points(Vec3::ZERO, Vec3::X, Vec3::Y).unwrap();
        assert_vec3_close(xy.normal, Vec3::Z);
        assert!(xy.distance.abs() < 1e-5);
        assert_eq!(xy.signed_distance(Vec3::ZERO), 0.0);
        assert_eq!(xy.signed_distance(Vec3::X), 0.0);
        assert_eq!(xy.signed_distance(Vec3::Y), 0.0);

        let flipped = Plane::from_points(Vec3::ZERO, Vec3::Y, Vec3::X).unwrap();
        assert_vec3_close(flipped.normal, -Vec3::Z);

        let lifted = Plane::from_points(
            vec3(0.0, 0.0, 2.0),
            vec3(1.0, 0.0, 2.0),
            vec3(0.0, 1.0, 2.0),
        )
        .unwrap();
        assert_vec3_close(lifted.normal, Vec3::Z);
        assert!((lifted.distance - 2.0).abs() < 1e-5);

        assert!(Plane::from_points(Vec3::ZERO, Vec3::X, vec3(2.0, 0.0, 0.0)).is_none());
        assert!(Plane::from_points(Vec3::X, Vec3::X, Vec3::Y).is_none());
        assert!(Plane::from_points(vec3(f32::NAN, 0.0, 0.0), Vec3::X, Vec3::Y).is_none());
    }

    #[test]
    fn ray_intersect_hits_in_front_and_rejects_what_is_behind_or_parallel() {
        let plane = xy_plane();
        let toward = plane
            .ray_intersect(vec3(1.0, 2.0, 5.0), vec3(0.0, 0.0, -1.0))
            .unwrap();
        assert!((toward.t - 5.0).abs() < 1e-5);
        assert_vec3_close(toward.point, vec3(1.0, 2.0, 0.0));
        assert_eq!(toward.normal, Vec3::Z);

        let scaled = plane
            .ray_intersect(vec3(1.0, 2.0, 5.0), vec3(0.0, 0.0, -2.0))
            .unwrap();
        assert!((scaled.t - 2.5).abs() < 1e-5);
        assert_vec3_close(scaled.point, toward.point);
        assert_eq!(scaled.normal, Vec3::Z);

        assert!(plane.ray_intersect(vec3(0.0, 0.0, 5.0), Vec3::Z).is_none());
        assert!(plane.ray_intersect(vec3(0.0, 0.0, 1.0), Vec3::X).is_none());
        assert!(plane
            .ray_intersect(vec3(0.0, 0.0, 1.0), Vec3::ZERO)
            .is_none());
        assert!(plane
            .ray_intersect(vec3(0.0, 0.0, 1.0), vec3(f32::NAN, 0.0, 0.0))
            .is_none());
    }

    #[test]
    fn ray_from_the_back_keeps_the_plane_normal_and_an_origin_on_the_plane_is_t_zero() {
        let plane = Plane::new(Vec3::Y, 2.0);
        let from_back = plane.ray_intersect(vec3(1.0, -1.0, 4.0), Vec3::Y).unwrap();
        assert!((from_back.t - 3.0).abs() < 1e-5);
        assert_vec3_close(from_back.point, vec3(1.0, 2.0, 4.0));
        assert_eq!(from_back.normal, Vec3::Y);

        let on_plane = plane.ray_intersect(vec3(1.0, 2.0, 4.0), Vec3::X).unwrap();
        assert_eq!(on_plane.t, 0.0);
        assert_eq!(on_plane.point, vec3(1.0, 2.0, 4.0));
        assert_eq!(on_plane.normal, Vec3::Y);

        let zero_dir = plane
            .ray_intersect(vec3(1.0, 2.0, 4.0), Vec3::ZERO)
            .unwrap();
        assert_eq!(zero_dir.t, 0.0);
        assert_eq!(zero_dir.normal, Vec3::Y);
    }

    #[test]
    fn segment_intersect_crosses_opposite_sides_and_includes_an_endpoint_on_the_plane() {
        let plane = xy_plane();
        assert_vec3_close(
            plane
                .segment_intersect(vec3(0.0, 0.0, 1.0), vec3(0.0, 0.0, -1.0))
                .unwrap(),
            Vec3::ZERO,
        );
        assert_vec3_close(
            plane
                .segment_intersect(vec3(0.0, 0.0, 4.0), vec3(0.0, 3.0, -2.0))
                .unwrap(),
            vec3(0.0, 2.0, 0.0),
        );
        assert!(plane
            .segment_intersect(vec3(0.0, 0.0, 1.0), vec3(1.0, 0.0, 2.0))
            .is_none());
        assert!(plane
            .segment_intersect(vec3(0.0, 0.0, 1.0), vec3(1.0, 0.0, 1.0))
            .is_none());

        assert_eq!(
            plane
                .segment_intersect(vec3(2.0, 3.0, 0.0), vec3(2.0, 3.0, 1.0))
                .unwrap(),
            vec3(2.0, 3.0, 0.0)
        );
        assert_eq!(
            plane
                .segment_intersect(vec3(2.0, 3.0, 1.0), vec3(2.0, 3.0, 0.0))
                .unwrap(),
            vec3(2.0, 3.0, 0.0)
        );
        assert_eq!(
            plane
                .segment_intersect(vec3(1.0, 0.0, 0.0), vec3(4.0, 5.0, 0.0))
                .unwrap(),
            vec3(1.0, 0.0, 0.0)
        );
    }

    #[test]
    fn segment_on_an_offset_plane_and_non_finite_input_are_rejected_or_reported() {
        let plane = Plane::from_normal_point(Vec3::Z, vec3(0.0, 0.0, 2.0)).unwrap();
        assert_vec3_close(
            plane
                .segment_intersect(vec3(1.0, 1.0, 0.0), vec3(-1.0, 3.0, 4.0))
                .unwrap(),
            vec3(0.0, 2.0, 2.0),
        );
        assert!(plane
            .segment_intersect(vec3(0.0, 0.0, 3.0), vec3(1.0, 1.0, 5.0))
            .is_none());
        assert!(plane
            .segment_intersect(vec3(f32::NAN, 0.0, 0.0), Vec3::ZERO)
            .is_none());
        assert!(xy_plane()
            .ray_intersect(vec3(f32::INFINITY, 0.0, 0.0), Vec3::Z)
            .is_none());
        // On the plane, a non-finite direction is still not a ray.
        assert!(xy_plane()
            .ray_intersect(Vec3::ZERO, vec3(f32::NAN, 0.0, 0.0))
            .is_none());
        let degenerate = Plane::new(Vec3::ZERO, 0.0);
        assert!(degenerate.ray_intersect(Vec3::X, Vec3::Y).is_none());
        assert!(degenerate.segment_intersect(Vec3::ZERO, Vec3::X).is_none());
    }
}
