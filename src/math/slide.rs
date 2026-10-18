//! Move a sphere so it stops on a box and keeps the motion along that face.
//!
//! [`Aabb`] and [`Sphere`] overlap tests are inclusive, so a sphere sitting on
//! a floor already counts as overlapping. [`slide_sphere`] treats that touch
//! as a surface to slide on. The caller does not pass a smaller radius.

use glam::{vec3, Vec3};

use super::{Aabb, Obb};

/// How far two contact times can be and still be the same hit.
const TIME_SLOP: f32 = 1e-5;
/// Motion into a surface below this is numeric noise, not a new collision.
const INTO_SLOP: f32 = 1e-5;
/// A plane distance this far inside is not the face the sphere is entering.
const PLANE_SLOP: f32 = 1e-4;

/// One contact along a move. `normal` points out of the solid.
#[derive(Clone, Copy)]
struct SlideHit {
    t: f32,
    normal: Vec3,
}

/// Moves a sphere by `delta` and slides it along [`Aabb`] and [`Obb`] faces.
///
/// The returned value is the new center. The sphere travels with `delta`
/// until it meets a box, then the leftover motion is projected onto that
/// face so a wall removes only the component into the wall. A sphere that
/// merely touches a shape (the gap equals `radius`; boundaries are inclusive)
/// still moves along it. Callers keep the real radius; this function does not
/// ask for a smaller one.
///
/// When `center` starts inside a box, it is pushed out along that box's
/// nearest face before `delta` is applied. The nearest face is the shortest
/// axis: a world axis for an [`Aabb`], one of the box's own axes for an
/// [`Obb`]. A tie keeps the earlier axis, minimum side before maximum side,
/// in X then Y then Z.
///
/// A negative or non-finite radius does not collide, and a finite `delta` is
/// applied as a free move. A non-finite `center` or `delta` is left where it
/// is. An inverted [`Aabb`] and an empty or unusable [`Obb`] are ignored, the
/// same as their overlap queries. There is no gravity and no rigid body here.
///
/// # Example
///
/// ```
/// use macroquad::math::{slide_sphere, Aabb, vec3};
///
/// let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(4.0, 1.0, 4.0));
/// let resting = vec3(0.0, 0.5, 0.0);
/// let moved = slide_sphere(resting, 0.5, vec3(1.0, -0.25, 0.0), &[floor], &[]);
/// assert!((moved.x - 1.0).abs() < 1e-4);
/// assert!((moved.y - 0.5).abs() < 1e-4);
/// ```
///
/// Gravity stays in the caller. Add it to the velocity, then slide. A downward
/// step that the floor refuses clears that speed. This is not a rigid body.
///
/// ```
/// use macroquad::math::{slide_sphere, Aabb, Vec3, vec3};
///
/// fn step_sphere(center: &mut Vec3, velocity: &mut Vec3, solids: &[Aabb], dt: f32) {
///     *velocity += vec3(0.0, -9.8, 0.0) * dt;
///     let next = slide_sphere(*center, 0.5, *velocity * dt, solids, &[]);
///     if velocity.y < 0.0 && next.y > center.y + velocity.y * dt + 1e-4 {
///         velocity.y = 0.0;
///     }
///     *center = next;
/// }
///
/// let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(8.0, 1.0, 8.0));
/// let mut center = vec3(0.0, 0.5, 0.0);
/// let mut velocity = vec3(2.0, 0.0, 0.0);
/// step_sphere(&mut center, &mut velocity, &[floor], 1.0 / 60.0);
/// assert!((center.y - 0.5).abs() < 1e-3);
/// assert!(center.x > 0.0);
/// assert_eq!(velocity.y, 0.0);
/// ```
pub fn slide_sphere(center: Vec3, radius: f32, delta: Vec3, aabbs: &[Aabb], obbs: &[Obb]) -> Vec3 {
    if !center.is_finite() || !delta.is_finite() {
        return center;
    }
    if !radius.is_finite() || radius < 0.0 {
        return center + delta;
    }

    let mut position = resolve_penetration(center, radius, aabbs, obbs);
    if !position.is_finite() {
        return center;
    }
    let mut remaining = delta;
    // Three planes meet at a corner. One extra pass applies the tangent.
    for _ in 0..4 {
        if remaining.length_squared() <= TIME_SLOP * TIME_SLOP {
            remaining = Vec3::ZERO;
            break;
        }
        let Some(hit) = first_hit(position, radius, remaining, aabbs, obbs) else {
            position += remaining;
            remaining = Vec3::ZERO;
            break;
        };
        let t = hit.t.clamp(0.0, 1.0);
        if t > 0.0 {
            position += remaining * t;
        }
        let mut left = remaining * (1.0 - t);
        let into = left.dot(hit.normal);
        if into < 0.0 {
            left -= hit.normal * into;
        }
        // Already on the surface and the leftover is the same motion: keep it.
        // Projecting again would drop a tangent that only looks like a hit.
        if t <= TIME_SLOP && (left - remaining).length_squared() <= TIME_SLOP * TIME_SLOP {
            position += left;
            remaining = Vec3::ZERO;
            break;
        }
        remaining = left;
    }
    // The last pass can itself be a hit and still leave a tangent. Apply that
    // tangent when it clears every shape. When it only presses into a surface
    // the sphere is already touching, drop that component and keep the rest
    // if the rest is clear. A tangent that still meets a shape later stays
    // put, so the sphere does not tunnel through the next solid.
    if remaining.length_squared() > TIME_SLOP * TIME_SLOP {
        match first_hit(position, radius, remaining, aabbs, obbs) {
            None => position += remaining,
            Some(hit) if hit.t <= TIME_SLOP => {
                let into = remaining.dot(hit.normal);
                if into < 0.0 {
                    remaining -= hit.normal * into;
                }
                if remaining.length_squared() > TIME_SLOP * TIME_SLOP
                    && first_hit(position, radius, remaining, aabbs, obbs).is_none()
                {
                    position += remaining;
                }
            }
            Some(_) => {}
        }
    }
    position
}

fn resolve_penetration(center: Vec3, radius: f32, aabbs: &[Aabb], obbs: &[Obb]) -> Vec3 {
    let mut center = center;
    for _ in 0..8 {
        let mut best: Option<Vec3> = None;
        let mut best_len = 0.0;
        for aabb in aabbs {
            if let Some(push) = push_out_aabb(center, radius, aabb) {
                let len = push.length_squared();
                if len > best_len {
                    best = Some(push);
                    best_len = len;
                }
            }
        }
        for obb in obbs {
            if let Some(push) = push_out_obb(center, radius, obb) {
                let len = push.length_squared();
                if len > best_len {
                    best = Some(push);
                    best_len = len;
                }
            }
        }
        let Some(push) = best else {
            break;
        };
        if best_len <= TIME_SLOP * TIME_SLOP || !push.is_finite() {
            break;
        }
        center += push;
    }
    center
}

fn push_out_aabb(center: Vec3, radius: f32, aabb: &Aabb) -> Option<Vec3> {
    if !usable_aabb(aabb) {
        return None;
    }
    if aabb.contains(center) {
        return Some(shortest_axis_push(center, radius, aabb.min, aabb.max));
    }
    let closest = aabb.closest_point(center);
    let offset = center - closest;
    let dist_sq = offset.length_squared();
    if dist_sq == 0.0 {
        return Some(shortest_axis_push(center, radius, aabb.min, aabb.max));
    }
    if dist_sq >= radius * radius {
        return None;
    }
    let dist = dist_sq.sqrt();
    let push = offset * ((radius - dist) / dist);
    if push.is_finite() {
        Some(push)
    } else {
        None
    }
}

fn push_out_obb(center: Vec3, radius: f32, obb: &Obb) -> Option<Vec3> {
    let rotation = obb.pose()?;
    let local = rotation.inverse() * (center - obb.center);
    let half = obb.half_extents;
    let local_box = Aabb::from_center_size(Vec3::ZERO, half * 2.0);
    let local_push = push_out_aabb(local, radius, &local_box)?;
    let push = rotation * local_push;
    if push.is_finite() {
        Some(push)
    } else {
        None
    }
}

/// Push `center` out of a box along the nearest face, then out by `radius`.
///
/// `center` is inside or on `min`/`max`. The shortest axis is the face with
/// the smaller distance. Equal distances keep the minimum side, and X before
/// Y before Z.
fn shortest_axis_push(center: Vec3, radius: f32, min: Vec3, max: Vec3) -> Vec3 {
    let mut best_dist = f32::MAX;
    let mut best = Vec3::ZERO;
    for axis in 0..3 {
        let value = center[axis];
        let dist_min = value - min[axis];
        let dist_max = max[axis] - value;
        if dist_min < best_dist {
            best_dist = dist_min;
            best = axis_vector(axis, -(dist_min + radius));
        }
        if dist_max < best_dist {
            best_dist = dist_max;
            best = axis_vector(axis, dist_max + radius);
        }
    }
    best
}

fn first_hit(
    center: Vec3,
    radius: f32,
    delta: Vec3,
    aabbs: &[Aabb],
    obbs: &[Obb],
) -> Option<SlideHit> {
    let mut best: Option<SlideHit> = None;
    for aabb in aabbs {
        if usable_aabb(aabb) {
            consider(
                &mut best,
                cast_sphere_aabb(center, radius, delta, aabb),
                delta,
            );
        }
    }
    for obb in obbs {
        consider(
            &mut best,
            cast_sphere_obb(center, radius, delta, obb),
            delta,
        );
    }
    best
}

fn consider(best: &mut Option<SlideHit>, hit: Option<SlideHit>, delta: Vec3) {
    let Some(hit) = hit else {
        return;
    };
    if !hit.t.is_finite() || !hit.normal.is_finite() || hit.normal.length_squared() < 1e-8 {
        return;
    }
    let replace = match *best {
        None => true,
        Some(current) => {
            if hit.t < current.t - TIME_SLOP {
                true
            } else if hit.t > current.t + TIME_SLOP {
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

fn cast_sphere_obb(center: Vec3, radius: f32, delta: Vec3, obb: &Obb) -> Option<SlideHit> {
    let rotation = obb.pose()?;
    let inverse = rotation.inverse();
    let local_center = inverse * (center - obb.center);
    let local_delta = inverse * delta;
    let local_box = Aabb::from_center_size(Vec3::ZERO, obb.half_extents * 2.0);
    let hit = cast_sphere_aabb(local_center, radius, local_delta, &local_box)?;
    let normal = rotation * hit.normal;
    if !normal.is_finite() {
        return None;
    }
    Some(SlideHit { t: hit.t, normal })
}

fn cast_sphere_aabb(center: Vec3, radius: f32, delta: Vec3, aabb: &Aabb) -> Option<SlideHit> {
    let mut best: Option<SlideHit> = None;
    for axis in 0..3 {
        consider(
            &mut best,
            cast_face(center, radius, delta, aabb, axis, false),
            delta,
        );
        consider(
            &mut best,
            cast_face(center, radius, delta, aabb, axis, true),
            delta,
        );
    }
    for edge in aabb_edges(aabb) {
        consider(
            &mut best,
            cast_edge(center, radius, delta, edge[0], edge[1]),
            delta,
        );
    }
    for corner in aabb_corners(aabb) {
        consider(&mut best, cast_corner(center, radius, delta, corner), delta);
    }
    best
}

fn cast_face(
    center: Vec3,
    radius: f32,
    delta: Vec3,
    aabb: &Aabb,
    axis: usize,
    max_side: bool,
) -> Option<SlideHit> {
    let (plane, outward) = if max_side {
        (aabb.max[axis], 1.0)
    } else {
        (aabb.min[axis], -1.0)
    };
    let into_plane = outward * delta[axis];
    if into_plane >= -INTO_SLOP {
        return None;
    }
    // Positive when the sphere is still outside this face.
    let dist = outward * (center[axis] - plane) - radius;
    if dist < -PLANE_SLOP {
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
    let impact = center + delta * t;
    for other in 0..3 {
        if other == axis {
            continue;
        }
        let coord = impact[other];
        if coord < aabb.min[other] - PLANE_SLOP || coord > aabb.max[other] + PLANE_SLOP {
            return None;
        }
    }
    Some(SlideHit {
        t,
        normal: axis_vector(axis, outward),
    })
}

fn cast_edge(center: Vec3, radius: f32, delta: Vec3, start: Vec3, end: Vec3) -> Option<SlideHit> {
    let edge = end - start;
    let length_sq = edge.length_squared();
    if length_sq <= PLANE_SLOP * PLANE_SLOP {
        return None;
    }
    let length = length_sq.sqrt();
    let axis = edge / length;
    let offset = center - start;
    let delta_perp = delta - axis * delta.dot(axis);
    let offset_perp = offset - axis * offset.dot(axis);
    // Parallel to the edge: distance stays constant, so this is not an entry.
    if delta_perp.length_squared() <= 1e-12 {
        return None;
    }
    let (t_enter, _) = super::sphere_quadratic_roots(offset_perp, delta_perp, radius)?;
    accept_radial(center, delta, start, axis, length, t_enter)
}

fn cast_corner(center: Vec3, radius: f32, delta: Vec3, corner: Vec3) -> Option<SlideHit> {
    let offset = center - corner;
    if delta.length_squared() <= 1e-12 {
        return None;
    }
    let (t_enter, _) = super::sphere_quadratic_roots(offset, delta, radius)?;
    if !t_enter.is_finite() || t_enter > 1.0 || t_enter < -PLANE_SLOP {
        return None;
    }
    let t = t_enter.max(0.0);
    let radial = center + delta * t - corner;
    let normal = radial.normalize_or_zero();
    if normal.length_squared() < 1e-8 || delta.dot(normal) >= -INTO_SLOP {
        return None;
    }
    Some(SlideHit { t, normal })
}

fn accept_radial(
    center: Vec3,
    delta: Vec3,
    origin: Vec3,
    axis: Vec3,
    length: f32,
    t_enter: f32,
) -> Option<SlideHit> {
    if !t_enter.is_finite() || t_enter > 1.0 || t_enter < -PLANE_SLOP {
        return None;
    }
    let t = t_enter.max(0.0);
    let at = center + delta * t;
    let along = (at - origin).dot(axis);
    if along < -PLANE_SLOP || along > length + PLANE_SLOP {
        return None;
    }
    let radial = at - (origin + axis * along);
    let normal = radial.normalize_or_zero();
    if normal.length_squared() < 1e-8 || delta.dot(normal) >= -INTO_SLOP {
        return None;
    }
    Some(SlideHit { t, normal })
}

fn usable_aabb(aabb: &Aabb) -> bool {
    aabb.min.is_finite()
        && aabb.max.is_finite()
        && aabb.min.x <= aabb.max.x
        && aabb.min.y <= aabb.max.y
        && aabb.min.z <= aabb.max.z
}

fn axis_vector(axis: usize, value: f32) -> Vec3 {
    match axis {
        0 => vec3(value, 0.0, 0.0),
        1 => vec3(0.0, value, 0.0),
        _ => vec3(0.0, 0.0, value),
    }
}

fn aabb_edges(aabb: &Aabb) -> [[Vec3; 2]; 12] {
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

fn aabb_corners(aabb: &Aabb) -> [Vec3; 8] {
    let min = aabb.min;
    let max = aabb.max;
    [
        vec3(min.x, min.y, min.z),
        vec3(max.x, min.y, min.z),
        vec3(min.x, max.y, min.z),
        vec3(max.x, max.y, min.z),
        vec3(min.x, min.y, max.z),
        vec3(max.x, min.y, max.z),
        vec3(min.x, max.y, max.z),
        vec3(max.x, max.y, max.z),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Quat, Sphere, Vec3};

    fn assert_close(left: Vec3, right: Vec3) {
        let delta = left - right;
        assert!(
            delta.x.abs() < 1e-3 && delta.y.abs() < 1e-3 && delta.z.abs() < 1e-3,
            "left {left:?} right {right:?}"
        );
    }

    fn assert_outside_aabb(center: Vec3, radius: f32, aabb: &Aabb) {
        let closest = aabb.closest_point(center);
        let gap = (center - closest).length();
        assert!(
            gap + 1e-3 >= radius,
            "sphere penetrated an aabb: gap {gap} radius {radius}"
        );
    }

    fn assert_outside_obb(center: Vec3, radius: f32, obb: &Obb) {
        let closest = obb.closest_point(center);
        let gap = (center - closest).length();
        assert!(
            gap + 1e-3 >= radius,
            "sphere penetrated an obb: gap {gap} radius {radius}"
        );
    }

    #[test]
    fn a_sphere_resting_on_a_floor_strafes() {
        let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(10.0, 1.0, 10.0));
        let radius = 0.5;
        let resting = vec3(0.2, radius, -0.3);
        // Inclusive overlap already counts this as a hit. The slide must not
        // stick, and it must not ask the caller to shrink the radius.
        assert!(Sphere::new(resting, radius).overlaps_aabb(&floor));

        let sideways = slide_sphere(resting, radius, vec3(1.25, 0.0, -0.5), &[floor], &[]);
        assert_close(sideways, resting + vec3(1.25, 0.0, -0.5));

        let with_gravity = slide_sphere(resting, radius, vec3(1.25, -0.4, -0.5), &[floor], &[]);
        assert_close(with_gravity, resting + vec3(1.25, 0.0, -0.5));
        assert_outside_aabb(with_gravity, radius, &floor);

        let away = slide_sphere(resting, radius, vec3(0.2, 1.0, 0.0), &[floor], &[]);
        assert_close(away, resting + vec3(0.2, 1.0, 0.0));

        let straight_down = slide_sphere(resting, radius, vec3(0.0, -1.0, 0.0), &[floor], &[]);
        assert_close(straight_down, resting);
    }

    #[test]
    fn an_axis_aligned_wall_keeps_the_tangent() {
        let wall = Aabb::from_center_size(vec3(0.0, 0.0, 0.0), vec3(0.05, 4.0, 4.0));
        let radius = 0.3;
        let start = vec3(-2.0, 0.0, 0.0);
        let end = slide_sphere(start, radius, vec3(10.0, 0.0, 1.0), &[wall], &[]);
        // Near face is x = -0.025. The center stops one radius short of it,
        // and the full sideways component of the move is kept.
        assert_close(end, vec3(-0.325, 0.0, 1.0));
        assert_outside_aabb(end, radius, &wall);
    }

    #[test]
    fn a_yawed_obb_is_slid_along_instead_of_stuck() {
        let yaw = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        // Long enough that the slide stays on the face instead of leaving an end.
        let wall = Obb::new(vec3(2.0, 0.0, 0.0), vec3(0.2, 1.0, 6.0), yaw);
        let radius = 0.25;
        let end = slide_sphere(Vec3::ZERO, radius, vec3(3.0, 0.0, 0.0), &[], &[wall]);

        // +X against a 45° face becomes motion along the wall, with +Z.
        // Zeroing the blocked world axis, the way a per-axis search does,
        // would leave z at 0 and stop at the first contact.
        assert!(end.z > 0.4, "expected to slide along the yaw, got {end:?}");
        assert!(end.x > 1.0, "expected to keep moving forward, got {end:?}");
        assert_outside_obb(end, radius, &wall);

        let normal = yaw * -Vec3::X;
        let face = wall.center + yaw * vec3(-wall.half_extents.x, 0.0, 0.0);
        let signed = (end - face).dot(normal);
        assert!(
            (signed - radius).abs() < 1e-2,
            "left the face: signed {signed} end {end:?}"
        );
    }

    #[test]
    fn a_center_inside_a_box_is_pushed_out_on_the_shortest_axis() {
        let crate_ = Aabb::from_min_max(Vec3::ZERO, vec3(2.0, 2.0, 2.0));
        let radius = 0.5;
        let buried = vec3(1.0, 1.0, 0.25);
        let pushed = slide_sphere(buried, radius, Vec3::ZERO, &[crate_], &[]);
        assert_close(pushed, vec3(1.0, 1.0, -radius));
        assert_outside_aabb(pushed, radius, &crate_);

        let then_moved = slide_sphere(buried, radius, vec3(0.4, 0.0, 0.0), &[crate_], &[]);
        assert_close(then_moved, vec3(1.4, 1.0, -radius));

        let yaw = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let obb = Obb::new(Vec3::ZERO, Vec3::ONE, yaw);
        let local = vec3(0.8, 0.0, 0.0);
        let world = yaw * local;
        let out = slide_sphere(world, radius, Vec3::ZERO, &[], &[obb]);
        assert_close(out, yaw * vec3(1.0 + radius, 0.0, 0.0));
        assert_outside_obb(out, radius, &obb);
    }

    #[test]
    fn a_fast_move_does_not_tunnel_through_a_thin_floor() {
        let floor = Aabb::from_center_size(Vec3::ZERO, vec3(4.0, 0.1, 4.0));
        let radius = 0.4;
        let start = vec3(0.0, 3.0, 0.0);
        let end = slide_sphere(start, radius, vec3(0.5, -10.0, -0.25), &[floor], &[]);
        assert_close(end, vec3(0.5, 0.45, -0.25));
        assert_outside_aabb(end, radius, &floor);
    }

    #[test]
    fn a_floor_and_a_wall_keep_the_crease_tangent() {
        let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(10.0, 1.0, 10.0));
        let wall = Aabb::from_min_max(vec3(1.0, 0.0, -2.0), vec3(1.2, 2.0, 2.0));
        let radius = 0.5;
        let start = vec3(0.0, radius, 0.0);
        let end = slide_sphere(start, radius, vec3(2.0, -1.0, 0.75), &[floor, wall], &[]);
        assert_close(end, vec3(0.5, radius, 0.75));
        assert_outside_aabb(end, radius, &floor);
        assert_outside_aabb(end, radius, &wall);
    }

    #[test]
    fn an_identity_obb_matches_the_same_aabb() {
        let center = vec3(2.0, 0.0, 0.0);
        let aabb = Aabb::from_center_size(center, vec3(1.0, 2.0, 2.0));
        let obb = Obb::new(center, vec3(0.5, 1.0, 1.0), Quat::IDENTITY);
        let start = Vec3::ZERO;
        let delta = vec3(3.0, 0.0, 1.0);
        let from_aabb = slide_sphere(start, 0.4, delta, &[aabb], &[]);
        let from_obb = slide_sphere(start, 0.4, delta, &[], &[obb]);
        assert_close(from_aabb, vec3(1.1, 0.0, 1.0));
        assert_close(from_obb, from_aabb);
    }

    #[test]
    fn empty_shapes_and_a_bad_radius_do_not_block() {
        let solid = Aabb::from_center_size(Vec3::ZERO, Vec3::splat(4.0));
        let inverted = Aabb::from_min_max(Vec3::ONE, Vec3::ZERO);
        let empty = Obb::new(Vec3::ZERO, vec3(-1.0, 1.0, 1.0), Quat::IDENTITY);
        let start = vec3(-1.0, 0.0, 0.0);
        let delta = vec3(3.0, 0.5, 0.0);

        assert_close(
            slide_sphere(start, 0.5, delta, &[inverted], &[empty]),
            start + delta,
        );
        assert_close(
            slide_sphere(start, -1.0, delta, &[solid], &[]),
            start + delta,
        );
        assert_close(
            slide_sphere(start, f32::NAN, delta, &[solid], &[]),
            start + delta,
        );
        assert!(
            slide_sphere(vec3(f32::NAN, 0.0, 0.0), 1.0, delta, &[solid], &[])
                .x
                .is_nan()
        );
        assert_close(
            slide_sphere(start, 0.5, vec3(f32::INFINITY, 0.0, 0.0), &[solid], &[]),
            start,
        );
    }

    #[test]
    fn a_free_tangent_survives_the_last_clip() {
        // Each post sits on the tangent left by the previous clip, so one
        // move hits four times. The motion after the fourth clip clears
        // every post; dropping it would stop on that last post.
        let posts = [
            Aabb::from_center_size(vec3(2.0, 0.0, 0.35), vec3(0.2, 2.0, 0.2)),
            Aabb::from_center_size(vec3(2.76, 0.0, -0.26), vec3(0.2, 2.0, 0.2)),
            Aabb::from_center_size(vec3(2.92, 0.0, -1.53), vec3(0.2, 2.0, 0.2)),
            Aabb::from_center_size(vec3(2.48, 0.0, -2.26), vec3(0.2, 2.0, 0.2)),
        ];
        let end = slide_sphere(Vec3::ZERO, 0.3, vec3(12.0, 0.0, 0.0), &posts, &[]);
        assert_close(end, vec3(0.4004557, 0.0, -2.381895));
        for post in &posts {
            assert_outside_aabb(end, 0.3, post);
        }
    }

    #[test]
    fn a_corner_deflects_instead_of_stopping_the_tangent() {
        let post = Aabb::from_center_size(vec3(1.0, 0.0, 0.0), Vec3::splat(0.4));
        let radius = 0.2;
        // Aimed at the post's -X/-Z corner from outside both faces.
        let start = vec3(0.0, 0.0, -0.6);
        let end = slide_sphere(start, radius, vec3(2.0, 0.0, 1.2), &[post], &[]);
        assert_outside_aabb(end, radius, &post);
        assert!(
            (end - start).length() > 0.5,
            "corner ate the whole move: {end:?}"
        );
    }
}
