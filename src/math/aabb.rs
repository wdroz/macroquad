//! Axis-aligned bounding boxes in 3D.
//!
//! Boxes are stored as minimum and maximum corners in world space. They do not
//! assume a particular up axis: macroquad cameras may be Y-up or Z-up.

use glam::{vec3, Vec3};

/// Parametric intersection of a ray with a 3D shape.
///
/// `point = origin + direction * t`. When `direction` is a unit vector, `t` is
/// the distance from the origin. The same struct is shared by every primitive
/// so callers can compare hits with `<` on `t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// Parametric time along the ray.
    pub t: f32,
    /// World-space position of the hit.
    pub point: Vec3,
    /// Outward unit normal at the impact.
    ///
    /// Zero when the origin is already inside the shape and the query reports
    /// the origin itself (`t == 0`) without an entering face. For an [`Aabb`]
    /// this is also zero when the origin lies on the boundary and the ray
    /// points outward, leaving the box immediately. A [`Plane`](crate::math::Plane)
    /// has no interior, so a hit on it reports the plane normal, including when
    /// the origin already lies on the plane.
    pub normal: Vec3,
}

/// An axis-aligned bounding box, defined by its minimum and maximum corners.
///
/// `min` is the corner with the smaller coordinates and `max` is the opposite
/// corner. A box with `min == max` is a single point. Inverted extents (`min`
/// greater than `max` on an axis) are preserved by the constructors; call
/// [`Aabb::normalized`] before overlap or ray queries if that happens.
///
/// This matches [`crate::models::draw_cube`] / [`crate::models::draw_cube_wires`]:
/// [`Aabb::from_center_size`] uses the same center and full size those
/// functions take, so a gameplay box and its wireframe stay in lockstep.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aabb {
    /// Corner with the smaller coordinates on each axis.
    pub min: Vec3,
    /// Corner with the larger coordinates on each axis.
    pub max: Vec3,
}

impl Aabb {
    /// Creates a box from its minimum and maximum corners.
    ///
    /// The corners are stored as given. Swap them with [`Aabb::normalized`]
    /// when a component of `min` is greater than `max`.
    pub const fn from_min_max(min: Vec3, max: Vec3) -> Aabb {
        Aabb { min, max }
    }

    /// Creates a box from its center and full size on each axis.
    ///
    /// `size` is the full width, height, and depth, the same vector passed to
    /// `draw_cube`. Prefer non-negative components; a negative component flips
    /// that axis.
    pub const fn from_center_size(center: Vec3, size: Vec3) -> Aabb {
        Aabb {
            min: vec3(
                center.x - size.x * 0.5,
                center.y - size.y * 0.5,
                center.z - size.z * 0.5,
            ),
            max: vec3(
                center.x + size.x * 0.5,
                center.y + size.y * 0.5,
                center.z + size.z * 0.5,
            ),
        }
    }

    /// Builds the smallest box containing every point.
    ///
    /// Returns [`None`] when `points` is empty.
    pub fn from_points<I>(points: I) -> Option<Aabb>
    where
        I: IntoIterator<Item = Vec3>,
    {
        let mut points = points.into_iter();
        let first = points.next()?;
        let mut aabb = Aabb {
            min: first,
            max: first,
        };
        for point in points {
            aabb.min = aabb.min.min(point);
            aabb.max = aabb.max.max(point);
        }
        Some(aabb)
    }

    /// Center of the box.
    pub const fn center(self) -> Vec3 {
        vec3(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }

    /// Full size on each axis (`max - min`).
    pub const fn size(self) -> Vec3 {
        vec3(
            self.max.x - self.min.x,
            self.max.y - self.min.y,
            self.max.z - self.min.z,
        )
    }

    /// Half the full size on each axis.
    pub const fn half_extents(self) -> Vec3 {
        vec3(
            (self.max.x - self.min.x) * 0.5,
            (self.max.y - self.min.y) * 0.5,
            (self.max.z - self.min.z) * 0.5,
        )
    }

    /// Returns true when `point` lies inside the box, including the boundary.
    ///
    /// Assumes the box is normalized (`min` <= `max` on every axis).
    pub const fn contains(&self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// Returns true when the two boxes overlap, including a shared face.
    ///
    /// Assumes both boxes are normalized.
    pub const fn overlaps(&self, other: &Aabb) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    /// Intersection of the two boxes, or [`None`] when they are disjoint.
    ///
    /// Boxes that only touch along a face, edge, or corner produce a flat
    /// result whose size is zero on the touching axes. Assumes both boxes are
    /// normalized.
    pub const fn intersect(&self, other: Aabb) -> Option<Aabb> {
        let min = vec3(
            if self.min.x > other.min.x {
                self.min.x
            } else {
                other.min.x
            },
            if self.min.y > other.min.y {
                self.min.y
            } else {
                other.min.y
            },
            if self.min.z > other.min.z {
                self.min.z
            } else {
                other.min.z
            },
        );
        let max = vec3(
            if self.max.x < other.max.x {
                self.max.x
            } else {
                other.max.x
            },
            if self.max.y < other.max.y {
                self.max.y
            } else {
                other.max.y
            },
            if self.max.z < other.max.z {
                self.max.z
            } else {
                other.max.z
            },
        );
        if min.x > max.x || min.y > max.y || min.z > max.z {
            None
        } else {
            Some(Aabb { min, max })
        }
    }

    /// Smallest box that contains both inputs.
    pub const fn combine_with(self, other: Aabb) -> Aabb {
        Aabb {
            min: vec3(
                if self.min.x < other.min.x {
                    self.min.x
                } else {
                    other.min.x
                },
                if self.min.y < other.min.y {
                    self.min.y
                } else {
                    other.min.y
                },
                if self.min.z < other.min.z {
                    self.min.z
                } else {
                    other.min.z
                },
            ),
            max: vec3(
                if self.max.x > other.max.x {
                    self.max.x
                } else {
                    other.max.x
                },
                if self.max.y > other.max.y {
                    self.max.y
                } else {
                    other.max.y
                },
                if self.max.z > other.max.z {
                    self.max.z
                } else {
                    other.max.z
                },
            ),
        }
    }

    /// Translates both corners by `offset`.
    pub const fn offset(self, offset: Vec3) -> Aabb {
        Aabb {
            min: vec3(
                self.min.x + offset.x,
                self.min.y + offset.y,
                self.min.z + offset.z,
            ),
            max: vec3(
                self.max.x + offset.x,
                self.max.y + offset.y,
                self.max.z + offset.z,
            ),
        }
    }

    /// Grows the box by `margin` on every side.
    ///
    /// A negative margin shrinks it. The box is not re-normalized, so a large
    /// negative margin can invert it.
    pub const fn expanded(self, margin: f32) -> Aabb {
        Aabb {
            min: vec3(
                self.min.x - margin,
                self.min.y - margin,
                self.min.z - margin,
            ),
            max: vec3(
                self.max.x + margin,
                self.max.y + margin,
                self.max.z + margin,
            ),
        }
    }

    /// Swaps inverted axes so `min` <= `max` on every component.
    pub const fn normalized(self) -> Aabb {
        Aabb {
            min: vec3(
                if self.min.x < self.max.x {
                    self.min.x
                } else {
                    self.max.x
                },
                if self.min.y < self.max.y {
                    self.min.y
                } else {
                    self.max.y
                },
                if self.min.z < self.max.z {
                    self.min.z
                } else {
                    self.max.z
                },
            ),
            max: vec3(
                if self.min.x > self.max.x {
                    self.min.x
                } else {
                    self.max.x
                },
                if self.min.y > self.max.y {
                    self.min.y
                } else {
                    self.max.y
                },
                if self.min.z > self.max.z {
                    self.min.z
                } else {
                    self.max.z
                },
            ),
        }
    }

    /// Closest point on or inside the box to `point`.
    ///
    /// A point already inside is returned unchanged. Assumes the box is
    /// normalized; otherwise the result is not meaningful.
    pub const fn closest_point(&self, point: Vec3) -> Vec3 {
        vec3(
            clamp_axis(point.x, self.min.x, self.max.x),
            clamp_axis(point.y, self.min.y, self.max.y),
            clamp_axis(point.z, self.min.z, self.max.z),
        )
    }

    /// Slab test of a ray against this box.
    ///
    /// `direction` does not need to be normalized. A component of exactly zero
    /// means the ray is parallel to that pair of planes: the origin must lie
    /// inside the slab or the ray misses. Hits with `t < 0` (the box is behind
    /// the origin) are rejected. Touching a face, edge, or corner counts as a
    /// hit.
    ///
    /// When the origin is inside the closed box, the hit is at `t == 0` with
    /// a zero normal. The same `t == 0` / zero-normal result is returned when
    /// the origin is on the boundary and `direction` points outward.
    ///
    /// A non-finite origin, direction, or corner misses. Assumes the box is
    /// normalized.
    pub fn ray_intersect(&self, origin: Vec3, direction: Vec3) -> Option<RayHit> {
        if !origin.is_finite()
            || !direction.is_finite()
            || !self.min.is_finite()
            || !self.max.is_finite()
        {
            return None;
        }

        let mut range = RayRange {
            t_min: 0.0,
            t_max: f32::INFINITY,
            normal: Vec3::ZERO,
        };

        if !range.slab(origin.x, direction.x, self.min.x, self.max.x, Vec3::X)
            || !range.slab(origin.y, direction.y, self.min.y, self.max.y, Vec3::Y)
            || !range.slab(origin.z, direction.z, self.min.z, self.max.z, Vec3::Z)
        {
            return None;
        }
        if !range.t_min.is_finite() {
            return None;
        }

        let point = vec3(
            origin.x + direction.x * range.t_min,
            origin.y + direction.y * range.t_min,
            origin.z + direction.z * range.t_min,
        );
        if !point.is_finite() {
            return None;
        }
        Some(RayHit {
            t: range.t_min,
            point,
            normal: range.normal,
        })
    }
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

/// Running entry/exit interval for the slab test.
struct RayRange {
    t_min: f32,
    t_max: f32,
    normal: Vec3,
}

impl RayRange {
    /// One axis of the Kay-Kajiya slab test.
    ///
    /// Updates the running entry time, exit time, and outward normal. Returns
    /// false when this axis rules the ray out.
    fn slab(&mut self, origin: f32, direction: f32, min_b: f32, max_b: f32, axis: Vec3) -> bool {
        if direction == 0.0 {
            return origin >= min_b && origin <= max_b;
        }

        let inv_d = 1.0 / direction;
        let mut t_near = (min_b - origin) * inv_d;
        let mut t_far = (max_b - origin) * inv_d;
        let mut near_normal = vec3(-axis.x, -axis.y, -axis.z);
        if t_near > t_far {
            std::mem::swap(&mut t_near, &mut t_far);
            near_normal = axis;
        }
        // `>=` so an origin sitting on a face and pointing inward still records
        // that face's outward normal. Strict `>` would leave the normal at zero.
        if t_near >= self.t_min {
            self.t_min = t_near;
            self.normal = near_normal;
        }
        if t_far < self.t_max {
            self.t_max = t_far;
        }
        self.t_min <= self.t_max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_box() -> Aabb {
        Aabb::from_min_max(vec3(0.0, 0.0, 0.0), vec3(2.0, 2.0, 2.0))
    }

    fn assert_vec3_close(left: Vec3, right: Vec3) {
        let delta = left - right;
        assert!(
            delta.x.abs() < 1e-5 && delta.y.abs() < 1e-5 && delta.z.abs() < 1e-5,
            "left {left:?} right {right:?}"
        );
    }

    #[test]
    fn center_size_matches_draw_cube_convention() {
        let aabb = Aabb::from_center_size(vec3(1.0, 2.0, 3.0), vec3(2.0, 4.0, 6.0));
        assert_vec3_close(aabb.min, vec3(0.0, 0.0, 0.0));
        assert_vec3_close(aabb.max, vec3(2.0, 4.0, 6.0));
        assert_vec3_close(aabb.center(), vec3(1.0, 2.0, 3.0));
        assert_vec3_close(aabb.size(), vec3(2.0, 4.0, 6.0));
        assert_vec3_close(aabb.half_extents(), vec3(1.0, 2.0, 3.0));
    }

    #[test]
    fn from_points_spans_the_cloud() {
        assert!(Aabb::from_points([]).is_none());
        let point = Aabb::from_points([vec3(1.0, 2.0, 3.0)]).unwrap();
        assert_eq!(point.min, point.max);
        let aabb = Aabb::from_points([
            vec3(1.0, 0.0, -2.0),
            vec3(-4.0, 3.0, 1.0),
            vec3(2.0, -1.0, 0.5),
        ])
        .unwrap();
        assert_vec3_close(aabb.min, vec3(-4.0, -1.0, -2.0));
        assert_vec3_close(aabb.max, vec3(2.0, 3.0, 1.0));
    }

    #[test]
    fn contains_is_inclusive_on_the_boundary() {
        let aabb = unit_box();
        assert!(aabb.contains(vec3(0.0, 0.0, 0.0)));
        assert!(aabb.contains(vec3(2.0, 2.0, 2.0)));
        assert!(aabb.contains(vec3(1.0, 1.0, 1.0)));
        assert!(aabb.contains(vec3(2.0, 0.0, 1.0)));
        assert!(!aabb.contains(vec3(-0.01, 1.0, 1.0)));
        assert!(!aabb.contains(vec3(1.0, 2.01, 1.0)));
        assert!(!aabb.contains(vec3(1.0, 1.0, -0.01)));
    }

    #[test]
    fn overlaps_includes_touching_and_containment() {
        let aabb = unit_box();
        assert!(aabb.overlaps(&aabb));
        let touching = Aabb::from_min_max(vec3(2.0, 0.0, 0.0), vec3(3.0, 1.0, 1.0));
        assert!(aabb.overlaps(&touching));
        assert!(touching.overlaps(&aabb));
        let separated = Aabb::from_min_max(vec3(2.01, 0.0, 0.0), vec3(3.0, 1.0, 1.0));
        assert!(!aabb.overlaps(&separated));
        let inside = Aabb::from_min_max(vec3(0.5, 0.5, 0.5), vec3(1.0, 1.0, 1.0));
        assert!(aabb.overlaps(&inside));
        assert!(inside.overlaps(&aabb));
    }

    #[test]
    fn intersect_partial_touch_and_miss() {
        let aabb = unit_box();
        let partial = Aabb::from_min_max(vec3(1.0, 1.0, 1.0), vec3(3.0, 3.0, 3.0));
        let hit = aabb.intersect(partial).unwrap();
        assert_vec3_close(hit.min, vec3(1.0, 1.0, 1.0));
        assert_vec3_close(hit.max, vec3(2.0, 2.0, 2.0));

        let touching = Aabb::from_min_max(vec3(2.0, 0.0, 0.0), vec3(3.0, 1.0, 1.0));
        let flat = aabb.intersect(touching).unwrap();
        assert_vec3_close(flat.min, vec3(2.0, 0.0, 0.0));
        assert_vec3_close(flat.max, vec3(2.0, 1.0, 1.0));

        let separated = Aabb::from_min_max(vec3(3.0, 0.0, 0.0), vec3(4.0, 1.0, 1.0));
        assert!(aabb.intersect(separated).is_none());
    }

    #[test]
    fn combine_offset_expand_and_normalize() {
        let aabb = Aabb::from_min_max(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
        let other = Aabb::from_min_max(vec3(2.0, -1.0, 0.5), vec3(3.0, 0.0, 4.0));
        let combined = aabb.combine_with(other);
        assert_vec3_close(combined.min, vec3(0.0, -1.0, 0.0));
        assert_vec3_close(combined.max, vec3(3.0, 1.0, 4.0));

        let moved = aabb.offset(vec3(1.0, 2.0, 3.0));
        assert_vec3_close(moved.min, vec3(1.0, 2.0, 3.0));
        assert_vec3_close(moved.max, vec3(2.0, 3.0, 4.0));

        let grown = aabb.expanded(0.5);
        assert_vec3_close(grown.min, vec3(-0.5, -0.5, -0.5));
        assert_vec3_close(grown.max, vec3(1.5, 1.5, 1.5));

        let inverted = Aabb::from_min_max(vec3(1.0, 2.0, 3.0), vec3(0.0, 5.0, 1.0));
        let fixed = inverted.normalized();
        assert_vec3_close(fixed.min, vec3(0.0, 2.0, 1.0));
        assert_vec3_close(fixed.max, vec3(1.0, 5.0, 3.0));
    }

    #[test]
    fn closest_point_clamps_outside_points() {
        let aabb = unit_box();
        assert_vec3_close(aabb.closest_point(vec3(1.0, 1.0, 1.0)), vec3(1.0, 1.0, 1.0));
        assert_vec3_close(
            aabb.closest_point(vec3(5.0, 1.0, -1.0)),
            vec3(2.0, 1.0, 0.0),
        );
        assert_vec3_close(
            aabb.closest_point(vec3(-1.0, -1.0, -1.0)),
            vec3(0.0, 0.0, 0.0),
        );
    }

    #[test]
    fn ray_hits_face_with_outward_normal() {
        let aabb = unit_box();
        let hit = aabb
            .ray_intersect(vec3(-1.0, 1.0, 1.0), vec3(1.0, 0.0, 0.0))
            .unwrap();
        assert!((hit.t - 1.0).abs() < 1e-5);
        assert_vec3_close(hit.point, vec3(0.0, 1.0, 1.0));
        assert_vec3_close(hit.normal, vec3(-1.0, 0.0, 0.0));

        let from_max = aabb
            .ray_intersect(vec3(3.0, 1.0, 1.0), vec3(-1.0, 0.0, 0.0))
            .unwrap();
        assert!((from_max.t - 1.0).abs() < 1e-5);
        assert_vec3_close(from_max.point, vec3(2.0, 1.0, 1.0));
        assert_vec3_close(from_max.normal, Vec3::X);
    }

    #[test]
    fn ray_misses_when_separated_or_pointing_away() {
        let aabb = unit_box();
        assert!(aabb
            .ray_intersect(vec3(-1.0, 3.0, 1.0), vec3(1.0, 0.0, 0.0))
            .is_none());
        assert!(aabb
            .ray_intersect(vec3(3.0, 1.0, 1.0), vec3(1.0, 0.0, 0.0))
            .is_none());
        assert!(aabb
            .ray_intersect(vec3(5.0, 1.0, 1.0), Vec3::ZERO)
            .is_none());
        assert!(aabb
            .ray_intersect(vec3(f32::NAN, 1.0, 1.0), Vec3::X)
            .is_none());
        // A NaN component used to be ignored, reporting a hit at the origin.
        assert!(aabb
            .ray_intersect(vec3(-1.0, 1.0, 1.0), vec3(f32::NAN, 0.0, 0.0))
            .is_none());
        assert!(aabb
            .ray_intersect(vec3(-1.0, 1.0, 1.0), vec3(f32::INFINITY, 0.0, 0.0))
            .is_none());
        let broken = Aabb::from_min_max(vec3(f32::NAN, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
        assert!(broken.ray_intersect(Vec3::ZERO, Vec3::X).is_none());
    }

    #[test]
    fn ray_inside_and_on_the_boundary() {
        let aabb = unit_box();
        let inside = aabb
            .ray_intersect(vec3(1.0, 1.0, 1.0), vec3(0.0, 1.0, 0.0))
            .unwrap();
        assert_eq!(inside.t, 0.0);
        assert_vec3_close(inside.point, vec3(1.0, 1.0, 1.0));
        assert_eq!(inside.normal, Vec3::ZERO);

        let entering = aabb
            .ray_intersect(vec3(0.0, 1.0, 1.0), vec3(1.0, 0.0, 0.0))
            .unwrap();
        assert_eq!(entering.t, 0.0);
        assert_vec3_close(entering.normal, vec3(-1.0, 0.0, 0.0));

        let leaving = aabb
            .ray_intersect(vec3(0.0, 1.0, 1.0), vec3(-1.0, 0.0, 0.0))
            .unwrap();
        assert_eq!(leaving.t, 0.0);
        assert_eq!(leaving.normal, Vec3::ZERO);

        let stationary_inside = aabb.ray_intersect(vec3(1.0, 1.0, 1.0), Vec3::ZERO).unwrap();
        assert_eq!(stationary_inside.t, 0.0);
        assert_eq!(stationary_inside.point, vec3(1.0, 1.0, 1.0));
    }

    #[test]
    fn ray_grazes_a_face_and_respects_non_unit_direction() {
        let aabb = unit_box();
        let grazing = aabb
            .ray_intersect(vec3(-1.0, 2.0, 1.0), vec3(1.0, 0.0, 0.0))
            .unwrap();
        assert_vec3_close(grazing.point, vec3(0.0, 2.0, 1.0));
        assert_vec3_close(grazing.normal, vec3(-1.0, 0.0, 0.0));

        let scaled = aabb
            .ray_intersect(vec3(1.0, 1.0, -5.0), vec3(0.0, 0.0, 2.0))
            .unwrap();
        assert!((scaled.t - 2.5).abs() < 1e-5);
        assert_vec3_close(scaled.point, vec3(1.0, 1.0, 0.0));
        assert_vec3_close(scaled.normal, vec3(0.0, 0.0, -1.0));
    }

    #[test]
    fn default_box_is_the_origin() {
        let aabb = Aabb::default();
        assert!(aabb.contains(Vec3::ZERO));
        assert!(!aabb.contains(vec3(0.1, 0.0, 0.0)));
        assert_eq!(aabb.size(), Vec3::ZERO);
    }
}
