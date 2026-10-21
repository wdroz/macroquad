//! Translational AABB/capsule sweep through the segment/box closest features.

use glam::{DVec3, Vec3};

use super::{Aabb, Capsule, Sphere, SweepHit};

const TOUCH_SLOP: f32 = 1e-4;
const INTO_SLOP: f64 = 1e-5;

impl Aabb {
    /// Sweeps this axis-aligned box along `delta` against a static `capsule`.
    ///
    /// The complete box and rounded capsule are tested, including cylinder
    /// sides, end caps and edge grazes. Neither shape is replaced by a bound.
    /// `t` is in `0..=1`, `normal` points out of the capsule toward the box,
    /// and a new impact's `point` is on both shapes in world space.
    ///
    /// An initial overlap uses [`Capsule::contact_aabb`], reversing its normal
    /// and keeping its point on the box. That point need not be on the capsule
    /// surface under containment. Penetration deeper than `1e-4` reports
    /// `t == 0` even with no motion or motion away. A touch or shallower overlap
    /// reports a hit only when moving inward by more than `1e-5`; tangential
    /// and outward motion miss. New impacts also require inward motion by
    /// more than `1e-5`, so an exact future tangency misses. A zero-length
    /// segment delegates to [`Aabb::sweep_sphere`] and shares its feature and
    /// precision conventions.
    ///
    /// Flat or point boxes and zero-radius capsules are allowed. Inverted or
    /// non-finite boxes, negative or non-finite radii, non-finite endpoints or
    /// deltas miss. Separated-feature calculations use `f64` intermediates;
    /// initial contacts retain the existing contact query's floating-point
    /// limits. Arbitrary extreme finite coordinates are not guaranteed, and
    /// an unusable final point or normal misses.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::math::{Aabb, Capsule, Vec3, vec3};
    ///
    /// let body = Aabb::from_center_size(vec3(-3.0, 0.0, 0.0), Vec3::ONE);
    /// let post = Capsule::new(vec3(0.0, -2.0, 0.0), vec3(0.0, 2.0, 0.0), 0.5);
    /// let hit = body.sweep_capsule(vec3(6.0, 0.0, 0.0), &post).unwrap();
    /// assert!((hit.t - 1.0 / 3.0).abs() < 1e-6);
    /// assert_eq!(hit.normal, -Vec3::X);
    /// assert_eq!(hit.point.x, -0.5);
    /// ```
    pub fn sweep_capsule(&self, delta: Vec3, capsule: &Capsule) -> Option<SweepHit> {
        if !delta.is_finite()
            || !self.min.is_finite()
            || !self.max.is_finite()
            || self.min.cmpgt(self.max).any()
            || !capsule.a.is_finite()
            || !capsule.b.is_finite()
            || !capsule.radius.is_finite()
            || capsule.radius < 0.0
        {
            return None;
        }
        if capsule.a == capsule.b {
            return self.sweep_sphere(delta, &Sphere::new(capsule.a, capsule.radius));
        }
        if let Some(contact) = capsule.contact_aabb(self) {
            let normal = -contact.normal;
            if !contact.point.is_finite()
                || !normal.is_finite()
                || (normal.length_squared() - 1.0).abs() > 1e-3
                || !contact.penetration.is_finite()
            {
                return None;
            }
            return (contact.penetration > TOUCH_SLOP
                || delta.as_dvec3().dot(normal.as_dvec3()) < -INTO_SLOP)
                .then_some(SweepHit {
                    t: 0.0,
                    normal,
                    point: contact.point,
                });
        }

        let cast = CapsuleCast {
            min: self.min.as_dvec3(),
            max: self.max.as_dvec3(),
            a: capsule.a.as_dvec3(),
            axis: capsule.b.as_dvec3() - capsule.a.as_dvec3(),
            delta: delta.as_dvec3(),
            radius: capsule.radius as f64,
        };
        let mut best: Option<(f64, SweepHit)> = None;
        // Each box coordinate is free, on its lower face, or on its upper
        // face. The segment parameter is at a, at b, or free. These are all
        // closest-feature pairs, including nonparallel edge/segment pairs.
        for code in 1..27 {
            let sides = [code % 3, (code / 3) % 3, code / 9];
            for segment in 0..3 {
                if let Some((t, hit)) = cast.feature(sides, segment) {
                    let replace = best.as_ref().is_none_or(|(time, current)| {
                        t < *time - 1e-12
                            || ((t - time).abs() <= 1e-12
                                && hit.normal.dot(delta) < current.normal.dot(delta))
                    });
                    if replace {
                        best = Some((t, hit));
                    }
                }
            }
        }
        best.map(|(_, hit)| hit)
    }
}

struct CapsuleCast {
    min: DVec3,
    max: DVec3,
    a: DVec3,
    axis: DVec3,
    delta: DVec3,
    radius: f64,
}

impl CapsuleCast {
    fn feature(&self, sides: [usize; 3], segment: usize) -> Option<(f64, SweepHit)> {
        let mut offset = DVec3::ZERO;
        let mut velocity = DVec3::ZERO;
        let mut axis = DVec3::ZERO;
        for i in 0..3 {
            if sides[i] != 0 {
                let bound = if sides[i] == 1 {
                    self.min[i]
                } else {
                    self.max[i]
                };
                offset[i] = self.a[i] - bound;
                velocity[i] = -self.delta[i];
                axis[i] = self.axis[i];
            }
        }
        let axis_sq = axis.length_squared();
        let (s0, ds) = if segment < 2 {
            (segment as f64, 0.0)
        } else if axis_sq > 0.0 {
            (-offset.dot(axis) / axis_sq, -velocity.dot(axis) / axis_sq)
        } else {
            // Parallel to this feature: any parameter on the segment has
            // the same distance. Pick a valid one after solving the time.
            (0.0, 0.0)
        };
        offset += axis * s0;
        velocity += axis * ds;
        let speed_sq = velocity.length_squared();
        if speed_sq == 0.0 {
            // Constant-distance travel is tangential. Endpoints or another
            // feature supply entry when a finite feature's boundary is met.
            return None;
        }
        // The closest-approach form avoids subtracting two large, nearly
        // equal quadratic discriminant terms for fast, narrow crossings.
        let middle = -offset.dot(velocity) / speed_sq;
        let nearest = offset + velocity * middle;
        let gap = self.radius * self.radius - nearest.length_squared();
        let zero_slack = (offset.abs() + velocity.abs() * middle.abs()) * (16.0 * f64::EPSILON);
        if gap < 0.0 && (self.radius != 0.0 || -gap > zero_slack.length_squared()) {
            return None;
        }
        let t = middle - (gap.max(0.0) / speed_sq).sqrt();
        if !t.is_finite() || !(-1e-12..=1.0 + 1e-12).contains(&t) {
            return None;
        }
        let t = t.clamp(0.0, 1.0);
        let s = if segment == 2 && axis_sq == 0.0 {
            self.parallel_parameter(sides, t)?
        } else {
            let s = s0 + ds * t;
            if !(-1e-10..=1.0 + 1e-10).contains(&s) {
                return None;
            }
            s.clamp(0.0, 1.0)
        };
        let relative = self.a - self.delta * t + self.axis * s;
        let on_box = relative.clamp(self.min, self.max);
        let residual = relative - on_box;
        // Validate the active set rather than accepting a rounded bounding
        // box. Slack only covers f64 arithmetic, not a world-space skin.
        let scale = (self.a - self.min).abs()
            + (self.max - self.min).abs()
            + self.axis.abs()
            + self.delta.abs() * t;
        let slack =
            scale.max(DVec3::splat(self.radius)).max(DVec3::splat(1.0)) * (64.0 * f64::EPSILON);
        for i in 0..3 {
            let valid = match sides[i] {
                0 => relative[i] >= self.min[i] - slack[i] && relative[i] <= self.max[i] + slack[i],
                1 => relative[i] <= self.min[i] + slack[i],
                _ => relative[i] >= self.max[i] - slack[i],
            };
            if !valid {
                return None;
            }
        }
        let derivative = self.axis.dot(residual);
        let derivative_slack = self.axis.abs().dot(slack);
        if (segment == 0 && derivative < -derivative_slack)
            || (segment == 1 && derivative > derivative_slack)
        {
            return None;
        }
        let normal = if self.radius == 0.0 {
            // At a zero-distance crossing the incoming limit supplies the
            // normal; normalizing the coincident points would produce zero.
            velocity.normalize()
        } else {
            -residual.try_normalize()?
        };
        if self.delta.dot(normal) >= -INTO_SLOP {
            return None;
        }
        let point = (on_box + self.delta * t).as_vec3();
        let normal = normal.as_vec3();
        if !point.is_finite() || !normal.is_finite() {
            return None;
        }
        Some((
            t,
            SweepHit {
                t: t as f32,
                normal,
                point,
            },
        ))
    }

    fn parallel_parameter(&self, sides: [usize; 3], t: f64) -> Option<f64> {
        let mut lo: f64 = 0.0;
        let mut hi: f64 = 1.0;
        for (i, side) in sides.iter().enumerate() {
            if *side != 0 {
                continue;
            }
            let start = self.a[i] - self.delta[i] * t;
            if self.axis[i] == 0.0 {
                if start < self.min[i] || start > self.max[i] {
                    return None;
                }
            } else {
                let t0 = (self.min[i] - start) / self.axis[i];
                let t1 = (self.max[i] - start) / self.axis[i];
                lo = lo.max(t0.min(t1));
                hi = hi.min(t0.max(t1));
            }
        }
        (lo <= hi).then_some((lo + hi) * 0.5)
    }
}
