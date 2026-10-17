//! Triangles in 3D.
//!
//! A triangle is three corners in world space. It does not assume a particular
//! up axis, matching [`Aabb`](crate::math::Aabb) and [`Plane`](crate::math::Plane).
//! The winding `a → b → c` follows the right-hand rule, the same order as
//! [`Plane::from_points`](crate::math::Plane::from_points).

use glam::{vec3, Vec3};

use super::{Aabb, Plane, RayHit};

/// A triangle, defined by three corners.
///
/// The corners are stored as given. A zero-area triangle (collinear or
/// repeated corners) has no plane: it contains nothing and the ray query
/// misses. [`Triangle::closest_point`] still returns a point on the corners
/// or the edges.
///
/// Boundaries are inclusive, matching [`Aabb`](crate::math::Aabb) and
/// [`Plane`](crate::math::Plane): a corner, an edge, and a coplanar point in
/// the interior are inside.
///
/// # Example
///
/// ```
/// use macroquad::math::{Triangle, Vec3, vec3};
///
/// let triangle = Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y);
/// assert!(triangle.contains(vec3(0.25, 0.25, 0.0)));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    /// First corner. The winding starts here.
    pub a: Vec3,
    /// Second corner.
    pub b: Vec3,
    /// Third corner.
    pub c: Vec3,
}

impl Triangle {
    /// Creates a triangle from three corners.
    ///
    /// The winding `a → b → c` is the right-hand rule for [`Triangle::plane`].
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Triangle, Vec3};
    ///
    /// let triangle = Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y);
    /// assert_eq!(triangle.a, Vec3::ZERO);
    /// ```
    pub const fn new(a: Vec3, b: Vec3, c: Vec3) -> Triangle {
        Triangle { a, b, c }
    }

    /// Plane through the three corners.
    ///
    /// The normal follows `(b - a) × (c - a)`, the same rule as
    /// [`Plane::from_points`](crate::math::Plane::from_points). Returns
    /// [`None`] when the corners are collinear, coincident, or non-finite.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Triangle, Vec3};
    ///
    /// let triangle = Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y);
    /// assert_eq!(triangle.plane().unwrap().normal, Vec3::Z);
    /// ```
    pub fn plane(self) -> Option<Plane> {
        Plane::from_points(self.a, self.b, self.c)
    }

    /// Smallest axis-aligned box containing the three corners.
    ///
    /// Built with [`Aabb::from_points`](crate::math::Aabb::from_points).
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Triangle, Vec3, vec3};
    ///
    /// let triangle = Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y);
    /// assert_eq!(triangle.aabb().min, Vec3::ZERO);
    /// assert_eq!(triangle.aabb().max, vec3(1.0, 1.0, 0.0));
    /// ```
    pub fn aabb(self) -> Aabb {
        match Aabb::from_points([self.a, self.b, self.c]) {
            Some(aabb) => aabb,
            None => Aabb {
                min: self.a,
                max: self.a,
            },
        }
    }

    /// Returns true when `point` lies in the triangle, including the boundary.
    ///
    /// The point must be on the plane. A point above the triangle is outside,
    /// even when its projection would land inside. A zero-area triangle
    /// contains nothing.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Triangle, Vec3, vec3};
    ///
    /// let triangle = Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y);
    /// assert!(triangle.contains(vec3(0.25, 0.25, 0.0)));
    /// assert!(!triangle.contains(vec3(0.25, 0.25, 0.1)));
    /// ```
    pub fn contains(self, point: Vec3) -> bool {
        let Some(plane) = self.plane() else {
            return false;
        };
        if !point.is_finite() || plane.signed_distance(point) != 0.0 {
            return false;
        }
        let Some((u, v, w)) = barycentric(self, point) else {
            return false;
        };
        u >= 0.0 && v >= 0.0 && w >= 0.0
    }

    /// Closest point on the triangle to `point`.
    ///
    /// A point already in the triangle is returned unchanged. A non-finite
    /// input returns `a`.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Triangle, Vec3, vec3};
    ///
    /// let triangle = Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y);
    /// assert_eq!(triangle.closest_point(vec3(0.25, 0.25, 0.0)), vec3(0.25, 0.25, 0.0));
    /// assert_eq!(triangle.closest_point(vec3(-1.0, 0.0, 0.0)), Vec3::ZERO);
    /// ```
    pub fn closest_point(self, point: Vec3) -> Vec3 {
        if !point.is_finite() || !self.a.is_finite() || !self.b.is_finite() || !self.c.is_finite() {
            return self.a;
        }
        if self.contains(point) {
            return point;
        }

        // Christer Ericson, Real-Time Collision Detection, section 5.1.5.
        let (a, b, c) = (self.a, self.b, self.c);
        let ab = b - a;
        let ac = c - a;
        let ap = point - a;
        let d1 = ab.dot(ap);
        let d2 = ac.dot(ap);
        if d1 <= 0.0 && d2 <= 0.0 {
            return a;
        }

        let bp = point - b;
        let d3 = ab.dot(bp);
        let d4 = ac.dot(bp);
        if d3 >= 0.0 && d4 <= d3 {
            return b;
        }

        let vc = d1 * d4 - d3 * d2;
        let ab_denom = d1 - d3;
        if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 && ab_denom > 0.0 {
            return a + ab * (d1 / ab_denom);
        }

        let cp = point - c;
        let d5 = ab.dot(cp);
        let d6 = ac.dot(cp);
        if d6 >= 0.0 && d5 <= d6 {
            return c;
        }

        let vb = d5 * d2 - d1 * d6;
        let ac_denom = d2 - d6;
        if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 && ac_denom > 0.0 {
            return a + ac * (d2 / ac_denom);
        }

        let va = d3 * d6 - d5 * d4;
        let bc_denom = (d4 - d3) + (d5 - d6);
        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 && bc_denom > 0.0 {
            return b + (c - b) * ((d4 - d3) / bc_denom);
        }

        let denom = va + vb + vc;
        if denom > 0.0 && denom.is_finite() {
            let inv = 1.0 / denom;
            let closest = a + ab * (vb * inv) + ac * (vc * inv);
            if closest.is_finite() {
                return closest;
            }
        }
        closest_on_boundary(point, a, b, c)
    }

    /// Möller–Trumbore ray test. Both winding orders count.
    ///
    /// `direction` does not need to be normalized. Hits with `t < 0` are
    /// rejected. Touching a corner or an edge counts. The reported normal is
    /// the unit winding normal, the same one as [`Triangle::plane`], and is
    /// not flipped to face the ray.
    ///
    /// When the origin already lies in the triangle, the hit is at `t == 0`
    /// with that normal, including a zero or parallel direction. A ray
    /// parallel to the plane misses when the origin is outside the triangle.
    /// A zero-area triangle misses. A non-finite origin, direction, or corner
    /// misses.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Triangle, Vec3, vec3};
    ///
    /// let triangle = Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y);
    /// let hit = triangle
    ///     .ray_intersect(vec3(0.2, 0.2, 1.0), -Vec3::Z)
    ///     .unwrap();
    /// assert!((hit.t - 1.0).abs() < 1e-4);
    /// assert!((hit.point - vec3(0.2, 0.2, 0.0)).length() < 1e-4);
    /// assert!((hit.normal - Vec3::Z).length() < 1e-4);
    /// ```
    pub fn ray_intersect(self, origin: Vec3, direction: Vec3) -> Option<RayHit> {
        self.intersect(origin, direction, false)
    }

    /// [`Triangle::ray_intersect`] that drops back faces.
    ///
    /// A back face is a hit whose ray travels with the winding normal
    /// (`normal · direction > 0`). The front face, where the ray travels
    /// against that normal, is kept. A ray that starts in the triangle is
    /// kept unless it points strictly out through the front.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Triangle, Vec3, vec3};
    ///
    /// let triangle = Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y);
    /// assert!(triangle
    ///     .ray_intersect_cull_backface(vec3(0.2, 0.2, 1.0), -Vec3::Z)
    ///     .is_some());
    /// assert!(triangle
    ///     .ray_intersect_cull_backface(vec3(0.2, 0.2, -1.0), Vec3::Z)
    ///     .is_none());
    /// ```
    pub fn ray_intersect_cull_backface(self, origin: Vec3, direction: Vec3) -> Option<RayHit> {
        self.intersect(origin, direction, true)
    }

    fn intersect(self, origin: Vec3, direction: Vec3, cull_backface: bool) -> Option<RayHit> {
        if !origin.is_finite()
            || !direction.is_finite()
            || !self.a.is_finite()
            || !self.b.is_finite()
            || !self.c.is_finite()
        {
            return None;
        }

        let edge1 = self.b - self.a;
        let edge2 = self.c - self.a;
        let normal = edge1.cross(edge2);
        let normal_len_sq = normal.length_squared();
        if normal_len_sq == 0.0 || !normal_len_sq.is_finite() {
            return None;
        }
        let unit_normal = normal / normal_len_sq.sqrt();
        if !unit_normal.is_finite() {
            return None;
        }

        if self.contains(origin) {
            let leaves_through_the_front = unit_normal.dot(direction) > 0.0;
            if cull_backface && leaves_through_the_front {
                return None;
            }
            return Some(RayHit {
                t: 0.0,
                point: origin,
                normal: unit_normal,
            });
        }

        let pvec = direction.cross(edge2);
        let det = edge1.dot(pvec);
        // Scale the parallel test with the edges and the direction so a small
        // triangle is not rejected just because `det` is small.
        let limit = 1e-8
            * (edge1.length_squared() + edge2.length_squared() + direction.length_squared() + 1.0);
        if cull_backface {
            if det <= limit {
                return None;
            }
        } else if !det.is_finite() || det.abs() <= limit {
            return None;
        }

        let inv_det = 1.0 / det;
        let tvec = origin - self.a;
        let u = tvec.dot(pvec) * inv_det;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let qvec = tvec.cross(edge1);
        let v = direction.dot(qvec) * inv_det;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = edge2.dot(qvec) * inv_det;
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
            normal: unit_normal,
        })
    }
}

fn barycentric(triangle: Triangle, point: Vec3) -> Option<(f32, f32, f32)> {
    let v0 = triangle.b - triangle.a;
    let v1 = triangle.c - triangle.a;
    let v2 = point - triangle.a;
    let dot00 = v0.dot(v0);
    let dot01 = v0.dot(v1);
    let dot02 = v0.dot(v2);
    let dot11 = v1.dot(v1);
    let dot12 = v1.dot(v2);
    let denom = dot00 * dot11 - dot01 * dot01;
    if denom == 0.0 || !denom.is_finite() {
        return None;
    }
    let inv = 1.0 / denom;
    let v = (dot11 * dot02 - dot01 * dot12) * inv;
    let w = (dot00 * dot12 - dot01 * dot02) * inv;
    let u = 1.0 - v - w;
    if u.is_finite() && v.is_finite() && w.is_finite() {
        Some((u, v, w))
    } else {
        None
    }
}

fn closest_on_segment(point: Vec3, start: Vec3, end: Vec3) -> Vec3 {
    let edge = end - start;
    let len_sq = edge.length_squared();
    if len_sq == 0.0 || !len_sq.is_finite() {
        return start;
    }
    let t = ((point - start).dot(edge) / len_sq).clamp(0.0, 1.0);
    start + edge * t
}

fn closest_on_boundary(point: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let mut best = closest_on_segment(point, a, b);
    let mut best_distance = point.distance_squared(best);
    for (start, end) in [(b, c), (c, a)] {
        let candidate = closest_on_segment(point, start, end);
        let distance = point.distance_squared(candidate);
        if distance < best_distance {
            best = candidate;
            best_distance = distance;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xy_triangle() -> Triangle {
        Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y)
    }

    fn assert_vec3_close(left: Vec3, right: Vec3) {
        let delta = left - right;
        assert!(
            delta.x.abs() < 1e-4 && delta.y.abs() < 1e-4 && delta.z.abs() < 1e-4,
            "left {left:?} right {right:?}"
        );
    }

    #[test]
    fn plane_and_aabb_follow_the_winding_and_the_corners() {
        let triangle = xy_triangle();
        let plane = triangle.plane().unwrap();
        assert_vec3_close(plane.normal, Vec3::Z);
        assert!(plane.distance.abs() < 1e-5);
        assert!(Triangle::new(Vec3::ZERO, Vec3::X, vec3(2.0, 0.0, 0.0))
            .plane()
            .is_none());

        let aabb = triangle.aabb();
        assert_vec3_close(aabb.min, Vec3::ZERO);
        assert_vec3_close(aabb.max, vec3(1.0, 1.0, 0.0));
        let lifted = Triangle::new(
            vec3(0.0, 0.0, 2.0),
            vec3(1.0, 0.0, 3.0),
            vec3(0.0, 1.0, 2.5),
        );
        assert_vec3_close(lifted.aabb().min, vec3(0.0, 0.0, 2.0));
        assert_vec3_close(lifted.aabb().max, vec3(1.0, 1.0, 3.0));
    }

    #[test]
    fn contains_includes_the_boundary_and_rejects_off_plane_points() {
        let triangle = xy_triangle();
        assert!(triangle.contains(Vec3::ZERO));
        assert!(triangle.contains(Vec3::X));
        assert!(triangle.contains(Vec3::Y));
        assert!(triangle.contains(vec3(0.5, 0.0, 0.0)));
        assert!(triangle.contains(vec3(0.25, 0.25, 0.0)));
        assert!(triangle.contains(vec3(1.0 / 3.0, 1.0 / 3.0, 0.0)));
        assert!(!triangle.contains(vec3(0.25, 0.25, 0.1)));
        assert!(!triangle.contains(vec3(1.0, 1.0, 0.0)));
        assert!(!triangle.contains(vec3(-0.01, 0.0, 0.0)));
        assert!(!Triangle::new(Vec3::ZERO, Vec3::X, vec3(2.0, 0.0, 0.0)).contains(Vec3::X));
        assert!(!triangle.contains(vec3(f32::NAN, 0.0, 0.0)));
    }

    #[test]
    fn closest_point_projects_inside_and_clamps_to_edges() {
        let triangle = xy_triangle();
        assert_vec3_close(
            triangle.closest_point(vec3(0.25, 0.25, 4.0)),
            vec3(0.25, 0.25, 0.0),
        );
        assert_eq!(
            triangle.closest_point(vec3(0.2, 0.2, 0.0)),
            vec3(0.2, 0.2, 0.0)
        );
        assert_vec3_close(triangle.closest_point(vec3(-1.0, -1.0, 0.5)), Vec3::ZERO);
        assert_vec3_close(triangle.closest_point(vec3(2.0, -1.0, 0.0)), Vec3::X);
        assert_vec3_close(
            triangle.closest_point(vec3(0.5, -1.0, 3.0)),
            vec3(0.5, 0.0, 0.0),
        );
        assert_vec3_close(
            triangle.closest_point(vec3(2.0, 2.0, 0.0)),
            vec3(0.5, 0.5, 0.0),
        );

        let flat = Triangle::new(Vec3::ZERO, vec3(2.0, 0.0, 0.0), Vec3::X);
        assert_vec3_close(flat.closest_point(vec3(1.0, 1.0, 0.0)), Vec3::X);
        assert_eq!(
            Triangle::new(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO).closest_point(Vec3::X),
            Vec3::ZERO
        );
        assert_eq!(triangle.closest_point(vec3(f32::NAN, 0.0, 0.0)), Vec3::ZERO);
    }

    #[test]
    fn ray_hits_both_sides_and_culls_the_back() {
        let triangle = xy_triangle();
        let front = triangle
            .ray_intersect(vec3(0.25, 0.25, 1.0), vec3(0.0, 0.0, -1.0))
            .unwrap();
        assert!((front.t - 1.0).abs() < 1e-5);
        assert_vec3_close(front.point, vec3(0.25, 0.25, 0.0));
        assert_vec3_close(front.normal, Vec3::Z);

        let front_culled = triangle
            .ray_intersect_cull_backface(vec3(0.25, 0.25, 1.0), vec3(0.0, 0.0, -1.0))
            .unwrap();
        assert_vec3_close(front_culled.point, front.point);

        let scaled = triangle
            .ray_intersect(vec3(0.25, 0.25, 1.0), vec3(0.0, 0.0, -2.0))
            .unwrap();
        assert!((scaled.t - 0.5).abs() < 1e-5);
        assert_vec3_close(scaled.point, front.point);
        assert_vec3_close(scaled.normal, Vec3::Z);

        let back = triangle
            .ray_intersect(vec3(0.25, 0.25, -1.0), Vec3::Z)
            .unwrap();
        assert!((back.t - 1.0).abs() < 1e-5);
        assert_vec3_close(back.point, vec3(0.25, 0.25, 0.0));
        assert_vec3_close(back.normal, Vec3::Z);
        assert!(triangle
            .ray_intersect_cull_backface(vec3(0.25, 0.25, -1.0), Vec3::Z)
            .is_none());
    }

    #[test]
    fn ray_includes_edges_and_rejects_misses() {
        let triangle = xy_triangle();
        let edge = triangle
            .ray_intersect(vec3(0.5, 0.0, 2.0), vec3(0.0, 0.0, -1.0))
            .unwrap();
        assert_vec3_close(edge.point, vec3(0.5, 0.0, 0.0));
        let vertex = triangle
            .ray_intersect(vec3(0.0, 0.0, 2.0), vec3(0.0, 0.0, -1.0))
            .unwrap();
        assert_vec3_close(vertex.point, Vec3::ZERO);

        assert!(triangle
            .ray_intersect(vec3(0.8, 0.8, 1.0), vec3(0.0, 0.0, -1.0))
            .is_none());
        assert!(triangle
            .ray_intersect(vec3(0.25, 0.25, 1.0), Vec3::Z)
            .is_none());
        assert!(triangle
            .ray_intersect(vec3(0.25, 0.25, 1.0), Vec3::X)
            .is_none());
        assert!(triangle
            .ray_intersect(vec3(0.25, 0.25, 1.0), Vec3::ZERO)
            .is_none());
        assert!(triangle
            .ray_intersect(vec3(f32::NAN, 0.0, 0.0), -Vec3::Z)
            .is_none());
        assert!(Triangle::new(Vec3::ZERO, Vec3::X, vec3(2.0, 0.0, 0.0))
            .ray_intersect(vec3(0.5, 0.0, 1.0), -Vec3::Z)
            .is_none());

        let small = Triangle::new(Vec3::ZERO, vec3(1e-3, 0.0, 0.0), vec3(0.0, 1e-3, 0.0));
        let tiny = small
            .ray_intersect(vec3(1e-4, 1e-4, 1.0), -Vec3::Z)
            .unwrap();
        assert!((tiny.t - 1.0).abs() < 1e-4);
        assert_vec3_close(tiny.point, vec3(1e-4, 1e-4, 0.0));
    }

    #[test]
    fn ray_from_inside_the_triangle_is_t_zero() {
        let triangle = xy_triangle();
        let on_it = triangle
            .ray_intersect(vec3(0.25, 0.25, 0.0), Vec3::X)
            .unwrap();
        assert_eq!(on_it.t, 0.0);
        assert_eq!(on_it.point, vec3(0.25, 0.25, 0.0));
        assert_vec3_close(on_it.normal, Vec3::Z);

        let stationary = triangle
            .ray_intersect(vec3(0.25, 0.25, 0.0), Vec3::ZERO)
            .unwrap();
        assert_eq!(stationary.t, 0.0);
        assert_vec3_close(stationary.normal, Vec3::Z);

        assert!(triangle
            .ray_intersect_cull_backface(vec3(0.25, 0.25, 0.0), Vec3::Z)
            .is_none());
        let through = triangle
            .ray_intersect_cull_backface(vec3(0.25, 0.25, 0.0), -Vec3::Z)
            .unwrap();
        assert_eq!(through.t, 0.0);
        assert_vec3_close(through.normal, Vec3::Z);
    }
}
