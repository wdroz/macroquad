//! A caller-stepped, translation-only distance constraint.

use super::{RigidBody, RigidBodyError};

/// Invalid joint settings, body fields or an unrepresentable corrected body.
/// Every error from [`DistanceJoint::solve`] leaves both bodies unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistanceJointError {
    /// Rest length must be finite and non-negative.
    InvalidRestLength,
    /// Both correction fractions must be finite and within `0..=1`.
    InvalidCorrection,
    /// A body input or proposed result failed body validation.
    Body(RigidBodyError),
}

impl std::fmt::Display for DistanceJointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRestLength => f.write_str("rest length must be finite and non-negative"),
            Self::InvalidCorrection => {
                f.write_str("correction fractions must be finite and within 0..=1")
            }
            Self::Body(error) => write!(f, "invalid distance-joint body: {error}"),
        }
    }
}

impl std::error::Error for DistanceJointError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Body(error) => Some(error),
            _ => None,
        }
    }
}

impl From<RigidBodyError> for DistanceJointError {
    fn from(error: RigidBodyError) -> Self {
        Self::Body(error)
    }
}

/// Keeps two [`RigidBody::position`] origins near a world-space rest length.
///
/// This stores settings only: the caller owns bodies, time, pair order and
/// iteration count. Call [`Self::solve`] after integration. There are no local
/// attachment offsets, rotations, collision filtering or world solver.
/// Fractions apply **per call**, independent of elapsed time; more calls change
/// the result. Position correction does not itself add velocity.
///
/// ```
/// use macroquad::prelude::*;
/// let shape = RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 0.1));
/// let mut anchor = RigidBody::new(Vec3::ZERO, shape, 0.0).unwrap();
/// let mut bob = RigidBody::new(Vec3::X * 4.0, shape, 1.0).unwrap();
/// bob.linear_velocity = Vec3::X * 5.0;
/// let joint = DistanceJoint::new(2.0).unwrap();
/// joint.solve(&mut anchor, &mut bob).unwrap();
/// assert!((bob.position.x - 3.6).abs() < 1e-6);
/// assert_eq!(bob.linear_velocity, Vec3::X * 4.0);
/// assert_eq!(anchor.position, Vec3::ZERO);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistanceJoint {
    /// Finite, non-negative distance between body origins, in world units.
    pub rest_length: f32,
    /// Fraction of length error removed per call (`0..=1`); default `0.2`.
    pub position_correction: f32,
    /// Fraction of relative radial speed removed per call (`0..=1`); default `0.2`.
    /// For zero rest length, removes this fraction of all relative velocity.
    pub velocity_correction: f32,
}

impl DistanceJoint {
    /// Creates a checked joint with both correction fractions set to `0.2`.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// assert_eq!(DistanceJoint::new(-1.0), Err(DistanceJointError::InvalidRestLength));
    /// let joint = DistanceJoint::new(0.0).unwrap();
    /// assert_eq!(joint.position_correction, 0.2);
    /// ```
    pub fn new(rest_length: f32) -> Result<Self, DistanceJointError> {
        let joint = Self {
            rest_length,
            position_correction: 0.2,
            velocity_correction: 0.2,
        };
        joint.validate()?;
        Ok(joint)
    }

    /// Applies one inverse-mass-weighted position and velocity correction.
    ///
    /// Positive-mass dynamic bodies share correction in proportion to inverse
    /// mass. Zero-mass bodies stay fixed and their stored velocity is ignored.
    /// Kinematic bodies stay unchanged; their prescribed velocity affects the
    /// dynamic partner. Two immovable bodies are a validated no-op.
    ///
    /// For positive rest length, corrects both extension and compression and
    /// both opening and closing radial speed, preserving tangential velocity.
    /// The direction is from `a` to `b` in the original pose. Coincident origins
    /// use world +X (argument order therefore matters in this ambiguous case).
    /// Zero rest length instead corrects the entire displacement and relative
    /// velocity, including at coincidence; no arbitrary normal is needed.
    ///
    /// All settings and body fields are checked even for disabled correction
    /// or immovable pairs. Arithmetic uses `f64`, rounded to `f32` for storage;
    /// finite rounding/underflow is accepted and can leave residual error.
    /// Invalid inputs or non-finite results/world shapes leave both bodies
    /// unchanged. Refresh broadphase bounds after positions change. This
    /// discrete step does not enforce contacts or prevent tunneling, and no
    /// convergence or stability guarantee is made for networks of joints.
    ///
    /// Invalid settings preserve both bodies, so callers can handle the error
    /// before choosing whether to continue their simulation step:
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let shape = RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 0.1));
    /// let mut a = RigidBody::new(Vec3::ZERO, shape, 0.0).unwrap();
    /// let mut b = RigidBody::new(Vec3::X, shape, 1.0).unwrap();
    /// let before = (a, b);
    /// let mut joint = DistanceJoint::new(1.0).unwrap();
    /// joint.velocity_correction = f32::NAN;
    /// assert_eq!(joint.solve(&mut a, &mut b), Err(DistanceJointError::InvalidCorrection));
    /// assert_eq!((a, b), before);
    /// ```
    pub fn solve(&self, a: &mut RigidBody, b: &mut RigidBody) -> Result<(), DistanceJointError> {
        self.validate()?;
        a.validate()?;
        b.validate()?;
        let wa = a.response_inverse_mass();
        let wb = b.response_inverse_mass();
        let total = wa + wb;
        if total == 0.0 {
            return Ok(());
        }
        let delta = b.position.as_dvec3() - a.position.as_dvec3();
        let relative_velocity = b.response_velocity() - a.response_velocity();
        let (position, velocity) = if self.rest_length == 0.0 {
            (delta, relative_velocity)
        } else {
            let length = delta.length();
            let normal = if length == 0.0 {
                glam::DVec3::X
            } else {
                delta / length
            };
            (
                normal * (length - self.rest_length as f64),
                normal * relative_velocity.dot(normal),
            )
        };
        let position = position * self.position_correction as f64;
        let velocity = velocity * self.velocity_correction as f64;
        let candidate = |body: &RigidBody, weight: f64| -> Result<RigidBody, DistanceJointError> {
            if weight == 0.0 {
                return Ok(*body);
            }
            let next = RigidBody {
                position: (body.position.as_dvec3() + position * weight).as_vec3(),
                linear_velocity: (body.linear_velocity.as_dvec3() + velocity * weight).as_vec3(),
                ..*body
            };
            if !next.position.is_finite() || !next.linear_velocity.is_finite() {
                return Err(RigidBodyError::NonFiniteResult.into());
            }
            next.world_shape()?;
            Ok(next)
        };
        let next_a = candidate(a, wa / total)?;
        let next_b = candidate(b, -wb / total)?;
        *a = next_a;
        *b = next_b;
        Ok(())
    }

    fn validate(&self) -> Result<(), DistanceJointError> {
        if !self.rest_length.is_finite() || self.rest_length < 0.0 {
            return Err(DistanceJointError::InvalidRestLength);
        }
        if [self.position_correction, self.velocity_correction]
            .iter()
            .any(|x| !x.is_finite() || !(0.0..=1.0).contains(x))
        {
            return Err(DistanceJointError::InvalidCorrection);
        }
        Ok(())
    }
}
