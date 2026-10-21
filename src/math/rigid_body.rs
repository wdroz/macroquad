//! Translation-only bodies with caller-owned time, gravity and pair response.

use super::{Aabb, Contact, Obb, Sphere, Vec3};

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
/// and velocity unchanged. Collision response is an explicit separate call.
///
/// ```
/// use macroquad::prelude::*;
/// let shape = RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 0.5));
/// let mut mover = RigidBody::new(Vec3::ZERO, shape, 1.0).unwrap();
/// mover.mode = RigidBodyMode::Kinematic;
/// mover.linear_velocity = Vec3::X * 2.0;
/// mover.integrate(0.5, -Vec3::Y * 10.0).unwrap();
/// assert_eq!(mover.position, Vec3::X); // Prescribed velocity; gravity is ignored.
/// mover.mass = 0.0;
/// let before = mover;
/// mover.integrate(0.5, -Vec3::Y * 10.0).unwrap();
/// assert_eq!(mover, before); // Zero mass also preserves the stored velocity.
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RigidBodyMode {
    /// Apply gravity before damping and translation.
    Dynamic,
    /// Ignore gravity; damp and integrate the caller-supplied velocity.
    ///
    /// Pair response does not move or accelerate this body, but its prescribed
    /// velocity affects a dynamic body that hits it.
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

/// Invalid input or arithmetic failure in an explicit collision-response step.
///
/// Every error leaves both bodies unchanged. The existing [`RigidBodyError`]
/// remains the error type for construction and integration.
///
/// Validation also runs for separated pairs, so an invalid coefficient is
/// reported even when no contact would need a response.
///
/// ```
/// use macroquad::prelude::*;
/// let shape = RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 0.5));
/// let mut a = RigidBody::new(Vec3::ZERO, shape, 1.0).unwrap();
/// let mut b = RigidBody::new(Vec3::X * 10.0, shape, 1.0).unwrap();
/// let before = (a, b);
/// assert_eq!(a.resolve_collision(&mut b, 2.0),
///     Err(CollisionResponseError::InvalidRestitution));
/// assert_eq!((a, b), before);
/// assert_eq!(a.resolve_collision(&mut b, 0.0), Ok(None));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionResponseError {
    /// A body field or its proposed result failed the usual body validation.
    Body(RigidBodyError),
    /// Restitution is non-finite or outside the inclusive range `0..=1`.
    InvalidRestitution,
    /// A static OBB has invalid coordinates, extents or an unusable rotation.
    InvalidCollider,
    /// The static-OBB helper was called with an AABB body.
    UnsupportedShape,
    /// Contact arithmetic overflowed, lost a squared distance to underflow,
    /// or produced an invalid normal, penetration or point.
    NumericalFailure,
}

impl std::fmt::Display for CollisionResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Body(error) => write!(f, "invalid collision body: {error}"),
            Self::InvalidRestitution => f.write_str("restitution must be finite and within 0..=1"),
            Self::InvalidCollider => f.write_str("static OBB must have a usable finite pose"),
            Self::UnsupportedShape => f.write_str("static OBB response requires a sphere body"),
            Self::NumericalFailure => {
                f.write_str("collision contact arithmetic is not reliable at this scale")
            }
        }
    }
}

impl std::error::Error for CollisionResponseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Body(error) => Some(error),
            _ => None,
        }
    }
}

impl From<RigidBodyError> for CollisionResponseError {
    fn from(error: RigidBodyError) -> Self {
        Self::Body(error)
    }
}

/// A caller-owned, translation-only body, available without a window.
///
/// `position` and `linear_velocity` use world coordinates; `shape` uses offsets
/// from `position`. Gravity is supplied per step, so no up axis is assumed.
/// Positive mass allows motion, with gravity independent of its magnitude.
/// Zero mass (including negative zero) makes either mode static. Explicit
/// [`Self::resolve_collision`] and [`Self::resolve_static_obb`] calls perform
/// one discrete response step. Rotation and world ownership are not included.
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
    /// Whether gravity and explicit collision response affect a positive-mass body.
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
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let local = Sphere::new(Vec3::Y, 0.5);
    /// let body = RigidBody::new(Vec3::X * 3.0,
    ///     RigidBodyShape::Sphere(local), 1.0).unwrap();
    /// assert_eq!(body.world_shape().unwrap(),
    ///     RigidBodyShape::Sphere(Sphere::new(vec3(3.0, 1.0, 0.0), 0.5)));
    /// assert_eq!(body.shape, RigidBodyShape::Sphere(local));
    /// ```
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

    /// Separates one overlapping pair and applies a normal impulse if closing.
    ///
    /// Call this explicitly after integration. All four AABB/sphere pairings
    /// use the existing shape contact queries on [`Self::world_shape`]. The
    /// returned contact describes the **original** pose; its normal points
    /// from `other` toward `self`. For mixed pairs, `point` follows the
    /// sphere/box query (on the box), regardless of argument order. A touching
    /// pair returns `Some` with zero penetration; separated pairs return `None`.
    ///
    /// Positive-mass dynamic bodies share the entire penetration in proportion
    /// to inverse mass. A closing relative normal velocity receives a
    /// frictionless impulse with `restitution` in `0..=1`: `0` disables bounce,
    /// `1` is elastic. Resting or separating velocity receives no impulse, and
    /// tangential velocity is preserved. Kinematic bodies are unchanged but
    /// their velocity participates in the relative velocity. Zero-mass bodies
    /// are unchanged and their stored velocity is ignored. If neither body is
    /// dynamic with positive mass, the contact is returned without correction.
    ///
    /// Every call validates both bodies and restitution, including no-op cases.
    /// Existing contact queries use `f32`; extreme geometry with overflowing or
    /// subnormal squared-distance intermediates returns
    /// [`CollisionResponseError::NumericalFailure`]. Response arithmetic uses
    /// `f64`, then rounds positions and velocities to `f32`. Finite rounding is
    /// accepted and may leave a small overlap. Invalid input or unrepresentable
    /// results leave **both** bodies unchanged.
    ///
    /// This is one discrete pair step, with no friction, angular response,
    /// iteration, sleeping, broadphase or continuous collision detection. It
    /// cannot prevent tunneling. The caller chooses pairs (optionally through
    /// [`super::UniformGrid`]) and must refresh bounds after moving bodies;
    /// sequential pair order can affect the result.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let shape = RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 1.0));
    /// let mut left = RigidBody::new(Vec3::ZERO, shape, 1.0).unwrap();
    /// let mut right = RigidBody::new(Vec3::X * 1.5, shape, 1.0).unwrap();
    /// left.linear_velocity = Vec3::X;
    /// let contact = left.resolve_collision(&mut right, 1.0).unwrap().unwrap();
    /// assert_eq!(contact.normal, -Vec3::X);
    /// assert_eq!(left.position, -Vec3::X * 0.25);
    /// assert_eq!(right.position, Vec3::X * 1.75);
    /// assert_eq!(left.linear_velocity, Vec3::ZERO);
    /// assert_eq!(right.linear_velocity, Vec3::X);
    /// ```
    pub fn resolve_collision(
        &mut self,
        other: &mut Self,
        restitution: f32,
    ) -> Result<Option<Contact>, CollisionResponseError> {
        validate_restitution(restitution)?;
        self.validate()?;
        other.validate()?;
        let contact = body_contact(self.world_shape()?, other.world_shape()?)?;
        let Some(contact) = contact else {
            return Ok(None);
        };
        let normal = checked_normal(contact)?;
        let left_inverse_mass = self.response_inverse_mass();
        let right_inverse_mass = other.response_inverse_mass();
        let inverse_mass = left_inverse_mass + right_inverse_mass;
        if inverse_mass == 0.0 {
            return Ok(Some(contact));
        }

        let relative_velocity = self.response_velocity() - other.response_velocity();
        let normal_speed = relative_velocity.dot(normal);
        let speed_correction = -(1.0 + restitution as f64) * normal_speed.min(0.0);
        let left = self.response_candidate(
            normal,
            contact.penetration as f64,
            speed_correction,
            left_inverse_mass / inverse_mass,
        )?;
        let right = other.response_candidate(
            -normal,
            contact.penetration as f64,
            speed_correction,
            right_inverse_mass / inverse_mass,
        )?;
        *self = left;
        *other = right;
        Ok(Some(contact))
    }

    /// Resolves this sphere body against one stationary world-space OBB.
    ///
    /// Uses [`Sphere::contact_obb`] and the same validation, restitution,
    /// rounding and discrete-step policy as [`Self::resolve_collision`]. The
    /// collider is never changed. The returned original contact points from
    /// the box toward the sphere. AABB bodies return
    /// [`CollisionResponseError::UnsupportedShape`]; this does not extend
    /// [`RigidBodyShape`] or approximate an AABB with a sphere.
    ///
    /// The OBB must have finite center/half extents, non-negative extents and a
    /// rotation accepted by its existing queries. Non-unit rotations are
    /// normalized by those queries. Local-transform arithmetic failure is an
    /// error, not a separated pair. Static and kinematic sphere bodies are
    /// validated and queried but remain unchanged.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let floor = Obb::new(-Vec3::Y, vec3(4.0, 1.0, 4.0), Quat::IDENTITY);
    /// let mut ball = RigidBody::new(Vec3::Y * 0.75,
    ///     RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, 1.0)), 2.0).unwrap();
    /// ball.linear_velocity = -Vec3::Y * 4.0;
    /// ball.resolve_static_obb(&floor, 0.5).unwrap();
    /// assert_eq!(ball.position, Vec3::Y);
    /// assert_eq!(ball.linear_velocity, Vec3::Y * 2.0);
    /// ```
    pub fn resolve_static_obb(
        &mut self,
        collider: &Obb,
        restitution: f32,
    ) -> Result<Option<Contact>, CollisionResponseError> {
        validate_restitution(restitution)?;
        self.validate()?;
        if collider.pose().is_none() {
            return Err(CollisionResponseError::InvalidCollider);
        }
        let RigidBodyShape::Sphere(sphere) = self.world_shape()? else {
            return Err(CollisionResponseError::UnsupportedShape);
        };
        let (local, bounds, _) = collider
            .in_local_space(&sphere)
            .ok_or(CollisionResponseError::NumericalFailure)?;
        check_sphere_aabb_arithmetic(local, bounds)?;
        // A valid local contact must not disappear because transforming its
        // point or normal back to world space overflowed.
        let local_contact = local.contact_aabb(&bounds);
        let contact = sphere.contact_obb(collider);
        if local_contact.is_some() && contact.is_none() {
            return Err(CollisionResponseError::NumericalFailure);
        }
        let Some(contact) = contact else {
            return Ok(None);
        };
        let normal = checked_normal(contact)?;
        if self.response_inverse_mass() != 0.0 {
            let normal_speed = self.response_velocity().dot(normal);
            let speed_correction = -(1.0 + restitution as f64) * normal_speed.min(0.0);
            *self =
                self.response_candidate(normal, contact.penetration as f64, speed_correction, 1.0)?;
        }
        Ok(Some(contact))
    }

    pub(super) fn response_inverse_mass(&self) -> f64 {
        if self.is_static() || self.mode == RigidBodyMode::Kinematic {
            0.0
        } else {
            1.0 / self.mass as f64
        }
    }

    pub(super) fn response_velocity(&self) -> glam::DVec3 {
        if self.is_static() {
            glam::DVec3::ZERO
        } else {
            self.linear_velocity.as_dvec3()
        }
    }

    fn response_candidate(
        &self,
        normal: glam::DVec3,
        penetration: f64,
        speed_correction: f64,
        weight: f64,
    ) -> Result<Self, CollisionResponseError> {
        if weight == 0.0 || (penetration == 0.0 && speed_correction == 0.0) {
            return Ok(*self);
        }
        let position = if penetration == 0.0 {
            self.position
        } else {
            (self.position.as_dvec3() + normal * (penetration * weight)).as_vec3()
        };
        let linear_velocity = if speed_correction == 0.0 {
            self.linear_velocity
        } else {
            (self.linear_velocity.as_dvec3() + normal * (speed_correction * weight)).as_vec3()
        };
        if !position.is_finite() || !linear_velocity.is_finite() {
            return Err(RigidBodyError::NonFiniteResult.into());
        }
        let candidate = Self {
            position,
            linear_velocity,
            ..*self
        };
        candidate.world_shape()?;
        Ok(candidate)
    }

    pub(super) fn validate(&self) -> Result<(), RigidBodyError> {
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

fn validate_restitution(restitution: f32) -> Result<(), CollisionResponseError> {
    if !restitution.is_finite() || !(0.0..=1.0).contains(&restitution) {
        return Err(CollisionResponseError::InvalidRestitution);
    }
    Ok(())
}

fn checked_normal(contact: Contact) -> Result<glam::DVec3, CollisionResponseError> {
    let normal = contact.normal.as_dvec3();
    let length_squared = normal.length_squared();
    if !contact.normal.is_finite()
        || !contact.point.is_finite()
        || !contact.penetration.is_finite()
        || contact.penetration < 0.0
        || (length_squared - 1.0).abs() > 1e-4
    {
        return Err(CollisionResponseError::NumericalFailure);
    }
    Ok(normal / length_squared.sqrt())
}

// Legacy contacts intentionally remain unchanged. Reject the numerical range
// where their f32 squared distances can fabricate a direction or overlap.
fn check_length_squared(delta: Vec3) -> Result<(), CollisionResponseError> {
    let squared = delta.length_squared();
    if !delta.is_finite() || !squared.is_finite() || (delta != Vec3::ZERO && !squared.is_normal()) {
        return Err(CollisionResponseError::NumericalFailure);
    }
    Ok(())
}

fn check_sphere_aabb_arithmetic(
    sphere: Sphere,
    bounds: Aabb,
) -> Result<(), CollisionResponseError> {
    if bounds.contains(sphere.center) {
        let lower = sphere.center - bounds.min;
        let upper = bounds.max - sphere.center;
        let depth = lower.min(upper).min_element();
        check_length_squared(Vec3::X * depth)
    } else {
        check_length_squared(sphere.center - bounds.closest_point(sphere.center))?;
        check_length_squared(Vec3::X * sphere.radius)
    }
}

fn body_contact(
    left: RigidBodyShape,
    right: RigidBodyShape,
) -> Result<Option<Contact>, CollisionResponseError> {
    Ok(match (left, right) {
        (RigidBodyShape::Aabb(left), RigidBodyShape::Aabb(right)) => left.contact(&right),
        (RigidBodyShape::Sphere(left), RigidBodyShape::Sphere(right)) => {
            check_length_squared(left.center - right.center)?;
            check_length_squared(Vec3::X * (left.radius + right.radius))?;
            left.contact(&right)
        }
        (RigidBodyShape::Sphere(sphere), RigidBodyShape::Aabb(bounds)) => {
            check_sphere_aabb_arithmetic(sphere, bounds)?;
            sphere.contact_aabb(&bounds)
        }
        (RigidBodyShape::Aabb(bounds), RigidBodyShape::Sphere(sphere)) => {
            check_sphere_aabb_arithmetic(sphere, bounds)?;
            sphere.contact_aabb(&bounds).map(|contact| Contact {
                normal: -contact.normal,
                ..contact
            })
        }
    })
}
