//! Oriented boxes in 3D.
//!
//! An [`Obb`] is a center, half extents, and a rotation. It does not assume an
//! up axis. [`Aabb`] is the axis-aligned special case (identity rotation).

use glam::{vec3, Mat4, Quat, Vec3};

use super::{Aabb, RayHit, Sphere};

/// An oriented box: center, half extents, and rotation.
///
/// `half_extents` is half of the full size [`Aabb::from_center_size`] and
/// [`crate::models::draw_cube`] take. A component of zero is a flat box. A
/// negative component is empty: the box contains nothing, overlaps nothing,
/// and the ray query misses.
///
/// `rotation` should be a unit quaternion. Queries normalize a non-zero finite
/// quaternion, so a uniform scale on the quaternion does not change the box. A
/// zero or non-finite quaternion misses. The rotation does not pick an up axis.
///
/// Boundaries are inclusive, matching [`Aabb`]: a point on a face is inside,
/// and a box that only touches an [`Aabb`] or a [`Sphere`] overlaps it.
///
/// # Example
///
/// ```
/// use macroquad::math::{Obb, Quat, Vec3, vec3};
///
/// let box_ = Obb::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY);
/// assert!(box_.contains(vec3(1.0, 0.0, 0.0)));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Obb {
    /// Center of the box.
    pub center: Vec3,
    /// Half of the full size along the box's local axes.
    pub half_extents: Vec3,
    /// Rotation from local axes to world axes.
    pub rotation: Quat,
}

impl Obb {
    /// Creates a box from its center, half extents, and rotation.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Obb, Quat, Vec3, vec3};
    ///
    /// let box_ = Obb::new(Vec3::ZERO, vec3(1.0, 1.0, 1.0), Quat::IDENTITY);
    /// assert!(!box_.contains(vec3(1.1, 0.0, 0.0)));
    /// ```
    pub const fn new(center: Vec3, half_extents: Vec3, rotation: Quat) -> Obb {
        Obb {
            center,
            half_extents,
            rotation,
        }
    }

    /// Box posed by `world`, using the same full size as `draw_cube`.
    ///
    /// The matrix is decomposed as translation, rotation, and scale.
    /// `center` is the translation. `half_extents` are half the absolute
    /// scale on each axis, so
    /// `Mat4::from_scale_rotation_translation(full_size, rotation, center)`
    /// matches a rotated `draw_cube(center, full_size, ...)`. A negative
    /// scale stays a positive extent. Shear is not preserved.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Mat4, Obb, Quat, vec3};
    ///
    /// let world = Mat4::from_scale_rotation_translation(
    ///     vec3(2.0, 4.0, 6.0),
    ///     Quat::IDENTITY,
    ///     vec3(1.0, 2.0, 3.0),
    /// );
    /// let box_ = Obb::from_world_matrix(world);
    /// assert!((box_.center - vec3(1.0, 2.0, 3.0)).length() < 1e-5);
    /// assert!((box_.half_extents - vec3(1.0, 2.0, 3.0)).length() < 1e-5);
    /// ```
    pub fn from_world_matrix(world: Mat4) -> Obb {
        let (scale, rotation, translation) = world.to_scale_rotation_translation();
        Obb {
            center: translation,
            half_extents: vec3(
                scale.x.abs() * 0.5,
                scale.y.abs() * 0.5,
                scale.z.abs() * 0.5,
            ),
            rotation,
        }
    }

    /// Returns true when `point` lies inside the box, including the boundary.
    ///
    /// An empty box (a negative half extent) contains nothing. A non-finite
    /// point, center, extent, or rotation misses.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Obb, Quat, Vec3};
    ///
    /// let box_ = Obb::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY);
    /// assert!(box_.contains(Vec3::ZERO));
    /// assert!(!box_.contains(Vec3::new(2.0, 0.0, 0.0)));
    /// ```
    pub fn contains(&self, point: Vec3) -> bool {
        let Some(local) = self.local_point(point) else {
            return false;
        };
        local.x.abs() <= self.half_extents.x
            && local.y.abs() <= self.half_extents.y
            && local.z.abs() <= self.half_extents.z
    }

    /// Closest point on or inside the solid box to `point`.
    ///
    /// A point already inside is returned unchanged. An empty box, or a box
    /// whose rotation cannot be used, returns `center`.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Obb, Quat, Vec3, vec3};
    ///
    /// let box_ = Obb::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY);
    /// assert_eq!(box_.closest_point(vec3(4.0, 0.0, 0.0)), Vec3::X);
    /// ```
    pub fn closest_point(&self, point: Vec3) -> Vec3 {
        let Some(rotation) = self.unit_rotation() else {
            return self.center;
        };
        if self.is_empty()
            || !self.center.is_finite()
            || !self.half_extents.is_finite()
            || !point.is_finite()
        {
            return self.center;
        }
        let local = rotation.inverse() * (point - self.center);
        let clamped = vec3(
            clamp_axis(local.x, -self.half_extents.x, self.half_extents.x),
            clamp_axis(local.y, -self.half_extents.y, self.half_extents.y),
            clamp_axis(local.z, -self.half_extents.z, self.half_extents.z),
        );
        rotation * clamped + self.center
    }

    /// Returns true when this box and `aabb` overlap, including a shared face,
    /// edge, or corner.
    ///
    /// Uses the separating-axis test: the box's three axes, the world's three
    /// axes, and their cross products. Assumes `aabb` is normalized (`min` <=
    /// `max` on every axis); an inverted box overlaps nothing. An empty
    /// oriented box overlaps nothing. A non-finite corner or rotation misses.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, Obb, Quat, Vec3, vec3};
    ///
    /// let box_ = Obb::new(vec3(1.5, 0.5, 0.5), Vec3::ONE, Quat::IDENTITY);
    /// let crate_ = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
    /// assert!(box_.overlaps_aabb(&crate_));
    /// ```
    pub fn overlaps_aabb(&self, aabb: &Aabb) -> bool {
        let Some(rotation) = self.pose() else {
            return false;
        };
        if !aabb.min.is_finite() || !aabb.max.is_finite() {
            return false;
        }
        let aabb_half = aabb.half_extents();
        if aabb_half.x < 0.0 || aabb_half.y < 0.0 || aabb_half.z < 0.0 {
            return false;
        }
        let delta = aabb.center() - self.center;
        let axes = world_axes(rotation);
        if face_separated(delta, self.half_extents, axes, aabb_half) {
            return false;
        }
        for obb_axis in axes {
            let cross_x = obb_axis.cross(Vec3::X);
            let cross_y = obb_axis.cross(Vec3::Y);
            let cross_z = obb_axis.cross(Vec3::Z);
            if separated(delta, cross_x, self.half_extents, axes, aabb_half)
                || separated(delta, cross_y, self.half_extents, axes, aabb_half)
                || separated(delta, cross_z, self.half_extents, axes, aabb_half)
            {
                return false;
            }
        }
        true
    }

    /// Returns true when this box and `sphere` overlap, including a touch.
    ///
    /// A negative sphere radius is empty and overlaps nothing, matching
    /// [`Sphere::overlaps`].
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Obb, Quat, Sphere, Vec3, vec3};
    ///
    /// let box_ = Obb::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY);
    /// let ball = Sphere::new(vec3(1.5, 0.0, 0.0), 0.6);
    /// assert!(box_.overlaps_sphere(&ball));
    /// ```
    pub fn overlaps_sphere(&self, sphere: &Sphere) -> bool {
        if sphere.radius < 0.0 || !sphere.radius.is_finite() || !sphere.center.is_finite() {
            return false;
        }
        if self.pose().is_none() {
            return false;
        }
        let closest = self.closest_point(sphere.center);
        distance_squared(closest, sphere.center) <= sphere.radius * sphere.radius
    }

    /// Ray test against this box.
    ///
    /// The ray is rotated into box space and tested with [`Aabb::ray_intersect`].
    /// `t` is unchanged by that rotation. The hit point and outward normal are
    /// rotated back to world space. `direction` does not need to be normalized.
    /// Hits with `t < 0` are rejected. An origin inside the closed box, or on
    /// the boundary and pointing outward, reports `t == 0` and a zero normal,
    /// same as [`Aabb`].
    ///
    /// An empty box misses. A non-finite origin, direction, center, extent, or
    /// rotation misses.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Obb, Quat, Vec3, vec3};
    ///
    /// let box_ = Obb::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY);
    /// let hit = box_.ray_intersect(vec3(-3.0, 0.0, 0.0), Vec3::X).unwrap();
    /// assert_eq!(hit.t, 2.0);
    /// assert_eq!(hit.point, -Vec3::X);
    /// assert_eq!(hit.normal, -Vec3::X);
    /// ```
    pub fn ray_intersect(&self, origin: Vec3, direction: Vec3) -> Option<RayHit> {
        let rotation = self.pose()?;
        if !origin.is_finite() || !direction.is_finite() {
            return None;
        }
        let inverse = rotation.inverse();
        let local_origin = inverse * (origin - self.center);
        let local_direction = inverse * direction;
        let local = Aabb::from_center_size(Vec3::ZERO, self.half_extents * 2.0);
        let hit = local.ray_intersect(local_origin, local_direction)?;
        let point = rotation * hit.point + self.center;
        let normal = rotation * hit.normal;
        if !hit.t.is_finite() || !point.is_finite() || !normal.is_finite() {
            return None;
        }
        Some(RayHit {
            t: hit.t,
            point,
            normal,
        })
    }

    fn is_empty(&self) -> bool {
        self.half_extents.x < 0.0 || self.half_extents.y < 0.0 || self.half_extents.z < 0.0
    }

    /// Unit rotation of a usable box, or [`None`] when a query should miss.
    fn pose(&self) -> Option<Quat> {
        if self.is_empty() || !self.center.is_finite() || !self.half_extents.is_finite() {
            return None;
        }
        self.unit_rotation()
    }

    fn unit_rotation(&self) -> Option<Quat> {
        if !self.rotation.is_finite() {
            return None;
        }
        let length = self.rotation.length();
        if length == 0.0 || !length.is_finite() {
            return None;
        }
        let rotation = self.rotation * (1.0 / length);
        if rotation.is_finite() {
            Some(rotation)
        } else {
            None
        }
    }

    fn local_point(&self, point: Vec3) -> Option<Vec3> {
        let rotation = self.pose()?;
        if !point.is_finite() {
            return None;
        }
        Some(rotation.inverse() * (point - self.center))
    }
}

fn world_axes(rotation: Quat) -> [Vec3; 3] {
    [rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z]
}

fn face_separated(delta: Vec3, obb_half: Vec3, obb_axes: [Vec3; 3], aabb_half: Vec3) -> bool {
    separated(delta, Vec3::X, obb_half, obb_axes, aabb_half)
        || separated(delta, Vec3::Y, obb_half, obb_axes, aabb_half)
        || separated(delta, Vec3::Z, obb_half, obb_axes, aabb_half)
        || separated(delta, obb_axes[0], obb_half, obb_axes, aabb_half)
        || separated(delta, obb_axes[1], obb_half, obb_axes, aabb_half)
        || separated(delta, obb_axes[2], obb_half, obb_axes, aabb_half)
}

/// True when `axis` separates the oriented box from the axis-aligned box.
///
/// A zero axis separates nothing. Both sides of the comparison scale with the
/// axis, so it does not need to be a unit vector.
fn separated(
    delta: Vec3,
    axis: Vec3,
    obb_half: Vec3,
    obb_axes: [Vec3; 3],
    aabb_half: Vec3,
) -> bool {
    let distance = delta.dot(axis).abs();
    let obb_radius = obb_half.x * obb_axes[0].dot(axis).abs()
        + obb_half.y * obb_axes[1].dot(axis).abs()
        + obb_half.z * obb_axes[2].dot(axis).abs();
    let aabb_radius =
        aabb_half.x * axis.x.abs() + aabb_half.y * axis.y.abs() + aabb_half.z * axis.z.abs();
    distance > obb_radius + aabb_radius
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

const fn distance_squared(left: Vec3, right: Vec3) -> f32 {
    let dx = left.x - right.x;
    let dy = left.y - right.y;
    let dz = left.z - right.z;
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{vec3, Mat4, Quat, Vec3};

    fn assert_vec3_close(left: Vec3, right: Vec3) {
        let delta = left - right;
        assert!(
            delta.x.abs() < 1e-5 && delta.y.abs() < 1e-5 && delta.z.abs() < 1e-5,
            "left {left:?} right {right:?}"
        );
    }

    fn unit_obb() -> Obb {
        Obb::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY)
    }

    #[test]
    fn identity_box_matches_an_aabb() {
        let obb = unit_obb();
        let aabb = Aabb::from_center_size(Vec3::ZERO, Vec3::splat(2.0));
        assert!(obb.contains(Vec3::ONE));
        assert!(obb.contains(vec3(-1.0, 0.0, 1.0)));
        assert!(!obb.contains(vec3(1.01, 0.0, 0.0)));
        assert!(obb.overlaps_aabb(&aabb));
        let touching = Aabb::from_min_max(Vec3::X, vec3(2.0, 1.0, 1.0));
        assert!(obb.overlaps_aabb(&touching));
        let separated = Aabb::from_min_max(vec3(1.01, -1.0, -1.0), vec3(2.0, 1.0, 1.0));
        assert!(!obb.overlaps_aabb(&separated));
        assert!(obb.overlaps_sphere(&Sphere::new(vec3(2.0, 0.0, 0.0), 1.0)));
        assert!(!obb.overlaps_sphere(&Sphere::new(vec3(2.0, 0.0, 0.0), 0.99)));
        assert!(!obb.overlaps_sphere(&Sphere::new(Vec3::ZERO, -1.0)));
    }

    #[test]
    fn rotation_moves_the_long_axis() {
        let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let long = Obb::new(Vec3::ZERO, vec3(2.0, 0.5, 0.5), rotation);
        // +90° about Y sends local +X to world -Z. Stay off the exact face so
        // a tiny cosine error cannot flip the inclusive test.
        assert!(long.contains(rotation * vec3(1.9, 0.0, 0.0)));
        assert!(!long.contains(rotation * vec3(2.1, 0.0, 0.0)));
        assert!(long.contains(rotation * vec3(0.0, 0.0, 0.4)));
        assert!(!long.contains(rotation * vec3(0.0, 0.0, 0.7)));
        assert!(long.contains(rotation * vec3(2.0, 0.0, 0.0)));

        let along_face = Aabb::from_center_size(rotation * vec3(2.0, 0.0, 0.0), Vec3::splat(0.2));
        assert!(long.overlaps_aabb(&along_face));
        let beside = Aabb::from_center_size(rotation * vec3(0.0, 0.0, 1.2), Vec3::splat(0.2));
        assert!(!long.overlaps_aabb(&beside));

        let end_center = rotation * vec3(3.0, 0.0, 0.0);
        assert!(long.overlaps_sphere(&Sphere::new(end_center, 1.2)));
        assert!(!long.overlaps_sphere(&Sphere::new(end_center, 0.8)));

        // Local origin x = -5, direction +X. The near face is local x = -2.
        let hit = long
            .ray_intersect(rotation * vec3(-5.0, 0.0, 0.0), rotation * Vec3::X)
            .unwrap();
        assert!((hit.t - 3.0).abs() < 1e-4);
        assert_vec3_close(hit.point, rotation * vec3(-2.0, 0.0, 0.0));
        assert_vec3_close(hit.normal, rotation * -Vec3::X);
        assert!(long
            .ray_intersect(rotation * vec3(-5.0, 0.0, 0.0), rotation * -Vec3::X)
            .is_none());
    }

    #[test]
    fn a_yaw_reaches_a_diagonal_an_aabb_would_miss() {
        let yaw = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        let obb = Obb::new(Vec3::ZERO, vec3(2.0, 0.5, 0.5), yaw);
        let diagonal = vec3(0.6, 0.0, -0.6);
        assert!(obb.contains(diagonal));
        assert!(!Obb::new(Vec3::ZERO, vec3(2.0, 0.5, 0.5), Quat::IDENTITY).contains(diagonal));

        let probe = Aabb::from_center_size(vec3(0.7, 0.0, -0.7), Vec3::splat(0.1));
        assert!(obb.overlaps_aabb(&probe));
        assert!(!Obb::new(Vec3::ZERO, vec3(2.0, 0.5, 0.5), Quat::IDENTITY).overlaps_aabb(&probe));

        let sphere = Sphere::new(vec3(0.7, 0.0, -0.7), 0.05);
        assert!(obb.overlaps_sphere(&sphere));
        assert!(!Obb::new(Vec3::ZERO, vec3(2.0, 0.5, 0.5), Quat::IDENTITY).overlaps_sphere(&sphere));
        assert_vec3_close(obb.closest_point(diagonal), diagonal);
        let outside = vec3(3.0, 0.0, -3.0);
        let closest = obb.closest_point(outside);
        assert!(obb.contains(closest));
        assert!(!obb.contains(outside));
    }

    #[test]
    fn ray_inside_and_non_finite_input_match_aabb() {
        let obb = unit_obb();
        let inside = obb.ray_intersect(Vec3::ZERO, Vec3::Y).unwrap();
        assert_eq!(inside.t, 0.0);
        assert_eq!(inside.point, Vec3::ZERO);
        assert_eq!(inside.normal, Vec3::ZERO);

        let leaving = obb.ray_intersect(Vec3::X, Vec3::X).unwrap();
        assert_eq!(leaving.t, 0.0);
        assert_eq!(leaving.normal, Vec3::ZERO);

        assert!(obb
            .ray_intersect(vec3(f32::NAN, 0.0, 0.0), Vec3::X)
            .is_none());
        assert!(obb
            .ray_intersect(vec3(-2.0, 0.0, 0.0), vec3(f32::INFINITY, 0.0, 0.0))
            .is_none());
        let broken = Obb::new(vec3(f32::NAN, 0.0, 0.0), Vec3::ONE, Quat::IDENTITY);
        assert!(broken.ray_intersect(Vec3::ZERO, Vec3::X).is_none());
        assert!(!broken.contains(Vec3::ZERO));
        assert!(!broken.overlaps_aabb(&Aabb::from_center_size(Vec3::ZERO, Vec3::ONE)));
    }

    #[test]
    fn an_empty_or_unusable_box_misses() {
        let empty = Obb::new(Vec3::ZERO, vec3(-1.0, 1.0, 1.0), Quat::IDENTITY);
        assert!(!empty.contains(Vec3::ZERO));
        assert!(!empty.overlaps_aabb(&Aabb::from_center_size(Vec3::ZERO, Vec3::splat(4.0))));
        assert!(!empty.overlaps_sphere(&Sphere::new(Vec3::ZERO, 4.0)));
        assert!(empty.ray_intersect(vec3(-2.0, 0.0, 0.0), Vec3::X).is_none());
        assert_eq!(empty.closest_point(Vec3::X), Vec3::ZERO);

        let zero_quat = Obb::new(Vec3::ZERO, Vec3::ONE, Quat::from_xyzw(0.0, 0.0, 0.0, 0.0));
        assert!(!zero_quat.contains(Vec3::ZERO));
        assert!(zero_quat.ray_intersect(Vec3::ZERO, Vec3::X).is_none());

        let inverted = Aabb::from_min_max(Vec3::ONE, Vec3::ZERO);
        assert!(!unit_obb().overlaps_aabb(&inverted));
    }

    #[test]
    fn world_matrix_uses_the_draw_cube_full_size() {
        let size = vec3(2.0, 4.0, 6.0);
        let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let center = vec3(1.0, 2.0, 3.0);
        let world = Mat4::from_scale_rotation_translation(size, rotation, center);
        let obb = Obb::from_world_matrix(world);
        assert_vec3_close(obb.center, center);
        assert_vec3_close(obb.half_extents, size * 0.5);
        assert!((obb.rotation.dot(rotation).abs() - 1.0).abs() < 1e-5);

        let corner = center + rotation * (size * 0.5);
        assert_vec3_close(obb.closest_point(corner), corner);
        assert!(obb.contains(center + rotation * (size * 0.5 * 0.99)));
        let outside = center + rotation * vec3(size.x * 0.5 + 0.2, 0.0, 0.0);
        assert!(!obb.contains(outside));

        let mirrored =
            Mat4::from_scale_rotation_translation(vec3(-2.0, 4.0, 6.0), Quat::IDENTITY, center);
        let mirror = Obb::from_world_matrix(mirrored);
        assert_vec3_close(mirror.half_extents, vec3(1.0, 2.0, 3.0));
        assert_vec3_close(mirror.center, center);
        assert!(mirror.contains(center + vec3(1.0, 2.0, 3.0)));
        assert!(!mirror.contains(center + vec3(1.1, 0.0, 0.0)));
    }

    #[test]
    fn default_box_is_a_point_at_the_origin() {
        let obb = Obb::default();
        assert!(obb.contains(Vec3::ZERO));
        assert!(!obb.contains(vec3(0.1, 0.0, 0.0)));
        assert_eq!(obb.half_extents, Vec3::ZERO);
        assert_eq!(obb.rotation, Quat::IDENTITY);
    }
}
