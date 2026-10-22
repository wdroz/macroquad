//! Example-owned composition, shared with headless tests. No window or solver world.
use macroquad::prelude::*;

// These fractions are PER CALL. One joint call per fixed tick is intentional:
// changing the timestep or adding iterations changes the apparent stiffness.
pub const STEP: f64 = 1.0 / 120.0;
pub const MAX_FRAME: f64 = 0.1;
pub const MAX_STEPS: u32 = 12;
pub const RADIUS: f32 = 0.35;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    JointThenContacts,
    ContactsThenJoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepError {
    FrameTime,
    Integrate(RigidBodyError),
    Contact(CollisionResponseError),
    Joint(DistanceJointError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bodies {
    pub anchor: RigidBody,
    pub bob: RigidBody,
    pub free: RigidBody,
    pub floor: RigidBody,
}

impl Bodies {
    fn new(kinematic: bool) -> Self {
        let sphere = RigidBodyShape::Sphere(Sphere::new(Vec3::ZERO, RADIUS));
        let mut anchor =
            RigidBody::new(vec3(0., 1.65, 0.), sphere, if kinematic { 1. } else { 0. }).unwrap();
        anchor.mode = RigidBodyMode::Kinematic;
        let bob = RigidBody::new(vec3(0.65, RADIUS, 0.), sphere, 1.).unwrap();
        let mut free = RigidBody::new(vec3(-2.6, 2.8, 0.), sphere, 1.).unwrap();
        free.linear_velocity = vec3(0.6, 0., 0.);
        free.linear_damping = 0.2;
        let floor = RigidBody::new(
            Vec3::ZERO,
            RigidBodyShape::Aabb(Aabb::from_min_max(vec3(-8., -0.25, -2.), vec3(8., 0., 2.))),
            0.,
        )
        .unwrap();
        Self {
            anchor,
            bob,
            free,
            floor,
        }
    }

    fn contacts(&mut self) -> Result<u32, StepError> {
        // Caller-selected, fixed pair order; no broadphase or hidden iterations.
        let free_floor = self
            .free
            .resolve_collision(&mut self.floor, 0.2)
            .map_err(StepError::Contact)?
            .is_some();
        let bob_floor = self
            .bob
            .resolve_collision(&mut self.floor, 0.2)
            .map_err(StepError::Contact)?
            .is_some();
        let free_bob = self
            .free
            .resolve_collision(&mut self.bob, 0.2)
            .map_err(StepError::Contact)?
            .is_some();
        Ok(u32::from(free_floor) + u32::from(bob_floor) + u32::from(free_bob))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene {
    pub bodies: Bodies,
    pub joint: DistanceJoint,
    pub order: Order,
    pub kinematic: bool,
    pub paused: bool,
    pub accumulator: f64,
    pub ticks: u32,
    pub contacts: u32,
    pub error: Option<StepError>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            bodies: Bodies::new(false),
            joint: DistanceJoint {
                rest_length: 1.5,
                position_correction: 0.8,
                velocity_correction: 0.2,
            },
            order: Order::JointThenContacts,
            kinematic: false,
            paused: false,
            accumulator: 0.,
            ticks: 0,
            contacts: 0,
            error: None,
        }
    }

    /// Reset the selected demonstration while retaining pause/order/anchor choices.
    pub fn reset(&mut self) {
        *self = Self {
            bodies: Bodies::new(self.kinematic),
            order: self.order,
            kinematic: self.kinematic,
            paused: self.paused,
            ..Self::new()
        };
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused || self.error.is_some();
        // Discard fractional time on BOTH transitions: resume cannot catch up.
        self.accumulator = 0.;
    }

    pub fn toggle_order(&mut self) {
        self.order = match self.order {
            Order::JointThenContacts => Order::ContactsThenJoint,
            Order::ContactsThenJoint => Order::JointThenContacts,
        };
        self.reset();
    }

    pub fn toggle_anchor(&mut self) {
        self.kinematic = !self.kinematic;
        self.reset();
    }

    fn fail(&mut self, error: StepError) -> Result<u32, StepError> {
        self.error = Some(error);
        self.set_paused(true);
        Err(error)
    }

    /// Bounded example clock: drop excess wall time, never exceed twelve steps.
    pub fn advance(&mut self, frame_seconds: f64) -> Result<u32, StepError> {
        if !frame_seconds.is_finite() || frame_seconds < 0. {
            return self.fail(StepError::FrameTime);
        }
        if self.paused || self.error.is_some() {
            self.accumulator = 0.;
            return Ok(0);
        }
        self.accumulator = (self.accumulator + frame_seconds).min(MAX_FRAME);
        let mut count = 0;
        while self.accumulator >= STEP && count < MAX_STEPS {
            if let Err(error) = self.tick() {
                return self.fail(error);
            }
            self.accumulator -= STEP;
            count += 1;
        }
        Ok(count)
    }

    fn tick(&mut self) -> Result<(), StepError> {
        // Each public call is transactional, but their composition is not.
        // Stage the COMPLETE tick locally, then commit only if every call passes.
        let mut next = self.bodies;
        next.anchor.linear_velocity = if self.kinematic {
            vec3(0.6 * (self.ticks as f64 * STEP).cos() as f32, 0., 0.)
        } else {
            // Zero mass ignores this stored velocity in integration/response.
            Vec3::ZERO
        };
        for body in [&mut next.anchor, &mut next.bob, &mut next.free] {
            body.integrate(STEP as f32, -Vec3::Y * 4.)
                .map_err(StepError::Integrate)?;
        }
        if self.order == Order::JointThenContacts {
            self.joint
                .solve(&mut next.anchor, &mut next.bob)
                .map_err(StepError::Joint)?;
        }
        let contacts = next.contacts()?;
        if self.order == Order::ContactsThenJoint {
            self.joint
                .solve(&mut next.anchor, &mut next.bob)
                .map_err(StepError::Joint)?;
        }
        self.bodies = next;
        self.contacts = contacts;
        self.ticks = self.ticks.saturating_add(1);
        Ok(())
    }

    pub fn residual(&self) -> f32 {
        ((self.bodies.bob.position - self.bodies.anchor.position).length() - self.joint.rest_length)
            .abs()
    }

    pub fn floor_overlap(&self) -> f32 {
        (RADIUS - self.bodies.bob.position.y).max(0.)
    }

    pub fn finite(&self) -> bool {
        self.accumulator.is_finite()
            && [
                self.bodies.anchor,
                self.bodies.bob,
                self.bodies.free,
                self.bodies.floor,
            ]
            .iter()
            .all(|b| {
                b.world_shape().is_ok()
                    && b.linear_velocity.is_finite()
                    && b.mass.is_finite()
                    && b.linear_damping.is_finite()
            })
            && self.joint.rest_length.is_finite()
            && self.joint.position_correction.is_finite()
            && self.joint.velocity_correction.is_finite()
            && self.residual().is_finite()
            && self.floor_overlap().is_finite()
    }
}

/// Same incompatible pose, one call each: final joint correction can overlap.
pub fn ordering_probe(order: Order) -> (f32, f32) {
    let mut scene = Scene::new();
    scene.order = order;
    scene.bodies.bob.position = vec3(0., RADIUS, 0.);
    if order == Order::JointThenContacts {
        scene
            .joint
            .solve(&mut scene.bodies.anchor, &mut scene.bodies.bob)
            .unwrap();
    }
    scene.bodies.contacts().unwrap();
    if order == Order::ContactsThenJoint {
        scene
            .joint
            .solve(&mut scene.bodies.anchor, &mut scene.bodies.bob)
            .unwrap();
    }
    (scene.residual(), scene.floor_overlap())
}

/// Identical deterministic public-API replay on native/WebGL and in CPU tests.
pub fn replay() -> u32 {
    let mut checks = 0;
    for kinematic in [false, true] {
        for order in [Order::JointThenContacts, Order::ContactsThenJoint] {
            let mut scene = Scene::new();
            scene.kinematic = kinematic;
            scene.order = order;
            scene.reset();
            let initial = scene.bodies;
            for _ in 0..180 {
                assert_eq!(scene.advance(1. / 60.), Ok(2));
                assert!(scene.finite());
                checks += 2;
            }
            assert_ne!(scene.bodies.free.position, initial.free.position);
            assert_eq!(scene.bodies.floor, initial.floor);
            if !kinematic {
                assert_eq!(scene.bodies.anchor, initial.anchor);
            } else {
                assert_ne!(scene.bodies.anchor.position, initial.anchor.position);
            }
            checks += 3;
            scene.set_paused(true);
            let held = scene;
            assert_eq!(scene.advance(30.), Ok(0));
            assert_eq!(scene, held);
            scene.reset();
            assert_eq!(scene.bodies, initial);
            assert_eq!(scene.ticks, 0);
            assert_eq!(scene.accumulator, 0.);
            scene.set_paused(false);
            assert_eq!(scene.advance(STEP / 2.), Ok(0));
            assert_eq!(scene.advance(STEP / 2.), Ok(1));
            checks += 7;
        }
    }
    let joint_first = ordering_probe(Order::JointThenContacts);
    let contacts_first = ordering_probe(Order::ContactsThenJoint);
    assert!((joint_first.0 - 0.2).abs() < 1e-5 && joint_first.1 < 1e-5);
    assert!((contacts_first.0 - 0.04).abs() < 1e-5 && (contacts_first.1 - 0.16).abs() < 1e-5);
    checks + 2
}
