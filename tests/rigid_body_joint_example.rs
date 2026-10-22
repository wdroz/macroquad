#[path = "../examples/support/rigid_body_joint_scene.rs"]
mod scene;
use macroquad::prelude::*;
use scene::*;

#[test]
fn deterministic_native_webgl_replay_is_headless() {
    assert_eq!(replay(), 1482);
}

#[test]
fn fixed_clock_bounds_catchup_and_preserves_fractional_ticks() {
    let mut scene = Scene::new();
    assert_eq!(scene.advance(STEP * 0.4), Ok(0));
    assert_eq!(scene.advance(STEP * 0.6), Ok(1));
    assert_eq!(scene.advance(60.), Ok(MAX_STEPS));
    assert!(scene.accumulator >= 0. && scene.accumulator < STEP);
    assert_eq!(scene.ticks, 13);
}

#[test]
fn pause_resume_and_all_resets_clear_carried_time() {
    let mut scene = Scene::new();
    scene.advance(STEP * 0.75).unwrap();
    scene.set_paused(true);
    let paused = scene;
    assert_eq!(scene.advance(90.), Ok(0));
    assert_eq!(scene, paused);
    scene.set_paused(false);
    assert_eq!(scene.advance(STEP * 0.25), Ok(0));
    scene.toggle_order();
    assert_eq!(scene.order, Order::ContactsThenJoint);
    assert_eq!(scene.accumulator, 0.);
    scene.advance(STEP * 0.75).unwrap();
    scene.toggle_anchor();
    assert!(scene.kinematic);
    assert_eq!(scene.bodies.anchor.mass, 1.);
    assert_eq!(scene.accumulator, 0.);
    scene.set_paused(true);
    scene.reset();
    assert!(scene.paused && scene.kinematic);
    assert_eq!(scene.order, Order::ContactsThenJoint);
    assert_eq!(scene.ticks, 0);
    assert_eq!(scene.error, None);
}

#[test]
fn ordering_probe_exposes_residual_vs_overlap_tradeoff() {
    let a = ordering_probe(Order::JointThenContacts);
    let b = ordering_probe(Order::ContactsThenJoint);
    assert!((a.0 - 0.2).abs() < 1e-5 && a.1 == 0.);
    assert!((b.0 - 0.04).abs() < 1e-5 && (b.1 - 0.16).abs() < 1e-5);
}

#[test]
fn finite_diagnostic_covers_velocities_and_settings_as_well_as_positions() {
    let mut scene = Scene::new();
    scene.bodies.free.linear_velocity.x = f32::NAN;
    assert!(!scene.finite());
    scene.reset();
    scene.bodies.anchor.mass = f32::INFINITY;
    assert!(!scene.finite());
    scene.reset();
    scene.joint.velocity_correction = f32::NAN;
    assert!(!scene.finite());
}

#[test]
fn invalid_frame_pauses_without_touching_physics_and_reset_recovers() {
    for delta in [-1., f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut scene = Scene::new();
        scene.advance(STEP * 0.75).unwrap();
        let bodies = scene.bodies;
        assert_eq!(scene.advance(delta), Err(StepError::FrameTime));
        assert_eq!(scene.bodies, bodies);
        assert_eq!(scene.ticks, 0);
        assert!(scene.paused);
        assert_eq!(scene.accumulator, 0.);
        scene.set_paused(false);
        assert!(scene.paused);
        scene.reset();
        scene.set_paused(false);
        assert_eq!(scene.advance(STEP), Ok(1));
    }
}

#[test]
fn late_joint_failure_rolls_back_completed_integration_and_contacts() {
    let mut scene = Scene::new();
    scene.order = Order::ContactsThenJoint;
    scene.joint.position_correction = 2.;
    let before = scene.bodies;
    assert_eq!(
        scene.advance(STEP),
        Err(StepError::Joint(DistanceJointError::InvalidCorrection))
    );
    assert_eq!(scene.bodies, before);
    assert_eq!(scene.ticks, 0);
    assert!(scene.paused);
    assert_eq!(scene.accumulator, 0.);
    scene.reset();
    scene.set_paused(false);
    assert_eq!(scene.advance(STEP), Ok(1));
}

#[test]
fn body_and_contact_failures_commit_no_partial_tick() {
    let mut scene = Scene::new();
    scene.bodies.free.mass = -1.;
    let before = scene.bodies;
    assert_eq!(
        scene.advance(STEP),
        Err(StepError::Integrate(RigidBodyError::InvalidMass))
    );
    assert_eq!(scene.bodies, before);
    scene.reset();
    scene.set_paused(false);
    scene.bodies.floor.mass = -1.;
    let before = scene.bodies;
    assert_eq!(
        scene.advance(STEP),
        Err(StepError::Contact(CollisionResponseError::Body(
            RigidBodyError::InvalidMass
        )))
    );
    assert_eq!(scene.bodies, before);
    assert_eq!(scene.ticks, 0);
    assert!(scene.paused);
}
