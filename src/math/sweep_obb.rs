//! Translational AABB/OBB sweeps. Both boxes keep their orientation.

use glam::{DQuat, DVec3, Vec3};

use super::{Aabb, Obb, SweepHit};

const TOUCH_SLOP: f64 = 1e-4;
const INTO_SLOP: f64 = 1e-5;
const WORLD_AXES: [DVec3; 3] = [DVec3::X, DVec3::Y, DVec3::Z];

impl Aabb {
    /// Sweeps this axis-aligned box along `delta` against the static `obb`.
    ///
    /// Both boxes keep their orientation. The complete box is tested with
    /// continuous separating axes, including edge/edge axes. `t` is in
    /// `0..=1`; `normal` points out of `obb`, and `point` is a representative
    /// position in the boxes' shared region at impact, all in world space.
    ///
    /// An initial overlap deeper than `1e-4` returns `t == 0`, even with no
    /// motion or motion away. A touch or shallower overlap returns `t == 0`
    /// only when the delta enters a touching boundary; tangential and outward
    /// motion miss. A degenerate flat box is allowed. Invalid boxes, non-finite
    /// inputs, and a zero or unusable rotation miss. Finite nonzero rotations
    /// are normalized, following the other [`Obb`] queries.
    ///
    /// Equal entry times or initial penetration depths keep the earlier axis:
    /// world X/Y/Z, box X/Y/Z, then each world/box axis cross product. A touch
    /// selects the most opposing touching normal, keeping that order on ties.
    /// Use [`Aabb::sweep`] when the static solid is an [`Aabb`].
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, Obb, Quat, Vec3, vec3};
    ///
    /// let wall = Obb::new(Vec3::ZERO, vec3(0.05, 2.0, 3.0),
    ///                     Quat::from_rotation_y(0.4));
    /// let body = Aabb::from_center_size(vec3(-3.0, 0.0, 0.0),
    ///                                   vec3(0.5, 1.8, 0.5));
    /// let hit = body.sweep_obb(vec3(6.0, 0.0, 0.0), &wall).unwrap();
    /// assert!((0.0..1.0).contains(&hit.t));
    /// assert!(hit.normal.x < 0.0);
    /// ```
    pub fn sweep_obb(&self, delta: Vec3, obb: &Obb) -> Option<SweepHit> {
        if !delta.is_finite()
            || !self.min.is_finite()
            || !self.max.is_finite()
            || self.min.cmpgt(self.max).any()
        {
            return None;
        }
        let rotation = obb.pose()?;
        // Do the projection arithmetic in f64, including center/half-size
        // construction: finite f32 corners can overflow (min + max) in f32.
        let rotation = DQuat::from_xyzw(
            rotation.x as f64,
            rotation.y as f64,
            rotation.z as f64,
            rotation.w as f64,
        )
        .normalize();
        let origin = obb.center.as_dvec3();
        let moving = SweepBox {
            center: (self.min.as_dvec3() + self.max.as_dvec3()) * 0.5 - origin,
            half: (self.max.as_dvec3() - self.min.as_dvec3()) * 0.5,
            axes: WORLD_AXES,
        };
        let solid = SweepBox {
            center: DVec3::ZERO,
            half: obb.half_extents.as_dvec3(),
            axes: WORLD_AXES.map(|axis| rotation * axis),
        };
        let delta = delta.as_dvec3();
        let mut axes = [DVec3::ZERO; 15];
        axes[..3].copy_from_slice(&WORLD_AXES);
        axes[3..6].copy_from_slice(&solid.axes);
        for (i, world) in WORLD_AXES.iter().enumerate() {
            for (j, local) in solid.axes.iter().enumerate() {
                axes[6 + i * 3 + j] = world.cross(*local);
            }
        }

        let mut entering = f64::NEG_INFINITY;
        let mut leaving = f64::INFINITY;
        let mut entry_normal = DVec3::ZERO;
        let mut penetration = f64::INFINITY;
        let mut overlap_normal = DVec3::ZERO;
        let mut touching_normal = DVec3::ZERO;
        let mut touching_velocity = -INTO_SLOP;
        let mut initially_overlapping = true;
        for axis in axes {
            let length = axis.length();
            if length == 0.0 {
                continue;
            }
            let axis = axis / length;
            let distance = moving.center.dot(axis);
            let radius = moving.radius(axis) + solid.radius(axis);
            let depth = radius - distance.abs();
            let velocity = delta.dot(axis);
            // f64 roundoff in a projection is distinct from the public
            // shallow-overlap tolerance, which must not turn a gap into t=0.
            let epsilon = 32.0 * f64::EPSILON * radius.max(distance.abs()).max(1.0);
            initially_overlapping &= depth >= -epsilon;
            let normal = if distance < 0.0 { -axis } else { axis };
            if depth < penetration {
                penetration = depth;
                overlap_normal = normal;
            }
            if depth <= TOUCH_SLOP {
                // Coincident flat intervals have two outward sides.
                let normal = if radius == 0.0 && distance == 0.0 && velocity > 0.0 {
                    -axis
                } else {
                    normal
                };
                let into = delta.dot(normal);
                if into < touching_velocity {
                    touching_velocity = into;
                    touching_normal = normal;
                }
            }
            if velocity == 0.0 {
                if depth < -epsilon {
                    return None;
                }
                continue;
            }
            let mut near = (-radius - distance) / velocity;
            let mut far = (radius - distance) / velocity;
            let mut normal = -axis;
            if near > far {
                std::mem::swap(&mut near, &mut far);
                normal = axis;
            }
            if near > entering {
                entering = near;
                entry_normal = normal;
            }
            leaving = leaving.min(far);
        }
        let (time, normal) = if initially_overlapping {
            if penetration > TOUCH_SLOP {
                (0.0, overlap_normal)
            } else if touching_normal != DVec3::ZERO {
                (0.0, touching_normal)
            } else {
                return None;
            }
        } else {
            if entering > leaving + 1e-12
                || leaving < 0.0
                || !(-1e-12..=1.0 + 1e-12).contains(&entering)
                || delta.dot(entry_normal) >= -INTO_SLOP
            {
                return None;
            }
            (entering.clamp(0.0, 1.0), entry_normal)
        };
        let at = SweepBox {
            center: moving.center + delta * time,
            ..moving
        };
        let point = shared_point(&at, &solid)? + origin;
        let point = point.as_vec3();
        let normal = normal.as_vec3();
        if !point.is_finite() || !normal.is_finite() {
            return None;
        }
        Some(SweepHit {
            t: time as f32,
            normal,
            point,
        })
    }
}

#[derive(Clone, Copy)]
struct SweepBox {
    // Coordinates relative to the static box center avoid cancellation when
    // clipping an intersection far from the world origin.
    center: DVec3,
    half: DVec3,
    axes: [DVec3; 3],
}

impl SweepBox {
    fn radius(&self, axis: DVec3) -> f64 {
        self.half.x * axis.dot(self.axes[0]).abs()
            + self.half.y * axis.dot(self.axes[1]).abs()
            + self.half.z * axis.dot(self.axes[2]).abs()
    }

    fn corner(&self, index: usize) -> DVec3 {
        let signs = DVec3::new(
            if index & 1 == 0 { -1.0 } else { 1.0 },
            if index & 2 == 0 { -1.0 } else { 1.0 },
            if index & 4 == 0 { -1.0 } else { 1.0 },
        ) * self.half;
        self.center + self.axes[0] * signs.x + self.axes[1] * signs.y + self.axes[2] * signs.z
    }

    fn clip_edge(&self, start: DVec3, end: DVec3) -> Option<(DVec3, DVec3)> {
        let offset = start - self.center;
        let delta = end - start;
        let mut near = 0.0_f64;
        let mut far = 1.0_f64;
        for (i, axis) in self.axes.iter().enumerate() {
            let position = offset.dot(*axis);
            let velocity = delta.dot(*axis);
            let extent = self.half[i];
            let epsilon = 64.0 * f64::EPSILON * extent.max(position.abs()).max(1.0);
            if velocity == 0.0 {
                if position.abs() > extent + epsilon {
                    return None;
                }
                continue;
            }
            let a = (-extent - position) / velocity;
            let b = (extent - position) / velocity;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far + 1e-12 {
                return None;
            }
        }
        // A touching pair can invert the final interval by a few f64 ulps.
        let near = near.clamp(0.0, 1.0);
        let far = far.clamp(near, 1.0);
        Some((start + delta * near, start + delta * far))
    }
}

/// Intersection vertices are corners inside the other solid or an edge
/// crossing its faces. Averaging clipped edge endpoints gives a representative
/// shared point for face, edge, corner and initial volume contacts, without
/// allocating a contact manifold. Duplicate endpoints do not affect containment.
fn shared_point(a: &SweepBox, b: &SweepBox) -> Option<DVec3> {
    let mut point = DVec3::ZERO;
    let mut count = 0;
    for (edges, clipper) in [(a, b), (b, a)] {
        for corner in 0..8 {
            for axis in 0..3 {
                let other = corner ^ (1 << axis);
                if other > corner {
                    if let Some((start, end)) =
                        clipper.clip_edge(edges.corner(corner), edges.corner(other))
                    {
                        point += start + end;
                        count += 2;
                    }
                }
            }
        }
    }
    if count == 0 {
        None
    } else {
        Some(point / count as f64)
    }
}
