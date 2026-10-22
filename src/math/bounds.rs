//! Checked world-space bounds, separate from precise contact and sweep queries.

use super::{vec3, Aabb, Capsule, Obb, Sphere, Vec3};

impl Sphere {
    /// Conservatively encloses this world-space sphere in an axis-aligned box.
    ///
    /// The center and radius must be finite and the radius nonnegative. Zero
    /// radius gives a point. Bounds round outward, including when a positive
    /// radius is smaller than the spacing of coordinates near the center.
    /// Returns `None` for invalid input or when no finite `f32` box can enclose
    /// the sphere. This does not mean a collision query misses: callers using
    /// bounds for candidate selection must then use their complete candidate set.
    /// This is a geometric snapshot, not a guarantee about the numerical behavior
    /// of a separate narrowphase query at extreme scales.
    ///
    /// ```
    /// use macroquad::math::{Sphere, Vec3};
    /// let bounds = Sphere::new(Vec3::ZERO, 2.0).bounding_aabb().unwrap();
    /// assert_eq!(bounds.min, Vec3::splat(-2.0));
    /// assert_eq!(bounds.max, Vec3::splat(2.0));
    /// // A tiny radius must still expand outward beside a large center.
    /// let sphere = Sphere::new(Vec3::splat(16_777_216.0), 0.25);
    /// let bounds = sphere.bounding_aabb().unwrap();
    /// assert!(bounds.min.cmplt(sphere.center).all());
    /// assert!(bounds.max.cmpgt(sphere.center).all());
    /// assert!(Sphere::new(Vec3::splat(f32::MAX), 1.0).bounding_aabb().is_none());
    /// ```
    pub fn bounding_aabb(&self) -> Option<Aabb> {
        rounded_segment_bounds(self.center, self.center, self.radius)
    }
}

impl Capsule {
    /// Conservatively encloses this world-space capsule in an axis-aligned box.
    ///
    /// Both inner-segment endpoints and the nonnegative radius must be finite.
    /// Caps extend the bounds by the radius on every axis, without choosing an
    /// up direction. A zero radius gives the segment's bounds; equal endpoints
    /// give a sphere's bounds. Endpoint subtraction and segment length are not
    /// needed, so very long finite segments remain usable.
    ///
    /// Rounding, invalid input, overflow and the complete-candidate fallback
    /// follow [`Sphere::bounding_aabb`]. These bounds do not replace a precise
    /// overlap, contact or sweep query.
    ///
    /// ```
    /// use macroquad::math::{Capsule, Vec3};
    /// let bounds = Capsule::new(-Vec3::X, Vec3::X, 0.5).bounding_aabb().unwrap();
    /// assert_eq!(bounds.min.x, -1.5);
    /// assert_eq!(bounds.max.x, 1.5);
    /// let point_segment = Capsule::new(Vec3::Z, Vec3::Z, 0.5);
    /// assert_eq!(point_segment.bounding_aabb(),
    ///     macroquad::math::Sphere::new(Vec3::Z, 0.5).bounding_aabb());
    /// ```
    pub fn bounding_aabb(&self) -> Option<Aabb> {
        rounded_segment_bounds(self.a, self.b, self.radius)
    }
}

impl Obb {
    /// Conservatively encloses this world-space oriented box in an AABB.
    ///
    /// Uses the existing query rotation normalization: finite center and
    /// nonnegative finite half extents, plus a quaternion with a usable finite
    /// nonzero length. Zero extents permit a flat box, line or point. A zero,
    /// non-finite, underflowed or overflowed quaternion length returns `None`,
    /// just as an unusable rotation makes the existing box queries miss.
    ///
    /// Bounds enclose the normalized quaternion's real-valued corner transform
    /// with outward rounding, as well as its finite `f32` corner transforms.
    /// The wider calculation avoids intermediate `f32` transform overflow.
    /// Returns `None` if an input is invalid or no finite `f32` enclosure exists.
    /// As with [`Sphere::bounding_aabb`], callers must fall back to complete
    /// candidates in that case. A bounding overlap alone is not a precise hit,
    /// nor a guarantee about extreme-scale arithmetic in other shape queries.
    ///
    /// ```
    /// use macroquad::math::{Obb, Quat, Vec3};
    /// let rotation = Quat::from_rotation_z(0.7);
    /// let box_shape = Obb::new(Vec3::X, Vec3::new(2.0, 0.5, 1.0), rotation);
    /// let bounds = box_shape.bounding_aabb().unwrap();
    /// for x in [-1.0, 1.0] {
    ///     for y in [-1.0, 1.0] {
    ///         for z in [-1.0, 1.0] {
    ///             let corner = box_shape.center
    ///                 + rotation * (box_shape.half_extents * Vec3::new(x, y, z));
    ///             assert!(bounds.contains(corner));
    ///         }
    ///     }
    /// }
    /// ```
    pub fn bounding_aabb(&self) -> Option<Aabb> {
        let rotation = self.pose()?;
        let quaternion = rotation.to_array().map(f64::from);
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for bits in 0..8 {
            let local = vec3(
                signed_extent(self.half_extents.x, bits & 1 != 0),
                signed_extent(self.half_extents.y, bits & 2 != 0),
                signed_extent(self.half_extents.z, bits & 4 != 0),
            );
            for axis in 0..3 {
                let coordinate = corner_coordinate(self.center[axis], local, quaternion, axis);
                min[axis] = min[axis].min(coordinate.round(false)?);
                max[axis] = max[axis].max(coordinate.round(true)?);
            }
            // Glam's f32 expression has intermediate rounding beyond the final
            // conversion above. Keep the actual finite corners inside too.
            let rounded = self.center + rotation * local;
            if rounded.is_finite() {
                min = min.min(rounded);
                max = max.max(rounded);
            }
        }
        Some(Aabb::from_min_max(min, max))
    }
}

fn signed_extent(extent: f32, positive: bool) -> f32 {
    if positive {
        extent
    } else {
        -extent
    }
}

fn rounded_segment_bounds(a: Vec3, b: Vec3, radius: f32) -> Option<Aabb> {
    if !a.is_finite() || !b.is_finite() || !radius.is_finite() || radius < 0.0 {
        return None;
    }
    let mut min = a.min(b);
    let mut max = a.max(b);
    for axis in 0..3 {
        let mut lower = ExactSum::new(min[axis]);
        lower.add(-f64::from(radius));
        min[axis] = lower.round(false)?;
        let mut upper = ExactSum::new(max[axis]);
        upper.add(f64::from(radius));
        max[axis] = upper.round(true)?;
    }
    Some(Aabb::from_min_max(min, max))
}

fn corner_coordinate(center: f32, local: Vec3, q: [f64; 4], axis: usize) -> ExactSum {
    // The same linear quaternion transform as Glam's Quat * Vec3. Keeping
    // products and their residuals avoids inward rounding at a corner and
    // losing tiny contributions beside large translations or extents.
    let j = (axis + 1) % 3;
    let k = (axis + 2) % 3;
    let v = local.to_array().map(f64::from);
    let mut sum = ExactSum::new(center);
    sum.add_product(q[3] * q[3], v[axis]);
    sum.add_product(q[axis] * q[axis], v[axis]);
    sum.add_product(-q[j] * q[j], v[axis]);
    sum.add_product(-q[k] * q[k], v[axis]);
    sum.add_product(2.0 * q[axis] * q[j], v[j]);
    sum.add_product(-2.0 * q[3] * q[k], v[j]);
    sum.add_product(2.0 * q[axis] * q[k], v[k]);
    sum.add_product(2.0 * q[3] * q[j], v[k]);
    sum
}

/// A small nonoverlapping expansion, from least to most significant.
///
/// Each corner adds at most 17 terms (one center and eight products with
/// residuals); comparisons add one more. All products originate from finite
/// f32 inputs, so neither the f64 products nor their residuals can overflow or
/// underflow. No heap storage or approximate epsilon is needed.
#[derive(Clone, Copy)]
struct ExactSum {
    terms: [f64; 24],
    len: usize,
}

impl ExactSum {
    fn new(value: f32) -> Self {
        let mut sum = Self {
            terms: [0.0; 24],
            len: 0,
        };
        sum.add(f64::from(value));
        sum
    }

    fn add(&mut self, mut value: f64) {
        let mut len = 0;
        for index in 0..self.len {
            let term = self.terms[index];
            let total = value + term;
            let term_part = total - value;
            let error = (value - (total - term_part)) + (term - term_part);
            if error != 0.0 {
                self.terms[len] = error;
                len += 1;
            }
            value = total;
        }
        if value != 0.0 || len == 0 {
            self.terms[len] = value;
            len += 1;
        }
        self.len = len;
    }

    fn add_product(&mut self, left: f64, right: f64) {
        let product = left * right;
        self.add(left.mul_add(right, -product));
        self.add(product);
    }

    fn sign_after_subtracting(mut self, value: f32) -> f64 {
        self.add(-f64::from(value));
        self.terms[self.len - 1]
    }

    fn round(self, upward: bool) -> Option<f32> {
        if self.sign_after_subtracting(f32::MAX) > 0.0
            || self.sign_after_subtracting(-f32::MAX) < 0.0
        {
            return None;
        }
        let nearest = (self.terms[self.len - 1] as f32).clamp(-f32::MAX, f32::MAX);
        let difference = self.sign_after_subtracting(nearest);
        let result = if upward && difference > 0.0 {
            adjacent(nearest, true)
        } else if !upward && difference < 0.0 {
            adjacent(nearest, false)
        } else {
            nearest
        };
        result.is_finite().then_some(result)
    }
}

fn adjacent(value: f32, upward: bool) -> f32 {
    if value == 0.0 {
        return f32::from_bits(if upward { 1 } else { (1 << 31) | 1 });
    }
    let bits = value.to_bits();
    f32::from_bits(if upward == (value > 0.0) {
        bits + 1
    } else {
        bits - 1
    })
}
