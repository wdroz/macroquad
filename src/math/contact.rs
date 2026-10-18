//! Overlap contacts for the 3D shapes.
//!
//! A [`Contact`] describes one overlapping pair. It does not move either
//! shape and it does not apply an impulse. The normal points from the second
//! shape toward the first, so pushing the first shape along the normal
//! separates it from the second.

use glam::{vec3, Vec3};

use super::{Aabb, Capsule, Sphere};

/// One overlapping pair of 3D shapes.
///
/// `normal` is a unit vector pointing from the second shape toward the first.
/// `penetration` is how far the shapes overlap along that normal. A pair that
/// only touches has penetration `0` and still returns a contact, matching the
/// inclusive boundaries of [`Aabb`] and [`Sphere`]. `point` lies in the
/// overlap: on the shared face when the pair only touches, and inside the
/// overlap otherwise. See each query for the exact point.
///
/// When no direction is defined (the shapes share a center), `normal` is
/// [`Vec3::X`]. An axis-aligned box that can leave along `+` or `-` by the
/// same distance uses the positive axis. When several axes share the smallest
/// penetration, X wins over Y and Y wins over Z.
///
/// # Example
///
/// ```
/// use macroquad::math::{Aabb, Contact, Sphere, vec3};
///
/// let ball = Sphere::new(vec3(0.0, 0.5, 0.0), 0.5);
/// let floor = Aabb::from_min_max(vec3(-1.0, -1.0, -1.0), vec3(1.0, 0.0, 1.0));
/// let contact = ball.contact_aabb(&floor).unwrap();
/// assert_eq!(contact.penetration, 0.0);
/// assert_eq!(contact.normal, macroquad::math::Vec3::Y);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    /// Unit normal from the second shape toward the first.
    pub normal: Vec3,
    /// Overlap along [`Self::normal`]. Zero when the shapes only touch.
    pub penetration: f32,
    /// A point in the overlap. See the query that produced this contact.
    pub point: Vec3,
}

impl Aabb {
    /// Contact between `self` and `other`, or [`None`] when they are separated.
    ///
    /// The normal points from `other` toward `self`. `point` is the center of
    /// the intersection. A shared face, edge, or corner is a contact with
    /// penetration `0`, and that center lies on the shared boundary.
    ///
    /// An inverted box (`min` greater than `max` on an axis) or a non-finite
    /// corner has no contact.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, vec3, Vec3};
    ///
    /// let left = Aabb::from_min_max(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
    /// let right = Aabb::from_min_max(vec3(1.0, 0.0, 0.0), vec3(2.0, 1.0, 1.0));
    /// let contact = left.contact(&right).unwrap();
    /// assert_eq!(contact.penetration, 0.0);
    /// assert_eq!(contact.normal, -Vec3::X);
    /// assert_eq!(contact.point, vec3(1.0, 0.5, 0.5));
    /// ```
    pub fn contact(&self, other: &Aabb) -> Option<Contact> {
        if !box_is_usable(self) || !box_is_usable(other) {
            return None;
        }
        let x = axis_separation(self.min.x, self.max.x, other.min.x, other.max.x)?;
        let y = axis_separation(self.min.y, self.max.y, other.min.y, other.max.y)?;
        let z = axis_separation(self.min.z, self.max.z, other.min.z, other.max.z)?;
        let (axis, sep) = [x, y, z]
            .into_iter()
            .enumerate()
            .min_by(|left, right| {
                left.1
                    .penetration
                    .partial_cmp(&right.1.penetration)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(left.0.cmp(&right.0))
            })
            .unwrap();
        let shared = self.intersect(*other)?;
        Some(Contact {
            normal: axis_normal(axis, sep.positive),
            penetration: sep.penetration,
            point: shared.center(),
        })
    }
}

impl Sphere {
    /// Contact between `self` and `other`, or [`None`] when they are separated.
    ///
    /// The normal points from `other` toward `self`. `point` lies on `other`,
    /// along that normal. A negative or non-finite radius is empty and has no
    /// contact. Coincident centers use [`Vec3::X`].
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Sphere, Vec3, vec3};
    ///
    /// let left = Sphere::new(Vec3::ZERO, 1.0);
    /// let right = Sphere::new(vec3(2.0, 0.0, 0.0), 1.0);
    /// let contact = left.contact(&right).unwrap();
    /// assert_eq!(contact.penetration, 0.0);
    /// assert_eq!(contact.normal, -Vec3::X);
    /// assert_eq!(contact.point, Vec3::X);
    /// ```
    pub fn contact(&self, other: &Sphere) -> Option<Contact> {
        if !sphere_is_usable(self) || !sphere_is_usable(other) {
            return None;
        }
        let delta = self.center - other.center;
        let dist_sq = delta.length_squared();
        let reach = self.radius + other.radius;
        if dist_sq > reach * reach {
            return None;
        }
        let dist = dist_sq.sqrt();
        let normal = unit_or_x(delta, dist);
        Some(Contact {
            normal,
            penetration: reach - dist,
            point: other.center + normal * other.radius,
        })
    }

    /// Contact between this sphere and `aabb`, or [`None`] when they are separated.
    ///
    /// The normal points from the box toward the sphere. `point` is on the
    /// box: the closest point to the center when the center is outside, and
    /// the projection onto the nearest face when the center is inside. A
    /// sphere sitting on a face has penetration `0`.
    ///
    /// An inverted or non-finite box has no contact. A negative or non-finite
    /// radius is empty.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, Sphere, Vec3, vec3};
    ///
    /// let ball = Sphere::new(vec3(0.0, 1.0, 0.0), 1.0);
    /// let floor = Aabb::from_min_max(vec3(-2.0, -1.0, -2.0), vec3(2.0, 0.0, 2.0));
    /// let contact = ball.contact_aabb(&floor).unwrap();
    /// assert_eq!(contact.penetration, 0.0);
    /// assert_eq!(contact.normal, Vec3::Y);
    /// assert_eq!(contact.point, Vec3::ZERO);
    /// ```
    pub fn contact_aabb(&self, aabb: &Aabb) -> Option<Contact> {
        sphere_aabb_contact(self, aabb)
    }
}

impl Capsule {
    /// Contact between this capsule and `sphere`, or [`None`] when they are separated.
    ///
    /// The capsule is the sphere of its radius centered on the closest point
    /// of the inner segment. The normal points from `sphere` toward that
    /// center. `point` lies on `sphere`. A negative or non-finite radius on
    /// either shape is empty.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Capsule, Sphere, Vec3, vec3};
    ///
    /// let body = Capsule::new(Vec3::ZERO, vec3(0.0, 2.0, 0.0), 0.5);
    /// let ball = Sphere::new(vec3(1.0, 1.0, 0.0), 0.5);
    /// let contact = body.contact_sphere(&ball).unwrap();
    /// assert_eq!(contact.penetration, 0.0);
    /// assert_eq!(contact.normal, -Vec3::X);
    /// ```
    pub fn contact_sphere(&self, sphere: &Sphere) -> Option<Contact> {
        if !capsule_is_usable(self) || !sphere_is_usable(sphere) {
            return None;
        }
        let center = closest_on_segment(self.a, self.b, sphere.center);
        Sphere::new(center, self.radius).contact(sphere)
    }

    /// Contact between this capsule and `aabb`, or [`None`] when they are separated.
    ///
    /// The normal points from the box toward the capsule. `point` is on the
    /// box. A capsule whose rounded hull only touches a face has penetration
    /// `0`. When the inner segment intersects the box, the normal is a face
    /// normal and the penetration is large enough to move the capsule onto
    /// that face. A long segment through a thin pillar is pushed out the
    /// short side, not along the segment. An inverted or non-finite box has
    /// no contact. A negative or non-finite radius is empty.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, Capsule, Vec3, vec3};
    ///
    /// let body = Capsule::new(vec3(0.0, 0.5, 0.0), vec3(0.0, 1.5, 0.0), 0.5);
    /// let floor = Aabb::from_min_max(vec3(-1.0, -1.0, -1.0), vec3(1.0, 0.0, 1.0));
    /// let contact = body.contact_aabb(&floor).unwrap();
    /// assert_eq!(contact.penetration, 0.0);
    /// assert_eq!(contact.normal, Vec3::Y);
    /// ```
    pub fn contact_aabb(&self, aabb: &Aabb) -> Option<Contact> {
        if !capsule_is_usable(self) || !box_is_usable(aabb) {
            return None;
        }
        if segment_inside(self.a, self.b, aabb).is_some() {
            return Some(segment_inside_contact(self, aabb));
        }
        let (on_segment, on_box, dist_sq) = closest_segment_box(self.a, self.b, aabb);
        let dist = dist_sq.sqrt();
        if dist > self.radius {
            return None;
        }
        let normal = unit_or_x(on_segment - on_box, dist);
        Some(Contact {
            normal,
            penetration: self.radius - dist,
            point: on_box,
        })
    }
}

fn sphere_aabb_contact(sphere: &Sphere, aabb: &Aabb) -> Option<Contact> {
    if !sphere_is_usable(sphere) || !box_is_usable(aabb) {
        return None;
    }
    if aabb.contains(sphere.center) {
        let (normal, depth, point) = interior_face(sphere.center, aabb);
        return Some(Contact {
            normal,
            penetration: sphere.radius + depth,
            point,
        });
    }
    let closest = aabb.closest_point(sphere.center);
    let delta = sphere.center - closest;
    let dist_sq = delta.length_squared();
    if dist_sq > sphere.radius * sphere.radius {
        return None;
    }
    let dist = dist_sq.sqrt();
    Some(Contact {
        normal: unit_or_x(delta, dist),
        penetration: sphere.radius - dist,
        point: closest,
    })
}

fn sphere_is_usable(sphere: &Sphere) -> bool {
    sphere.radius.is_finite() && sphere.radius >= 0.0 && sphere.center.is_finite()
}

fn capsule_is_usable(capsule: &Capsule) -> bool {
    capsule.radius.is_finite()
        && capsule.radius >= 0.0
        && capsule.a.is_finite()
        && capsule.b.is_finite()
}

fn box_is_usable(aabb: &Aabb) -> bool {
    aabb.min.is_finite()
        && aabb.max.is_finite()
        && aabb.min.x <= aabb.max.x
        && aabb.min.y <= aabb.max.y
        && aabb.min.z <= aabb.max.z
}

struct AxisSep {
    penetration: f32,
    positive: bool,
}

fn axis_separation(a_min: f32, a_max: f32, b_min: f32, b_max: f32) -> Option<AxisSep> {
    let push_pos = b_max - a_min;
    let push_neg = a_max - b_min;
    if push_pos < 0.0 || push_neg < 0.0 {
        return None;
    }
    // Equal exits use the positive axis. `push_neg < push_pos` is the only
    // case that leaves through the negative side.
    if push_neg < push_pos {
        Some(AxisSep {
            penetration: push_neg,
            positive: false,
        })
    } else {
        Some(AxisSep {
            penetration: push_pos,
            positive: true,
        })
    }
}

fn axis_normal(axis: usize, positive: bool) -> Vec3 {
    let normal = match axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    };
    if positive {
        normal
    } else {
        -normal
    }
}

fn unit_or_x(delta: Vec3, length: f32) -> Vec3 {
    if length > 0.0 && delta.is_finite() {
        delta / length
    } else {
        Vec3::X
    }
}

/// Outward normal of the nearest face, the depth, and the point on that face.
fn interior_face(center: Vec3, aabb: &Aabb) -> (Vec3, f32, Vec3) {
    let (normal, point) = nearest_face(center, aabb);
    let depth = (center - point).length();
    (normal, depth, point)
}

fn nearest_face(center: Vec3, aabb: &Aabb) -> (Vec3, Vec3) {
    // Positive faces come first so an exact tie uses +X, then +Y, then +Z.
    let candidates = [
        (aabb.max.x, aabb.max.x - center.x, Vec3::X, 0),
        (aabb.min.x, center.x - aabb.min.x, -Vec3::X, 0),
        (aabb.max.y, aabb.max.y - center.y, Vec3::Y, 1),
        (aabb.min.y, center.y - aabb.min.y, -Vec3::Y, 1),
        (aabb.max.z, aabb.max.z - center.z, Vec3::Z, 2),
        (aabb.min.z, center.z - aabb.min.z, -Vec3::Z, 2),
    ];
    let mut best = &candidates[0];
    for candidate in &candidates[1..] {
        if candidate.1 < best.1 {
            best = candidate;
        }
    }
    let mut point = center;
    match best.3 {
        0 => point.x = best.0,
        1 => point.y = best.0,
        _ => point.z = best.0,
    }
    (best.2, point)
}

fn closest_on_segment(a: Vec3, b: Vec3, point: Vec3) -> Vec3 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq == 0.0 {
        return a;
    }
    let t = ((point - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    a + ab * t
}

/// Inclusive slab clip. `Some` when the segment meets the closed box.
fn segment_inside(a: Vec3, b: Vec3, aabb: &Aabb) -> Option<(f32, f32)> {
    let origin = [a.x, a.y, a.z];
    let delta = [b.x - a.x, b.y - a.y, b.z - a.z];
    let lower = [aabb.min.x, aabb.min.y, aabb.min.z];
    let upper = [aabb.max.x, aabb.max.y, aabb.max.z];
    let mut t_min = 0.0;
    let mut t_max = 1.0;
    for i in 0..3 {
        if delta[i] == 0.0 {
            if origin[i] < lower[i] || origin[i] > upper[i] {
                return None;
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
                return None;
            }
        }
    }
    Some((t_min, t_max))
}

fn point_at(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

/// Face-axis separation for a capsule whose inner segment meets the box.
///
/// Pushing the capsule by `penetration` along `normal` lands it on that face.
/// The shortest of the six face axes wins. Equal lengths keep X, then Y, then
/// Z, and a center that sits on the axis uses the positive direction.
fn segment_inside_contact(capsule: &Capsule, aabb: &Aabb) -> Contact {
    let center = (capsule.a + capsule.b) * 0.5;
    let half = (capsule.b - capsule.a) * 0.5;
    let box_center = aabb.center();
    let box_half = aabb.half_extents();
    let delta = center - box_center;
    let mut best_pen = f32::INFINITY;
    let mut best_axis = 0;
    let mut best_positive = true;
    for axis in 0..3 {
        let along = component(half, axis).abs() + capsule.radius;
        let extent = component(box_half, axis);
        let dist = component(delta, axis);
        let pen = extent + along - dist.abs();
        if pen < best_pen {
            best_pen = pen;
            best_axis = axis;
            best_positive = dist >= 0.0;
        }
    }
    Contact {
        normal: axis_normal(best_axis, best_positive),
        penetration: best_pen.max(0.0),
        point: point_on_face(center, aabb, best_axis, best_positive),
    }
}

fn point_on_face(center: Vec3, aabb: &Aabb, axis: usize, positive: bool) -> Vec3 {
    let mut point = vec3(
        center.x.clamp(aabb.min.x, aabb.max.x),
        center.y.clamp(aabb.min.y, aabb.max.y),
        center.z.clamp(aabb.min.z, aabb.max.z),
    );
    let face = if positive {
        component(aabb.max, axis)
    } else {
        component(aabb.min, axis)
    };
    set_component(&mut point, axis, face);
    point
}

fn closest_segment_box(a: Vec3, b: Vec3, aabb: &Aabb) -> (Vec3, Vec3, f32) {
    let min = [aabb.min.x, aabb.min.y, aabb.min.z];
    let max = [aabb.max.x, aabb.max.y, aabb.max.z];
    let mut best_on_segment = a;
    let mut best_on_box = aabb.closest_point(a);
    let mut best_sq = (best_on_segment - best_on_box).length_squared();
    for axis in 0..3 {
        for plane in [min[axis], max[axis]] {
            let (on_segment, on_box, dist_sq) = closest_segment_rect(a, b, axis, plane, min, max);
            if dist_sq < best_sq {
                best_sq = dist_sq;
                best_on_segment = on_segment;
                best_on_box = on_box;
            }
        }
    }
    (best_on_segment, best_on_box, best_sq)
}

fn closest_segment_rect(
    a: Vec3,
    b: Vec3,
    axis: usize,
    plane: f32,
    min: [f32; 3],
    max: [f32; 3],
) -> (Vec3, Vec3, f32) {
    let mut best_on_segment = a;
    let mut best_on_rect = project_to_rect(a, axis, plane, min, max);
    let mut best_sq = (best_on_segment - best_on_rect).length_squared();
    let mut keep = |on_segment: Vec3, on_rect: Vec3| {
        let dist_sq = (on_segment - on_rect).length_squared();
        if dist_sq < best_sq {
            best_sq = dist_sq;
            best_on_segment = on_segment;
            best_on_rect = on_rect;
        }
    };
    keep(b, project_to_rect(b, axis, plane, min, max));
    let a_axis = component(a, axis);
    let delta_axis = component(b, axis) - a_axis;
    if delta_axis.abs() > f32::EPSILON {
        let t = ((plane - a_axis) / delta_axis).clamp(0.0, 1.0);
        let on_segment = point_at(a, b, t);
        if rect_contains(on_segment, axis, min, max) {
            let mut on_rect = on_segment;
            set_component(&mut on_rect, axis, plane);
            keep(on_segment, on_rect);
        }
    }
    let corners = face_corners(axis, plane, min, max);
    for i in 0..4 {
        let (on_segment, on_edge) = closest_segment_segment(a, b, corners[i], corners[(i + 1) % 4]);
        keep(on_segment, on_edge);
    }
    (best_on_segment, best_on_rect, best_sq)
}

fn project_to_rect(point: Vec3, axis: usize, plane: f32, min: [f32; 3], max: [f32; 3]) -> Vec3 {
    let mut closest = [point.x, point.y, point.z];
    closest[axis] = plane;
    for i in 0..3 {
        if i != axis {
            closest[i] = closest[i].clamp(min[i], max[i]);
        }
    }
    vec3(closest[0], closest[1], closest[2])
}

fn rect_contains(point: Vec3, axis: usize, min: [f32; 3], max: [f32; 3]) -> bool {
    for i in 0..3 {
        if i != axis {
            let value = component(point, i);
            if value < min[i] || value > max[i] {
                return false;
            }
        }
    }
    true
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
        p[u] = u_coord[i];
        p[v] = v_coord[i];
        corners[i] = vec3(p[0], p[1], p[2]);
    }
    corners
}

fn closest_segment_segment(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (Vec3, Vec3) {
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
            let mut s = if denom.abs() > f32::EPSILON {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    (p1 + d1 * s, p2 + d2 * t)
}

fn component(point: Vec3, axis: usize) -> f32 {
    match axis {
        0 => point.x,
        1 => point.y,
        _ => point.z,
    }
}

fn set_component(point: &mut Vec3, axis: usize, value: f32) {
    match axis {
        0 => point.x = value,
        1 => point.y = value,
        _ => point.z = value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec3_close(left: Vec3, right: Vec3) {
        let delta = left - right;
        assert!(
            delta.x.abs() < 1e-4 && delta.y.abs() < 1e-4 && delta.z.abs() < 1e-4,
            "left {left:?} right {right:?}"
        );
    }

    fn floor() -> Aabb {
        Aabb::from_min_max(vec3(-2.0, -1.0, -2.0), vec3(2.0, 0.0, 2.0))
    }

    #[test]
    fn a_sphere_sitting_on_a_floor_is_a_contact() {
        let ball = Sphere::new(vec3(0.0, 0.5, 0.0), 0.5);
        let contact = ball.contact_aabb(&floor()).unwrap();
        assert_eq!(contact.penetration, 0.0);
        assert_eq!(contact.normal, Vec3::Y);
        assert_vec3_close(contact.point, Vec3::ZERO);

        let sunk = Sphere::new(vec3(0.0, 0.25, 0.0), 0.5);
        let contact = sunk.contact_aabb(&floor()).unwrap();
        assert!((contact.penetration - 0.25).abs() < 1e-5);
        assert_eq!(contact.normal, Vec3::Y);

        assert!(Sphere::new(vec3(0.0, 0.51, 0.0), 0.5)
            .contact_aabb(&floor())
            .is_none());
        assert!(Sphere::new(Vec3::ZERO, -1.0)
            .contact_aabb(&floor())
            .is_none());
        assert!(Sphere::new(vec3(0.0, 1.0, 0.0), f32::NAN)
            .contact_aabb(&floor())
            .is_none());
    }

    #[test]
    fn sphere_centers_report_the_second_surface() {
        let left = Sphere::new(Vec3::ZERO, 1.0);
        let right = Sphere::new(vec3(1.0, 0.0, 0.0), 1.0);
        let contact = left.contact(&right).unwrap();
        assert!((contact.penetration - 1.0).abs() < 1e-5);
        assert_eq!(contact.normal, -Vec3::X);
        assert_vec3_close(contact.point, Vec3::ZERO);

        let touching = Sphere::new(vec3(2.0, 0.0, 0.0), 1.0);
        let contact = left.contact(&touching).unwrap();
        assert_eq!(contact.penetration, 0.0);
        assert!(left
            .contact(&Sphere::new(vec3(2.01, 0.0, 0.0), 1.0))
            .is_none());

        let same = left.contact(&Sphere::new(Vec3::ZERO, 1.0)).unwrap();
        assert_eq!(same.normal, Vec3::X);
        assert!((same.penetration - 2.0).abs() < 1e-5);

        assert!(left.contact(&Sphere::new(Vec3::X, -0.5)).is_none());
        assert!(Sphere::new(Vec3::ZERO, f32::INFINITY)
            .contact(&left)
            .is_none());
    }

    #[test]
    fn boxes_touch_on_a_face_and_separate_past_it() {
        let left = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
        let right = Aabb::from_min_max(vec3(1.0, 0.0, 0.0), vec3(2.0, 1.0, 1.0));
        let contact = left.contact(&right).unwrap();
        assert_eq!(contact.penetration, 0.0);
        assert_eq!(contact.normal, -Vec3::X);
        assert_vec3_close(contact.point, vec3(1.0, 0.5, 0.5));
        assert!(left.contact(&right.offset(vec3(0.01, 0.0, 0.0))).is_none());

        let overlap = Aabb::from_min_max(vec3(0.5, 0.0, 0.0), vec3(1.5, 1.0, 1.0));
        let contact = left.contact(&overlap).unwrap();
        assert!((contact.penetration - 0.5).abs() < 1e-5);
        assert_eq!(contact.normal, -Vec3::X);

        let shell = Aabb::from_min_max(Vec3::ZERO, vec3(10.0, 10.0, 10.0));
        let inner = Aabb::from_min_max(vec3(8.0, 4.0, 4.0), vec3(9.0, 6.0, 6.0));
        let contact = inner.contact(&shell).unwrap();
        assert!((contact.penetration - 2.0).abs() < 1e-4);
        assert_eq!(contact.normal, Vec3::X);

        let inverted = Aabb::from_min_max(Vec3::ONE, Vec3::ZERO);
        assert!(left.contact(&inverted).is_none());
    }

    #[test]
    fn equal_box_penetrations_prefer_x() {
        let box_ = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
        let other = Aabb::from_min_max(vec3(0.75, 0.75, -10.0), vec3(2.0, 2.0, 10.0));
        let contact = box_.contact(&other).unwrap();
        assert!((contact.penetration - 0.25).abs() < 1e-5);
        assert_eq!(contact.normal, -Vec3::X);
    }

    #[test]
    fn a_capsule_on_a_floor_and_against_a_sphere() {
        let body = Capsule::new(vec3(0.0, 0.5, 0.0), vec3(0.0, 1.5, 0.0), 0.5);
        let contact = body.contact_aabb(&floor()).unwrap();
        assert!((contact.penetration).abs() < 1e-4);
        assert_eq!(contact.normal, Vec3::Y);
        assert_vec3_close(contact.point, Vec3::ZERO);

        let sunk = Capsule::new(vec3(0.0, 0.3, 0.0), vec3(0.0, 1.3, 0.0), 0.5);
        let contact = sunk.contact_aabb(&floor()).unwrap();
        assert!((contact.penetration - 0.2).abs() < 1e-4);
        assert_eq!(contact.normal, Vec3::Y);

        let buried = Capsule::new(vec3(0.0, -0.2, 0.0), vec3(0.0, 1.0, 0.0), 0.3);
        let contact = buried.contact_aabb(&floor()).unwrap();
        assert!((contact.penetration - 0.5).abs() < 1e-4);
        assert_eq!(contact.normal, Vec3::Y);

        let crossing = Capsule::new(vec3(0.0, -0.25, -5.0), vec3(0.0, -0.25, 5.0), 0.1);
        let contact = crossing.contact_aabb(&floor()).unwrap();
        assert!((contact.penetration - 0.35).abs() < 1e-3);
        assert_eq!(contact.normal, Vec3::Y);
        assert!(contact.point.y.abs() < 1e-4);
        assert!(contact.point.z.abs() <= 2.0);

        assert!(Capsule::new(vec3(0.0, 1.0, 0.0), vec3(0.0, 2.0, 0.0), 0.5)
            .contact_aabb(&floor())
            .is_none());
        assert!(Capsule::new(Vec3::ZERO, Vec3::Y, -1.0)
            .contact_aabb(&floor())
            .is_none());

        let ball = Sphere::new(vec3(1.0, 1.0, 0.0), 0.5);
        let contact = body.contact_sphere(&ball).unwrap();
        assert!((contact.penetration).abs() < 1e-4);
        assert_eq!(contact.normal, -Vec3::X);
        assert_vec3_close(contact.point, vec3(0.5, 1.0, 0.0));
        assert!(body
            .contact_sphere(&Sphere::new(vec3(1.1, 1.0, 0.0), 0.5))
            .is_none());
        assert!(body
            .contact_sphere(&Sphere::new(Vec3::ZERO, f32::NAN))
            .is_none());
    }

    #[test]
    fn a_capsule_through_a_thin_pillar_exits_on_the_short_side() {
        let pillar = Aabb::from_center_size(vec3(0.0, 5.0, 0.0), vec3(0.2, 10.0, 0.2));
        let body = Capsule::new(vec3(-5.0, 5.0, 0.0), vec3(5.0, 5.0, 0.0), 0.3);
        let contact = body.contact_aabb(&pillar).unwrap();
        assert_eq!(contact.normal, Vec3::Z);
        assert!((contact.penetration - 0.4).abs() < 1e-3);
        // The point sits on the pillar face the normal pushes away from.
        assert_vec3_close(contact.point, vec3(0.0, 5.0, 0.1));
        let moved = Capsule::new(
            body.a + contact.normal * contact.penetration,
            body.b + contact.normal * contact.penetration,
            body.radius,
        );
        let resting = moved.contact_aabb(&pillar).unwrap();
        assert!(
            resting.penetration < 1e-3,
            "still inside by {}",
            resting.penetration
        );
    }

    #[test]
    fn a_sphere_inside_a_box_uses_the_nearest_face() {
        let ball = Sphere::new(vec3(0.2, 0.0, 0.0), 0.5);
        let shell = Aabb::from_min_max(vec3(-1.0, -1.0, -1.0), Vec3::ONE);
        let contact = ball.contact_aabb(&shell).unwrap();
        assert_eq!(contact.normal, Vec3::X);
        assert!((contact.penetration - 1.3).abs() < 1e-4);
        assert_vec3_close(contact.point, vec3(1.0, 0.0, 0.0));

        let centered = Sphere::new(Vec3::ZERO, 0.25).contact_aabb(&shell).unwrap();
        assert_eq!(centered.normal, Vec3::X);
    }
}
