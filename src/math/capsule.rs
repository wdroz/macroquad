//! Capsules in 3D.
//!
//! A capsule is a line segment swept by a radius. It does not assume an up
//! axis, matching [`Sphere`](crate::math::Sphere) and [`Aabb`](crate::math::Aabb).

use glam::{vec3, Vec3};

use super::{Aabb, RayHit, Sphere};

/// A capsule: every point within `radius` of the segment from `a` to `b`.
///
/// `a` and `b` are the endpoints of the inner segment, not of the rounded
/// hull. The caps extend `radius` past those endpoints. Swapping `a` and `b`
/// does not change the shape. A character body is this segment standing along
/// whatever direction the caller treats as up.
///
/// `radius` should be non-negative and finite. A negative or non-finite radius
/// is empty: it contains nothing, overlaps nothing, and the ray query misses.
/// A radius of zero is the segment itself (a single point when `a == b`).
///
/// Boundaries are inclusive, matching [`Sphere`] and [`Aabb`]: a point on the
/// surface is inside, and two capsules that only touch overlap.
///
/// # Example
///
/// ```
/// use macroquad::math::{Capsule, vec3};
///
/// let body = Capsule::new(vec3(0.0, 0.5, 0.0), vec3(0.0, 1.5, 0.0), 0.5);
/// assert!(body.contains(vec3(0.0, 0.0, 0.0)));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Capsule {
    /// One endpoint of the inner segment.
    pub a: Vec3,
    /// The other endpoint of the inner segment.
    pub b: Vec3,
    /// Radius around the segment. See the struct docs for negative, non-finite, and zero.
    pub radius: f32,
}

impl Capsule {
    /// Creates a capsule from the inner segment and a radius.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Capsule, Vec3};
    ///
    /// let body = Capsule::new(Vec3::ZERO, Vec3::Y, 0.25);
    /// assert_eq!(body.radius, 0.25);
    /// ```
    pub const fn new(a: Vec3, b: Vec3, radius: f32) -> Capsule {
        Capsule { a, b, radius }
    }

    /// Translates both endpoints by `offset`. The radius is unchanged.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Capsule, Vec3, vec3};
    ///
    /// let moved = Capsule::new(Vec3::ZERO, Vec3::Y, 0.5).offset(vec3(2.0, 0.0, 0.0));
    /// assert_eq!(moved.a, vec3(2.0, 0.0, 0.0));
    /// assert_eq!(moved.b, vec3(2.0, 1.0, 0.0));
    /// ```
    pub const fn offset(self, offset: Vec3) -> Capsule {
        Capsule {
            a: vec3(
                self.a.x + offset.x,
                self.a.y + offset.y,
                self.a.z + offset.z,
            ),
            b: vec3(
                self.b.x + offset.x,
                self.b.y + offset.y,
                self.b.z + offset.z,
            ),
            radius: self.radius,
        }
    }

    /// Returns true when `point` lies inside the capsule, including the surface.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Capsule, Vec3, vec3};
    ///
    /// let body = Capsule::new(Vec3::ZERO, vec3(0.0, 2.0, 0.0), 1.0);
    /// assert!(body.contains(vec3(0.0, 1.0, 1.0)));
    /// assert!(!body.contains(vec3(0.0, 1.0, 1.2)));
    /// ```
    pub const fn contains(&self, point: Vec3) -> bool {
        if !self.radius.is_finite() || self.radius < 0.0 {
            return false;
        }
        point_segment_distance_sq(point, self.a, self.b) <= self.radius * self.radius
    }

    /// Returns true when the two capsules overlap, including a shared point.
    ///
    /// A negative or non-finite radius is empty and overlaps nothing, even when
    /// the other capsule would cover this segment.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Capsule, vec3};
    ///
    /// let left = Capsule::new(vec3(0.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), 0.5);
    /// let right = Capsule::new(vec3(1.0, 0.0, 0.0), vec3(1.0, 1.0, 0.0), 0.5);
    /// assert!(left.overlaps(&right));
    /// ```
    pub fn overlaps(&self, other: &Capsule) -> bool {
        if !self.radius.is_finite()
            || self.radius < 0.0
            || !other.radius.is_finite()
            || other.radius < 0.0
        {
            return false;
        }
        let reach = self.radius + other.radius;
        segment_segment_distance_sq(self.a, self.b, other.a, other.b) <= reach * reach
    }

    /// Returns true when this capsule and `sphere` overlap, including a touch.
    ///
    /// A negative or non-finite radius on either shape is empty and overlaps nothing.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Capsule, Sphere, vec3};
    ///
    /// let body = Capsule::new(vec3(0.0, 0.0, 0.0), vec3(0.0, 2.0, 0.0), 0.5);
    /// let ball = Sphere::new(vec3(1.0, 1.0, 0.0), 0.5);
    /// assert!(body.overlaps_sphere(&ball));
    /// ```
    pub fn overlaps_sphere(&self, sphere: &Sphere) -> bool {
        if !self.radius.is_finite()
            || self.radius < 0.0
            || !sphere.radius.is_finite()
            || sphere.radius < 0.0
        {
            return false;
        }
        let reach = self.radius + sphere.radius;
        point_segment_distance_sq(sphere.center, self.a, self.b) <= reach * reach
    }

    /// Returns true when this capsule and `aabb` overlap, including a touch.
    ///
    /// Assumes the box is normalized (`min` <= `max` on every axis). An
    /// inverted box overlaps nothing. A non-finite corner or endpoint misses.
    /// A negative or non-finite capsule radius is empty and overlaps nothing.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, Capsule, vec3};
    ///
    /// let body = Capsule::new(vec3(0.0, 0.5, 0.0), vec3(0.0, 1.5, 0.0), 0.5);
    /// let floor = Aabb::from_min_max(vec3(-1.0, -0.2, -1.0), vec3(1.0, 0.0, 1.0));
    /// assert!(body.overlaps_aabb(&floor));
    /// ```
    pub fn overlaps_aabb(&self, aabb: &Aabb) -> bool {
        if !self.radius.is_finite() || self.radius < 0.0 {
            return false;
        }
        if !self.a.is_finite()
            || !self.b.is_finite()
            || !aabb.min.is_finite()
            || !aabb.max.is_finite()
        {
            return false;
        }
        if aabb.min.x > aabb.max.x || aabb.min.y > aabb.max.y || aabb.min.z > aabb.max.z {
            return false;
        }
        segment_aabb_distance_sq(self.a, self.b, aabb.min, aabb.max) <= self.radius * self.radius
    }

    /// Closest point on or inside the solid capsule to `point`.
    ///
    /// A point already inside is returned unchanged. An empty capsule (negative
    /// radius), or a radius that is not finite, returns the closest point on
    /// the inner segment.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Capsule, vec3};
    ///
    /// let body = Capsule::new(vec3(0.0, 0.0, 0.0), vec3(0.0, 2.0, 0.0), 1.0);
    /// assert_eq!(body.closest_point(vec3(0.0, 1.0, 3.0)), vec3(0.0, 1.0, 1.0));
    /// ```
    pub fn closest_point(&self, point: Vec3) -> Vec3 {
        if !self.a.is_finite() || !self.b.is_finite() || !point.is_finite() {
            return self.a;
        }
        let on_segment = closest_point_on_segment(self.a, self.b, point);
        if !self.radius.is_finite() || self.radius <= 0.0 {
            return on_segment;
        }
        let delta = point - on_segment;
        let dist_sq = delta.length_squared();
        if dist_sq <= self.radius * self.radius {
            return point;
        }
        on_segment + delta * (self.radius / dist_sq.sqrt())
    }

    /// Ray test against this capsule.
    ///
    /// `direction` does not need to be normalized. Hits with `t < 0` (the
    /// capsule is behind the origin) are rejected. Touching the surface counts
    /// as a hit. The hit is on the cylinder between `a` and `b`, or on a
    /// spherical cap past one of those endpoints.
    ///
    /// When the origin is inside the closed capsule, the hit is at `t == 0`
    /// with a zero normal. The same `t == 0` / zero-normal result is returned
    /// when the origin is on the surface and `direction` points outward (or is
    /// zero). An origin on the surface with `direction` pointing inward or
    /// tangent reports `t == 0` and the outward normal.
    ///
    /// A non-finite origin, direction, endpoint, or radius misses. An infinite
    /// radius misses: it is not a usable capsule.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Capsule, Vec3, vec3};
    ///
    /// let body = Capsule::new(Vec3::ZERO, vec3(0.0, 2.0, 0.0), 1.0);
    /// let hit = body
    ///     .ray_intersect(vec3(-3.0, 1.0, 0.0), Vec3::X)
    ///     .unwrap();
    /// assert_eq!(hit.t, 2.0);
    /// assert_eq!(hit.point, vec3(-1.0, 1.0, 0.0));
    /// ```
    pub fn ray_intersect(&self, origin: Vec3, direction: Vec3) -> Option<RayHit> {
        if !self.radius.is_finite() || self.radius < 0.0 {
            return None;
        }
        if !self.a.is_finite()
            || !self.b.is_finite()
            || !origin.is_finite()
            || !direction.is_finite()
        {
            return None;
        }

        let on_segment = closest_point_on_segment(self.a, self.b, origin);
        let offset = origin - on_segment;
        let dist_sq = offset.length_squared();
        let radius_sq = self.radius * self.radius;
        if dist_sq <= radius_sq {
            return Some(surface_origin_hit(
                offset,
                direction,
                dist_sq,
                radius_sq,
                self.radius,
                origin,
            ));
        }
        if direction == Vec3::ZERO {
            return None;
        }

        let mut best: Option<RayHit> = None;
        consider_hit(
            &mut best,
            cylinder_hit(self.a, self.b, self.radius, origin, direction),
        );
        consider_hit(
            &mut best,
            cap_hit(self.a, self.b - self.a, self.radius, origin, direction),
        );
        consider_hit(
            &mut best,
            cap_hit(self.b, self.a - self.b, self.radius, origin, direction),
        );
        best
    }
}

fn surface_origin_hit(
    offset: Vec3,
    direction: Vec3,
    dist_sq: f32,
    radius_sq: f32,
    radius: f32,
    origin: Vec3,
) -> RayHit {
    let outward = offset.dot(direction) > 0.0;
    let normal = if direction == Vec3::ZERO || dist_sq < radius_sq || outward || radius == 0.0 {
        Vec3::ZERO
    } else {
        offset.normalize_or_zero()
    };
    RayHit {
        t: 0.0,
        point: origin,
        normal,
    }
}

fn consider_hit(best: &mut Option<RayHit>, hit: Option<RayHit>) {
    let Some(hit) = hit else {
        return;
    };
    if !hit.t.is_finite() || hit.t < 0.0 || !hit.point.is_finite() || !hit.normal.is_finite() {
        return;
    }
    let closer = match best {
        Some(current) => hit.t < current.t,
        None => true,
    };
    if closer {
        *best = Some(hit);
    }
}

/// First hit on the finite cylinder, ignoring the open disks at the ends.
///
/// Those disks are covered by the spherical caps. A ray parallel to the axis
/// does not meet the wall.
fn cylinder_hit(a: Vec3, b: Vec3, radius: f32, origin: Vec3, direction: Vec3) -> Option<RayHit> {
    let axis = b - a;
    let axis_len_sq = axis.length_squared();
    if axis_len_sq <= f32::EPSILON {
        return None;
    }
    let axis_len = axis_len_sq.sqrt();
    let axis_n = axis / axis_len;

    let oc = origin - a;
    let dir_along = direction.dot(axis_n);
    let oc_along = oc.dot(axis_n);
    let dir_perp = direction - axis_n * dir_along;
    let oc_perp = oc - axis_n * oc_along;

    // Parallel to the axis: the wall is not crossed. Caps handle the ends.
    if dir_perp.length_squared() <= 1e-12 {
        return None;
    }
    let (t_near, t_far) = super::sphere_quadratic_roots(oc_perp, dir_perp, radius)?;

    let mut best: Option<RayHit> = None;
    for t in [t_near, t_far] {
        if !t.is_finite() || t < 0.0 {
            continue;
        }
        let point = origin + direction * t;
        let h = (point - a).dot(axis_n);
        // The infinite cylinder continues past the segment, outside the capsule.
        // Caps cover those ends, including the rim. Accepting `h` slightly
        // outside `[0, axis_len]` reports a hit that `contains` rejects.
        if !(0.0..=axis_len).contains(&h) {
            continue;
        }
        let axis_point = a + axis_n * h;
        let normal = if radius == 0.0 {
            Vec3::ZERO
        } else {
            (point - axis_point).normalize_or_zero()
        };
        consider_hit(&mut best, Some(RayHit { t, point, normal }));
    }
    best
}

/// Ray hit on the sphere at `center` that lies on the outer hemisphere.
///
/// `toward_other` points from `center` toward the other endpoint. The outer
/// hemisphere is the side facing away from the rest of the segment, including
/// the ring where the cap meets the cylinder.
fn cap_hit(
    center: Vec3,
    toward_other: Vec3,
    radius: f32,
    origin: Vec3,
    direction: Vec3,
) -> Option<RayHit> {
    let hit = Sphere::new(center, radius).ray_intersect(origin, direction)?;
    let axis_len_sq = toward_other.length_squared();
    if axis_len_sq <= f32::EPSILON {
        return Some(hit);
    }
    let axis_len = axis_len_sq.sqrt();
    let along = (hit.point - center).dot(toward_other / axis_len);
    let slack = 1e-4 * axis_len.max(1.0);
    if along <= slack {
        Some(hit)
    } else {
        None
    }
}

const fn point_segment_distance_sq(point: Vec3, a: Vec3, b: Vec3) -> f32 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let abz = b.z - a.z;
    let len_sq = abx * abx + aby * aby + abz * abz;
    let px = point.x - a.x;
    let py = point.y - a.y;
    let pz = point.z - a.z;
    if len_sq == 0.0 {
        return px * px + py * py + pz * pz;
    }
    let mut t = (px * abx + py * aby + pz * abz) / len_sq;
    if t < 0.0 {
        t = 0.0;
    } else if t > 1.0 {
        t = 1.0;
    }
    let dx = point.x - (a.x + abx * t);
    let dy = point.y - (a.y + aby * t);
    let dz = point.z - (a.z + abz * t);
    dx * dx + dy * dy + dz * dz
}

fn closest_point_on_segment(a: Vec3, b: Vec3, point: Vec3) -> Vec3 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq == 0.0 {
        return a;
    }
    let t = ((point - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    a + ab * t
}

/// Squared distance between two segments. Parallel segments take an endpoint pair.
fn segment_segment_distance_sq(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> f32 {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.length_squared();
    let e = d2.length_squared();
    let f = d2.dot(r);
    let (s, t) = if a <= f32::EPSILON && e <= f32::EPSILON {
        (0.0, 0.0)
    } else if a <= f32::EPSILON {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= f32::EPSILON {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s = if denom > 1e-8 * a * e {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = ((-c) / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    let c1 = p1 + d1 * s;
    let c2 = p2 + d2 * t;
    (c1 - c2).length_squared()
}

fn segment_aabb_distance_sq(a: Vec3, b: Vec3, min: Vec3, max: Vec3) -> f32 {
    if segment_hits_aabb(a, b, min, max) {
        return 0.0;
    }
    let mut best = f32::MAX;
    let min_a = [min.x, min.y, min.z];
    let max_a = [max.x, max.y, max.z];
    for axis in 0..3 {
        for plane in [min_a[axis], max_a[axis]] {
            let distance = segment_rect_distance_sq(a, b, axis, plane, min_a, max_a);
            if distance < best {
                best = distance;
            }
        }
    }
    best
}

fn segment_hits_aabb(a: Vec3, b: Vec3, min: Vec3, max: Vec3) -> bool {
    let origin = [a.x, a.y, a.z];
    let delta = [b.x - a.x, b.y - a.y, b.z - a.z];
    let lower = [min.x, min.y, min.z];
    let upper = [max.x, max.y, max.z];
    let mut t_min = 0.0;
    let mut t_max = 1.0;
    for i in 0..3 {
        if delta[i] == 0.0 {
            if origin[i] < lower[i] || origin[i] > upper[i] {
                return false;
            }
        } else {
            let inv = 1.0 / delta[i];
            let mut t_near = (lower[i] - origin[i]) * inv;
            let mut t_far = (upper[i] - origin[i]) * inv;
            if t_near > t_far {
                std::mem::swap(&mut t_near, &mut t_far);
            }
            if t_near > t_min {
                t_min = t_near;
            }
            if t_far < t_max {
                t_max = t_far;
            }
            if t_min > t_max {
                return false;
            }
        }
    }
    true
}

/// Squared distance from a segment to one axis-aligned rectangular face.
///
/// The face is outside the caller when the segment misses the box, so the
/// closest point is an endpoint projection or a point on a face edge. A
/// segment that stabs the rectangle is reported by [`segment_hits_aabb`].
fn segment_rect_distance_sq(
    a: Vec3,
    b: Vec3,
    axis: usize,
    plane: f32,
    min: [f32; 3],
    max: [f32; 3],
) -> f32 {
    let mut best = point_rect_distance_sq(a, axis, plane, min, max)
        .min(point_rect_distance_sq(b, axis, plane, min, max));
    let corners = face_corners(axis, plane, min, max);
    for i in 0..4 {
        let distance = segment_segment_distance_sq(a, b, corners[i], corners[(i + 1) % 4]);
        if distance < best {
            best = distance;
        }
    }
    best
}

fn point_rect_distance_sq(
    point: Vec3,
    axis: usize,
    plane: f32,
    min: [f32; 3],
    max: [f32; 3],
) -> f32 {
    let mut closest = [point.x, point.y, point.z];
    closest[axis] = plane;
    for i in 0..3 {
        if i != axis {
            closest[i] = clamp_axis(closest[i], min[i], max[i]);
        }
    }
    let dx = point.x - closest[0];
    let dy = point.y - closest[1];
    let dz = point.z - closest[2];
    dx * dx + dy * dy + dz * dz
}

fn face_corners(axis: usize, plane: f32, min: [f32; 3], max: [f32; 3]) -> [Vec3; 4] {
    let (u, v) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let u_coord = [min[u], max[u], max[u], min[u]];
    let v_coord = [min[v], min[v], max[v], max[v]];
    let mut corners = [Vec3::ZERO; 4];
    for i in 0..4 {
        let mut p = [plane, plane, plane];
        p[axis] = plane;
        p[u] = u_coord[i];
        p[v] = v_coord[i];
        corners[i] = vec3(p[0], p[1], p[2]);
    }
    corners
}

const fn clamp_axis(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standing() -> Capsule {
        Capsule::new(vec3(0.0, 0.0, 0.0), vec3(0.0, 2.0, 0.0), 1.0)
    }

    fn assert_vec3_close(left: Vec3, right: Vec3) {
        let delta = left - right;
        assert!(
            delta.x.abs() < 1e-4 && delta.y.abs() < 1e-4 && delta.z.abs() < 1e-4,
            "left {left:?} right {right:?}"
        );
    }

    #[test]
    fn contains_includes_the_cylinder_and_the_caps() {
        let body = standing();
        assert!(body.contains(vec3(0.0, 1.0, 0.0)));
        assert!(body.contains(vec3(0.0, 1.0, 1.0)));
        assert!(body.contains(vec3(0.0, -1.0, 0.0)));
        assert!(body.contains(vec3(0.0, 3.0, 0.0)));
        assert!(!body.contains(vec3(0.0, 1.0, 1.01)));
        assert!(!body.contains(vec3(0.0, -1.01, 0.0)));
        assert!(!body.contains(vec3(0.0, 3.01, 0.0)));
        assert!(!Capsule::new(Vec3::ZERO, Vec3::Y, -0.5).contains(Vec3::ZERO));
        assert!(Capsule::new(Vec3::X, Vec3::X, 0.0).contains(Vec3::X));
        assert!(!Capsule::new(Vec3::X, Vec3::X, 0.0).contains(vec3(1.1, 0.0, 0.0)));
    }

    #[test]
    fn swapping_endpoints_does_not_change_contains_or_overlap() {
        let body = standing();
        let flipped = Capsule::new(body.b, body.a, body.radius);
        let sample = vec3(0.2, -0.4, 0.3);
        assert_eq!(body.contains(sample), flipped.contains(sample));
        let other = Capsule::new(vec3(1.5, 0.0, 0.0), vec3(1.5, 2.0, 0.0), 0.5);
        assert_eq!(body.overlaps(&other), flipped.overlaps(&other));
        assert_eq!(body.overlaps(&other), other.overlaps(&body));
    }

    #[test]
    fn overlaps_capsules_by_segment_distance() {
        let body = standing();
        let touching = Capsule::new(vec3(2.0, 0.0, 0.0), vec3(2.0, 2.0, 0.0), 1.0);
        assert!(body.overlaps(&touching));
        let separated = Capsule::new(vec3(2.01, 0.0, 0.0), vec3(2.01, 2.0, 0.0), 1.0);
        assert!(!body.overlaps(&separated));

        let crossing = Capsule::new(vec3(-1.0, 1.0, 0.0), vec3(1.0, 1.0, 0.0), 0.0);
        assert!(Capsule::new(Vec3::ZERO, vec3(0.0, 2.0, 0.0), 0.0).overlaps(&crossing));

        let parallel = Capsule::new(vec3(0.0, 0.5, 0.0), vec3(2.0, 0.5, 0.0), 0.5);
        let spine = Capsule::new(Vec3::ZERO, vec3(2.0, 0.0, 0.0), 0.0);
        assert!(parallel.overlaps(&spine));
        assert!(!Capsule::new(vec3(0.0, 0.51, 0.0), vec3(2.0, 0.51, 0.0), 0.5).overlaps(&spine));

        assert!(!body.overlaps(&Capsule::new(Vec3::ZERO, Vec3::Y, -1.0)));
        assert!(!Capsule::new(Vec3::ZERO, Vec3::Y, -1.0).overlaps(&body));
        assert!(!body.overlaps(&Capsule::new(Vec3::ZERO, Vec3::Y, f32::INFINITY)));
        assert!(!Capsule::new(Vec3::ZERO, Vec3::Y, f32::NAN).overlaps(&body));
        assert!(!Capsule::new(Vec3::ZERO, Vec3::Y, f32::INFINITY).contains(Vec3::ZERO));
    }

    #[test]
    fn overlaps_sphere_including_a_degenerate_capsule() {
        let body = standing();
        assert!(body.overlaps_sphere(&Sphere::new(vec3(0.0, 1.0, 2.0), 1.0)));
        assert!(!body.overlaps_sphere(&Sphere::new(vec3(0.0, 1.0, 2.0), 0.99)));
        assert!(body.overlaps_sphere(&Sphere::new(vec3(0.0, 1.0, 0.0), 0.1)));
        assert!(!body.overlaps_sphere(&Sphere::new(vec3(0.0, 1.0, 0.0), -1.0)));
        assert!(!body.overlaps_sphere(&Sphere::new(vec3(0.0, 1.0, 0.0), f32::INFINITY)));
        assert!(!body.overlaps_sphere(&Sphere::new(vec3(0.0, 1.0, 0.0), f32::NAN)));
        assert!(!Capsule::new(Vec3::ZERO, Vec3::Y, -0.25)
            .overlaps_sphere(&Sphere::new(Vec3::ZERO, 10.0)));
        assert!(!Capsule::new(Vec3::ZERO, Vec3::Y, f32::INFINITY)
            .overlaps_sphere(&Sphere::new(Vec3::ZERO, 10.0)));
        assert!(!Capsule::new(Vec3::ZERO, Vec3::Y, f32::INFINITY)
            .overlaps_aabb(&Aabb::from_min_max(Vec3::ZERO, Vec3::ONE,)));

        let point = Capsule::new(Vec3::ZERO, Vec3::ZERO, 1.0);
        assert!(point.overlaps_sphere(&Sphere::new(Vec3::X, 0.0)));
        assert!(!point.overlaps_sphere(&Sphere::new(vec3(1.01, 0.0, 0.0), 0.0)));
    }

    #[test]
    fn overlaps_aabb_on_a_face_a_corner_and_a_pass_through() {
        let box_ = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);

        // The middle of the segment is 1 unit above the top face. Endpoints are farther.
        let over_face = Capsule::new(vec3(-1.0, 0.5, 2.0), vec3(2.0, 0.5, 2.0), 1.0);
        assert!(over_face.overlaps_aabb(&box_));
        assert!(!Capsule::new(over_face.a, over_face.b, 0.99).overlaps_aabb(&box_));

        // Closest feature is the edge x = 0, y = 0. Distance is sqrt(2) at the middle.
        let along_edge = Capsule::new(vec3(-1.0, -1.0, -1.0), vec3(-1.0, -1.0, 2.0), 1.5);
        assert!(along_edge.overlaps_aabb(&box_));
        assert!(!Capsule::new(along_edge.a, along_edge.b, 1.4).overlaps_aabb(&box_));

        let through = Capsule::new(vec3(-1.0, 0.5, 0.5), vec3(2.0, 0.5, 0.5), 0.0);
        assert!(through.overlaps_aabb(&box_));

        // Distance to the corner (1, 1, 0.5) is sqrt(2). Stay off that exact value:
        // `sqrt(2) * sqrt(2)` is not a reliable `f32` 2.
        let corner = Capsule::new(vec3(2.0, 2.0, 0.5), vec3(3.0, 2.0, 0.5), 1.42);
        assert!(corner.overlaps_aabb(&box_));
        assert!(!Capsule::new(corner.a, corner.b, 1.4).overlaps_aabb(&box_));

        let standing_on = Capsule::new(vec3(0.5, 0.5, 0.5), vec3(0.5, 1.5, 0.5), 0.5);
        let floor = Aabb::from_min_max(vec3(0.0, -0.2, 0.0), vec3(1.0, 0.0, 1.0));
        assert!(standing_on.overlaps_aabb(&floor));
        assert!(!standing_on.overlaps_aabb(&Aabb::from_min_max(
            vec3(0.0, -0.2, 0.0),
            vec3(1.0, -0.01, 1.0),
        )));

        assert!(!standing().overlaps_aabb(&Aabb::from_min_max(Vec3::ONE, Vec3::ZERO)));
        assert!(!Capsule::new(Vec3::ZERO, Vec3::Y, -1.0).overlaps_aabb(&box_));
        assert!(
            !standing().overlaps_aabb(&Aabb::from_min_max(vec3(f32::NAN, 0.0, 0.0), Vec3::ONE,))
        );
    }

    #[test]
    fn overlaps_aabb_matches_sampled_spine_distance() {
        let box_ = Aabb::from_min_max(vec3(-0.5, 0.0, -0.25), vec3(1.25, 0.8, 1.5));
        let segments = [
            (vec3(-2.0, 1.2, 0.4), vec3(2.5, 1.2, 0.4)),
            (vec3(-1.0, -1.0, -1.0), vec3(-1.0, -1.0, 3.0)),
            (vec3(0.4, -1.0, 0.4), vec3(0.4, 3.0, 0.4)),
            (vec3(-2.0, 0.4, 0.2), vec3(3.0, 0.4, 1.2)),
            (vec3(3.0, 3.0, 3.0), vec3(4.0, 3.5, 2.0)),
            (vec3(0.2, 0.2, -2.0), vec3(0.7, 0.5, 3.0)),
            (vec3(-0.2, 2.0, 0.5), vec3(1.4, -1.0, 0.5)),
        ];
        for (a, b) in segments {
            let gap = sampled_spine_gap(a, b, &box_);
            let spacing = (b - a).length() / 128.0;
            let margin = spacing * 0.5 + 1e-3;
            assert!(
                Capsule::new(a, b, gap + 1e-3).overlaps_aabb(&box_),
                "sample gap {gap} should reach the box"
            );
            if gap > margin {
                assert!(
                    !Capsule::new(a, b, gap - margin).overlaps_aabb(&box_),
                    "radius {} is inside the sampled gap {gap}",
                    gap - margin
                );
            }
        }
    }

    fn sampled_spine_gap(a: Vec3, b: Vec3, aabb: &Aabb) -> f32 {
        let mut best = f32::MAX;
        for i in 0..=128 {
            let t = i as f32 / 128.0;
            let point = a + (b - a) * t;
            let distance = (point - aabb.closest_point(point)).length();
            if distance < best {
                best = distance;
            }
        }
        best
    }

    #[test]
    fn closest_point_projects_to_the_surface_and_keeps_an_interior_point() {
        let body = standing();
        assert_vec3_close(body.closest_point(vec3(0.0, 1.0, 3.0)), vec3(0.0, 1.0, 1.0));
        assert_vec3_close(
            body.closest_point(vec3(0.0, -3.0, 0.0)),
            vec3(0.0, -1.0, 0.0),
        );
        assert_vec3_close(body.closest_point(vec3(0.0, 5.0, 0.0)), vec3(0.0, 3.0, 0.0));
        assert_vec3_close(body.closest_point(vec3(0.2, 1.0, 0.2)), vec3(0.2, 1.0, 0.2));
        assert_vec3_close(
            Capsule::new(body.a, body.b, 0.0).closest_point(vec3(4.0, 1.0, 0.0)),
            vec3(0.0, 1.0, 0.0),
        );
        assert_vec3_close(
            Capsule::new(body.a, body.b, -1.0).closest_point(vec3(0.0, 1.0, 5.0)),
            vec3(0.0, 1.0, 0.0),
        );

        let moved = body.offset(vec3(2.0, 0.0, -1.0));
        assert_vec3_close(moved.a, vec3(2.0, 0.0, -1.0));
        assert_vec3_close(moved.b, vec3(2.0, 2.0, -1.0));
        assert_eq!(moved.radius, 1.0);
    }

    #[test]
    fn ray_hits_the_cylinder_and_either_cap() {
        let body = standing();
        let side = body.ray_intersect(vec3(-3.0, 1.0, 0.0), Vec3::X).unwrap();
        assert!((side.t - 2.0).abs() < 1e-4);
        assert_vec3_close(side.point, vec3(-1.0, 1.0, 0.0));
        assert_vec3_close(side.normal, -Vec3::X);
        assert!((side.normal.length() - 1.0).abs() < 1e-4);

        let scaled = body
            .ray_intersect(vec3(-3.0, 1.0, 0.0), vec3(2.0, 0.0, 0.0))
            .unwrap();
        assert!((scaled.t - 1.0).abs() < 1e-4);
        assert_vec3_close(scaled.point, side.point);

        let bottom = body.ray_intersect(vec3(0.0, -3.0, 0.0), Vec3::Y).unwrap();
        assert!((bottom.t - 2.0).abs() < 1e-4);
        assert_vec3_close(bottom.point, vec3(0.0, -1.0, 0.0));
        assert_vec3_close(bottom.normal, -Vec3::Y);

        let top = body.ray_intersect(vec3(0.0, 5.0, 0.0), -Vec3::Y).unwrap();
        assert!((top.t - 2.0).abs() < 1e-4);
        assert_vec3_close(top.point, vec3(0.0, 3.0, 0.0));
        assert_vec3_close(top.normal, Vec3::Y);

        let side_cap = body.ray_intersect(vec3(-3.0, -0.5, 0.0), Vec3::X).unwrap();
        let expected_x = -(0.75_f32.sqrt());
        assert!((side_cap.t - (3.0 - 0.75_f32.sqrt())).abs() < 1e-4);
        assert_vec3_close(side_cap.point, vec3(expected_x, -0.5, 0.0));
        assert!(side_cap.normal.y < 0.0);
        assert!((side_cap.normal.length() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn ray_grazes_misses_and_rejects_non_finite_input() {
        let body = standing();
        let grazing = body.ray_intersect(vec3(1.0, 1.0, -5.0), Vec3::Z).unwrap();
        assert!((grazing.t - 5.0).abs() < 1e-4);
        assert_vec3_close(grazing.point, vec3(1.0, 1.0, 0.0));
        assert_vec3_close(grazing.normal, Vec3::X);

        assert!(body.ray_intersect(vec3(1.01, 1.0, -5.0), Vec3::Z).is_none());
        assert!(body.ray_intersect(vec3(3.0, 1.0, 0.0), Vec3::X).is_none());
        assert!(body.ray_intersect(vec3(0.0, 4.0, 0.0), Vec3::Y).is_none());
        assert!(body
            .ray_intersect(vec3(4.0, 1.0, 0.0), Vec3::ZERO)
            .is_none());
        assert!(Capsule::new(body.a, body.b, -1.0)
            .ray_intersect(vec3(-3.0, 1.0, 0.0), Vec3::X)
            .is_none());
        assert!(body
            .ray_intersect(vec3(f32::NAN, 0.0, 0.0), Vec3::X)
            .is_none());
        assert!(body
            .ray_intersect(vec3(-3.0, 1.0, 0.0), vec3(f32::INFINITY, 0.0, 0.0))
            .is_none());
        assert!(Capsule::new(vec3(f32::NAN, 0.0, 0.0), Vec3::Y, 1.0)
            .ray_intersect(vec3(-3.0, 1.0, 0.0), Vec3::X)
            .is_none());
        assert!(Capsule::new(Vec3::ZERO, Vec3::Y, f32::INFINITY)
            .ray_intersect(vec3(-3.0, 1.0, 0.0), Vec3::X)
            .is_none());
    }

    #[test]
    fn ray_inside_and_on_the_surface() {
        let body = standing();
        let inside = body.ray_intersect(vec3(0.0, 1.0, 0.0), Vec3::X).unwrap();
        assert_eq!(inside.t, 0.0);
        assert_eq!(inside.point, vec3(0.0, 1.0, 0.0));
        assert_eq!(inside.normal, Vec3::ZERO);

        let entering = body.ray_intersect(vec3(0.0, 1.0, 1.0), -Vec3::Z).unwrap();
        assert_eq!(entering.t, 0.0);
        assert_vec3_close(entering.normal, Vec3::Z);

        let leaving = body.ray_intersect(vec3(0.0, 1.0, 1.0), Vec3::Z).unwrap();
        assert_eq!(leaving.t, 0.0);
        assert_eq!(leaving.normal, Vec3::ZERO);

        let tangent = body.ray_intersect(vec3(0.0, 1.0, 1.0), Vec3::X).unwrap();
        assert_eq!(tangent.t, 0.0);
        assert_vec3_close(tangent.normal, Vec3::Z);

        let stationary = body.ray_intersect(vec3(0.0, 1.0, 0.0), Vec3::ZERO).unwrap();
        assert_eq!(stationary.t, 0.0);
        assert_eq!(stationary.normal, Vec3::ZERO);

        let flipped = Capsule::new(body.b, body.a, body.radius);
        let hit = flipped
            .ray_intersect(vec3(-3.0, 1.0, 0.0), Vec3::X)
            .unwrap();
        assert_vec3_close(hit.point, vec3(-1.0, 1.0, 0.0));
        assert_vec3_close(hit.normal, -Vec3::X);
    }

    #[test]
    fn ray_hits_a_segment_and_a_point_capsule() {
        let segment = Capsule::new(Vec3::ZERO, vec3(0.0, 2.0, 0.0), 0.0);
        let hit = segment
            .ray_intersect(vec3(-2.0, 1.0, 0.0), Vec3::X)
            .unwrap();
        assert!((hit.t - 2.0).abs() < 1e-4);
        assert_vec3_close(hit.point, vec3(0.0, 1.0, 0.0));
        assert_eq!(hit.normal, Vec3::ZERO);
        assert!(segment
            .ray_intersect(vec3(-2.0, 1.0, 1.0), Vec3::X)
            .is_none());

        let point = Capsule::new(Vec3::ZERO, Vec3::ZERO, 1.0);
        let sphere_hit = point.ray_intersect(vec3(-2.0, 0.0, 0.0), Vec3::X).unwrap();
        assert!((sphere_hit.t - 1.0).abs() < 1e-4);
        assert_vec3_close(sphere_hit.point, -Vec3::X);
        assert_vec3_close(sphere_hit.normal, -Vec3::X);
    }

    #[test]
    fn ray_hits_a_diagonal_cylinder() {
        let body = Capsule::new(Vec3::ZERO, vec3(2.0, 0.0, 0.0), 1.0);
        let hit = body.ray_intersect(vec3(1.0, -3.0, 0.0), Vec3::Y).unwrap();
        assert!((hit.t - 2.0).abs() < 1e-4);
        assert_vec3_close(hit.point, vec3(1.0, -1.0, 0.0));
        assert_vec3_close(hit.normal, -Vec3::Y);
        assert_vec3_close(hit.point, vec3(1.0, -3.0, 0.0) + Vec3::Y * hit.t);
    }

    #[test]
    fn ray_does_not_accept_the_infinite_cylinder_past_either_cap() {
        // Axial slack used to grow with the segment. On a long body the wall
        // past the end (radius 1 at y = -0.05) is outside the solid, and that
        // hit was closer than the real one on the spherical cap.
        let body = Capsule::new(Vec3::ZERO, vec3(0.0, 1000.0, 0.0), 1.0);
        assert!(!body.contains(vec3(1.0, -0.05, 0.0)));

        let below = body.ray_intersect(vec3(4.0, -0.05, 0.0), -Vec3::X).unwrap();
        assert!(
            below.point.x < 0.999,
            "hit the cylinder wall past the cap: {:?}",
            below.point
        );
        assert!((below.point.length() - 1.0).abs() < 1e-3);
        assert!(below.point.y < 0.0);
        assert!((body.closest_point(below.point) - below.point).length() < 1e-3);

        let above = body
            .ray_intersect(vec3(4.0, 1000.05, 0.0), -Vec3::X)
            .unwrap();
        let cap_center = vec3(0.0, 1000.0, 0.0);
        assert!(((above.point - cap_center).length() - 1.0).abs() < 1e-3);
        assert!(above.point.y > 1000.0);
        assert!((body.closest_point(above.point) - above.point).length() < 1e-3);

        assert!(body.ray_intersect(vec3(4.0, -2.0, 0.0), -Vec3::X).is_none());

        let rim = body.ray_intersect(vec3(-3.0, 0.0, 0.0), Vec3::X).unwrap();
        assert!((rim.t - 2.0).abs() < 1e-3);
        assert_vec3_close(rim.point, -Vec3::X);
        assert_vec3_close(rim.normal, -Vec3::X);

        let side_cap = body.ray_intersect(vec3(-3.0, -0.5, 0.0), Vec3::X).unwrap();
        assert!(side_cap.point.y < 0.0);
        assert!((body.closest_point(side_cap.point) - side_cap.point).length() < 1e-3);
    }
}
