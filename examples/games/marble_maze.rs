//! Roll a marble through a Blender maze to the gold ring.
//!
//! ```sh
//! cargo run --features gltf --example marble_maze
//! ```
//!
//! The level is `examples/assets/marble_maze.glb`, exported by
//! `examples/assets/gen_marble_maze.py`. Arrow keys or WASD roll. R restarts.
//! Escape quits.
//!
//! Without the `gltf` feature the example only prints how to enable it.

#[cfg(feature = "gltf")]
use macroquad::prelude::*;

#[cfg(feature = "gltf")]
const MARBLE_RADIUS: f32 = 0.35;

/// Inclusive overlap treats a marble sitting on the floor as already inside it,
/// so a test at the full radius rejects every sideways move. The probe is this
/// much smaller than the drawn marble. `slide_sphere` does not exist yet.
#[cfg(feature = "gltf")]
const SKIN: f32 = 0.02;

#[cfg(feature = "gltf")]
enum Solid {
    /// Box collider. The imported shape is an [`Aabb`] even when the node is
    /// rotated, so this is [`Obb::from_world_matrix`] on the node instead.
    /// That matches the mesh only because the Blender cubes were created at
    /// size 1 (vertices at ±0.5). A default Blender cube is size 2.
    Box(Obb),
    Sphere(Sphere),
}

#[cfg(feature = "gltf")]
struct Level {
    solids: Vec<Solid>,
    goals: Vec<GltfColliderShape>,
    spawn: Vec3,
}

#[cfg(feature = "gltf")]
impl Level {
    fn from_bytes(bytes: &[u8]) -> Result<Level, GltfError> {
        let scene = GltfScene::from_bytes(bytes)?;
        let mut solids = Vec::new();
        let mut goals = Vec::new();
        for collider in &scene.colliders {
            if collider.trigger {
                goals.push(collider.shape);
                continue;
            }
            // `GltfColliderShape` has no overlap helper, and a rotated box is
            // stored as the mesh's world AABB, which fills the bumper's gap.
            solids.push(match collider.shape {
                GltfColliderShape::Sphere(sphere) => Solid::Sphere(sphere),
                GltfColliderShape::Aabb(_) => {
                    Solid::Box(Obb::from_world_matrix(scene.nodes[collider.node].world))
                }
            });
        }
        let spawn = scene
            .spawns
            .first()
            .map(|spawn| spawn.position)
            .unwrap_or(vec3(0.0, 1.0, 0.0));
        Ok(Level {
            solids,
            goals,
            spawn,
        })
    }

    fn reached(&self, center: Vec3) -> bool {
        let marble = Sphere::new(center, MARBLE_RADIUS);
        self.goals.iter().any(|goal| match *goal {
            GltfColliderShape::Sphere(sphere) => marble.overlaps(&sphere),
            GltfColliderShape::Aabb(aabb) => marble.overlaps_aabb(&aabb),
        })
    }
}

#[cfg(feature = "gltf")]
#[cfg_attr(test, allow(dead_code))]
struct Marble {
    center: Vec3,
    velocity: Vec3,
}

#[cfg(feature = "gltf")]
#[cfg_attr(test, allow(dead_code))]
impl Marble {
    fn at(spawn: Vec3) -> Marble {
        Marble {
            center: spawn,
            velocity: Vec3::ZERO,
        }
    }

    fn step(&mut self, solids: &[Solid], dt: f32, wish: Vec3) {
        let substeps = 4;
        let h = dt / substeps as f32;
        for _ in 0..substeps {
            if wish.length_squared() > 0.0 {
                self.velocity += wish.normalize() * 16.0 * h;
            }
            self.velocity.y += -9.8 * h;
            let drag = (1.0 - 3.0 * h).max(0.0);
            self.velocity.x *= drag;
            self.velocity.z *= drag;
            let horiz = vec2(self.velocity.x, self.velocity.z);
            if horiz.length() > 5.0 {
                let scale = 5.0 / horiz.length();
                self.velocity.x *= scale;
                self.velocity.z *= scale;
            }
            for axis in 0..3 {
                let delta = self.velocity[axis] * h;
                let (next, hit) = slide_axis(self.center, delta, axis, solids);
                self.center = next;
                if hit {
                    self.velocity[axis] = 0.0;
                }
            }
        }
    }
}

#[cfg(feature = "gltf")]
fn probe(center: Vec3) -> Sphere {
    Sphere::new(center, (MARBLE_RADIUS - SKIN).max(0.0))
}

#[cfg(feature = "gltf")]
fn blocked(center: Vec3, solids: &[Solid]) -> bool {
    let probe = probe(center);
    solids.iter().any(|solid| match solid {
        Solid::Box(obb) => obb.overlaps_sphere(&probe),
        Solid::Sphere(sphere) => probe.overlaps(sphere),
    })
}

/// Move on one world axis. The step is chopped so a thin floor is not skipped
/// when the destination is already on the other side. A hit drops the rest of
/// that component instead of projecting it onto the surface, so a 45° bumper
/// can snag the marble. There is no sweep query yet.
#[cfg(feature = "gltf")]
fn slide_axis(center: Vec3, delta: f32, axis: usize, solids: &[Solid]) -> (Vec3, bool) {
    if delta == 0.0 || !delta.is_finite() {
        return (center, false);
    }
    let max_piece = (MARBLE_RADIUS * 0.5).max(0.05);
    let pieces = (delta.abs() / max_piece).ceil().max(1.0) as i32;
    let piece = delta / pieces as f32;
    let mut pos = center;
    for _ in 0..pieces {
        let offset = axis_offset(axis, piece);
        if !blocked(pos + offset, solids) {
            pos += offset;
            continue;
        }
        let mut lo = 0.0;
        let mut hi = 1.0;
        for _ in 0..10 {
            let mid = (lo + hi) * 0.5;
            if blocked(pos + offset * mid, solids) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        return (pos + offset * lo, true);
    }
    (pos, false)
}

#[cfg(feature = "gltf")]
fn axis_offset(axis: usize, distance: f32) -> Vec3 {
    match axis {
        0 => vec3(distance, 0.0, 0.0),
        1 => vec3(0.0, distance, 0.0),
        _ => vec3(0.0, 0.0, distance),
    }
}

#[cfg(feature = "gltf")]
#[cfg_attr(test, allow(dead_code))]
fn wish_dir() -> Vec3 {
    let mut wish = Vec3::ZERO;
    if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
        wish.x -= 1.0;
    }
    if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
        wish.x += 1.0;
    }
    if is_key_down(KeyCode::Up) || is_key_down(KeyCode::W) {
        wish.z -= 1.0;
    }
    if is_key_down(KeyCode::Down) || is_key_down(KeyCode::S) {
        wish.z += 1.0;
    }
    wish
}

#[cfg(all(feature = "gltf", not(test)))]
#[macroquad::main("Marble Maze")]
async fn main() {
    let bytes = include_bytes!("../assets/marble_maze.glb");
    // Markers and meshes are parsed separately. Triggers are not drawn.
    let level = Level::from_bytes(bytes).expect("marble maze markers");
    let model = GltfModel::from_bytes(bytes).expect("marble maze mesh");
    let mut marble = Marble::at(level.spawn);
    let mut won = false;
    // `Camera3D::default().up` is +Z. This file is glTF Y-up.
    let mut frames_left = std::env::var("MARBLE_MAZE_FRAMES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok());

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::R) {
            marble = Marble::at(level.spawn);
            won = false;
        }

        let dt = get_frame_time().clamp(0.0, 1.0 / 20.0);
        if !won && dt > 0.0 {
            marble.step(&level.solids, dt, wish_dir());
            if marble.center.y < -4.0 {
                marble = Marble::at(level.spawn);
            }
            if level.reached(marble.center) {
                won = true;
                marble.velocity = Vec3::ZERO;
            }
        }

        clear_background(Color::new(0.53, 0.74, 0.90, 1.0));
        set_camera(&Camera3D {
            position: marble.center + vec3(0.0, 8.0, 9.0),
            target: marble.center + vec3(0.0, 0.0, -1.5),
            up: Vec3::Y,
            fovy: 50.0_f32.to_radians(),
            z_near: 0.05,
            z_far: 200.0,
            ..Default::default()
        });
        model.draw();
        for goal in &level.goals {
            match *goal {
                GltfColliderShape::Sphere(sphere) => {
                    draw_sphere(sphere.center, 0.18, None, GOLD);
                    draw_sphere_wires(sphere.center, sphere.radius, None, YELLOW);
                }
                GltfColliderShape::Aabb(aabb) => {
                    draw_cube_wires(aabb.center(), aabb.size(), YELLOW);
                }
            }
        }
        draw_sphere(
            marble.center,
            MARBLE_RADIUS,
            None,
            if won { GREEN } else { ORANGE },
        );

        set_default_camera();
        draw_text(
            "Roll into the gold ring. Arrows or WASD. R restarts. Esc quits.",
            18.0,
            28.0,
            22.0,
            BLACK,
        );
        if won {
            draw_text("The marble is home.", 18.0, 56.0, 28.0, DARKGREEN);
        }

        if let Some(left) = frames_left.as_mut() {
            if *left == 0 {
                break;
            }
            *left -= 1;
        }
        next_frame().await;
    }
}

#[cfg(all(not(feature = "gltf"), not(test)))]
fn main() {
    eprintln!(
        "marble_maze needs the gltf feature: cargo run --features gltf --example marble_maze"
    );
}

#[cfg(all(test, feature = "gltf"))]
mod tests {
    use super::*;

    fn level() -> Level {
        Level::from_bytes(include_bytes!("../assets/marble_maze.glb")).expect("level")
    }

    #[test]
    fn exported_level_has_a_spawn_outside_the_walls_and_a_rotated_bumper() {
        let level = level();
        assert_eq!(level.solids.len(), 8);
        assert_eq!(level.goals.len(), 1);
        assert!((level.spawn - vec3(0.0, 0.35, 3.0)).length() < 1e-3);
        assert!(
            !blocked(level.spawn, &level.solids),
            "spawn overlaps a solid, so the marble cannot roll"
        );

        let (sideways, hit) = slide_axis(level.spawn, 0.5, 0, &level.solids);
        assert!(!hit);
        assert!((sideways.x - 0.5).abs() < 1e-4);

        let (dropped, landed) = slide_axis(level.spawn, -2.0, 1, &level.solids);
        assert!(landed);
        assert!(dropped.y > 0.3 && dropped.y < level.spawn.y);

        let bumper = level.solids.iter().find(|solid| match solid {
            Solid::Box(obb) => obb.rotation.y.abs() > 0.2,
            Solid::Sphere(_) => false,
        });
        let Some(Solid::Box(bumper)) = bumper else {
            panic!("diagonal bumper was imported as an axis-aligned box");
        };
        assert!((bumper.center - vec3(3.6, 0.5, -1.2)).length() < 1e-3);
        let mut extents = [
            bumper.half_extents.x,
            bumper.half_extents.y,
            bumper.half_extents.z,
        ];
        extents.sort_by(|left, right| left.partial_cmp(right).unwrap());
        assert!((extents[0] - 0.15).abs() < 0.02);
        assert!((extents[1] - 0.5).abs() < 0.02);
        assert!((extents[2] - 0.5).abs() < 0.02);

        assert!(blocked(vec3(4.5, 0.5, 0.0), &level.solids));
        assert!(level.reached(vec3(-2.5, 0.45, -2.8)));
        assert!(!level.reached(level.spawn));
    }
}
