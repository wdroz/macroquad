//! View frustum as six inward planes.
//!
//! The planes come from a view-projection matrix. They cull points, spheres,
//! and boxes. They are not a collision shape.

use glam::{vec3, Mat4, Vec3, Vec4};

use super::{Aabb, Plane, Sphere};

/// Closed view volume bounded by six inward planes.
///
/// `left`, `right`, `bottom`, `top`, `near`, and `far` point into the volume.
/// Each plane is a unit [`Plane`]: positive [`Plane::signed_distance`] is inside.
/// A point on a plane is inside, matching the inclusive bounds of [`Aabb`] and
/// [`Sphere`].
///
/// [`Frustum::from_view_projection`] reads an OpenGL clip matrix, `clip = M * world`,
/// with x, y, and z in `-1..=1` after the perspective divide. That is the matrix
/// from [`Mat4::perspective_rh_gl`] or [`Mat4::orthographic_rh_gl`] times
/// [`Mat4::look_at_rh`], and the one [`Camera::matrix`](crate::camera::Camera::matrix)
/// builds. [`Camera3D::frustum`](crate::camera::Camera3D::frustum) calls this.
///
/// Containment (`contains_point`, `contains_sphere`, `contains_aabb`) is the
/// closed volume. Intersection tests a shape against the six infinite planes, so
/// a sphere or box that only crosses a plane past a corner can still count as
/// intersecting. That false positive is safe for culling: a shape that really
/// meets the volume is not dropped. This is not a collision query.
///
/// # Example
///
/// ```
/// use macroquad::math::{Frustum, Mat4, Vec3};
///
/// let projection = Mat4::orthographic_rh_gl(-1.0, 1.0, -1.0, 1.0, 0.1, 10.0);
/// let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
/// let frustum = Frustum::from_view_projection(projection * view).unwrap();
/// assert!(frustum.contains_point(Vec3::ZERO));
/// assert!(!frustum.contains_point(Vec3::new(0.0, 0.0, 6.0)));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    /// Inward plane on the view's left.
    pub left: Plane,
    /// Inward plane on the view's right.
    pub right: Plane,
    /// Inward plane on the view's bottom.
    pub bottom: Plane,
    /// Inward plane on the view's top.
    pub top: Plane,
    /// Inward near plane. The camera sits on the outside of this plane.
    pub near: Plane,
    /// Inward far plane.
    pub far: Plane,
}

impl Frustum {
    /// Six inward planes of a GL view-projection matrix.
    ///
    /// `matrix` maps a world column vector to clip space, and clip `z` runs
    /// from `-1` on the near plane to `1` on the far plane. Returns [`None`]
    /// when `matrix` is non-finite or a plane normal is zero.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Frustum, Mat4, Vec3};
    ///
    /// let projection = Mat4::perspective_rh_gl(90.0_f32.to_radians(), 1.0, 1.0, 10.0);
    /// let view = Mat4::look_at_rh(Vec3::ZERO, -Vec3::Z, Vec3::Y);
    /// let frustum = Frustum::from_view_projection(projection * view).unwrap();
    /// assert!(frustum.contains_point(Vec3::new(0.0, 0.0, -2.0)));
    /// assert!(Frustum::from_view_projection(Mat4::ZERO).is_none());
    /// ```
    pub fn from_view_projection(matrix: Mat4) -> Option<Frustum> {
        if !matrix.is_finite() {
            return None;
        }
        let r0 = matrix.row(0);
        let r1 = matrix.row(1);
        let r2 = matrix.row(2);
        let r3 = matrix.row(3);
        Some(Frustum {
            left: plane_from_clip(r3 + r0)?,
            right: plane_from_clip(r3 - r0)?,
            bottom: plane_from_clip(r3 + r1)?,
            top: plane_from_clip(r3 - r1)?,
            near: plane_from_clip(r3 + r2)?,
            far: plane_from_clip(r3 - r2)?,
        })
    }

    /// The six planes in order: left, right, bottom, top, near, far.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Frustum, Mat4};
    ///
    /// let frustum = Frustum::from_view_projection(Mat4::IDENTITY).unwrap();
    /// assert_eq!(frustum.planes().len(), 6);
    /// ```
    pub const fn planes(self) -> [Plane; 6] {
        [
            self.left,
            self.right,
            self.bottom,
            self.top,
            self.near,
            self.far,
        ]
    }

    /// Returns true when `point` is inside every plane, including the surface.
    ///
    /// A non-finite point is outside.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Frustum, Mat4, Vec3};
    ///
    /// let frustum = Frustum::from_view_projection(Mat4::IDENTITY).unwrap();
    /// assert!(frustum.contains_point(Vec3::ZERO));
    /// assert!(frustum.contains_point(Vec3::ONE));
    /// assert!(!frustum.contains_point(Vec3::new(1.1, 0.0, 0.0)));
    /// ```
    pub fn contains_point(&self, point: Vec3) -> bool {
        if !point.is_finite() {
            return false;
        }
        self.planes()
            .iter()
            .all(|plane| plane.signed_distance(point) >= 0.0)
    }

    /// Returns true when every point of `sphere` is inside the volume.
    ///
    /// A sphere that only touches a plane from the inside is contained. A
    /// negative or non-finite radius is empty and is not contained. This test
    /// is exact: a contained sphere meets the convex volume, not only the
    /// infinite planes.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Frustum, Mat4, Sphere, Vec3};
    ///
    /// let frustum = Frustum::from_view_projection(Mat4::IDENTITY).unwrap();
    /// let inside = Sphere::new(Vec3::ZERO, 0.5);
    /// assert!(frustum.contains_sphere(&inside));
    /// assert!(!frustum.contains_sphere(&Sphere::new(Vec3::ZERO, 1.5)));
    /// ```
    pub fn contains_sphere(&self, sphere: &Sphere) -> bool {
        if !usable_sphere(sphere) {
            return false;
        }
        self.planes()
            .iter()
            .all(|plane| plane.signed_distance(sphere.center) >= sphere.radius)
    }

    /// Returns true when `sphere` may overlap the volume, including a touch.
    ///
    /// A center more than `radius` outside any plane is rejected. A sphere that
    /// only reaches a plane past a corner can still return true. A negative or
    /// non-finite radius is empty and does not intersect.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Frustum, Mat4, Sphere, Vec3};
    ///
    /// let frustum = Frustum::from_view_projection(Mat4::IDENTITY).unwrap();
    /// let touching = Sphere::new(Vec3::new(1.5, 0.0, 0.0), 0.5);
    /// assert!(frustum.intersects_sphere(&touching));
    /// assert!(!frustum.intersects_sphere(&Sphere::new(Vec3::new(3.0, 0.0, 0.0), 0.5)));
    /// ```
    pub fn intersects_sphere(&self, sphere: &Sphere) -> bool {
        if !usable_sphere(sphere) {
            return false;
        }
        self.planes()
            .iter()
            .all(|plane| plane.signed_distance(sphere.center) >= -sphere.radius)
    }

    /// Returns true when every point of `aabb` is inside the volume.
    ///
    /// Assumes the box is normalized (`min` <= `max` on every axis). A box that
    /// only touches a plane from the inside is contained. A non-finite corner
    /// is not. This test is exact for the convex volume.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, Frustum, Mat4, vec3};
    ///
    /// let frustum = Frustum::from_view_projection(Mat4::IDENTITY).unwrap();
    /// let inside = Aabb::from_min_max(vec3(-0.2, -0.2, -0.2), vec3(0.2, 0.2, 0.2));
    /// assert!(frustum.contains_aabb(&inside));
    /// ```
    pub fn contains_aabb(&self, aabb: &Aabb) -> bool {
        if !aabb.min.is_finite() || !aabb.max.is_finite() {
            return false;
        }
        self.planes().iter().all(|plane| {
            let inner = extreme_corner(aabb, plane.normal, false);
            plane.signed_distance(inner) >= 0.0
        })
    }

    /// Returns true when `aabb` may overlap the volume, including a touch.
    ///
    /// Assumes the box is normalized. A box completely outside one plane is
    /// rejected. A box that stays outside the volume but still crosses a plane
    /// past a corner can return true, so a cull pass keeps it. A non-finite
    /// corner does not intersect.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, Frustum, Mat4, vec3};
    ///
    /// let frustum = Frustum::from_view_projection(Mat4::IDENTITY).unwrap();
    /// let crossing = Aabb::from_min_max(vec3(0.5, -0.2, -0.2), vec3(1.5, 0.2, 0.2));
    /// let outside = Aabb::from_min_max(vec3(2.0, -0.2, -0.2), vec3(3.0, 0.2, 0.2));
    /// assert!(frustum.intersects_aabb(&crossing));
    /// assert!(!frustum.intersects_aabb(&outside));
    /// ```
    pub fn intersects_aabb(&self, aabb: &Aabb) -> bool {
        if !aabb.min.is_finite() || !aabb.max.is_finite() {
            return false;
        }
        self.planes().iter().all(|plane| {
            let outer = extreme_corner(aabb, plane.normal, true);
            plane.signed_distance(outer) >= 0.0
        })
    }
}

fn usable_sphere(sphere: &Sphere) -> bool {
    sphere.radius.is_finite() && sphere.radius >= 0.0 && sphere.center.is_finite()
}

fn plane_from_clip(row: Vec4) -> Option<Plane> {
    let length_sq = row.x * row.x + row.y * row.y + row.z * row.z;
    if !length_sq.is_finite() || length_sq <= 1e-16 || !row.w.is_finite() {
        return None;
    }
    let inv = 1.0 / length_sq.sqrt();
    let normal = vec3(row.x * inv, row.y * inv, row.z * inv);
    let distance = -row.w * inv;
    if !normal.is_finite() || !distance.is_finite() {
        return None;
    }
    Some(Plane::new(normal, distance))
}

/// Corner furthest along `normal` when `along` is set, otherwise furthest against it.
fn extreme_corner(aabb: &Aabb, normal: Vec3, along: bool) -> Vec3 {
    let choose = |component: f32, min: f32, max: f32| {
        let take_max = if along {
            component >= 0.0
        } else {
            component < 0.0
        };
        if take_max {
            max
        } else {
            min
        }
    };
    vec3(
        choose(normal.x, aabb.min.x, aabb.max.x),
        choose(normal.y, aabb.min.y, aabb.max.y),
        choose(normal.z, aabb.min.z, aabb.max.z),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{vec3, Mat4, Vec3};

    fn assert_close(left: f32, right: f32) {
        assert!(
            (left - right).abs() < 1e-3,
            "left {left} right {right} delta {}",
            (left - right).abs()
        );
    }

    fn perspective_at_origin() -> Frustum {
        let projection = Mat4::perspective_rh_gl(90.0_f32.to_radians(), 1.0, 1.0, 10.0);
        let view = Mat4::look_at_rh(Vec3::ZERO, -Vec3::Z, Vec3::Y);
        Frustum::from_view_projection(projection * view).unwrap()
    }

    #[test]
    fn identity_matrix_is_the_ndc_cube() {
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY).unwrap();
        assert!(frustum.contains_point(Vec3::ZERO));
        assert!(frustum.contains_point(Vec3::ONE));
        assert!(frustum.contains_point(-Vec3::ONE));
        assert!(!frustum.contains_point(vec3(1.1, 0.0, 0.0)));
        assert!(!frustum.contains_point(vec3(0.0, -1.1, 0.0)));
        assert!(!frustum.contains_point(vec3(0.0, 0.0, 1.1)));
        assert!(!frustum.contains_point(vec3(f32::NAN, 0.0, 0.0)));

        let inside = Sphere::new(Vec3::ZERO, 0.25);
        assert!(frustum.contains_sphere(&inside));
        assert!(frustum.intersects_sphere(&inside));
        let touching = Sphere::new(vec3(1.25, 0.0, 0.0), 0.25);
        assert!(frustum.intersects_sphere(&touching));
        assert!(!frustum.contains_sphere(&touching));
        assert!(!frustum.intersects_sphere(&Sphere::new(vec3(2.0, 0.0, 0.0), 0.25)));
        assert!(!frustum.intersects_sphere(&Sphere::new(Vec3::ZERO, -1.0)));
        assert!(!frustum.contains_sphere(&Sphere::new(Vec3::ZERO, f32::NAN)));

        let box_inside = Aabb::from_min_max(vec3(-0.5, -0.5, -0.5), vec3(0.5, 0.5, 0.5));
        assert!(frustum.contains_aabb(&box_inside));
        assert!(frustum.intersects_aabb(&box_inside));
        let straddling = Aabb::from_min_max(vec3(0.8, -0.1, -0.1), vec3(1.2, 0.1, 0.1));
        assert!(frustum.intersects_aabb(&straddling));
        assert!(!frustum.contains_aabb(&straddling));
        let outside = Aabb::from_min_max(vec3(2.0, 2.0, 2.0), vec3(3.0, 3.0, 3.0));
        assert!(!frustum.intersects_aabb(&outside));
        let broken = Aabb::from_min_max(vec3(f32::NAN, 0.0, 0.0), Vec3::ONE);
        assert!(!frustum.intersects_aabb(&broken));
        assert!(!frustum.contains_aabb(&broken));
    }

    #[test]
    fn a_zero_matrix_has_no_planes() {
        assert!(Frustum::from_view_projection(Mat4::ZERO).is_none());
        assert!(Frustum::from_view_projection(Mat4::NAN).is_none());
    }

    #[test]
    fn perspective_bounds_follow_the_fov_and_clip_distances() {
        let frustum = perspective_at_origin();
        assert!(frustum.contains_point(vec3(0.0, 0.0, -2.0)));
        assert!(!frustum.contains_point(vec3(0.0, 0.0, -0.5)));
        assert!(!frustum.contains_point(vec3(0.0, 0.0, 1.0)));

        assert_close(frustum.near.signed_distance(vec3(0.0, 0.0, -1.0)), 0.0);
        assert_close(frustum.far.signed_distance(vec3(0.0, 0.0, -10.0)), 0.0);
        assert!(frustum.contains_point(vec3(0.0, 0.0, -1.0)));
        assert!(frustum.contains_point(vec3(0.0, 0.0, -10.0)));
        assert!(!frustum.contains_point(vec3(0.0, 0.0, -10.2)));

        // 90° fov, aspect 1: at z = -5 the cross section is x, y in -5..=5.
        let edge = vec3(5.0, 0.0, -5.0);
        assert_close(frustum.right.signed_distance(edge), 0.0);
        assert!(frustum.contains_point(edge));
        assert!(!frustum.contains_point(edge + Vec3::X));
        let top = vec3(0.0, 5.0, -5.0);
        assert_close(frustum.top.signed_distance(top), 0.0);
        assert!(!frustum.contains_point(top + Vec3::Y));

        let inside = Sphere::new(vec3(0.0, 0.0, -5.0), 0.5);
        assert!(frustum.contains_sphere(&inside));
        let grazing = Sphere::new(vec3(0.0, 0.0, -0.5), 0.5);
        assert!(frustum.intersects_sphere(&grazing));
        assert!(!frustum.contains_sphere(&grazing));
        assert!(!frustum.intersects_sphere(&Sphere::new(vec3(0.0, 0.0, 2.0), 0.4)));

        let contained = Aabb::from_min_max(vec3(-0.2, -0.2, -2.0), vec3(0.2, 0.2, -1.5));
        assert!(frustum.contains_aabb(&contained));
        let right = Aabb::from_min_max(vec3(20.0, -0.2, -3.0), vec3(21.0, 0.2, -2.0));
        assert!(!frustum.intersects_aabb(&right));
    }

    #[test]
    fn a_box_past_the_far_corner_can_still_count_as_intersecting() {
        let frustum = perspective_at_origin();
        // Outside the far face (z < -10) and outside the x = -z / y = -z
        // edges, but each infinite plane still has a corner of the box on its
        // inside. The convex volume itself does not meet the box.
        let past_corner = Aabb::from_min_max(vec3(10.5, 10.5, -12.0), vec3(12.0, 12.0, -9.0));
        assert!(frustum.intersects_aabb(&past_corner));
        assert!(!frustum.contains_aabb(&past_corner));
        for x in [past_corner.min.x, past_corner.max.x] {
            for y in [past_corner.min.y, past_corner.max.y] {
                for z in [past_corner.min.z, past_corner.max.z] {
                    assert!(!frustum.contains_point(vec3(x, y, z)));
                }
            }
        }

        let hits_corner = Aabb::from_min_max(vec3(9.0, 9.0, -10.5), vec3(11.0, 11.0, -9.5));
        assert!(frustum.intersects_aabb(&hits_corner));
    }

    #[test]
    fn an_offset_orthographic_frustum_uses_the_view_translation() {
        let projection = Mat4::orthographic_rh_gl(-2.0, 2.0, -1.0, 1.0, 1.0, 20.0);
        let eye = vec3(4.0, 5.0, 6.0);
        let view = Mat4::look_at_rh(eye, vec3(4.0, 5.0, 0.0), Vec3::Y);
        let frustum = Frustum::from_view_projection(projection * view).unwrap();

        assert!(frustum.contains_point(vec3(4.0, 5.0, 0.0)));
        assert_close(frustum.near.signed_distance(vec3(4.0, 5.0, 5.0)), 0.0);
        assert_close(frustum.far.signed_distance(vec3(4.0, 5.0, -14.0)), 0.0);
        assert_close(frustum.right.signed_distance(vec3(6.0, 5.0, 0.0)), 0.0);
        assert_close(frustum.top.signed_distance(vec3(4.0, 6.0, 0.0)), 0.0);
        assert!(!frustum.contains_point(vec3(6.5, 5.0, 0.0)));
        assert!(!frustum.contains_point(vec3(4.0, 6.5, 0.0)));
        assert!(!frustum.contains_point(vec3(4.0, 5.0, 5.5)));
        assert!(!frustum.contains_point(vec3(4.0, 5.0, -14.5)));

        let slab = Aabb::from_min_max(vec3(3.0, 4.5, -1.0), vec3(5.0, 5.5, 1.0));
        assert!(frustum.contains_aabb(&slab));
    }
}
