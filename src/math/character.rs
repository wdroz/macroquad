//! Caller-owned AABB characters moving against explicitly selected static solids.

use super::{Aabb, Capsule, Obb, Sphere, SweepHit, Vec3};

/// Borrowed static solids for [`CharacterController::move_body`].
///
/// These slices contain **only blocking colliders**. Keep triggers in a separate
/// collection and query their overlaps after moving. Nothing here owns a world,
/// filters imported marker roles, or changes the supplied shapes.
#[derive(Clone, Copy, Debug, Default)]
pub struct CharacterSolids<'a> {
    /// Axis-aligned boxes in world coordinates.
    pub aabbs: &'a [Aabb],
    /// Oriented boxes in world coordinates.
    pub obbs: &'a [Obb],
    /// Static spheres in world coordinates.
    pub spheres: &'a [Sphere],
    /// Static capsules in world coordinates; the moving body remains an AABB.
    pub capsules: &'a [Capsule],
}

/// Settings for swept, kinematic AABB movement; no GPU or window is needed.
///
/// The caller supplies the complete world-space body and velocity each step,
/// applies gravity, and retains the result. Motion stops at the first body
/// sweep against each solid and slides along blocking normals. Steep upward
/// faces cannot turn horizontal input into uphill motion; falling can slide
/// downhill. [`Self::move_body_with_step`] adds opt-in stair stepping. This is
/// translation only: no capsule players, moving platforms, impulses, or
/// rigid-body integration.
///
/// Initial overlaps are pushed out using the existing contacts/sweep normals,
/// deepest first. Recovery and sliding are bounded, so mutually penetrating
/// solids may prevent complete recovery. Exhausting the slide budget discards
/// the remaining displacement rather than moving through an unchecked solid.
/// The existing sweeps' touch tolerances and finite-precision limits apply.
///
/// # Example
///
/// ```
/// use macroquad::math::{Aabb, CharacterController, CharacterSolids, Vec3, vec3};
/// let floor = Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(20.0, 1.0, 20.0));
/// let body = Aabb::from_center_size(vec3(0.0, 0.9, 0.0), vec3(0.6, 1.8, 0.6));
/// let solids = CharacterSolids { aabbs: &[floor], ..Default::default() };
/// // The caller adds gravity to its chosen horizontal velocity.
/// let velocity = vec3(2.0, -9.8 / 60.0, 0.0);
/// let moved = CharacterController::default().move_body(body, velocity, 1.0 / 60.0, solids);
/// assert!(moved.on_ground);
/// assert_eq!(moved.ground_normal, Some(Vec3::Y));
/// assert_eq!(moved.velocity.y, 0.0);
/// assert!(moved.body.center().x > body.center().x);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterController {
    /// World-space up direction, normalized internally. Default: +Y.
    pub up: Vec3,
    /// Largest walkable angle from `up`, in radians, in `0..=PI/2`.
    /// Default: 45 degrees. Vertical faces are never ground.
    pub max_slope_angle: f32,
    /// Maximum downward attachment distance, in world units. Default: 0.1.
    /// Zero disables attachment across gaps but still detects a touching floor.
    pub ground_snap_distance: f32,
    /// Maximum slide and overlap-recovery passes, separately. Default: 8.
    /// Values above 32 are capped at 32; zero makes the call a no-op.
    pub max_iterations: usize,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            up: Vec3::Y,
            max_slope_angle: std::f32::consts::FRAC_PI_4,
            ground_snap_distance: 0.1,
            max_iterations: 8,
        }
    }
}

/// Result of one [`CharacterController::move_body`] call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterMotion {
    /// Resolved world-space body; its full dimensions are preserved.
    pub body: Aabb,
    /// Input velocity with blocked components removed. Gravity is caller-owned.
    pub velocity: Vec3,
    /// Whether the final body has walkable support (including a successful snap).
    pub on_ground: bool,
    /// Outward world-space support normal, or `None` when airborne.
    pub ground_normal: Option<Vec3>,
}

/// Outcome of the opt-in stair/ledge attachment in
/// [`CharacterController::move_body_with_step`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterStepStatus {
    /// A zero step height keeps ordinary controller behavior.
    Disabled,
    /// Invalid height, controller settings, body, time or velocity.
    InvalidInput,
    /// Positive input velocity along up disables stepping and attachment.
    Jumping,
    /// The initial body has no touching walkable support.
    Airborne,
    /// Ordinary movement already reaches the requested horizontal position.
    Unneeded,
    /// Overhead geometry leaves insufficient clearance for the attempted step.
    HeadBlocked,
    /// Raising the body does not improve movement past the obstacle.
    ForwardBlocked,
    /// The attempted final position has no support within the allowed drop.
    NoLanding,
    /// The first downward contact exceeds the controller's slope limit.
    SteepLanding,
    /// The candidate does not gain height or improve horizontal progress.
    NoProgress,
    /// An upward step was accepted.
    Stepped,
    /// A grounded body attached to lower support within the step height.
    Descended,
}

/// World-space contact diagnostics for one optional step attempt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterStepDiagnostics {
    /// Whether stepping was used, skipped or rejected.
    pub status: CharacterStepStatus,
    /// First horizontal obstruction, or the overhead contact when head blocked.
    /// Its `t` is a fraction of that particular horizontal/upward probe.
    pub obstacle: Option<SweepHit>,
    /// First downward contact of the attempted landing, even when too steep.
    /// Its `t` is a fraction of that downward probe, not the whole movement.
    pub landing: Option<SweepHit>,
    /// Accepted displacement from the recovered starting body along normalized
    /// `up`; zero for rejected steps.
    pub rise: f32,
}

/// Ordinary movement plus optional stair diagnostics; existing
/// [`CharacterMotion`] and [`CharacterController`] literals remain unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterStepMotion {
    /// Final body, velocity and support state.
    pub motion: CharacterMotion,
    /// Step decision and contacts, suitable for an application debug overlay.
    pub step: CharacterStepDiagnostics,
}

impl CharacterController {
    /// Opt-in step offset, preserving [`Self::move_body`] when stepping fails.
    ///
    /// `step_height` is a maximum vertical rise/drop in world units. Zero
    /// disables this helper; negative or non-finite heights are rejected. The
    /// original controller and motion structs gain no fields. All the ordinary
    /// controller validation, sweep tolerances and bounded sliding still apply.
    ///
    /// Only a body initially touching walkable ground (within 0.0001 world
    /// units) can step. Positive input velocity along `up` always jumps without
    /// stepping. If ordinary movement is horizontally obstructed, try a swept
    /// lift, ordinary horizontal sliding, then a downward sweep onto walkable
    /// support. Overhead geometry limits the lift. Accept only a higher landing
    /// within `step_height` that improves forward progress; a low barrier cannot
    /// be vaulted to the floor behind it. At most one rise is attempted per
    /// call, so use normal simulation time steps for a flight of stairs.
    ///
    /// Grounded ordinary movement can also attach to a lower surface within
    /// `step_height`. The first downward solid must be walkable: attachment
    /// cannot pass through steep geometry. Without lower support, the ordinary
    /// result remains airborne at a ledge. Falling/jumping bodies keep ordinary
    /// landing behavior and never gain the extra attachment distance.
    ///
    /// The returned diagnostics describe the attempted probes. They do not
    /// identify application collider indices or provide a contact manifold.
    ///
    /// ```
    /// use macroquad::math::{Aabb, CharacterController, CharacterSolids, CharacterStepStatus, vec3};
    /// let boxes = [
    ///     Aabb::from_min_max(vec3(-5.0, -1.0, -5.0), vec3(5.0, 0.0, 5.0)),
    ///     Aabb::from_min_max(vec3(0.5, 0.0, -1.0), vec3(2.0, 0.2, 1.0)),
    /// ];
    /// let body = Aabb::from_center_size(vec3(0.0, 0.5, 0.0), vec3(0.4, 1.0, 0.4));
    /// let stepped = CharacterController::default().move_body_with_step(
    ///     body, vec3(1.0, -1.0, 0.0), 0.6,
    ///     CharacterSolids { aabbs: &boxes, ..Default::default() }, 0.25,
    /// );
    /// assert_eq!(stepped.step.status, CharacterStepStatus::Stepped);
    /// assert!(stepped.motion.on_ground);
    /// assert!((stepped.motion.body.min.y - 0.2).abs() < 0.0001);
    /// ```
    pub fn move_body_with_step(
        &self,
        body: Aabb,
        velocity: Vec3,
        dt: f32,
        solids: CharacterSolids<'_>,
        step_height: f32,
    ) -> CharacterStepMotion {
        let ordinary = self.move_body(body, velocity, dt, solids);
        let mut result = CharacterStepMotion {
            motion: ordinary,
            step: CharacterStepDiagnostics {
                status: CharacterStepStatus::Unneeded,
                obstacle: None,
                landing: None,
                rise: 0.0,
            },
        };
        if step_height == 0.0 {
            result.step.status = CharacterStepStatus::Disabled;
            return result;
        }
        let up_length = self.up.as_dvec3().length();
        let delta = velocity * dt;
        if !step_height.is_finite()
            || step_height < 0.0
            || !body.min.is_finite()
            || !body.max.is_finite()
            || body.min.cmpgt(body.max).any()
            || !velocity.is_finite()
            || !delta.is_finite()
            || !dt.is_finite()
            || dt <= 0.0
            || !up_length.is_finite()
            || up_length == 0.0
            || !self.max_slope_angle.is_finite()
            || !(0.0..=std::f32::consts::FRAC_PI_2).contains(&self.max_slope_angle)
            || !self.ground_snap_distance.is_finite()
            || self.ground_snap_distance < 0.0
            || self.max_iterations == 0
        {
            result.step.status = CharacterStepStatus::InvalidInput;
            return result;
        }
        let up = (self.up.as_dvec3() / up_length).as_vec3();
        let up_velocity = velocity.as_dvec3().dot(up.as_dvec3());
        if up_velocity > 0.0 {
            result.step.status = CharacterStepStatus::Jumping;
            return result;
        }
        let slope_cos = self.max_slope_angle.cos().max(0.0);
        // Recover exactly as ordinary movement does, but never snap an airborne
        // start down to manufacture eligibility for an upward step.
        let no_snap = Self {
            ground_snap_distance: 0.0,
            ..*self
        };
        let start = no_snap.move_body(body, Vec3::ZERO, dt, solids).body;
        let support = solids.first_hit(start, -up * 0.001);
        if solids.deepest_push(start).is_some()
            || !support
                .is_some_and(|hit| hit.t * 0.001 <= 0.0001 && walkable(hit.normal, up, slope_cos))
        {
            result.step.status = CharacterStepStatus::Airborne;
            return result;
        }
        let up64 = up.as_dvec3();
        let horizontal = (delta.as_dvec3() - up64 * delta.as_dvec3().dot(up64)).as_vec3();
        if !horizontal.is_finite() {
            result.step.status = CharacterStepStatus::InvalidInput;
            return result;
        }
        let horizontal_length = horizontal.as_dvec3().length();
        let direction = if horizontal_length > 0.0 {
            (horizontal.as_dvec3() / horizontal_length).as_vec3()
        } else {
            Vec3::ZERO
        };
        let progress =
            (ordinary.body.min.as_dvec3() - start.min.as_dvec3()).dot(direction.as_dvec3());
        let obstructed = progress + 0.00001 < horizontal_length;
        if !obstructed {
            // The ordinary snap remains authoritative when already grounded.
            // Extend it only for a character that walked down off its support.
            if !ordinary.on_ground {
                let ordinary_rise = (ordinary.body.min.as_dvec3() - start.min.as_dvec3()).dot(up64);
                let remaining_drop = (step_height as f64 + ordinary_rise) as f32;
                // Gravity may already have moved the body farther down than
                // a step. Never extend attachment below the initial limit.
                if !remaining_drop.is_finite() || remaining_drop <= 0.0 {
                    result.step.status = CharacterStepStatus::NoLanding;
                    return result;
                }
                if let Some(hit) = solids.first_hit(ordinary.body, -up * remaining_drop) {
                    result.step.landing = Some(hit);
                    if walkable(hit.normal, up, slope_cos) {
                        if let Some(landed) =
                            translated(ordinary.body, -up * (hit.t * remaining_drop))
                        {
                            if solids.deepest_push(landed).is_none() {
                                result.motion = CharacterMotion {
                                    body: landed,
                                    velocity: self.supported_velocity(
                                        ordinary.velocity,
                                        landed,
                                        &[hit.normal],
                                        dt,
                                        solids,
                                    ),
                                    on_ground: true,
                                    ground_normal: Some(hit.normal),
                                };
                                result.step.status = CharacterStepStatus::Descended;
                                result.step.rise =
                                    (landed.min.as_dvec3() - start.min.as_dvec3()).dot(up64) as f32;
                            }
                        }
                    } else {
                        result.step.status = CharacterStepStatus::SteepLanding;
                    }
                } else {
                    result.step.status = CharacterStepStatus::NoLanding;
                }
            }
            return result;
        }
        result.step.obstacle = solids.first_hit(start, horizontal);
        let upward = up * step_height;
        let overhead = solids.first_hit(start, upward);
        let lift = overhead.map_or(step_height, |hit| step_height * hit.t);
        let Some(raised) = translated(start, up * lift) else {
            result.step.status = CharacterStepStatus::InvalidInput;
            return result;
        };
        if lift <= 0.00001 || solids.deepest_push(raised).is_some() {
            result.step.status = CharacterStepStatus::HeadBlocked;
            result.step.obstacle = overhead.or(result.step.obstacle);
            return result;
        }
        let horizontal_velocity = (velocity.as_dvec3() - up64 * up_velocity).as_vec3();
        if !horizontal_velocity.is_finite() {
            result.step.status = CharacterStepStatus::InvalidInput;
            return result;
        }
        let forward = no_snap.move_body(raised, horizontal_velocity, dt, solids);
        let forward_progress =
            (forward.body.min.as_dvec3() - start.min.as_dvec3()).dot(direction.as_dvec3());
        if forward_progress <= progress + 0.00001 {
            result.step.status = if overhead.is_some() {
                result.step.obstacle = overhead;
                CharacterStepStatus::HeadBlocked
            } else {
                CharacterStepStatus::ForwardBlocked
            };
            return result;
        }
        // This sweep may be longer than the upward lift if the horizontal
        // portion followed a walkable ramp. The accepted rise is checked below.
        let above_start = (forward.body.min.as_dvec3() - start.min.as_dvec3()).dot(up64);
        let down_distance = above_start.max(0.0) as f32;
        if !down_distance.is_finite() {
            result.step.status = CharacterStepStatus::InvalidInput;
            return result;
        }
        let Some(hit) = solids.first_hit(forward.body, -up * down_distance.max(0.001)) else {
            result.step.status = CharacterStepStatus::NoLanding;
            return result;
        };
        result.step.landing = Some(hit);
        if !walkable(hit.normal, up, slope_cos) {
            result.step.status = CharacterStepStatus::SteepLanding;
            return result;
        }
        let Some(landed) = translated(forward.body, -up * (hit.t * down_distance.max(0.001)))
        else {
            result.step.status = CharacterStepStatus::InvalidInput;
            return result;
        };
        let rise = (landed.min.as_dvec3() - start.min.as_dvec3()).dot(up64) as f32;
        if rise <= 0.00001 || rise > step_height + 0.00001 || solids.deepest_push(landed).is_some()
        {
            result.step.status = CharacterStepStatus::NoProgress;
            return result;
        }
        let candidate_velocity = (forward.velocity.as_dvec3() + up64 * up_velocity).as_vec3();
        if !candidate_velocity.is_finite() {
            result.step.status = CharacterStepStatus::InvalidInput;
            return result;
        }
        result.motion = CharacterMotion {
            body: landed,
            velocity: self.supported_velocity(
                candidate_velocity,
                landed,
                &[hit.normal],
                dt,
                solids,
            ),
            on_ground: true,
            ground_normal: Some(hit.normal),
        };
        result.step.status = CharacterStepStatus::Stepped;
        result.step.rise = rise;
        result
    }

    /// Moves `body` by `velocity * dt`, then checks support at its final position.
    ///
    /// All positions, normals and velocity use world coordinates; `dt` is in
    /// the caller's time units. The AABB remains aligned to the world axes even
    /// with a non-Y `up`. Body dimensions stay in the caller, not these settings.
    ///
    /// When the **input** velocity has a positive component along `up`, ground
    /// snapping and grounding are skipped so a jump immediately detaches.
    /// Otherwise a downward probe stops at the first solid, even if that face
    /// is unwalkable, and attaches only to walkable support within the snap
    /// distance. A floor touched earlier in the step does not imply support
    /// after walking off its edge. Snap distance can be set to zero in flight
    /// when the caller wants attachment only after an ordinary landing.
    /// For AABB solids, a stationary-axis projection that only meets at its
    /// endpoints is tangential, including exact ledge-edge support. Thus a
    /// zero-width body/platform cannot stand on another projection's boundary.
    ///
    /// Invalid/non-finite body bounds, velocity, displacement, `dt <= 0`, zero
    /// or non-finite `up`, invalid slope/snap settings, and zero iterations return
    /// the original body/velocity, ungrounded. Invalid static solids are ignored
    /// by their existing queries. No input is mutated.
    pub fn move_body(
        &self,
        body: Aabb,
        velocity: Vec3,
        dt: f32,
        solids: CharacterSolids<'_>,
    ) -> CharacterMotion {
        let mut result = CharacterMotion {
            body,
            velocity,
            on_ground: false,
            ground_normal: None,
        };
        let up_length = self.up.as_dvec3().length();
        let delta = velocity * dt;
        if !body.min.is_finite()
            || !body.max.is_finite()
            || body.min.cmpgt(body.max).any()
            || !velocity.is_finite()
            || !delta.is_finite()
            || !dt.is_finite()
            || dt <= 0.0
            || !up_length.is_finite()
            || up_length == 0.0
            || !self.max_slope_angle.is_finite()
            || !(0.0..=std::f32::consts::FRAC_PI_2).contains(&self.max_slope_angle)
            || !self.ground_snap_distance.is_finite()
            || self.ground_snap_distance < 0.0
            || self.max_iterations == 0
        {
            return result;
        }
        let up = (self.up.as_dvec3() / up_length).as_vec3();
        let slope_cos = self.max_slope_angle.cos().max(0.0);
        let budget = self.max_iterations.min(32);
        // Strictly positive depth only: an inclusive floor touch is not a push.
        for _ in 0..budget {
            let Some(push) = solids.deepest_push(result.body) else {
                break;
            };
            let Some(next) = translated(result.body, push) else {
                return result;
            };
            result.body = next;
        }
        // An impossible or budget-exhausted starting pose is not a supported
        // landing. Keep the bounded recovery, but do not apply an unchecked
        // move or report ground while still substantially inside a solid.
        if solids.deepest_push(result.body).is_some() {
            return result;
        }
        let mut remaining = delta;
        let mut planes = [Vec3::ZERO; 33];
        let mut plane_count = 0;
        for _ in 0..budget {
            if remaining == Vec3::ZERO {
                break;
            }
            let Some(hit) = solids.first_hit(result.body, remaining) else {
                if let Some(next) = translated(result.body, remaining) {
                    result.body = next;
                }
                break;
            };
            if let Some(next) = translated(result.body, remaining * hit.t) {
                result.body = next;
            } else {
                break;
            }
            let left = remaining * (1.0 - hit.t);
            let normal = blocking_normal(left, hit.normal, up, slope_cos);
            planes[plane_count] = normal;
            plane_count += 1;
            remaining = clip_planes(left, &planes[..plane_count]);
            result.velocity = clip_planes(result.velocity, &planes[..plane_count]);
            // A deep unresolved overlap can return t=0 while moving outward.
            // Do not interpret the unchanged vector as permission to tunnel.
            if hit.t == 0.0 && remaining == left {
                break;
            }
        }
        if velocity.dot(up) <= 0.0 && solids.deepest_push(result.body).is_none() {
            // A nonzero probe also finds touching support when snapping is off.
            // It never moves across more than the configured attachment gap.
            let distance = self.ground_snap_distance.max(0.001);
            if let Some(hit) = solids.first_hit(result.body, -up * distance) {
                let travel = hit.t * distance;
                if walkable(hit.normal, up, slope_cos) && travel <= self.ground_snap_distance {
                    if let Some(next) = translated(result.body, -up * travel) {
                        result.body = next;
                        result.ground_normal = Some(hit.normal);
                        result.on_ground = true;
                        // Preserve previous contacts and inspect the final
                        // pose: snapping onto a slope can introduce velocity
                        // into a resting wall not hit during ordinary motion.
                        planes[plane_count] = hit.normal;
                        result.velocity = self.supported_velocity(
                            result.velocity,
                            result.body,
                            &planes[..=plane_count],
                            dt,
                            solids,
                        );
                    }
                }
            }
        }
        result
    }
}

impl CharacterController {
    // Keep earlier contacts, landing support and walls touched at the final
    // pose together. Sequential projections can reenter an earlier plane.
    fn supported_velocity(
        &self,
        input: Vec3,
        body: Aabb,
        initial_planes: &[Vec3],
        dt: f32,
        solids: CharacterSolids<'_>,
    ) -> Vec3 {
        let up = (self.up.as_dvec3() / self.up.as_dvec3().length()).as_vec3();
        let slope_cos = self.max_slope_angle.cos().max(0.0);
        // At most 32 movement contacts plus ground, then 32 final-pose probes.
        let mut planes = [Vec3::ZERO; 65];
        let count = initial_planes.len();
        planes[..count].copy_from_slice(initial_planes);
        let mut velocity = clip_planes(input, initial_planes);
        for count in count..count + self.max_iterations.min(32) {
            let delta = velocity * dt;
            if !delta.is_finite() {
                return Vec3::ZERO;
            }
            let Some(hit) = solids.first_hit(body, delta) else {
                break;
            };
            if hit.t > 0.00001 {
                break;
            }
            let normal = blocking_normal(delta, hit.normal, up, slope_cos);
            planes[count] = normal;
            let next = clip_planes(input, &planes[..=count]);
            if next == velocity {
                break;
            }
            velocity = next;
        }
        velocity
    }
}

fn translated(body: Aabb, delta: Vec3) -> Option<Aabb> {
    let next = Aabb::from_min_max(body.min + delta, body.max + delta);
    (next.min.is_finite() && next.max.is_finite()).then_some(next)
}

fn walkable(normal: Vec3, up: Vec3, slope_cos: f32) -> bool {
    let height = normal.dot(up);
    height > 0.0 && height + 1e-6 >= slope_cos
}

fn blocking_normal(delta: Vec3, normal: Vec3, up: Vec3, slope_cos: f32) -> Vec3 {
    let height = normal.dot(up);
    if height > 0.0 && !walkable(normal, up, slope_cos) {
        let slide = delta - normal * delta.dot(normal).min(0.0);
        // Treat an unwalkable incline as a horizontal wall when projection
        // would create uphill motion; preserve deliberate jumping/falling.
        if slide.dot(up) > delta.dot(up).max(0.0) {
            return (normal - up * height).normalize();
        }
    }
    normal
}

fn clip_planes(input: Vec3, planes: &[Vec3]) -> Vec3 {
    // Test single-plane projections and pairwise creases against *all* earlier
    // planes. Repeated projection alone can reintroduce another wall's inward
    // component in a corner. No feasible tangent means the body stops.
    if planes.iter().all(|n| input.dot(*n) >= -1e-6) {
        return input;
    }
    for (i, normal) in planes.iter().enumerate() {
        let candidate = input - *normal * input.dot(*normal).min(0.0);
        if planes.iter().all(|n| candidate.dot(*n) >= -1e-6) {
            return candidate;
        }
        for other in &planes[..i] {
            let crease = normal.cross(*other);
            let length = crease.length_squared();
            if length > 1e-12 {
                let candidate = crease * (input.dot(crease) / length);
                if planes.iter().all(|n| candidate.dot(*n) >= -1e-6) {
                    return candidate;
                }
            }
        }
    }
    Vec3::ZERO
}

#[derive(Clone, Copy)]
enum Solid<'a> {
    Aabb(&'a Aabb),
    Obb(&'a Obb),
    Sphere(&'a Sphere),
    Capsule(&'a Capsule),
}

impl CharacterSolids<'_> {
    fn each(&self, mut visit: impl FnMut(Solid<'_>)) {
        for shape in self.aabbs {
            visit(Solid::Aabb(shape));
        }
        for shape in self.obbs {
            visit(Solid::Obb(shape));
        }
        for shape in self.spheres {
            visit(Solid::Sphere(shape));
        }
        for shape in self.capsules {
            visit(Solid::Capsule(shape));
        }
    }

    fn first_hit(&self, body: Aabb, delta: Vec3) -> Option<SweepHit> {
        let mut first: Option<SweepHit> = None;
        self.each(|solid| {
            let hit = match solid {
                Solid::Aabb(shape) => {
                    // A stationary axis with a gap or mere touch cannot enter
                    // the solid. In particular, reaching the edge of a box
                    // whose top only touches our feet is a tangent, not a wall.
                    // This also avoids interpreting the pair sweep's shallow-touch
                    // tolerance as a fictitious entering face along another
                    // axis after roundoff at a previous wall impact.
                    let separated = (0..3).any(|axis| {
                        delta[axis] == 0.0
                            && (body.max[axis] <= shape.min[axis]
                                || body.min[axis] >= shape.max[axis])
                    });
                    if separated {
                        None
                    } else {
                        body.sweep(delta, shape)
                    }
                }
                Solid::Obb(shape) => body.sweep_obb(delta, shape),
                Solid::Sphere(shape) => body.sweep_sphere(delta, shape),
                Solid::Capsule(shape) => body.sweep_capsule(delta, shape),
            };
            if let Some(hit) = hit {
                if first.is_none_or(|current| {
                    hit.t < current.t
                        || (hit.t == current.t && hit.normal.dot(delta) < current.normal.dot(delta))
                }) {
                    first = Some(hit);
                }
            }
        });
        first
    }

    fn deepest_push(&self, body: Aabb) -> Option<Vec3> {
        let mut push = None;
        let mut deepest = 1e-5;
        self.each(|solid| {
            let contact = match solid {
                Solid::Aabb(shape) => body.contact(shape).map(|c| (c.normal, c.penetration)),
                Solid::Sphere(shape) => shape
                    .contact_aabb(&body)
                    .map(|c| (-c.normal, c.penetration)),
                Solid::Capsule(shape) => shape
                    .contact_aabb(&body)
                    .map(|c| (-c.normal, c.penetration)),
                Solid::Obb(shape) => body.sweep_obb(Vec3::ZERO, shape).and_then(|hit| {
                    let normal = hit.normal.as_dvec3();
                    let axes = shape.pose()?;
                    let radius = shape.half_extents.as_dvec3().dot(super::dvec3(
                        (axes * Vec3::X).as_dvec3().dot(normal).abs(),
                        (axes * Vec3::Y).as_dvec3().dot(normal).abs(),
                        (axes * Vec3::Z).as_dvec3().dot(normal).abs(),
                    ));
                    let center = (body.min.as_dvec3() + body.max.as_dvec3()) * 0.5;
                    let half = (body.max.as_dvec3() - body.min.as_dvec3()) * 0.5;
                    let depth = shape.center.as_dvec3().dot(normal) + radius
                        - (center.dot(normal) - half.dot(normal.abs()));
                    Some((hit.normal, depth as f32))
                }),
            };
            if let Some((normal, depth)) = contact {
                if depth.is_finite() && depth > deepest && normal.is_finite() {
                    deepest = depth;
                    push = Some(normal * depth);
                }
            }
        });
        push
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{vec3, Quat};

    fn body(center: Vec3) -> Aabb {
        Aabb::from_center_size(center, vec3(0.6, 1.8, 0.6))
    }

    fn floor() -> Aabb {
        Aabb::from_center_size(vec3(0.0, -0.5, 0.0), vec3(20.0, 1.0, 20.0))
    }

    fn close(a: Vec3, b: Vec3) {
        assert!((a - b).length() < 2e-4, "{a:?} != {b:?}");
    }

    #[test]
    fn step_exact_height_and_recovered_start_keep_body_dimensions() {
        let boxes = [
            floor(),
            Aabb::from_min_max(vec3(0.5, 0.0, -1.0), vec3(2.0, 0.2, 1.0)),
        ];
        let start = body(vec3(0.0, 0.89, 0.0));
        let stepped = CharacterController::default().move_body_with_step(
            start,
            vec3(1.0, -1.0, 0.0),
            0.6,
            CharacterSolids {
                aabbs: &boxes,
                ..Default::default()
            },
            0.2,
        );
        assert_eq!(
            stepped.step.status,
            CharacterStepStatus::Stepped,
            "{stepped:?}"
        );
        close(stepped.motion.body.size(), start.size());
        assert!((stepped.step.rise - 0.2).abs() < 1e-5);
        let landing = stepped.step.landing.unwrap();
        assert!(landing.point.is_finite());
        assert_eq!(landing.normal, Vec3::Y);
    }

    #[test]
    fn arriving_at_a_box_top_edge_with_stationary_height_is_tangential() {
        let ledge = Aabb::from_min_max(vec3(0.5, 0.0, -1.0), vec3(2.0, 0.2, 1.0));
        let start = Aabb::from_min_max(vec3(-0.3, 0.2, -0.3), vec3(0.3, 2.0, 0.3));
        let moved = CharacterController::default().move_body(
            start,
            Vec3::X,
            1.0,
            CharacterSolids {
                aabbs: &[ledge],
                ..Default::default()
            },
        );
        assert!((moved.body.min.x - 0.7).abs() < 1e-5, "{moved:?}");
        assert!(moved.on_ground);
        assert_eq!(moved.velocity, Vec3::X);
    }

    #[test]
    fn a_step_is_not_a_vault_over_a_thin_barrier_to_the_original_floor() {
        let boxes = [
            floor(),
            Aabb::from_min_max(vec3(0.5, 0.0, -1.0), vec3(0.6, 0.2, 1.0)),
        ];
        let solids = CharacterSolids {
            aabbs: &boxes,
            ..Default::default()
        };
        let controller = CharacterController::default();
        let start = body(vec3(0.0, 0.9, 0.0));
        let ordinary = controller.move_body(start, Vec3::X, 2.0, solids);
        let stepped = controller.move_body_with_step(start, Vec3::X, 2.0, solids, 0.25);
        assert_eq!(
            stepped.step.status,
            CharacterStepStatus::NoProgress,
            "{stepped:?}"
        );
        assert_eq!(stepped.motion, ordinary);
        assert!(stepped.step.landing.is_some());
    }

    #[test]
    fn descending_attachment_bounds_total_drop_from_start_not_extra_snap() {
        let boxes = [
            Aabb::from_min_max(vec3(-3.0, -1.0, -2.0), vec3(0.0, 0.0, 2.0)),
            Aabb::from_min_max(vec3(0.0, -1.0, -2.0), vec3(3.0, -0.3, 2.0)),
        ];
        let solids = CharacterSolids {
            aabbs: &boxes,
            ..Default::default()
        };
        let controller = CharacterController {
            ground_snap_distance: 0.0,
            ..Default::default()
        };
        let start = body(vec3(-0.3, 0.9, 0.0));
        let velocity = vec3(1.0, -0.2, 0.0);
        let ordinary = controller.move_body(start, velocity, 1.0, solids);
        assert!(!ordinary.on_ground, "{ordinary:?}");
        let stepped = controller.move_body_with_step(start, velocity, 1.0, solids, 0.25);
        assert_eq!(stepped.motion, ordinary);
        assert_eq!(
            stepped.step.status,
            CharacterStepStatus::NoLanding,
            "{stepped:?}"
        );
        let attached = controller.move_body_with_step(start, velocity, 1.0, solids, 0.35);
        assert!(attached.motion.on_ground, "{attached:?}");
        assert!((attached.step.rise + 0.3).abs() < 1e-5, "{attached:?}");
    }

    #[test]
    fn invalid_step_settings_return_ordinary_movement_without_a_boost() {
        let boxes = [floor()];
        let solids = CharacterSolids {
            aabbs: &boxes,
            ..Default::default()
        };
        let start = body(vec3(0.0, 0.9, 0.0));
        let controller = CharacterController::default();
        let ordinary = controller.move_body(start, Vec3::X, 1.0, solids);
        for height in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let stepped = controller.move_body_with_step(start, Vec3::X, 1.0, solids, height);
            assert_eq!(stepped.motion, ordinary);
            assert_eq!(stepped.step.status, CharacterStepStatus::InvalidInput);
        }
        let invalid_controller = CharacterController {
            up: Vec3::ZERO,
            ..controller
        };
        let stepped = invalid_controller.move_body_with_step(start, Vec3::X, 1.0, solids, 0.2);
        assert_eq!(stepped.motion.body, start);
        assert_eq!(stepped.step.status, CharacterStepStatus::InvalidInput);
    }

    #[test]
    fn stepping_into_a_wall_keeps_landing_velocity_outside_both_planes() {
        let boxes = [
            floor(),
            Aabb::from_min_max(vec3(0.5, 0.0, -1.0), vec3(2.0, 0.2, 1.0)),
            Aabb::from_min_max(vec3(1.0, 0.0, -2.0), vec3(1.1, 3.0, 2.0)),
        ];
        let stepped = CharacterController::default().move_body_with_step(
            body(vec3(0.0, 0.9, 0.0)),
            vec3(2.0, -1.0, 0.1),
            1.0,
            CharacterSolids {
                aabbs: &boxes,
                ..Default::default()
            },
            0.25,
        );
        assert_eq!(
            stepped.step.status,
            CharacterStepStatus::Stepped,
            "{stepped:?}"
        );
        assert!(stepped.motion.velocity.x.abs() < 1e-5);
        assert!(stepped.motion.velocity.y.abs() < 1e-5);
        assert!((stepped.motion.velocity.z - 0.1).abs() < 1e-5);
        assert!(stepped.motion.body.max.x <= 1.00001);
    }

    #[test]
    fn resting_floor_strafe_preserves_body_and_reports_support() {
        let solids = CharacterSolids {
            aabbs: &[floor()],
            ..Default::default()
        };
        for velocity in [vec3(2.0, 0.0, 1.0), vec3(2.0, -3.0, 1.0)] {
            let start = body(vec3(0.0, 0.9, 0.0));
            let moved = CharacterController::default().move_body(start, velocity, 0.5, solids);
            close(moved.body.center(), vec3(1.0, 0.9, 0.5));
            close(moved.body.size(), start.size());
            close(moved.velocity, vec3(2.0, 0.0, 1.0));
            assert!(moved.on_ground);
            assert_eq!(moved.ground_normal, Some(Vec3::Y));
        }
    }

    #[test]
    fn full_body_stops_at_thin_wall_and_keeps_tangent() {
        let wall = Aabb::from_center_size(vec3(0.0, 1.0, 0.0), vec3(0.01, 4.0, 10.0));
        let solids = CharacterSolids {
            aabbs: &[floor(), wall],
            ..Default::default()
        };
        let moved = CharacterController::default().move_body(
            body(vec3(-3.0, 0.9, 0.0)),
            vec3(100.0, -1.0, 2.0),
            0.1,
            solids,
        );
        close(moved.body.center(), vec3(-0.305, 0.9, 0.2));
        close(moved.velocity, vec3(0.0, 0.0, 2.0));
        assert!(moved.on_ground);
    }

    #[test]
    fn jump_ignores_snap_then_fall_lands_and_ceiling_stops_upward_velocity() {
        let ceiling = Aabb::from_center_size(vec3(0.0, 3.5, 0.0), vec3(10.0, 1.0, 10.0));
        let solids = CharacterSolids {
            aabbs: &[floor(), ceiling],
            ..Default::default()
        };
        let controller = CharacterController::default();
        let jumped = controller.move_body(body(vec3(0.0, 0.9, 0.0)), Vec3::Y, 0.01, solids);
        assert!(!jumped.on_ground);
        close(jumped.body.center(), vec3(0.0, 0.91, 0.0));
        let hit = controller.move_body(jumped.body, Vec3::Y * 30.0, 1.0, solids);
        close(hit.body.center(), vec3(0.0, 2.1, 0.0));
        close(hit.velocity, Vec3::ZERO);
        assert!(!hit.on_ground);
        let landed = controller.move_body(hit.body, -Vec3::Y * 100.0, 1.0, solids);
        close(landed.body.center(), vec3(0.0, 0.9, 0.0));
        assert!(landed.on_ground);
        close(landed.velocity, Vec3::ZERO);
    }

    #[test]
    fn snap_distance_is_bounded_and_can_be_disabled() {
        let solids = CharacterSolids {
            aabbs: &[floor()],
            ..Default::default()
        };
        let controller = CharacterController::default();
        let near = controller.move_body(body(vec3(0.0, 0.95, 0.0)), Vec3::ZERO, 1.0, solids);
        assert!(near.on_ground);
        close(near.body.min, vec3(-0.3, 0.0, -0.3));
        let far = controller.move_body(body(vec3(0.0, 1.1, 0.0)), Vec3::ZERO, 1.0, solids);
        assert!(!far.on_ground);
        let controller = CharacterController {
            ground_snap_distance: 0.0,
            ..controller
        };
        let near = controller.move_body(body(vec3(0.0, 0.95, 0.0)), Vec3::ZERO, 1.0, solids);
        assert!(!near.on_ground);
        let touching = controller.move_body(body(vec3(0.0, 0.9, 0.0)), Vec3::ZERO, 1.0, solids);
        assert!(touching.on_ground);
    }

    #[test]
    fn walking_off_a_ledge_clears_ground_at_final_position() {
        let platform = Aabb::from_min_max(vec3(-2.0, -1.0, -2.0), vec3(0.0, 0.0, 2.0));
        let solids = CharacterSolids {
            aabbs: &[platform],
            ..Default::default()
        };
        let moved = CharacterController::default().move_body(
            body(vec3(-1.0, 0.9, 0.0)),
            vec3(4.0, -1.0, 0.0),
            1.0,
            solids,
        );
        assert!(!moved.on_ground);
        assert_eq!(moved.ground_normal, None);
        assert!(moved.body.min.x > platform.max.x);
    }

    #[test]
    fn non_y_up_and_nonunit_up_detect_support() {
        let wall = Aabb::from_min_max(vec3(-2.0, -2.0, -1.0), vec3(2.0, 2.0, 0.0));
        let solids = CharacterSolids {
            aabbs: &[wall],
            ..Default::default()
        };
        let controller = CharacterController {
            up: Vec3::Z * 7.0,
            ..Default::default()
        };
        let start = Aabb::from_center_size(vec3(0.0, 0.0, 0.5), Vec3::ONE);
        let moved = controller.move_body(start, vec3(1.0, 0.5, -1.0), 0.25, solids);
        close(moved.body.center(), vec3(0.25, 0.125, 0.5));
        assert_eq!(moved.ground_normal, Some(Vec3::Z));
    }

    #[test]
    fn walkable_ramp_climbs_but_steep_ramp_does_not() {
        for (angle, allowed) in [(0.35_f32, true), (1.0, false)] {
            let rotation = Quat::from_rotation_z(angle);
            let ramp = Obb::new(Vec3::ZERO, vec3(6.0, 0.1, 6.0), rotation);
            let normal = rotation * Vec3::Y;
            let half = vec3(0.3, 0.9, 0.3);
            let height = (0.1 + half.dot(normal.abs())) / normal.y;
            let start = body(vec3(0.0, height, 0.0));
            let solids = CharacterSolids {
                obbs: &[ramp],
                ..Default::default()
            };
            let moved = CharacterController::default().move_body(start, Vec3::X, 0.5, solids);
            if allowed {
                assert!(moved.body.center().y > height + 0.05, "{moved:?}");
                assert!(moved.on_ground, "{moved:?}");
                close(moved.ground_normal.unwrap(), normal);
            } else {
                assert!(moved.body.center().y <= height + 1e-4, "{moved:?}");
                assert!(!moved.on_ground);
                assert!(moved.velocity.x.abs() < 1e-4);
            }
        }
    }

    #[test]
    fn falling_on_steep_ramp_slides_down_without_grounding() {
        let ramp = Obb::new(Vec3::ZERO, vec3(6.0, 0.1, 6.0), Quat::from_rotation_z(1.0));
        let solids = CharacterSolids {
            obbs: &[ramp],
            ..Default::default()
        };
        let moved = CharacterController::default().move_body(
            body(vec3(0.0, 3.0, 0.0)),
            -Vec3::Y * 10.0,
            0.3,
            solids,
        );
        assert!(!moved.on_ground);
        assert!(moved.velocity.x < 0.0);
        assert!(moved.velocity.y < 0.0);
    }

    #[test]
    fn rounded_solids_stop_full_box_without_enlarging_their_bounds() {
        let sphere = Sphere::new(Vec3::ZERO, 0.5);
        let capsule = Capsule::new(vec3(0.0, -3.0, 0.0), vec3(0.0, 3.0, 0.0), 0.5);
        for solids in [
            CharacterSolids {
                spheres: &[sphere],
                ..Default::default()
            },
            CharacterSolids {
                capsules: &[capsule],
                ..Default::default()
            },
        ] {
            let moved = CharacterController::default().move_body(
                body(vec3(-3.0, 0.0, 0.0)),
                vec3(6.0, 0.0, 0.0),
                1.0,
                solids,
            );
            close(moved.body.center(), vec3(-0.8, 0.0, 0.0));
            close(moved.velocity, Vec3::ZERO);
        }
    }

    #[test]
    fn initial_overlap_is_recovered_before_moving() {
        let solids = CharacterSolids {
            aabbs: &[floor()],
            ..Default::default()
        };
        let moved = CharacterController::default().move_body(
            body(vec3(0.0, 0.85, 0.0)),
            Vec3::X,
            0.5,
            solids,
        );
        close(moved.body.center(), vec3(0.5, 0.9, 0.0));
        assert!(moved.on_ground);
    }

    #[test]
    fn corner_keeps_all_previous_blocking_planes() {
        let walls = [
            Aabb::from_center_size(vec3(1.0, 1.0, 0.0), vec3(1.0, 5.0, 10.0)),
            Aabb::from_center_size(vec3(0.0, 1.0, 1.0), vec3(10.0, 5.0, 1.0)),
            floor(),
        ];
        let solids = CharacterSolids {
            aabbs: &walls,
            ..Default::default()
        };
        let moved = CharacterController::default().move_body(
            body(vec3(-2.0, 0.9, -2.0)),
            vec3(10.0, -1.0, 10.0),
            1.0,
            solids,
        );
        close(moved.body.center(), vec3(0.2, 0.9, 0.2));
        close(moved.velocity, Vec3::ZERO);
        assert!(moved.on_ground);
    }

    #[test]
    fn invalid_settings_and_inputs_do_not_move() {
        let start = body(Vec3::ZERO);
        let velocity = Vec3::X;
        for controller in [
            CharacterController {
                up: Vec3::ZERO,
                ..Default::default()
            },
            CharacterController {
                up: Vec3::NAN,
                ..Default::default()
            },
            CharacterController {
                max_slope_angle: -1.0,
                ..Default::default()
            },
            CharacterController {
                max_slope_angle: f32::NAN,
                ..Default::default()
            },
            CharacterController {
                ground_snap_distance: -1.0,
                ..Default::default()
            },
            CharacterController {
                max_iterations: 0,
                ..Default::default()
            },
        ] {
            let moved = controller.move_body(start, velocity, 1.0, CharacterSolids::default());
            assert_eq!(moved.body, start);
            assert_eq!(moved.velocity, velocity);
            assert!(!moved.on_ground);
        }
        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let moved = CharacterController::default().move_body(
                start,
                velocity,
                dt,
                CharacterSolids::default(),
            );
            assert_eq!(moved.body, start);
            assert!(!moved.on_ground);
        }
        let invalid = Aabb::from_min_max(Vec3::ONE, Vec3::ZERO);
        let moved = CharacterController::default().move_body(
            invalid,
            velocity,
            1.0,
            CharacterSolids::default(),
        );
        assert_eq!(moved.body, invalid);
    }

    #[test]
    fn excluded_trigger_does_not_block_and_invalid_solids_are_ignored() {
        let trigger = Aabb::from_center_size(Vec3::ZERO, Vec3::splat(10.0));
        let invalid = Aabb::from_min_max(Vec3::ONE, Vec3::ZERO);
        let solids = CharacterSolids {
            aabbs: &[invalid],
            ..Default::default()
        };
        let moved =
            CharacterController::default().move_body(body(Vec3::ZERO), Vec3::X, 1.0, solids);
        close(moved.body.center(), Vec3::X);
        assert!(moved.body.overlaps(&trigger));
    }
}
