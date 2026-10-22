//! Caller-owned fixed ticks, one anchored pair, one free body and discrete contacts.
//! P/Space pause, R reset, O change pair order, K change anchor, T replay, Esc exit.
#[path = "support/rigid_body_joint_scene.rs"]
mod scene;
use macroquad::prelude::*;
use scene::{Order, Scene};
use std::sync::atomic::{AtomicU32, Ordering};

// Small observable surface for the same native and external WebGL input replay.
static STATUS: AtomicU32 = AtomicU32::new(0);
static FRAMES: AtomicU32 = AtomicU32::new(0);
static TICKS: AtomicU32 = AtomicU32::new(0);
static STATE: AtomicU32 = AtomicU32::new(0);
static CHECKS: AtomicU32 = AtomicU32::new(0);
static RESIDUAL: AtomicU32 = AtomicU32::new(0);
static OVERLAP: AtomicU32 = AtomicU32::new(0);
macro_rules! export_counter {
    ($name:ident, $counter:ident) => {
        #[no_mangle]
        pub extern "C" fn $name() -> u32 {
            $counter.load(Ordering::Relaxed)
        }
    };
}
export_counter!(rigid_body_joint_status, STATUS);
export_counter!(rigid_body_joint_frames, FRAMES);
export_counter!(rigid_body_joint_ticks, TICKS);
export_counter!(rigid_body_joint_state, STATE);
export_counter!(rigid_body_joint_checks, CHECKS);
export_counter!(rigid_body_joint_residual_milli, RESIDUAL);
export_counter!(rigid_body_joint_overlap_milli, OVERLAP);

fn config() -> Conf {
    Conf {
        window_title: "Rigid body and distance joint".into(),
        window_width: 960,
        window_height: 640,
        high_dpi: true,
        ..Default::default()
    }
}

fn draw(scene: &Scene) {
    clear_background(Color::from_rgba(26, 33, 48, 255));
    set_camera(&Camera3D {
        position: vec3(4., 4., 7.),
        target: vec3(-0.3, 0.8, 0.),
        up: Vec3::Y,
        ..Default::default()
    });
    draw_cube(vec3(0., -0.125, 0.), vec3(16., 0.25, 4.), None, DARKGRAY);
    draw_grid(16, 0.5, GRAY, DARKGRAY);
    draw_line_3d(
        scene.bodies.anchor.position,
        scene.bodies.bob.position,
        GOLD,
    );
    for (body, color) in [
        (scene.bodies.free, SKYBLUE),
        (scene.bodies.bob, ORANGE),
        (scene.bodies.anchor, GOLD),
    ] {
        draw_sphere(body.position, scene::RADIUS, None, color);
        draw_sphere_wires(body.position, scene::RADIUS + 0.002, None, BLACK);
    }
    set_default_camera();
    let joint_first = scene::ordering_probe(Order::JointThenContacts);
    let contact_first = scene::ordering_probe(Order::ContactsThenJoint);
    let lines = [
        "Rigid body + one distance joint".to_string(),
        "P/Space pause  R reset  O order  K anchor  T replay".to_string(),
        format!(
            "{}  ticks {}  last contacts {}",
            if scene.paused { "PAUSED" } else { "RUNNING" },
            scene.ticks,
            scene.contacts
        ),
        format!(
            "Order: {}",
            if scene.order == Order::JointThenContacts {
                "joint -> contacts"
            } else {
                "contacts -> joint"
            }
        ),
        format!(
            "Anchor: {}",
            if scene.kinematic {
                "prescribed kinematic (mass 1)"
            } else {
                "fixed (mass 0)"
            }
        ),
        format!(
            "Final distance error {:.4}  floor overlap {:.4}",
            scene.residual(),
            scene.floor_overlap()
        ),
        format!(
            "Probe J>C error {:.2}, overlap {:.2}",
            joint_first.0, joint_first.1
        ),
        format!(
            "Probe C>J error {:.2}, overlap {:.2}",
            contact_first.0, contact_first.1
        ),
        format!(
            "120 Hz, max 12 ticks/frame; replay {} checks",
            CHECKS.load(Ordering::Relaxed)
        ),
    ];
    draw_rectangle(
        0.,
        0.,
        screen_width(),
        190.,
        Color::from_rgba(10, 15, 24, 230),
    );
    let font = if screen_width() < 600. { 15. } else { 18. };
    for (n, line) in lines.iter().enumerate() {
        draw_text(line, 10., 22. + n as f32 * 20., font, WHITE);
    }
    if let Some(error) = scene.error {
        draw_text(
            format!("Paused on {error:?}; R resets"),
            10.,
            212.,
            18.,
            RED,
        );
    }
}

#[macroquad::main(config)]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        STATUS.store(2, Ordering::Relaxed);
        macroquad::logging::error!("RIGID_BODY_JOINT_FAIL: {}", info);
    }));
    let mut scene = Scene::new();
    let mut checks = scene::replay();
    let mut discard_replay_time = false;
    CHECKS.store(checks, Ordering::Relaxed);
    macroquad::logging::info!("RIGID_BODY_JOINT_REPLAY_PASS checks={}", checks);
    loop {
        let skip_elapsed = std::mem::take(&mut discard_replay_time);
        let mut transition = false;
        if is_key_pressed(KeyCode::P) || is_key_pressed(KeyCode::Space) {
            scene.set_paused(!scene.paused);
            transition = true;
        }
        if is_key_pressed(KeyCode::R) {
            scene.reset();
            transition = true;
        }
        if is_key_pressed(KeyCode::O) {
            scene.toggle_order();
            transition = true;
        }
        if is_key_pressed(KeyCode::K) {
            scene.toggle_anchor();
            transition = true;
        }
        if is_key_pressed(KeyCode::T) {
            checks += scene::replay();
            CHECKS.store(checks, Ordering::Relaxed);
            macroquad::logging::info!("RIGID_BODY_JOINT_REPLAY_PASS checks={}", checks);
            transition = true;
            discard_replay_time = true;
        }
        // Elapsed time from the previous state, and the replay itself, is discarded.
        let _ = scene.advance(if transition || skip_elapsed {
            0.
        } else {
            get_frame_time() as f64
        });
        let state = u32::from(scene.paused)
            | (u32::from(scene.order == Order::ContactsThenJoint) << 1)
            | (u32::from(scene.kinematic) << 2)
            | (u32::from(scene.finite()) << 3)
            | (u32::from(scene.error.is_some()) << 4);
        STATE.store(state, Ordering::Relaxed);
        TICKS.store(scene.ticks, Ordering::Relaxed);
        RESIDUAL.store((scene.residual() * 1000.).round() as u32, Ordering::Relaxed);
        OVERLAP.store(
            (scene.floor_overlap() * 1000.).round() as u32,
            Ordering::Relaxed,
        );
        STATUS.store(
            if scene.error.is_none() && scene.finite() {
                1
            } else {
                2
            },
            Ordering::Relaxed,
        );
        let frame = FRAMES.fetch_add(1, Ordering::Relaxed);
        #[cfg(not(target_arch = "wasm32"))]
        if transition || frame.is_multiple_of(120) {
            println!("RIGID_BODY_JOINT state={state} ticks={} checks={checks} residual={:.5} overlap={:.5}", scene.ticks, scene.residual(), scene.floor_overlap());
        }
        #[cfg(target_arch = "wasm32")]
        let _ = frame;
        draw(&scene);
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
