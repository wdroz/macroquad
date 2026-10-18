//! Continuous overlap of a moving box or sphere against a static box.
//!
//! [`Aabb`] and [`Sphere`] overlap tests are inclusive, so a shape sitting on
//! a floor already counts as touching. A sweep along a delta that does not
//! move further into that floor is not an impact. Callers keep the real size
//! and radius; this query does not ask for a smaller one.

use glam::{vec3, Vec3};

use super::{Aabb, Contact, Sphere};

/// Overlap deeper than this is "clearly inside". A shallower overlap, including
/// a pure touch, still lets a sideways delta pass.
const TOUCH_SLOP: f32 = 1e-4;
/// Motion into a surface below this is numeric noise, not a new impact.
const INTO_SLOP: f32 = 1e-5;

/// First impact of a shape moving in a straight line against a static box.
///
/// `t` is the fraction of `delta` at which the shapes meet, in `0..=1`.
/// `normal` is a unit vector pointing out of the static box, toward the moving
/// shape. `point` lies on the static box: the center of the touching region for
/// two boxes, and the closest point on the box for a sphere.
///
/// A pair that is already overlapping by more than `1e-4` reports `t == 0`.
/// A pair that only touches, or overlaps by less than that, reports `t == 0`
/// only when `delta` moves further into the static box. Otherwise the sweep
/// misses, so a sphere resting on a floor can still move sideways.
///
/// # Example
///
/// ```
/// use macroquad::math::{Aabb, vec3, Vec3};
///
/// let floor = Aabb::from_min_max(vec3(-1.0, -1.0, -1.0), vec3(1.0, 0.0, 1.0));
/// let body = Aabb::from_min_max(vec3(-0.25, 0.0, -0.25), vec3(0.25, 1.0, 0.25));
/// assert!(body.sweep(vec3(1.0, 0.0, 0.0), &floor).is_none());
/// let into = body.sweep(vec3(0.0, -0.5, 0.0), &floor).unwrap();
/// assert_eq!(into.t, 0.0);
/// assert_eq!(into.normal, Vec3::Y);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepHit {
    /// Fraction of the delta where the shapes first meet, in `0..=1`.
    pub t: f32,
    /// Unit normal pointing out of the static box.
    pub normal: Vec3,
    /// A point on the static box at the impact.
    pub point: Vec3,
}

impl Aabb {
    /// Sweeps this box along `delta` against the static `other`.
    ///
    /// `t` is in `0..=1`. The normal points out of `other`. A path that misses
    /// returns [`None`], including a box that only touches `other` and moves
    /// along or away from that face. A box that starts clearly inside `other`
    /// returns `t == 0` even when `delta` points back out.
    ///
    /// An inverted box (`min` greater than `max` on an axis) and a non-finite
    /// corner or `delta` miss. Equal entry times keep the earlier axis, X
    /// before Y before Z.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, vec3, Vec3};
    ///
    /// let wall = Aabb::from_min_max(vec3(2.0, 0.0, 0.0), vec3(3.0, 1.0, 1.0));
    /// let body = Aabb::from_min_max(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
    /// let hit = body.sweep(vec3(2.0, 0.0, 0.0), &wall).unwrap();
    /// assert_eq!(hit.t, 0.5);
    /// assert_eq!(hit.normal, -Vec3::X);
    /// ```
    pub fn sweep(&self, delta: Vec3, other: &Aabb) -> Option<SweepHit> {
        if !delta.is_finite() || !box_usable(self) || !box_usable(other) {
            return None;
        }
        let overlaps = [
            axis_overlap(self.min.x, self.max.x, other.min.x, other.max.x),
            axis_overlap(self.min.y, self.max.y, other.min.y, other.max.y),
            axis_overlap(self.min.z, self.max.z, other.min.z, other.max.z),
        ];
        let separated = overlaps.iter().any(|overlap| *overlap < -TOUCH_SLOP);
        if !separated {
            if let Some(contact) = self.contact(other) {
                if contact.penetration > TOUCH_SLOP {
                    return Some(hit_at_start(contact));
                }
                return touching_box(self, other, delta, contact);
            }
        }
        sweep_separated_box(self, delta, other)
    }
}

impl Sphere {
    /// Sweeps this sphere along `delta` against the static `aabb`.
    ///
    /// `t` is in `0..=1`. The normal points out of `aabb`. A path that misses
    /// returns [`None`], including a sphere that only touches the box and moves
    /// along or away from it. A sphere that starts clearly inside the box
    /// returns `t == 0`. Faces, edges, and corners all count; a hit on a corner
    /// uses the direction from that corner to the center.
    ///
    /// A negative or non-finite radius is empty and misses. An inverted or
    /// non-finite box misses, and so does a non-finite center or `delta`.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, Sphere, vec3, Vec3};
    ///
    /// let wall = Aabb::from_min_max(vec3(0.0, -1.0, -1.0), vec3(1.0, 1.0, 1.0));
    /// let ball = Sphere::new(vec3(-2.0, 0.0, 0.0), 0.5);
    /// let hit = ball.sweep_aabb(vec3(4.0, 0.0, 0.0), &wall).unwrap();
    /// assert!((hit.t - 0.375).abs() < 1e-4);
    /// assert_eq!(hit.normal, -Vec3::X);
    /// ```
    pub fn sweep_aabb(&self, delta: Vec3, aabb: &Aabb) -> Option<SweepHit> {
        if !delta.is_finite() || !self.center.is_finite() || !box_usable(aabb) {
            return None;
        }
        if !self.radius.is_finite() || self.radius < 0.0 {
            return None;
        }
        if let Some(contact) = self.contact_aabb(aabb) {
            if contact.penetration > TOUCH_SLOP || delta.dot(contact.normal) < -INTO_SLOP {
                return Some(hit_at_start(contact));
            }
            // Touching a convex box and not moving into it cannot meet again.
            return None;
        }
        sweep_separated_sphere(self, delta, aabb)
    }
}

fn hit_at_start(contact: Contact) -> SweepHit {
    SweepHit {
        t: 0.0,
        normal: contact.normal,
        point: contact.point,
    }
}

/// A box that already touches `other`. `None` when `delta` does not enter.
fn touching_box(moving: &Aabb, other: &Aabb, delta: Vec3, contact: Contact) -> Option<SweepHit> {
    let mut best: Option<SweepHit> = None;
    for axis in 0..3 {
        let overlap = axis_overlap(
            component(moving.min, axis),
            component(moving.max, axis),
            component(other.min, axis),
            component(other.max, axis),
        );
        if overlap > TOUCH_SLOP {
            continue;
        }
        match face_side(moving, other, axis) {
            FaceSide::Positive => consider_touch(&mut best, axis_normal(axis, 1.0), delta, contact),
            FaceSide::Negative => {
                consider_touch(&mut best, axis_normal(axis, -1.0), delta, contact)
            }
            FaceSide::Both => {
                let velocity = component(delta, axis);
                if velocity > INTO_SLOP {
                    consider_touch(&mut best, axis_normal(axis, -1.0), delta, contact);
                } else if velocity < -INTO_SLOP {
                    consider_touch(&mut best, axis_normal(axis, 1.0), delta, contact);
                }
            }
            FaceSide::Neither => {}
        }
    }
    best
}

fn consider_touch(best: &mut Option<SweepHit>, normal: Vec3, delta: Vec3, contact: Contact) {
    if delta.dot(normal) >= -INTO_SLOP {
        return;
    }
    let hit = SweepHit {
        t: 0.0,
        normal,
        point: contact.point,
    };
    let replace = match *best {
        None => true,
        Some(current) => hit.normal.dot(delta) < current.normal.dot(delta),
    };
    if replace {
        *best = Some(hit);
    }
}

fn sweep_separated_box(moving: &Aabb, delta: Vec3, other: &Aabb) -> Option<SweepHit> {
    let mut entered: Option<(f32, Vec3)> = None;
    let mut t_exit = 1.0_f32;
    for axis in 0..3 {
        let velocity = component(delta, axis);
        let moving_min = component(moving.min, axis);
        let moving_max = component(moving.max, axis);
        let static_min = component(other.min, axis);
        let static_max = component(other.max, axis);
        let overlap = axis_overlap(moving_min, moving_max, static_min, static_max);
        if velocity == 0.0 {
            if overlap < -TOUCH_SLOP {
                return None;
            }
            continue;
        }
        let inv = 1.0 / velocity;
        let mut t_near = (static_min - moving_max) * inv;
        let mut t_far = (static_max - moving_min) * inv;
        let mut near_normal = axis_normal(axis, -1.0);
        if t_near > t_far {
            std::mem::swap(&mut t_near, &mut t_far);
            near_normal = axis_normal(axis, 1.0);
        }
        if !t_near.is_finite() || !t_far.is_finite() {
            return None;
        }
        let replace = match entered {
            None => true,
            Some((time, _)) => t_near > time,
        };
        if replace {
            entered = Some((t_near, near_normal));
        }
        if t_far < t_exit {
            t_exit = t_far;
        }
        let t_enter = entered.map(|(time, _)| time).unwrap_or(0.0);
        if t_enter > t_exit + TOUCH_SLOP {
            return None;
        }
    }

    let (t_enter, normal) = entered?;
    if t_exit < -TOUCH_SLOP || t_enter > 1.0 + TOUCH_SLOP {
        return None;
    }
    if t_enter < -TOUCH_SLOP {
        // The slabs say the boxes already overlap. `contact` missed it by a
        // rounding error; report the start instead of letting the move tunnel.
        let recovered = moving.contact(other).unwrap_or(Contact {
            normal,
            penetration: 0.0,
            point: overlap_center(moving, Vec3::ZERO, 0.0, other)?,
        });
        return Some(hit_at_start(recovered));
    }
    let t = t_enter.clamp(0.0, 1.0);
    if delta.dot(normal) >= -INTO_SLOP {
        return None;
    }
    Some(SweepHit {
        t,
        normal,
        point: overlap_center(moving, delta, t, other)?,
    })
}

fn overlap_center(moving: &Aabb, delta: Vec3, t: f32, other: &Aabb) -> Option<Vec3> {
    let shift = delta * t;
    let min = vec3(
        (moving.min.x + shift.x).max(other.min.x),
        (moving.min.y + shift.y).max(other.min.y),
        (moving.min.z + shift.z).max(other.min.z),
    );
    let max = vec3(
        (moving.max.x + shift.x).min(other.max.x),
        (moving.max.y + shift.y).min(other.max.y),
        (moving.max.z + shift.z).min(other.max.z),
    );
    let center = (min + max) * 0.5;
    if center.is_finite() {
        Some(center)
    } else {
        None
    }
}

/// Faces, edges, and corners. [`slide_sphere`](super::slide_sphere) casts the
/// same features, but it pushes a center that starts inside out before moving.
/// A sweep reports that start as `t == 0`, so the two queries stay separate.
fn sweep_separated_sphere(sphere: &Sphere, delta: Vec3, aabb: &Aabb) -> Option<SweepHit> {
    let mut best: Option<SweepHit> = None;
    for axis in 0..3 {
        consider_sphere(
            &mut best,
            cast_face(sphere, delta, aabb, axis, false),
            delta,
        );
        consider_sphere(&mut best, cast_face(sphere, delta, aabb, axis, true), delta);
    }
    for edge in box_edges(aabb) {
        consider_sphere(&mut best, cast_edge(sphere, delta, edge[0], edge[1]), delta);
    }
    for corner in 0..8 {
        consider_sphere(
            &mut best,
            cast_corner(sphere, delta, box_corner(aabb, corner)),
            delta,
        );
    }
    best
}

fn consider_sphere(best: &mut Option<SweepHit>, hit: Option<SweepHit>, delta: Vec3) {
    let Some(hit) = hit else {
        return;
    };
    if !hit.t.is_finite() || !(0.0..=1.0).contains(&hit.t) || !hit.point.is_finite() {
        return;
    }
    if !hit.normal.is_finite() || (hit.normal.length_squared() - 1.0).abs() > 1e-3 {
        return;
    }
    if delta.dot(hit.normal) >= -INTO_SLOP {
        return;
    }
    let replace = match *best {
        None => true,
        Some(current) => {
            if hit.t < current.t - INTO_SLOP {
                true
            } else if hit.t > current.t + INTO_SLOP {
                false
            } else {
                hit.normal.dot(delta) < current.normal.dot(delta)
            }
        }
    };
    if replace {
        *best = Some(hit);
    }
}

fn cast_face(
    sphere: &Sphere,
    delta: Vec3,
    aabb: &Aabb,
    axis: usize,
    max_side: bool,
) -> Option<SweepHit> {
    let (plane, outward) = if max_side {
        (component(aabb.max, axis), 1.0)
    } else {
        (component(aabb.min, axis), -1.0)
    };
    let into_plane = outward * component(delta, axis);
    if into_plane >= -INTO_SLOP {
        return None;
    }
    let dist = outward * (component(sphere.center, axis) - plane) - sphere.radius;
    if dist < -TOUCH_SLOP {
        return None;
    }
    let t = if dist <= INTO_SLOP {
        0.0
    } else {
        -dist / into_plane
    };
    if !t.is_finite() || t > 1.0 {
        return None;
    }
    let t = t.max(0.0);
    let center = sphere.center + delta * t;
    for other in 0..3 {
        if other == axis {
            continue;
        }
        let coord = component(center, other);
        if coord < component(aabb.min, other) - TOUCH_SLOP
            || coord > component(aabb.max, other) + TOUCH_SLOP
        {
            return None;
        }
    }
    let mut point = center;
    match axis {
        0 => point.x = plane,
        1 => point.y = plane,
        _ => point.z = plane,
    }
    Some(SweepHit {
        t,
        normal: axis_normal(axis, outward),
        point,
    })
}

fn cast_edge(sphere: &Sphere, delta: Vec3, start: Vec3, end: Vec3) -> Option<SweepHit> {
    let edge = end - start;
    let length_sq = edge.length_squared();
    if length_sq <= TOUCH_SLOP * TOUCH_SLOP {
        return None;
    }
    let length = length_sq.sqrt();
    let axis = edge / length;
    let offset = sphere.center - start;
    let delta_perp = delta - axis * delta.dot(axis);
    let offset_perp = offset - axis * offset.dot(axis);
    let a = delta_perp.length_squared();
    if a <= 1e-12 {
        return None;
    }
    let b = 2.0 * offset_perp.dot(delta_perp);
    let c = offset_perp.length_squared() - sphere.radius * sphere.radius;
    let t = quadratic_entry(a, b, c)?;
    if !t.is_finite() || t > 1.0 || t < -TOUCH_SLOP {
        return None;
    }
    let t = t.max(0.0);
    let at = sphere.center + delta * t;
    let along = (at - start).dot(axis);
    if along < -TOUCH_SLOP || along > length + TOUCH_SLOP {
        return None;
    }
    let along = along.clamp(0.0, length);
    let point = start + axis * along;
    let normal = (at - point).normalize_or_zero();
    if normal.length_squared() < 1.0 - 1e-3 {
        return None;
    }
    Some(SweepHit { t, normal, point })
}

fn cast_corner(sphere: &Sphere, delta: Vec3, corner: Vec3) -> Option<SweepHit> {
    let offset = sphere.center - corner;
    let a = delta.length_squared();
    if a <= 1e-12 {
        return None;
    }
    let b = 2.0 * offset.dot(delta);
    let c = offset.length_squared() - sphere.radius * sphere.radius;
    let t = quadratic_entry(a, b, c)?;
    if !t.is_finite() || t > 1.0 || t < -TOUCH_SLOP {
        return None;
    }
    let t = t.max(0.0);
    let normal = (sphere.center + delta * t - corner).normalize_or_zero();
    if normal.length_squared() < 1.0 - 1e-3 {
        return None;
    }
    Some(SweepHit {
        t,
        normal,
        point: corner,
    })
}

/// Smaller root of `a t^2 + b t + c = 0`. `a` must be positive.
fn quadratic_entry(a: f32, b: f32, c: f32) -> Option<f32> {
    let disc = b * b - 4.0 * a * c;
    if !disc.is_finite() || disc < -1e-6 {
        return None;
    }
    let sqrt_disc = disc.max(0.0).sqrt();
    let t = (-b - sqrt_disc) / (2.0 * a);
    if t.is_finite() {
        Some(t)
    } else {
        None
    }
}

fn axis_overlap(min_a: f32, max_a: f32, min_b: f32, max_b: f32) -> f32 {
    max_a.min(max_b) - min_a.max(min_b)
}

fn component(value: Vec3, axis: usize) -> f32 {
    match axis {
        0 => value.x,
        1 => value.y,
        _ => value.z,
    }
}

fn axis_normal(axis: usize, sign: f32) -> Vec3 {
    match axis {
        0 => vec3(sign, 0.0, 0.0),
        1 => vec3(0.0, sign, 0.0),
        _ => vec3(0.0, 0.0, sign),
    }
}

#[derive(Clone, Copy)]
enum FaceSide {
    Positive,
    Negative,
    /// Both intervals are the same flat line. Either direction crosses it.
    Both,
    /// This axis is not a single shared face.
    Neither,
}

fn face_side(moving: &Aabb, other: &Aabb, axis: usize) -> FaceSide {
    let moving_min = component(moving.min, axis);
    let moving_max = component(moving.max, axis);
    let static_min = component(other.min, axis);
    let static_max = component(other.max, axis);
    let on_positive = moving_min >= static_max - TOUCH_SLOP;
    let on_negative = moving_max <= static_min + TOUCH_SLOP;
    match (on_positive, on_negative) {
        (true, false) => FaceSide::Positive,
        (false, true) => FaceSide::Negative,
        (true, true) => FaceSide::Both,
        (false, false) => FaceSide::Neither,
    }
}

fn box_usable(aabb: &Aabb) -> bool {
    aabb.min.is_finite()
        && aabb.max.is_finite()
        && aabb.min.x <= aabb.max.x
        && aabb.min.y <= aabb.max.y
        && aabb.min.z <= aabb.max.z
}

fn box_corner(aabb: &Aabb, index: usize) -> Vec3 {
    vec3(
        if index & 1 == 0 {
            aabb.min.x
        } else {
            aabb.max.x
        },
        if index & 2 == 0 {
            aabb.min.y
        } else {
            aabb.max.y
        },
        if index & 4 == 0 {
            aabb.min.z
        } else {
            aabb.max.z
        },
    )
}

fn box_edges(aabb: &Aabb) -> [[Vec3; 2]; 12] {
    let min = aabb.min;
    let max = aabb.max;
    [
        [vec3(min.x, min.y, min.z), vec3(max.x, min.y, min.z)],
        [vec3(min.x, max.y, min.z), vec3(max.x, max.y, min.z)],
        [vec3(min.x, min.y, max.z), vec3(max.x, min.y, max.z)],
        [vec3(min.x, max.y, max.z), vec3(max.x, max.y, max.z)],
        [vec3(min.x, min.y, min.z), vec3(min.x, max.y, min.z)],
        [vec3(max.x, min.y, min.z), vec3(max.x, max.y, min.z)],
        [vec3(min.x, min.y, max.z), vec3(min.x, max.y, max.z)],
        [vec3(max.x, min.y, max.z), vec3(max.x, max.y, max.z)],
        [vec3(min.x, min.y, min.z), vec3(min.x, min.y, max.z)],
        [vec3(max.x, min.y, min.z), vec3(max.x, min.y, max.z)],
        [vec3(min.x, max.y, min.z), vec3(min.x, max.y, max.z)],
        [vec3(max.x, max.y, min.z), vec3(max.x, max.y, max.z)],
    ]
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

    fn assert_box_impact(moving: Aabb, delta: Vec3, solid: Aabb) {
        let Some(hit) = moving.sweep(delta, &solid) else {
            return;
        };
        assert!(hit.t >= 0.0 && hit.t <= 1.0, "t {}", hit.t);
        assert!(
            (hit.normal.length() - 1.0).abs() < 1e-4,
            "normal {:?}",
            hit.normal
        );
        assert!(hit.point.is_finite());
        let at = moving.offset(delta * hit.t);
        assert!(
            at.overlaps(&solid),
            "no overlap at t {} ({:?} vs {:?})",
            hit.t,
            at,
            solid
        );
        if hit.t > 1e-3 {
            let before = moving.offset(delta * (hit.t - 1e-3));
            assert!(
                !before.overlaps(&solid),
                "overlapped before impact at {}",
                hit.t
            );
            assert!(
                delta.dot(hit.normal) < -INTO_SLOP,
                "normal {:?} is not against delta {:?}",
                hit.normal,
                delta
            );
        }
    }

    #[test]
    fn box_hits_a_wall_and_stops_on_the_face() {
        let wall = Aabb::from_min_max(vec3(2.0, 0.0, 0.0), vec3(3.0, 1.0, 1.0));
        let body = Aabb::from_min_max(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
        let hit = body.sweep(vec3(2.0, 0.0, 0.0), &wall).unwrap();
        assert!((hit.t - 0.5).abs() < 1e-5);
        assert_eq!(hit.normal, -Vec3::X);
        assert_vec3_close(hit.point, vec3(2.0, 0.5, 0.5));
        assert_box_impact(body, vec3(2.0, 0.0, 0.0), wall);

        let from_right = Aabb::from_min_max(vec3(4.0, 0.0, 0.0), vec3(5.0, 1.0, 1.0));
        let back = from_right.sweep(vec3(-2.0, 0.0, 0.0), &wall).unwrap();
        assert!((back.t - 0.5).abs() < 1e-5);
        assert_eq!(back.normal, Vec3::X);
    }

    #[test]
    fn box_misses_when_the_path_is_short_or_passes_beside() {
        let wall = Aabb::from_min_max(vec3(2.0, 0.0, 0.0), vec3(3.0, 1.0, 1.0));
        let body = Aabb::from_min_max(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
        assert!(body.sweep(vec3(0.5, 0.0, 0.0), &wall).is_none());
        let above = Aabb::from_min_max(vec3(0.0, 2.0, 0.0), vec3(1.0, 3.0, 1.0));
        assert!(above.sweep(vec3(4.0, 0.0, 0.0), &wall).is_none());
        let leaving = Aabb::from_min_max(vec3(4.0, 0.0, 0.0), vec3(5.0, 1.0, 1.0));
        assert!(leaving.sweep(vec3(1.0, 0.0, 0.0), &wall).is_none());
    }

    #[test]
    fn box_resting_on_a_floor_sweeps_sideways() {
        let floor = Aabb::from_min_max(vec3(-2.0, -1.0, -2.0), vec3(2.0, 0.0, 2.0));
        let body = Aabb::from_min_max(vec3(-0.25, 0.0, -0.25), vec3(0.25, 1.0, 0.25));
        assert!(body.sweep(vec3(1.0, 0.0, 0.5), &floor).is_none());
        assert!(body.sweep(Vec3::ZERO, &floor).is_none());
        assert!(body.sweep(vec3(0.0, 0.4, 0.0), &floor).is_none());

        let sunk = Aabb::from_min_max(vec3(-0.25, -1e-5, -0.25), vec3(0.25, 1.0, 0.25));
        assert!(sunk.contact(&floor).unwrap().penetration <= TOUCH_SLOP);
        assert!(sunk.sweep(vec3(0.5, 0.0, 0.0), &floor).is_none());

        let into = body.sweep(vec3(1.0, -0.25, 0.0), &floor).unwrap();
        assert_eq!(into.t, 0.0);
        assert_eq!(into.normal, Vec3::Y);
        assert!(into.point.y.abs() < 1e-5);
    }

    #[test]
    fn box_corner_touch_uses_the_axis_that_enters() {
        let solid = Aabb::from_min_max(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
        let body = Aabb::from_min_max(vec3(-1.0, -1.0, 0.2), vec3(0.0, 0.0, 0.8));
        let down = body.sweep(vec3(0.0, 1.0, 0.0), &solid).unwrap();
        assert_eq!(down.t, 0.0);
        assert_eq!(down.normal, -Vec3::Y);
        let right = body.sweep(vec3(1.0, 0.0, 0.0), &solid).unwrap();
        assert_eq!(right.normal, -Vec3::X);
        assert!(body.sweep(vec3(-0.2, -0.3, 0.4), &solid).is_none());
    }

    #[test]
    fn box_inside_reports_t_zero_even_when_leaving() {
        let solid = Aabb::from_min_max(vec3(-1.0, -1.0, -1.0), vec3(1.0, 1.0, 1.0));
        let body = Aabb::from_center_size(Vec3::ZERO, vec3(0.5, 0.5, 0.5));
        let hit = body.sweep(vec3(4.0, 0.0, 0.0), &solid).unwrap();
        assert_eq!(hit.t, 0.0);
        let contact = body.contact(&solid).unwrap();
        assert_eq!(hit.normal, contact.normal);
        assert_vec3_close(hit.point, contact.point);
        assert!(body.sweep(Vec3::ZERO, &solid).unwrap().t == 0.0);
    }

    #[test]
    fn box_hits_at_the_end_of_the_delta_and_not_past_it() {
        let wall = Aabb::from_min_max(vec3(2.0, 0.0, 0.0), vec3(3.0, 1.0, 1.0));
        let body = Aabb::from_min_max(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
        let hit = body.sweep(Vec3::X, &wall).unwrap();
        assert!((hit.t - 1.0).abs() < 1e-5);
        assert_eq!(hit.normal, -Vec3::X);
        assert!(body.sweep(vec3(0.999, 0.0, 0.0), &wall).is_none());
    }

    #[test]
    fn box_sweep_catches_a_thin_wall_a_discrete_test_skips() {
        let wall = Aabb::from_min_max(vec3(0.0, -1.0, -1.0), vec3(0.1, 1.0, 1.0));
        let body = Aabb::from_center_size(vec3(-1.0, 0.0, 0.0), vec3(0.4, 0.4, 0.4));
        let delta = vec3(3.0, 0.0, 0.0);
        let hit = body.sweep(delta, &wall).unwrap();
        assert!((hit.t - (0.8 / 3.0)).abs() < 1e-4);
        assert_eq!(hit.normal, -Vec3::X);
        assert!(!body.offset(delta).overlaps(&wall));
        assert_box_impact(body, delta, wall);
    }

    #[test]
    fn box_rejects_inverted_and_non_finite_input() {
        let body = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
        let wall = Aabb::from_min_max(vec3(2.0, 0.0, 0.0), vec3(3.0, 1.0, 1.0));
        let inverted = Aabb::from_min_max(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 1.0));
        assert!(inverted.sweep(Vec3::X, &wall).is_none());
        assert!(body.sweep(Vec3::X, &inverted).is_none());
        assert!(body.sweep(vec3(f32::NAN, 0.0, 0.0), &wall).is_none());
        let broken = Aabb::from_min_max(vec3(f32::NAN, 0.0, 0.0), Vec3::ONE);
        assert!(broken.sweep(Vec3::X, &wall).is_none());
    }

    #[test]
    fn flat_plane_slides_and_blocks_the_crossing_direction() {
        let plane = Aabb::from_min_max(vec3(-1.0, 0.0, -1.0), vec3(1.0, 0.0, 1.0));
        let sheet = Aabb::from_min_max(vec3(-0.2, 0.0, -0.2), vec3(0.2, 0.0, 0.2));
        assert!(sheet.sweep(vec3(0.4, 0.0, -0.1), &plane).is_none());
        let through = sheet.sweep(vec3(0.0, -0.5, 0.0), &plane).unwrap();
        assert_eq!(through.t, 0.0);
        assert_eq!(through.normal, Vec3::Y);
    }

    #[test]
    fn sphere_resting_on_a_floor_sweeps_sideways() {
        let floor = Aabb::from_min_max(vec3(-2.0, -1.0, -2.0), vec3(2.0, 0.0, 2.0));
        let ball = Sphere::new(vec3(0.0, 0.5, 0.0), 0.5);
        assert!(ball.sweep_aabb(vec3(1.0, 0.0, -0.25), &floor).is_none());
        assert!(ball.sweep_aabb(vec3(0.0, 0.2, 0.0), &floor).is_none());

        let sunk = Sphere::new(vec3(0.0, 0.5 - 1e-5, 0.0), 0.5);
        assert!(sunk.contact_aabb(&floor).unwrap().penetration <= TOUCH_SLOP);
        assert!(sunk.sweep_aabb(vec3(0.4, 0.0, 0.0), &floor).is_none());

        let into = ball.sweep_aabb(vec3(0.2, -1.0, 0.0), &floor).unwrap();
        assert_eq!(into.t, 0.0);
        assert_eq!(into.normal, Vec3::Y);
        assert_vec3_close(into.point, Vec3::ZERO);
    }

    #[test]
    fn sphere_hits_a_face_at_the_expected_time() {
        let wall = Aabb::from_min_max(vec3(0.0, -1.0, -1.0), vec3(1.0, 1.0, 1.0));
        let ball = Sphere::new(vec3(-2.0, 0.0, 0.0), 0.5);
        let hit = ball.sweep_aabb(vec3(4.0, 0.0, 0.0), &wall).unwrap();
        assert!((hit.t - 0.375).abs() < 1e-4);
        assert_eq!(hit.normal, -Vec3::X);
        assert_vec3_close(hit.point, vec3(0.0, 0.0, 0.0));
        let center = ball.center + vec3(4.0, 0.0, 0.0) * hit.t;
        assert!((center.x + 0.5).abs() < 1e-4);
    }

    #[test]
    fn sphere_hits_an_edge_and_a_corner() {
        let solid = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
        let edge_ball = Sphere::new(vec3(-1.0, -1.0, 0.5), 0.5);
        let edge = edge_ball.sweep_aabb(vec3(2.0, 2.0, 0.0), &solid).unwrap();
        let expected_t = 0.5 - 0.5 * (0.125_f32).sqrt();
        assert!((edge.t - expected_t).abs() < 1e-4, "t {}", edge.t);
        assert_vec3_close(edge.normal, vec3(-1.0, -1.0, 0.0).normalize());
        assert!((edge.point.z - 0.5).abs() < 1e-4);
        assert!(edge.point.x.abs() < 1e-4 && edge.point.y.abs() < 1e-4);

        let corner_ball = Sphere::new(vec3(-1.0, -1.0, -1.0), 0.5);
        let corner = corner_ball.sweep_aabb(vec3(2.0, 2.0, 2.0), &solid).unwrap();
        let corner_t = 0.5 - 0.5 / 12.0_f32.sqrt();
        assert!((corner.t - corner_t).abs() < 1e-4, "t {}", corner.t);
        assert_vec3_close(corner.normal, -Vec3::ONE.normalize());
        assert_vec3_close(corner.point, Vec3::ZERO);
    }

    #[test]
    fn sphere_graze_along_a_face_is_not_an_impact() {
        let solid = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
        let ball = Sphere::new(vec3(-2.0, 1.5, 0.5), 0.5);
        assert!(ball.sweep_aabb(vec3(4.0, 0.0, 0.0), &solid).is_none());
    }

    #[test]
    fn sphere_clips_a_box_whose_face_does_not_contain_the_center() {
        let solid = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
        let ball = Sphere::new(vec3(-2.0, 1.4, 0.5), 0.5);
        let delta = vec3(4.0, 0.0, 0.0);
        let hit = ball.sweep_aabb(delta, &solid).unwrap();
        assert!(hit.t > 0.0 && hit.t < 1.0);
        assert!(hit.normal.y > 0.5, "normal {:?}", hit.normal);
        assert!(hit.normal.x < -0.4, "normal {:?}", hit.normal);
        // The analytic root can sit a ulp outside the inclusive overlap test.
        let at = ball.offset(delta * hit.t);
        let gap = (at.center - solid.closest_point(at.center)).length() - ball.radius;
        assert!(gap.abs() < 1e-4, "gap {gap}");
        let before = ball.offset(delta * (hit.t - 1e-3));
        assert!(!before.overlaps_aabb(&solid));
    }

    #[test]
    fn sphere_inside_reports_t_zero() {
        let solid = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
        let ball = Sphere::new(vec3(0.5, 0.5, 0.5), 0.2);
        let hit = ball.sweep_aabb(vec3(-3.0, 1.0, 0.0), &solid).unwrap();
        assert_eq!(hit.t, 0.0);
        let contact = ball.contact_aabb(&solid).unwrap();
        assert_eq!(hit.normal, contact.normal);
        assert_vec3_close(hit.point, contact.point);

        let outside_center = Sphere::new(vec3(-0.2, 0.5, 0.5), 0.5);
        let sunk = outside_center
            .sweep_aabb(vec3(-1.0, 0.0, 0.0), &solid)
            .unwrap();
        assert_eq!(sunk.t, 0.0);
        assert_eq!(sunk.normal, -Vec3::X);
    }

    #[test]
    fn sphere_sweep_catches_a_thin_wall_a_discrete_test_skips() {
        let wall = Aabb::from_min_max(vec3(0.0, -1.0, -1.0), vec3(0.1, 1.0, 1.0));
        let ball = Sphere::new(vec3(-1.0, 0.0, 0.0), 0.5);
        let delta = vec3(3.0, 0.0, 0.0);
        let hit = ball.sweep_aabb(delta, &wall).unwrap();
        assert!((hit.t - (0.5 / 3.0)).abs() < 1e-4);
        assert_eq!(hit.normal, -Vec3::X);
        assert!(!ball.offset(delta).overlaps_aabb(&wall));
    }

    #[test]
    fn sphere_point_and_empty_radius() {
        let wall = Aabb::from_min_max(vec3(1.0, -1.0, -1.0), vec3(2.0, 1.0, 1.0));
        let point = Sphere::new(Vec3::ZERO, 0.0);
        let hit = point.sweep_aabb(vec3(2.0, 0.0, 0.0), &wall).unwrap();
        assert!((hit.t - 0.5).abs() < 1e-5);
        assert_eq!(hit.normal, -Vec3::X);
        assert_vec3_close(hit.point, vec3(1.0, 0.0, 0.0));
        assert!(Sphere::new(Vec3::ZERO, -1.0)
            .sweep_aabb(Vec3::X, &wall)
            .is_none());
        assert!(point.sweep_aabb(vec3(f32::NAN, 0.0, 0.0), &wall).is_none());
        let inverted = Aabb::from_min_max(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 1.0));
        assert!(point.sweep_aabb(Vec3::X, &inverted).is_none());
    }

    #[test]
    fn sphere_misses_beside_the_box() {
        let solid = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
        let ball = Sphere::new(vec3(-1.0, 3.0, 0.5), 0.5);
        assert!(ball.sweep_aabb(vec3(4.0, 0.0, 0.0), &solid).is_none());
        assert!(ball.sweep_aabb(Vec3::ZERO, &solid).is_none());
    }
}
