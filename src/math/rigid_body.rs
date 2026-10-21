//! Translation-only bodies with caller-owned time and gravity.

use super::{Aabb, Sphere, Vec3};

/// Local-space geometry attached to a [`RigidBody`].
///
/// Corners and centers are offsets from the body's `position`, in the same
/// units as world coordinates. A box stays axis-aligned; there is no rotation.
/// [`RigidBody::world_shape`] returns this enum with world-space coordinates
/// for the existing shape queries. Shapes may be offset from the body origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RigidBodyShape {
    /// An axis-aligned box with finite, ordered local-space corners.
    Aabb(Aabb),
    /// A sphere with a finite local center and finite, non-negative radius.
    Sphere(Sphere),
}

/// How a positive-mass [`RigidBody`] receives velocity during integration.
///
/// In either mode, zero mass makes the body static and leaves both position
/// and velocity unchanged. The mode does not enable any collision response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RigidBodyMode {
    /// Apply gravity before damping and translation.
    Dynamic,
    /// Ignore gravity; damp and integrate the caller-supplied velocity.
    Kinematic,
}

/// Invalid input or an unrepresentable result from a [`RigidBody`] operation.
///
/// Failed integration leaves every field unchanged. If several inputs are
/// invalid, callers should not rely on which error is reported first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RigidBodyError {
    /// Elapsed time is negative or non-finite.
    InvalidTime,
    /// Mass is negative or non-finite.
    InvalidMass,
    /// Linear damping is negative or non-finite.
    InvalidDamping,
    /// The body position contains a non-finite component.
    InvalidPosition,
    /// Linear velocity contains a non-finite component.
    InvalidVelocity,
    /// Gravity contains a non-finite component.
    InvalidGravity,
    /// The local shape contains invalid coordinates or dimensions.
    InvalidShape,
    /// Updated velocity, position or translated shape is not finite in `f32`.
    NonFiniteResult,
}

impl std::fmt::Display for RigidBodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidTime => "elapsed time must be finite and non-negative",
            Self::InvalidMass => "mass must be finite and non-negative",
            Self::InvalidDamping => "linear damping must be finite and non-negative",
            Self::InvalidPosition => "body position must be finite",
            Self::InvalidVelocity => "linear velocity must be finite",
            Self::InvalidGravity => "gravity must be finite",
            Self::InvalidShape => "body shape must have finite, valid coordinates and dimensions",
            Self::NonFiniteResult => "body result is not representable as finite f32 coordinates",
        })
    }
}

impl std::error::Error for RigidBodyError {}

/// A caller-owned, translation-only body, available without a window.
///
/// `position` and `linear_velocity` use world coordinates; `shape` uses offsets
/// from `position`. Gravity is supplied per step, so no up axis is assumed.
/// Positive mass allows motion, with gravity independent of its magnitude.
/// Zero mass (including negative zero) makes either mode static. Rotation,
/// contacts, collision response and world ownership are not included.
///
/// Fields are public for direct editing and struct literals. [`Self::new`] and
/// [`Self::integrate`] check their inputs rather than repairing invalid values.
///
/// ```
/// use macroquad::prelude::*;
///
/// let mut ball = RigidBody::new(
///     vec3(0.0, 4.0, 0.0),
///     RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 0.5)),
///     2.0,
/// ).unwrap();
/// ball.linear_velocity = Vec3::X * 2.0;
/// ball.integrate(0.5, -Vec3::Y * 8.0).unwrap();
/// assert_eq!(ball.linear_velocity, vec3(2.0, -4.0, 0.0));
/// assert_eq!(ball.position, vec3(1.0, 2.0, 0.0));
/// assert_eq!(ball.world_shape().unwrap(),
///     RigidBodyShape::Sphere(Sphere::new(ball.position, 0.5)));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidBody {
    /// World-space translation of the local shape's origin.
    pub position: Vec3,
    /// World-space velocity, in position units per unit of elapsed time.
    pub linear_velocity: Vec3,
    /// Finite, non-negative mass; zero freezes position and velocity.
    pub mass: f32,
    /// Finite, non-negative decay rate, in inverse units of elapsed time.
    ///
    /// Velocity is multiplied by `exp(-linear_damping * dt)` after gravity.
    /// Zero disables damping; the default from [`Self::new`] is zero.
    pub linear_damping: f32,
    /// Whether gravity affects a positive-mass body.
    pub mode: RigidBodyMode,
    /// Local-space box or sphere translated by `position`.
    pub shape: RigidBodyShape,
}

impl RigidBody {
    /// Creates a dynamic body with zero velocity and zero damping.
    ///
    /// Mass must be finite and non-negative. Position and shape coordinates
    /// must be finite, box corners ordered, and sphere radius non-negative.
    /// Degenerate boxes and zero-radius spheres are valid. The translated
    /// world shape must also be finite. Zero mass creates a static body.
    /// Set [`Self::mode`] to [`RigidBodyMode::Kinematic`] to ignore gravity.
    pub fn new(position: Vec3, shape: RigidBodyShape, mass: f32) -> Result<Self, RigidBodyError> {
        let body = Self {
            position,
            linear_velocity: Vec3::ZERO,
            mass,
            linear_damping: 0.0,
            mode: RigidBodyMode::Dynamic,
            shape,
        };
        body.validate()?;
        Ok(body)
    }

    /// Whether mass is zero, including negative zero.
    ///
    /// This is a classification only; invalid fields are checked by
    /// [`Self::new`] and [`Self::integrate`].
    pub fn is_static(&self) -> bool {
        self.mass == 0.0
    }

    /// Returns a translated copy of the local shape for world-space queries.
    ///
    /// Checks position, local shape, and overflow of translated coordinates;
    /// velocity, mass and damping are irrelevant to this query and not checked.
    /// Neither the local shape nor the position is modified.
    pub fn world_shape(&self) -> Result<RigidBodyShape, RigidBodyError> {
        if !self.position.is_finite() {
            return Err(RigidBodyError::InvalidPosition);
        }
        let shape = match self.shape {
            RigidBodyShape::Aabb(bounds) => {
                if !valid_aabb(bounds) {
                    return Err(RigidBodyError::InvalidShape);
                }
                let translated =
                    Aabb::from_min_max(bounds.min + self.position, bounds.max + self.position);
                if !valid_aabb(translated) {
                    return Err(RigidBodyError::NonFiniteResult);
                }
                RigidBodyShape::Aabb(translated)
            }
            RigidBodyShape::Sphere(sphere) => {
                if !sphere.center.is_finite() || !sphere.radius.is_finite() || sphere.radius < 0.0 {
                    return Err(RigidBodyError::InvalidShape);
                }
                let translated = sphere.offset(self.position);
                if !translated.center.is_finite() {
                    return Err(RigidBodyError::NonFiniteResult);
                }
                RigidBodyShape::Sphere(translated)
            }
        };
        Ok(shape)
    }

    /// Advances velocity and then position, without checking collisions.
    ///
    /// For positive mass, a dynamic body first adds `gravity * dt` to velocity;
    /// a kinematic body skips that addition. Both multiply the velocity by
    /// `exp(-linear_damping * dt)`, then add the updated velocity times `dt`
    /// to position. Intermediate arithmetic uses `f64`, with velocity rounded
    /// to `f32` before computing position. This is a semi-implicit Euler step,
    /// not an exact trajectory; subdividing time may change the result.
    ///
    /// Time, mass and damping must be finite and non-negative. Position,
    /// velocity, gravity and the local/world shape must be valid and finite.
    /// **Every call checks all these inputs**, even for static or kinematic
    /// bodies and zero time. After validation, zero time and zero mass are
    /// successful no-ops that preserve velocity as well as position.
    ///
    /// An invalid input, or an updated velocity/position/world shape that is
    /// not finite in `f32`, returns an error without changing any field.
    /// Finite underflow/rounding is accepted; extreme damping can stop a body.
    ///
    /// ```
    /// use macroquad::math::{RigidBody, RigidBodyError, RigidBodyShape, Sphere, Vec3};
    /// let shape = RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 1.0));
    /// let mut body = RigidBody::new(Vec3::ZERO, shape, 1.0).unwrap();
    /// let before = body;
    /// assert_eq!(body.integrate(-1.0, Vec3::ZERO), Err(RigidBodyError::InvalidTime));
    /// assert_eq!(body, before);
    /// ```
    pub fn integrate(&mut self, dt: f32, gravity: Vec3) -> Result<(), RigidBodyError> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(RigidBodyError::InvalidTime);
        }
        if !gravity.is_finite() {
            return Err(RigidBodyError::InvalidGravity);
        }
        self.validate()?;
        if dt == 0.0 || self.is_static() {
            return Ok(());
        }

        let dt = dt as f64;
        let decay = (-(self.linear_damping as f64) * dt).exp();
        let acceleration = match self.mode {
            RigidBodyMode::Dynamic => gravity,
            RigidBodyMode::Kinematic => Vec3::ZERO,
        };
        let velocity = (self.linear_velocity.as_dvec3() + acceleration.as_dvec3() * dt) * decay;
        let velocity = velocity.as_vec3();
        let position = (self.position.as_dvec3() + velocity.as_dvec3() * dt).as_vec3();
        if !velocity.is_finite() || !position.is_finite() {
            return Err(RigidBodyError::NonFiniteResult);
        }
        let candidate = Self {
            position,
            linear_velocity: velocity,
            ..*self
        };
        candidate.world_shape()?;
        *self = candidate;
        Ok(())
    }

    fn validate(&self) -> Result<(), RigidBodyError> {
        if !self.mass.is_finite() || self.mass < 0.0 {
            return Err(RigidBodyError::InvalidMass);
        }
        if !self.linear_damping.is_finite() || self.linear_damping < 0.0 {
            return Err(RigidBodyError::InvalidDamping);
        }
        if !self.linear_velocity.is_finite() {
            return Err(RigidBodyError::InvalidVelocity);
        }
        self.world_shape()?;
        Ok(())
    }
}

fn valid_aabb(bounds: Aabb) -> bool {
    bounds.min.is_finite() && bounds.max.is_finite() && bounds.min.cmple(bounds.max).all()
}
