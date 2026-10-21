//! Move a sphere so it stops on a solid and keeps the motion along that face.
//!
//! [`Aabb`] and [`Sphere`] overlap tests are inclusive, so a sphere sitting on
//! a floor already counts as overlapping. [`slide_sphere`] treats that touch
//! as a surface to slide on. The caller does not pass a smaller radius.
//! [`slide_sphere_solids`] is the same move against spheres and capsules too.
//! [`slide_spheres_solids`] moves a rigid group with one shared translation.

use glam::{vec3, Vec3};

use super::{Aabb, Capsule, Obb, Sphere};

/// Tolerance for stopping negligible motion and zero-time contact handling.
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
/// Static spheres and capsules are [`slide_sphere_solids`]. This function does
/// not grow a parameter list for them.
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
    slide_sphere_solids(center, radius, delta, aabbs, obbs, &[], &[])
}

/// Moves a sphere by `delta` and slides it along boxes, spheres, and capsules.
///
/// The returned value is the new center. [`slide_sphere`] is this function
/// with empty `spheres` and `capsules` slices, and its parameter list is
/// unchanged. The rules are the same: the sphere stops on a surface and the
/// leftover motion stays on that face. A sphere that merely touches a shape
/// (the gap equals the sum of the radii, or `radius` against a box) still
/// moves along it.
///
/// When `center` starts inside a solid, it is pushed out before `delta` is
/// applied. A box uses that box's shortest axis, as [`slide_sphere`] does. A
/// sphere pushes from the other center, or along [`Vec3::X`] when the centers
/// coincide. A capsule pushes from the closest point on the inner segment.
/// A center that lies on that segment uses the world axis most perpendicular
/// to it, X before Y before Z. The push is long enough that the moving sphere
/// sits on the surface.
///
/// A negative or non-finite moving radius does not collide, and a finite
/// `delta` is applied as a free move. A non-finite `center` or `delta` is
/// left where it is. An empty box, an empty oriented box, and a sphere or
/// capsule with a negative or non-finite radius are ignored. There is no
/// gravity and no rigid body here.
///
/// Pass only solid colliders in these slices. Test triggers separately with
/// overlap queries; including one here makes it block movement. This function
/// returns no velocity or grounded flag, and does not implement stepping or
/// slope limits. Resolution uses a bounded number of passes, so a crowded or
/// mutually penetrating set of solids may leave an initial overlap unresolved.
///
/// # Example
///
/// ```
/// use macroquad::math::{slide_sphere_solids, Capsule, vec3};
///
/// let column = Capsule::new(vec3(0.0, 0.0, 0.0), vec3(0.0, 2.0, 0.0), 0.45);
/// let moved = slide_sphere_solids(
///     vec3(-2.0, 1.0, 0.0),
///     0.4,
///     vec3(3.0, 0.0, 0.0),
///     &[],
///     &[],
///     &[],
///     &[column],
/// );
/// assert!((moved.x + 0.85).abs() < 1e-3);
/// assert!((moved.y - 1.0).abs() < 1e-3);
/// ```
///
/// A sphere that only touches another sphere keeps a sideways step.
///
/// ```
/// use macroquad::math::{slide_sphere_solids, Sphere, vec3};
///
/// let rock = Sphere::new(vec3(0.0, 0.5, 0.0), 0.5);
/// let resting = vec3(-0.9, 0.5, 0.0);
/// let moved = slide_sphere_solids(resting, 0.4, vec3(0.0, 0.0, 1.0), &[], &[], &[rock], &[]);
/// assert!((moved.x - resting.x).abs() < 1e-4);
/// assert!((moved.z - 1.0).abs() < 1e-4);
/// ```
pub fn slide_sphere_solids(
    center: Vec3,
    radius: f32,
    delta: Vec3,
    aabbs: &[Aabb],
    obbs: &[Obb],
    spheres: &[Sphere],
    capsules: &[Capsule],
) -> Vec3 {
    if !center.is_finite() || !delta.is_finite() {
        return center;
    }
    if !radius.is_finite() || radius < 0.0 {
        return center + delta;
    }

    let solids = SlideSolids {
        aabbs,
        obbs,
        spheres,
        capsules,
    };
    let mut position = resolve_penetration(center, radius, &solids);
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
        let Some(hit) = first_hit(position, radius, remaining, &solids) else {
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
        match first_hit(position, radius, remaining, &solids) {
            None => position += remaining,
            Some(hit) if hit.t <= TIME_SLOP => {
                let into = remaining.dot(hit.normal);
                if into < 0.0 {
                    remaining -= hit.normal * into;
                }
                if remaining.length_squared() > TIME_SLOP * TIME_SLOP
                    && first_hit(position, radius, remaining, &solids).is_none()
                {
                    position += remaining;
                }
            }
            Some(_) => {}
        }
    }
    position
}

/// Slides equal-radius spheres as one rigid group against static solids.
///
/// `centers` and `delta` are in world coordinates. The result is a **shared
/// translation**, including any initial push-out, not a new center. Add it
/// once to the owning body's position or to every center. All members advance
/// to the earliest collision of any member, then share the remaining tangent.
/// Their separation stays fixed; independent slide results are never averaged.
/// Spheres in the group may overlap each other and do not collide with each
/// other. Yaw, rotation, gravity and trigger detection remain in the caller.
///
/// Pairwise touch and push-out rules are those of [`slide_sphere_solids`], with
/// the same real radius and the same invalid-solid handling. Each push-out
/// pass chooses the largest penetration correction across every member and
/// applies it to the entire group. Equal corrections keep the earlier center,
/// then AABBs, OBBs, spheres and capsules in slice order. Eight such passes
/// bound overlap recovery; an impossible or crowded arrangement may remain
/// overlapping. Motion uses four sweep/slide passes and a final clear-path
/// check, retaining encountered contact planes so a later slide cannot push
/// another member back through an earlier face. Complex arrangements may stop
/// conservatively when that budget is exhausted.
///
/// An empty `centers` slice or a negative/non-finite `radius` permits the free
/// `delta`. Any non-finite center or delta instead returns zero translation.
/// A single center returns the displacement from [`slide_sphere_solids`].
///
/// If using [`super::UniformGrid`] to select solids, query an envelope that
/// covers the whole group and possible deflection, not just the straight start
/// and end bounds. A path-length expansion can cover slide motion; initial
/// overlap recovery can move farther, so recover against the complete solid
/// set before relying on a reduced candidate set. Exclude triggers as usual.
///
/// # Example
///
/// Two spheres stand in for a hull: the leading sphere stops both at a wall,
/// while floor contact keeps the sideways motion.
///
/// ```
/// use macroquad::prelude::*;
///
/// let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(20.0, 1.0, 20.0));
/// let wall = Aabb::from_center_size(vec3(3.0, 1.0, 0.0), vec3(1.0, 4.0, 20.0));
/// let centers = [vec3(-1.0, 0.5, 0.0), vec3(1.0, 0.5, 0.0)];
/// let moved = slide_spheres_solids(
///     &centers, 0.5, vec3(4.0, -0.25, 1.0), &[floor, wall], &[], &[], &[],
/// );
/// assert!((moved - vec3(1.0, 0.0, 1.0)).length() < 1e-4);
/// let next = centers.map(|center| center + moved);
/// assert!((next[1] - next[0] - (centers[1] - centers[0])).length() < 1e-4);
/// ```
pub fn slide_spheres_solids(
    centers: &[Vec3],
    radius: f32,
    delta: Vec3,
    aabbs: &[Aabb],
    obbs: &[Obb],
    spheres: &[Sphere],
    capsules: &[Capsule],
) -> Vec3 {
    if !delta.is_finite() || centers.iter().any(|center| !center.is_finite()) {
        return Vec3::ZERO;
    }
    if centers.is_empty() || !radius.is_finite() || radius < 0.0 {
        return delta;
    }
    if let [center] = centers {
        return slide_sphere_solids(*center, radius, delta, aabbs, obbs, spheres, capsules)
            - *center;
    }

    let solids = SlideSolids {
        aabbs,
        obbs,
        spheres,
        capsules,
    };
    let mut translation = resolve_group_penetration(centers, radius, &solids);
    if !translation.is_finite() {
        return Vec3::ZERO;
    }
    let mut remaining = delta;
    let mut planes = [Vec3::ZERO; 4];
    for pass in 0..planes.len() {
        if remaining.length_squared() <= TIME_SLOP * TIME_SLOP {
            return translation;
        }
        let Some(hit) = first_group_hit(centers, translation, radius, remaining, &solids) else {
            return translation + remaining;
        };
        let t = hit.t.clamp(0.0, 1.0);
        translation += remaining * t;
        planes[pass] = hit.normal;
        remaining = group_tangent(remaining * (1.0 - t), &planes[..=pass]);
    }
    // Never apply a leftover based only on one member's clear path, even
    // after a zero-time contact or a numerically tiny projection.
    if remaining.length_squared() > TIME_SLOP * TIME_SLOP
        && first_group_hit(centers, translation, radius, remaining, &solids).is_none()
    {
        translation += remaining;
    }
    translation
}

/// Project onto the intersection of the encountered outward half-spaces.
/// The nearest feasible vector is on a face, a two-plane crease, or the
/// origin. Checking all candidates avoids sequential projections adding a
/// component back into a previously encountered (non-orthogonal) plane.
fn group_tangent(delta: Vec3, planes: &[Vec3]) -> Vec3 {
    // Rotated normals are only approximately unit length. Use their squared
    // length and wider intermediates so a long projected tangent does not
    // appear to enter its own plane through rounding.
    let delta = delta.as_dvec3();
    let feasible = |candidate: glam::DVec3| {
        planes
            .iter()
            .all(|normal| candidate.dot(normal.as_dvec3()) >= -f64::from(INTO_SLOP))
    };
    if feasible(delta) {
        return delta.as_vec3();
    }
    let mut best = glam::DVec3::ZERO;
    let mut best_score = 0.0;
    let mut consider = |candidate: glam::DVec3| {
        // Minimize |candidate - delta|² without its common |delta|² term.
        // Keeping that term can round a tiny valid tangent to a tie with zero.
        let score = 2.0 * delta.dot(candidate) - candidate.length_squared();
        if score > best_score && candidate.is_finite() && feasible(candidate) {
            best = candidate;
            best_score = score;
        }
    };
    for (i, normal) in planes.iter().enumerate() {
        let normal = normal.as_dvec3();
        consider(delta - normal * (delta.dot(normal) / normal.length_squared()));
        for other in &planes[..i] {
            let crease = normal.cross(other.as_dvec3()).normalize_or_zero();
            consider(crease * delta.dot(crease));
        }
    }
    best.as_vec3()
}

fn first_group_hit(
    centers: &[Vec3],
    translation: Vec3,
    radius: f32,
    delta: Vec3,
    solids: &SlideSolids<'_>,
) -> Option<SlideHit> {
    let mut best = None;
    for center in centers {
        consider(
            &mut best,
            first_hit(*center + translation, radius, delta, solids),
            delta,
        );
    }
    best
}

fn resolve_group_penetration(centers: &[Vec3], radius: f32, solids: &SlideSolids<'_>) -> Vec3 {
    let mut translation = Vec3::ZERO;
    for _ in 0..8 {
        let mut best = None;
        let mut best_len = 0.0;
        for center in centers {
            let center = *center + translation;
            for aabb in solids.aabbs {
                consider_push(
                    &mut best,
                    &mut best_len,
                    push_out_aabb(center, radius, aabb),
                );
            }
            for obb in solids.obbs {
                consider_push(&mut best, &mut best_len, push_out_obb(center, radius, obb));
            }
            for sphere in solids.spheres {
                consider_push(
                    &mut best,
                    &mut best_len,
                    push_out_sphere(center, radius, sphere),
                );
            }
            for capsule in solids.capsules {
                consider_push(
                    &mut best,
                    &mut best_len,
                    push_out_capsule(center, radius, capsule),
                );
            }
        }
        let Some(push) = best else {
            break;
        };
        if best_len <= TIME_SLOP * TIME_SLOP || !push.is_finite() {
            break;
        }
        translation += push;
    }
    translation
}

struct SlideSolids<'a> {
    aabbs: &'a [Aabb],
    obbs: &'a [Obb],
    spheres: &'a [Sphere],
    capsules: &'a [Capsule],
}

fn resolve_penetration(center: Vec3, radius: f32, solids: &SlideSolids<'_>) -> Vec3 {
    let mut center = center;
    for _ in 0..8 {
        let mut best: Option<Vec3> = None;
        let mut best_len = 0.0;
        for aabb in solids.aabbs {
            consider_push(
                &mut best,
                &mut best_len,
                push_out_aabb(center, radius, aabb),
            );
        }
        for obb in solids.obbs {
            consider_push(&mut best, &mut best_len, push_out_obb(center, radius, obb));
        }
        for sphere in solids.spheres {
            consider_push(
                &mut best,
                &mut best_len,
                push_out_sphere(center, radius, sphere),
            );
        }
        for capsule in solids.capsules {
            consider_push(
                &mut best,
                &mut best_len,
                push_out_capsule(center, radius, capsule),
            );
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

fn consider_push(best: &mut Option<Vec3>, best_len: &mut f32, push: Option<Vec3>) {
    let Some(push) = push else {
        return;
    };
    let len = push.length_squared();
    if len > *best_len {
        *best = Some(push);
        *best_len = len;
    }
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

fn first_hit(center: Vec3, radius: f32, delta: Vec3, solids: &SlideSolids<'_>) -> Option<SlideHit> {
    let mut best: Option<SlideHit> = None;
    for aabb in solids.aabbs {
        if usable_aabb(aabb) {
            consider(
                &mut best,
                cast_sphere_aabb(center, radius, delta, aabb),
                delta,
            );
        }
    }
    for obb in solids.obbs {
        consider(
            &mut best,
            cast_sphere_obb(center, radius, delta, obb),
            delta,
        );
    }
    for sphere in solids.spheres {
        consider(
            &mut best,
            cast_sphere_sphere(center, radius, delta, sphere),
            delta,
        );
    }
    for capsule in solids.capsules {
        consider(
            &mut best,
            cast_sphere_capsule(center, radius, delta, capsule),
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
            // A time tolerance must not choose a later hit: its displacement
            // can already pass an earlier face, and a subsequent sweep from
            // inside the solid may no longer find that entry. This applies
            // both to different solids and to different members of a group.
            if hit.t < current.t {
                true
            } else if hit.t > current.t {
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

fn push_out_sphere(center: Vec3, radius: f32, sphere: &Sphere) -> Option<Vec3> {
    if !usable_sphere(sphere) {
        return None;
    }
    let reach = radius + sphere.radius;
    radial_push(center - sphere.center, reach, Vec3::X)
}

fn push_out_capsule(center: Vec3, radius: f32, capsule: &Capsule) -> Option<Vec3> {
    if !usable_capsule(capsule) {
        return None;
    }
    let reach = radius + capsule.radius;
    let on_segment = closest_on_segment(capsule.a, capsule.b, center);
    radial_push(
        center - on_segment,
        reach,
        perpendicular_to_segment(capsule.a, capsule.b),
    )
}

/// Push `center` out to `reach` along `offset`.
///
/// `fallback` is the direction when `offset` is too short to normalize. A
/// distance already at or past `reach` is a touch or a miss, not a push.
fn radial_push(offset: Vec3, reach: f32, fallback: Vec3) -> Option<Vec3> {
    if !reach.is_finite() || reach < 0.0 || !offset.is_finite() {
        return None;
    }
    let dist_sq = offset.length_squared();
    if dist_sq >= reach * reach {
        return None;
    }
    let push = if dist_sq <= 1e-12 {
        fallback * reach
    } else {
        let dist = dist_sq.sqrt();
        offset * ((reach - dist) / dist)
    };
    if push.is_finite() {
        Some(push)
    } else {
        None
    }
}

/// World axis most perpendicular to `b - a`, X before Y before Z on a tie.
///
/// A segment with no length is not an axis, so the result is [`Vec3::X`].
fn perpendicular_to_segment(a: Vec3, b: Vec3) -> Vec3 {
    let axis = b - a;
    let len_sq = axis.length_squared();
    if !len_sq.is_finite() || len_sq <= PLANE_SLOP * PLANE_SLOP {
        return Vec3::X;
    }
    let axis = axis / len_sq.sqrt();
    let ax = axis.x.abs();
    let ay = axis.y.abs();
    let az = axis.z.abs();
    let seed = if ay < ax {
        if az < ay {
            Vec3::Z
        } else {
            Vec3::Y
        }
    } else if az < ax {
        Vec3::Z
    } else {
        Vec3::X
    };
    let perp = seed - axis * seed.dot(axis);
    let perp_len_sq = perp.length_squared();
    if !perp_len_sq.is_finite() || perp_len_sq <= 1e-12 {
        Vec3::X
    } else {
        perp / perp_len_sq.sqrt()
    }
}

fn closest_on_segment(a: Vec3, b: Vec3, point: Vec3) -> Vec3 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if !len_sq.is_finite() || len_sq <= PLANE_SLOP * PLANE_SLOP {
        return a;
    }
    let t = ((point - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    a + ab * t
}

fn cast_sphere_sphere(center: Vec3, radius: f32, delta: Vec3, sphere: &Sphere) -> Option<SlideHit> {
    if !usable_sphere(sphere) {
        return None;
    }
    cast_corner(center, radius + sphere.radius, delta, sphere.center)
}

fn cast_sphere_capsule(
    center: Vec3,
    radius: f32,
    delta: Vec3,
    capsule: &Capsule,
) -> Option<SlideHit> {
    if !usable_capsule(capsule) {
        return None;
    }
    let reach = radius + capsule.radius;
    let mut best: Option<SlideHit> = None;
    consider(
        &mut best,
        cast_edge(center, reach, delta, capsule.a, capsule.b),
        delta,
    );
    consider(
        &mut best,
        cast_corner(center, reach, delta, capsule.a),
        delta,
    );
    consider(
        &mut best,
        cast_corner(center, reach, delta, capsule.b),
        delta,
    );
    best
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
    let t_enter = radial_entry_time(offset_perp, delta_perp, radius)?;
    accept_radial(center, delta, start, axis, length, t_enter)
}

fn cast_corner(center: Vec3, radius: f32, delta: Vec3, corner: Vec3) -> Option<SlideHit> {
    let offset = center - corner;
    if delta.length_squared() <= 1e-12 {
        return None;
    }
    let t_enter = radial_entry_time(offset, delta, radius)?;
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

fn radial_entry_time(offset: Vec3, delta: Vec3, radius: f32) -> Option<f32> {
    let (t_enter, _) = super::sphere_quadratic_roots(offset, delta, radius)?;
    if t_enter < 0.0 {
        // Push-out deliberately leaves tiny overlaps alone. Measure that
        // overlap in world units: a small inward delta can otherwise put the
        // entry far enough before t=0 that the radial cast discards the hit.
        let penetration = radius - offset.length();
        if (0.0..=PLANE_SLOP).contains(&penetration) {
            return Some(0.0);
        }
    }
    Some(t_enter)
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

fn usable_sphere(sphere: &Sphere) -> bool {
    sphere.radius.is_finite() && sphere.radius >= 0.0 && sphere.center.is_finite()
}

fn usable_capsule(capsule: &Capsule) -> bool {
    capsule.radius.is_finite()
        && capsule.radius >= 0.0
        && capsule.a.is_finite()
        && capsule.b.is_finite()
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
    use crate::math::{Capsule, Quat, Sphere, Vec3};

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

    fn cinder_column() -> Capsule {
        Capsule::new(vec3(-1.6, 0.45, -2.4), vec3(-1.6, 1.55, -2.4), 0.45)
    }

    fn cinder_rock() -> Sphere {
        Sphere::new(vec3(2.6, 0.5, 1.2), 0.5)
    }

    #[test]
    fn a_floor_strafe_still_goes_through_slide_sphere() {
        let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(10.0, 1.0, 10.0));
        let radius = 0.5;
        let resting = vec3(0.2, radius, -0.3);
        let delta = vec3(1.25, -0.4, -0.5);
        let through_slide = slide_sphere(resting, radius, delta, &[floor], &[]);
        let through_solids = slide_sphere_solids(resting, radius, delta, &[floor], &[], &[], &[]);
        assert_close(through_slide, resting + vec3(1.25, 0.0, -0.5));
        assert_close(through_solids, through_slide);
    }

    #[test]
    fn a_touching_sphere_keeps_a_strafe_and_drops_the_inward_step() {
        let rock = Sphere::new(Vec3::ZERO, 0.5);
        let radius = 0.4;
        let resting = vec3(0.9, 0.0, 0.0);
        assert!(Sphere::new(resting, radius).overlaps(&rock));

        let sideways = slide_sphere_solids(
            resting,
            radius,
            vec3(0.0, 0.25, 1.25),
            &[],
            &[],
            &[rock],
            &[],
        );
        assert_close(sideways, resting + vec3(0.0, 0.25, 1.25));

        let into = slide_sphere_solids(
            resting,
            radius,
            vec3(-0.5, 0.0, 1.25),
            &[],
            &[],
            &[rock],
            &[],
        );
        assert_close(into, resting + vec3(0.0, 0.0, 1.25));
        // The tangent plane leaves a round rock. The inward step is gone.
        let gap = (into - rock.center).length() - rock.radius - radius;
        assert!(gap > -1e-3, "sank into the rock: gap {gap}");
    }

    #[test]
    fn a_shallow_round_overlap_blocks_inward_motion_without_sticking() {
        let rock = Sphere::new(Vec3::Y, 0.5);
        let column = Capsule::new(Vec3::ZERO, Vec3::Y * 2.0, 0.5);
        let radius = 0.4;
        // A nine-micrometer overlap is below the push-out threshold. Its
        // negative entry time must not discard a four-centimeter inward move.
        let resting = vec3(-0.899991, 1.0, 0.0);
        let spheres = [rock];
        let capsules = [column];
        for (spheres, capsules) in [(&spheres[..], &[][..]), (&[][..], &capsules[..])] {
            let inward = slide_sphere_solids(
                resting,
                radius,
                vec3(0.04, 0.0, 0.0),
                &[],
                &[],
                spheres,
                capsules,
            );
            assert_close(inward, resting);

            for delta in [vec3(0.0, 0.0, 0.04), vec3(-0.04, 0.0, 0.0)] {
                let moved =
                    slide_sphere_solids(resting, radius, delta, &[], &[], spheres, capsules);
                assert_close(moved, resting + delta);
            }
        }
    }

    #[test]
    fn a_capsule_column_stops_a_horizontal_move() {
        let column = Capsule::new(Vec3::ZERO, vec3(0.0, 2.0, 0.0), 0.45);
        let radius = 0.4;
        let start = vec3(-2.0, 1.0, 0.0);
        let end = slide_sphere_solids(start, radius, vec3(4.0, 0.5, 0.0), &[], &[], &[], &[column]);
        assert_close(end, vec3(-0.85, 1.5, 0.0));
        let gap = (-end.x) - column.radius - radius;
        assert!(gap.abs() < 1e-3, "left the column: gap {gap}");
    }

    #[test]
    fn cinder_court_column_and_rock_block_a_pass_through() {
        let radius = 0.4;
        let column = cinder_column();
        let rock = cinder_rock();
        // 0.35 outside the surface. Delta 1.9 puts the column's midpoint gap
        // at -0.60; delta 2.0 puts the rock's at -0.65. `slide_sphere` never
        // sees either solid, so those steps used to finish on start + delta.
        let column_start = vec3(-2.8, 1.0, -2.4);
        let column_delta = vec3(1.9, 0.0, 0.0);
        let column_mid = column_start + column_delta * 0.5;
        let column_gap = (column_mid - vec3(-1.6, column_mid.y, -2.4)).length() - 0.45 - radius;
        assert!((column_gap + 0.60).abs() < 1e-4, "gap {column_gap}");

        let rock_start = vec3(1.35, 0.5, 1.2);
        let rock_delta = vec3(2.0, 0.0, 0.0);
        let rock_mid = rock_start + rock_delta * 0.5;
        let rock_gap = (rock_mid - rock.center).length() - rock.radius - radius;
        assert!((rock_gap + 0.65).abs() < 1e-4, "gap {rock_gap}");

        let stopped_column = slide_sphere_solids(
            column_start,
            radius,
            column_delta,
            &[],
            &[],
            &[rock],
            &[column],
        );
        assert_close(stopped_column, vec3(-2.45, 1.0, -2.4));
        assert!(
            (stopped_column - (column_start + column_delta)).length() > 1.0,
            "column did not block: {stopped_column:?}"
        );

        let stopped_rock =
            slide_sphere_solids(rock_start, radius, rock_delta, &[], &[], &[rock], &[column]);
        assert_close(stopped_rock, vec3(1.7, 0.5, 1.2));
        assert!((stopped_rock - (rock_start + rock_delta)).length() > 1.0);
    }

    #[test]
    fn a_round_column_does_not_block_like_its_square_stand_in() {
        let column = cinder_column();
        let radius = 0.4;
        let axis = vec3(-1.6, 1.0, -2.4);
        // Beside the expanded box's face: the square stand-in reaches 0.85
        // from the axis, and the cylinder only reaches about 0.72 here.
        let start = axis + vec3(1.1, 0.0, 0.45);
        let delta = vec3(-0.3, 0.0, 0.0);
        let end = slide_sphere_solids(start, radius, delta, &[], &[], &[], &[column]);
        assert_close(end, start + delta);
        assert!(!column.overlaps_sphere(&Sphere::new(end, radius)));

        let square = Aabb::from_points([column.a, column.b])
            .unwrap()
            .expanded(column.radius);
        let blocked = slide_sphere(start, radius, delta, &[square], &[]);
        assert!(
            (blocked.x - (start.x + delta.x)).abs() > 0.04,
            "the aabb stand-in should catch this corner, got {blocked:?}"
        );
    }

    #[test]
    fn a_center_inside_a_sphere_or_capsule_is_pushed_out() {
        let rock = Sphere::new(Vec3::ZERO, 1.0);
        let radius = 0.5;
        let buried = vec3(0.2, 0.0, 0.0);
        let pushed = slide_sphere_solids(buried, radius, Vec3::ZERO, &[], &[], &[rock], &[]);
        assert_close(pushed, vec3(1.5, 0.0, 0.0));

        let coincident =
            slide_sphere_solids(Vec3::ZERO, radius, Vec3::ZERO, &[], &[], &[rock], &[]);
        assert_close(coincident, vec3(1.5, 0.0, 0.0));

        let then_in =
            slide_sphere_solids(buried, radius, vec3(-0.4, 0.0, 0.3), &[], &[], &[rock], &[]);
        assert_close(then_in, vec3(1.5, 0.0, 0.3));

        let column = Capsule::new(Vec3::ZERO, vec3(0.0, 2.0, 0.0), 0.5);
        let on_segment = slide_sphere_solids(
            vec3(0.0, 1.0, 0.0),
            0.4,
            Vec3::ZERO,
            &[],
            &[],
            &[],
            &[column],
        );
        assert_close(on_segment, vec3(0.9, 1.0, 0.0));

        // The rail runs along X, so +X is the long way out. Y wins the tie with Z.
        let rail = Capsule::new(vec3(-1.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), 0.25);
        let off_rail = slide_sphere_solids(Vec3::ZERO, 0.25, Vec3::ZERO, &[], &[], &[], &[rail]);
        assert_close(off_rail, vec3(0.0, 0.5, 0.0));
    }

    #[test]
    fn a_fast_move_does_not_tunnel_through_a_thin_capsule() {
        let wire = Capsule::new(vec3(0.0, -2.0, 0.0), vec3(0.0, 2.0, 0.0), 0.02);
        let end = slide_sphere_solids(
            vec3(-3.0, 0.0, 0.0),
            0.3,
            vec3(10.0, 0.5, 0.0),
            &[],
            &[],
            &[],
            &[wire],
        );
        assert_close(end, vec3(-0.32, 0.5, 0.0));
    }

    #[test]
    fn a_capsule_end_cap_stops_a_move_past_the_segment() {
        let rail = Capsule::new(vec3(-1.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), 0.3);
        let end = slide_sphere_solids(
            vec3(1.0, 2.0, 0.0),
            0.2,
            vec3(0.0, -4.0, 0.0),
            &[],
            &[],
            &[],
            &[rail],
        );
        assert_close(end, vec3(1.0, 0.5, 0.0));
    }

    #[test]
    fn a_slide_far_from_the_origin_matches_the_same_sphere() {
        let rock = Sphere::new(Vec3::ZERO, 0.5);
        let start = vec3(-2.0, 0.4, 0.2);
        let delta = vec3(5.0, -0.1, 0.4);
        let near = slide_sphere_solids(start, 0.4, delta, &[], &[], &[rock], &[]);
        let shift = vec3(20_000.0, -15_000.0, 8_000.0);
        let far = slide_sphere_solids(
            start + shift,
            0.4,
            delta,
            &[],
            &[],
            &[rock.offset(shift)],
            &[],
        );
        let shifted = far - shift;
        let drift = (shifted - near).length();
        // f32 at this distance is coarser than the local slide. A radius-early
        // miss would be tenths, not a couple of hundredths.
        assert!(
            drift < 0.02,
            "far slide drifted {drift}: near {near:?} far {shifted:?}"
        );
        assert!(
            (near - (start + delta)).length() > 0.5,
            "did not stop: {near:?}"
        );
        let gap = near.length() - rock.radius - 0.4;
        assert!(gap > -1e-3, "penetrated: gap {gap}");
        let far_gap = (far - rock.offset(shift).center).length() - rock.radius - 0.4;
        assert!(far_gap > -0.02, "penetrated far away: gap {far_gap}");
    }

    #[test]
    fn empty_spheres_and_capsules_do_not_block() {
        let solid = Sphere::new(Vec3::ZERO, 1.0);
        let bad_spheres = [
            Sphere::new(Vec3::ZERO, -1.0),
            Sphere::new(Vec3::ZERO, f32::NAN),
            Sphere::new(vec3(f32::NAN, 0.0, 0.0), 1.0),
        ];
        let bad_capsules = [
            Capsule::new(Vec3::ZERO, Vec3::Y, -0.5),
            Capsule::new(Vec3::ZERO, vec3(f32::NAN, 1.0, 0.0), 0.5),
        ];
        let start = vec3(-2.0, 0.0, 0.0);
        let delta = vec3(5.0, 0.2, -0.3);
        assert_close(
            slide_sphere_solids(start, 0.4, delta, &[], &[], &bad_spheres, &bad_capsules),
            start + delta,
        );
        assert_close(
            slide_sphere_solids(start, -1.0, delta, &[], &[], &[solid], &[]),
            start + delta,
        );
        let blocked = slide_sphere_solids(start, 0.4, delta, &[], &[], &[solid], &[]);
        assert!(blocked.x < -0.5, "solid sphere did not block: {blocked:?}");
    }

    #[test]
    fn a_floor_and_a_rock_keep_the_floor_and_stop_on_the_rock() {
        let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(10.0, 1.0, 10.0));
        let rock = Sphere::new(vec3(2.0, 0.5, 0.0), 0.5);
        let radius = 0.5;
        let start = vec3(0.0, radius, 0.0);
        let end = slide_sphere_solids(
            start,
            radius,
            vec3(3.0, -0.4, 0.0),
            &[floor],
            &[],
            &[rock],
            &[],
        );
        assert_close(end, vec3(1.0, radius, 0.0));
        assert_outside_aabb(end, radius, &floor);
    }
}
