//! Move a marker and watch a ray and a segment meet two planes.
//!
//! The ground is `Plane::new(Vec3::Y, 0.0)`, the same horizontal slice
//! `draw_plane` fills. The tilted plane comes from `Plane::from_points`.
//! The marker's color is `Plane::side` against the ground.

use macroquad::prelude::*;

fn draw_tilted(plane: Plane, color: Color) {
    let center = plane.normal * plane.distance;
    let helper = if plane.normal.cross(Vec3::Y).length_squared() > 1e-8 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let tangent = plane.normal.cross(helper).normalize();
    let bitangent = plane.normal.cross(tangent).normalize();
    draw_affine_parallelogram(
        center - tangent * 2.5 - bitangent * 2.5,
        tangent * 5.0,
        bitangent * 5.0,
        None,
        color,
    );
    draw_line_3d(center, center + plane.normal, color);
}

#[macroquad::main("Plane hit")]
async fn main() {
    let ground = Plane::new(Vec3::Y, 0.0);
    let tilted = Plane::from_points(
        vec3(-2.0, 0.0, 1.0),
        vec3(2.0, 0.0, 1.0),
        vec3(0.0, 2.0, -1.0),
    )
    .unwrap();
    let mut marker = vec3(0.0, 1.6, 2.4);

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        let dt = get_frame_time();
        let mut step = Vec3::ZERO;
        if is_key_down(KeyCode::Left) {
            step.x -= 1.0;
        }
        if is_key_down(KeyCode::Right) {
            step.x += 1.0;
        }
        if is_key_down(KeyCode::Up) {
            step.z -= 1.0;
        }
        if is_key_down(KeyCode::Down) {
            step.z += 1.0;
        }
        if is_key_down(KeyCode::Q) {
            step.y += 1.0;
        }
        if is_key_down(KeyCode::E) {
            step.y -= 1.0;
        }
        if step.length_squared() > 0.0 {
            marker += step.normalize() * 2.5 * dt;
        }

        let ray = Ray::new(marker, vec3(0.0, -1.0, -0.45));
        let ground_hit = ground.ray_intersect(ray.origin, ray.direction);
        let tilted_hit = tilted.ray_intersect(ray.origin, ray.direction);
        let closest = match (ground_hit, tilted_hit) {
            (Some(ground_hit), Some(tilted_hit)) if ground_hit.t <= tilted_hit.t => {
                Some(ground_hit)
            }
            (Some(_), Some(tilted_hit)) => Some(tilted_hit),
            (Some(ground_hit), None) => Some(ground_hit),
            (None, Some(tilted_hit)) => Some(tilted_hit),
            (None, None) => None,
        };

        let segment_end = marker + vec3(0.0, -3.2, -1.2);
        let ground_cross = ground.segment_intersect(marker, segment_end);
        let tilted_cross = tilted.segment_intersect(marker, segment_end);
        let crossing = match (ground_cross, tilted_cross) {
            (Some(ground_point), Some(tilted_point)) => {
                if ground_point.distance_squared(marker) <= tilted_point.distance_squared(marker) {
                    Some(ground_point)
                } else {
                    Some(tilted_point)
                }
            }
            (Some(point), None) | (None, Some(point)) => Some(point),
            (None, None) => None,
        };

        clear_background(LIGHTGRAY);

        set_camera(&Camera3D {
            position: vec3(8.0, 8.0, 8.0),
            up: vec3(0.0, 1.0, 0.0),
            target: vec3(0.0, 0.5, 0.0),
            ..Default::default()
        });

        draw_grid(20, 1., BLACK, GRAY);
        draw_plane(
            Vec3::ZERO,
            vec2(2.5, 2.5),
            None,
            Color::new(0.2, 0.55, 0.3, 0.45),
        );
        draw_tilted(tilted, Color::new(0.2, 0.35, 0.75, 0.45));

        let marker_color = match ground.side(marker) {
            PlaneSide::Front => BLUE,
            PlaneSide::Back => RED,
            PlaneSide::Coplanar => GREEN,
        };
        draw_sphere(marker, 0.16, None, marker_color);
        draw_line_3d(marker, segment_end, DARKGRAY);
        if let Some(point) = crossing {
            draw_sphere(point, 0.12, None, ORANGE);
        }

        let ray_end = if let Some(hit) = closest {
            draw_sphere(hit.point, 0.12, None, YELLOW);
            draw_line_3d(hit.point, hit.point + hit.normal * 0.75, YELLOW);
            hit.point
        } else {
            ray.point_at(8.0)
        };
        draw_line_3d(ray.origin, ray_end, YELLOW);

        set_default_camera();
        let side = match ground.side(marker) {
            PlaneSide::Front => "above",
            PlaneSide::Back => "below",
            PlaneSide::Coplanar => "on",
        };
        draw_text(
            "Arrows move the marker. Q/E raise and lower it.",
            16.0,
            28.0,
            22.0,
            BLACK,
        );
        draw_text(
            format!("The marker is {side} the ground. Yellow is the ray hit, orange the segment."),
            16.0,
            54.0,
            22.0,
            DARKGRAY,
        );

        next_frame().await
    }
}
