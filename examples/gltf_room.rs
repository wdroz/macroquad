//! `cargo run --features gltf --example gltf_room` (no Blender needed).
#[cfg(feature = "gltf")]
#[path = "support/room_scene.rs"]
mod room_scene;
#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
#[macroquad::main("Authored glTF room")]
async fn main() {
    let room = room_scene::Room::load();
    let model = RetainedGltfModel::from_model(
        GltfModel::from_bytes(room_scene::BYTES).expect("room draw"),
        false,
    )
    .expect("room upload");
    let mut center = room.spawn;
    println!(
        "ROOM_READY nodes={} solids={} triggers=1 spawns=1",
        room.scene.nodes.len(),
        room.solids.len()
    );
    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::R) {
            center = room.spawn;
        }
        let mut wish = Vec3::ZERO;
        if is_key_down(KeyCode::A) {
            wish.x -= 1.;
        }
        if is_key_down(KeyCode::D) {
            wish.x += 1.;
        }
        if is_key_down(KeyCode::W) {
            wish.z -= 1.;
        }
        if is_key_down(KeyCode::S) {
            wish.z += 1.;
        }
        center = room.step(
            center,
            wish.normalize_or_zero() * 3. * get_frame_time().min(0.05),
        );
        clear_background(DARKGRAY);
        set_camera(&Camera3D {
            position: vec3(10., 11., 13.),
            target: Vec3::ZERO,
            up: Vec3::Y,
            ..Default::default()
        });
        model.draw(Mat4::IDENTITY);
        draw_sphere(center, room_scene::RADIUS, None, YELLOW);
        draw_sphere_wires(room.goal.center, room.goal.radius, None, GREEN);
        set_default_camera();
        draw_text("WASD move | R reset | Escape quit", 12., 26., 22., WHITE);
        if room.reached(center) {
            draw_text("Goal reached!", 12., 52., 22., GREEN);
        }
        next_frame().await;
    }
}

#[cfg(not(feature = "gltf"))]
fn main() {
    eprintln!("Run with --features gltf");
}
