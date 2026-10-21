//! 2D shapes rendering.

use crate::{color::Color, get_context};

use crate::quad_gl::{DrawMode, Vertex};
use glam::{vec2, vec3, vec4, Mat4, Vec2};

/// Draws a solid triangle between points `v1`, `v2`, and `v3` with a given `color`.
pub fn draw_triangle(v1: Vec2, v2: Vec2, v3: Vec2, color: Color) {
    let context = get_context();

    let vertices = [
        Vertex::new(v1.x, v1.y, 0., 0., 0., color),
        Vertex::new(v2.x, v2.y, 0., 0., 0., color),
        Vertex::new(v3.x, v3.y, 0., 0., 0., color),
    ];

    let indices: [u16; 3] = [0, 1, 2];

    context.gl.texture(None);
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(&vertices, &indices);
}

/// Draws a triangle outline between points `v1`, `v2`, and `v3` with a given line `thickness` and `color`.
pub fn draw_triangle_lines(v1: Vec2, v2: Vec2, v3: Vec2, thickness: f32, color: Color) {
    draw_line(v1.x, v1.y, v2.x, v2.y, thickness, color);
    draw_line(v2.x, v2.y, v3.x, v3.y, thickness, color);
    draw_line(v3.x, v3.y, v1.x, v1.y, thickness, color);
}

/// Draws a solid rectangle with its top-left corner at `[x, y]` with size `[w, h]` (width going to
/// the right, height going down), with a given `color`.
pub fn draw_rectangle(x: f32, y: f32, w: f32, h: f32, color: Color) {
    let context = get_context();

    #[rustfmt::skip]
    let vertices = [
        Vertex::new(x    , y    , 0., 0.0, 0.0, color),
        Vertex::new(x + w, y    , 0., 1.0, 0.0, color),
        Vertex::new(x + w, y + h, 0., 1.0, 1.0, color),
        Vertex::new(x    , y + h, 0., 0.0, 1.0, color),
    ];
    let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

    context.gl.texture(None);
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(&vertices, &indices);
}

/// Draws a filled rounded rectangle with top-left corner `[x, y]` and size `[w, h]`.
///
/// Like [`draw_rectangle`], coordinates use the current camera's space: logical
/// window pixels with the default camera, or world coordinates with a custom
/// camera. This helper does not change the camera, material or clip state.
///
/// A finite `radius <= 0` produces square corners. Positive radii are clamped to
/// half the smaller dimension. Non-finite coordinates, dimensions or radius,
/// non-positive dimensions, and bounds that overflow, collapse or invert the
/// tessellation at `f32` precision draw nothing. Invalid calls do not change renderer state.
///
/// Each quarter-circle uses eight line segments. A single triangle fan
/// fills the shape without overlapping triangle interiors, so translucent fills
/// have uniform coverage instead of darker seams. As with the other shape
/// helpers, edge antialiasing depends on the active framebuffer's multisampling.
/// This is a fill only; it does not style the built-in UI widgets.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # async fn example() {
/// draw_rectangle_rounded(20., 20., 240., 100., 12., Color::new(0.1, 0.2, 0.4, 0.5));
/// # }
/// ```
pub fn draw_rectangle_rounded(x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
    let Some((vertices, indices)) = rounded_rectangle_geometry(x, y, w, h, radius, color) else {
        return;
    };
    let context = get_context();
    context.gl.texture(None);
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(&vertices, &indices);
}

// The shared perimeter vertices make every internal edge exact on both sides.
// Cardinal endpoints are explicit rather than independently evaluated trig
// expressions, including when clamping creates a capsule or a circle.
fn rounded_rectangle_geometry(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    color: Color,
) -> Option<([Vertex; 37], [u16; 108])> {
    if ![x, y, w, h, radius].iter().all(|v| v.is_finite()) || w <= 0. || h <= 0. {
        return None;
    }
    let right = x + w;
    let bottom = y + h;
    if !right.is_finite() || !bottom.is_finite() || right <= x || bottom <= y {
        return None;
    }
    let radius = radius.max(0.).min(w * 0.5).min(h * 0.5);
    let center = vec2(x + w * 0.5, y + h * 0.5);
    let mut vertices = [Vertex::new(center.x, center.y, 0., 0.5, 0.5, color); 37];
    let mut indices = [0; 108];
    for (corner, (cx, cy, sx, sy)) in [
        (right - radius, y + radius, 1., -1.),
        (right - radius, bottom - radius, 1., 1.),
        (x + radius, bottom - radius, -1., 1.),
        (x + radius, y + radius, -1., -1.),
    ]
    .into_iter()
    .enumerate()
    {
        for step in 0..=8 {
            // Walk clockwise in screen coordinates, starting on the top edge.
            let angle_step = if corner % 2 == 0 { step } else { 8 - step };
            let (sin, cos) = match angle_step {
                0 => (0., 1.),
                8 => (1., 0.),
                _ => (angle_step as f32 * std::f32::consts::FRAC_PI_2 / 8.).sin_cos(),
            };
            let px = if angle_step == 8 {
                if sx > 0. {
                    right
                } else {
                    x
                }
            } else {
                cx + sx * radius * sin
            };
            let py = if angle_step == 0 {
                if sy > 0. {
                    bottom
                } else {
                    y
                }
            } else {
                cy + sy * radius * cos
            };
            let index = corner * 9 + step;
            vertices[index + 1] = Vertex::new(px, py, 0., (px - x) / w, (py - y) / h, color);
            indices[index * 3..index * 3 + 3].copy_from_slice(&[
                0,
                (index + 1) as u16,
                ((index + 1) % 36 + 1) as u16,
            ]);
        }
    }
    // Very large origins with small dimensions can round corner centers past
    // each other. Reject inverted fan sectors instead of blending overlapping
    // triangles. f64 arithmetic also keeps this validation safe near f32::MAX.
    for edge in 0..36 {
        let a = vertices[edge + 1].position.as_dvec3() - vertices[0].position.as_dvec3();
        let b = vertices[(edge + 1) % 36 + 1].position.as_dvec3() - vertices[0].position.as_dvec3();
        if a.x * b.y - a.y * b.x < 0. {
            return None;
        }
    }
    Some((vertices, indices))
}

/// Draws a rectangle outline with its top-left corner at `[x, y]` with size `[w, h]` (width going to
/// the right, height going down), with a given line `thickness` and `color`.
pub fn draw_rectangle_lines(x: f32, y: f32, w: f32, h: f32, thickness: f32, color: Color) {
    let context = get_context();
    let t = thickness / 2.;

    #[rustfmt::skip]
    let vertices = [
        Vertex::new(x    , y    , 0., 0.0, 1.0, color),
        Vertex::new(x + w, y    , 0., 1.0, 0.0, color),
        Vertex::new(x + w, y + h, 0., 1.0, 1.0, color),
        Vertex::new(x    , y + h, 0., 0.0, 0.0, color),
        //inner rectangle
        Vertex::new(x + t    , y + t    , 0., 0.0, 0.0, color),
        Vertex::new(x + w - t, y + t    , 0., 0.0, 0.0, color),
        Vertex::new(x + w - t, y + h - t, 0., 0.0, 0.0, color),
        Vertex::new(x + t    , y + h - t, 0., 0.0, 0.0, color),
    ];
    let indices: [u16; 24] = [
        0, 1, 4, 1, 4, 5, 1, 5, 6, 1, 2, 6, 3, 7, 2, 2, 7, 6, 0, 4, 3, 3, 4, 7,
    ];

    context.gl.texture(None);
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(&vertices, &indices);
}

pub fn draw_rectangle_lines_ex(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    thickness: f32,
    params: DrawRectangleParams,
) {
    let context = get_context();
    let tx = thickness / w;
    let ty = thickness / h;

    let transform_matrix = Mat4::from_translation(vec3(x, y, 0.0))
        * Mat4::from_axis_angle(vec3(0.0, 0.0, 1.0), params.rotation)
        * Mat4::from_scale(vec3(w, h, 1.0));

    #[rustfmt::skip]
    let v = [
        transform_matrix * vec4( 0.0 - params.offset.x,  0.0 - params.offset.y, 0.0, 1.0),
        transform_matrix * vec4( 0.0 - params.offset.x,  1.0 - params.offset.y, 0.0, 1.0),
        transform_matrix * vec4( 1.0 - params.offset.x,  1.0 - params.offset.y, 0.0, 1.0),
        transform_matrix * vec4( 1.0 - params.offset.x,  0.0 - params.offset.y, 0.0, 1.0),

        transform_matrix * vec4( 0.0 - params.offset.x + tx,  0.0 - params.offset.y + ty, 0.0, 1.0),
        transform_matrix * vec4( 0.0 - params.offset.x + tx,  1.0 - params.offset.y - ty, 0.0, 1.0),
        transform_matrix * vec4( 1.0 - params.offset.x - tx,  1.0 - params.offset.y - ty, 0.0, 1.0),
        transform_matrix * vec4( 1.0 - params.offset.x - tx,  0.0 - params.offset.y + ty, 0.0, 1.0),
    ];

    // TODO: fix UVs
    #[rustfmt::skip]
    let vertices = [
        Vertex::new(v[0].x, v[0].y, v[0].z, 0.0, 1.0, params.color),
        Vertex::new(v[1].x, v[1].y, v[1].z, 1.0, 0.0, params.color),
        Vertex::new(v[2].x, v[2].y, v[2].z, 1.0, 1.0, params.color),
        Vertex::new(v[3].x, v[3].y, v[3].z, 1.0, 0.0, params.color),

        Vertex::new(v[4].x, v[4].y, v[4].z, 0.0, 0.0, params.color),
        Vertex::new(v[5].x, v[5].y, v[5].z, 0.0, 0.0, params.color),
        Vertex::new(v[6].x, v[6].y, v[6].z, 0.0, 0.0, params.color),
        Vertex::new(v[7].x, v[7].y, v[7].z, 0.0, 0.0, params.color),
    ];
    #[rustfmt::skip]
    let indices: [u16; 24] = [
        0, 4, 3,
        4, 3, 7,
        4, 0, 1,
        4, 5, 1,
        1, 5, 6,
        1, 6, 2,
        2, 3, 6,
        3, 6, 7,
    ];

    context.gl.texture(None);
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(&vertices, &indices);
}

#[derive(Debug, Clone)]
pub struct DrawRectangleParams {
    /// Adds an offset to the position
    /// E.g. offset (0,0) positions the rectangle at the top left corner of the screen, while offset
    /// (0.5, 0.5) centers it
    pub offset: Vec2,

    /// Rotation in radians
    pub rotation: f32,

    pub color: Color,
}

impl Default for DrawRectangleParams {
    fn default() -> Self {
        Self {
            offset: vec2(0.0, 0.0),
            rotation: 0.0,
            color: Color::from_rgba(255, 255, 255, 255),
        }
    }
}

/// Draws a solid rectangle with its position at `[x, y]` with size `[w, h]`,
/// with parameters.
pub fn draw_rectangle_ex(x: f32, y: f32, w: f32, h: f32, params: DrawRectangleParams) {
    let context = get_context();
    let transform_matrix = Mat4::from_translation(vec3(x, y, 0.0))
        * Mat4::from_axis_angle(vec3(0.0, 0.0, 1.0), params.rotation)
        * Mat4::from_scale(vec3(w, h, 1.0));

    #[rustfmt::skip]
    let v = [
        transform_matrix * vec4( 0.0 - params.offset.x,  0.0 - params.offset.y, 0.0, 1.0),
        transform_matrix * vec4( 0.0 - params.offset.x,  1.0 - params.offset.y, 0.0, 1.0),
        transform_matrix * vec4( 1.0 - params.offset.x,  1.0 - params.offset.y, 0.0, 1.0),
        transform_matrix * vec4( 1.0 - params.offset.x,  0.0 - params.offset.y, 0.0, 1.0),
    ];

    #[rustfmt::skip]
    let vertices = [
        Vertex::new(v[0].x, v[0].y, v[0].z, 0.0, 0.0, params.color),
        Vertex::new(v[1].x, v[1].y, v[1].z, 1.0, 0.0, params.color),
        Vertex::new(v[2].x, v[2].y, v[2].z, 1.0, 1.0, params.color),
        Vertex::new(v[3].x, v[3].y, v[3].z, 0.0, 1.0, params.color),
    ];
    let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

    context.gl.texture(None);
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(&vertices, &indices);
}

/// Draws an outlined solid hexagon centered at `[x, y]` with a radius `size`, outline thickness
/// defined by `border`, orientation defined by `vertical` (when `true`, the hexagon points along
/// the `y` axis), and colors for outline given by `border_color` and fill by `fill_color`.
pub fn draw_hexagon(
    x: f32,
    y: f32,
    size: f32,
    border: f32,
    vertical: bool,
    border_color: Color,
    fill_color: Color,
) {
    let rotation = if vertical { 90. } else { 0. };
    draw_poly(x, y, 6, size, rotation, fill_color);
    if border > 0. {
        draw_poly_lines(x, y, 6, size, rotation, border, border_color);
    }
}

/// Draws a solid regular polygon centered at `[x, y]` with a given number of `sides`, `radius`,
/// clockwise `rotation` (in degrees) and `color`.
pub fn draw_poly(x: f32, y: f32, sides: u8, radius: f32, rotation: f32, color: Color) {
    let context = get_context();

    let mut vertices = Vec::<Vertex>::with_capacity(sides as usize + 2);
    let mut indices = Vec::<u16>::with_capacity(sides as usize * 3);

    let rot = rotation.to_radians();
    vertices.push(Vertex::new(x, y, 0., 0., 0., color));
    for i in 0..=sides {
        let rx = (i as f32 / sides as f32 * std::f32::consts::PI * 2. + rot).cos();
        let ry = (i as f32 / sides as f32 * std::f32::consts::PI * 2. + rot).sin();

        let vertex = Vertex::new(x + radius * rx, y + radius * ry, 0., rx, ry, color);

        vertices.push(vertex);

        if i != sides {
            indices.extend_from_slice(&[0, i as u16 + 1, i as u16 + 2]);
        }
    }

    context.gl.texture(None);
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(&vertices, &indices);
}

/// Draws a regular polygon outline centered at `[x, y]` with a given number of `sides`, `radius`,
/// clockwise `rotation` (in degrees), line `thickness`, and `color`.
pub fn draw_poly_lines(
    x: f32,
    y: f32,
    sides: u8,
    radius: f32,
    rotation: f32,
    thickness: f32,
    color: Color,
) {
    draw_arc(x, y, sides, radius, rotation, thickness, 360.0, color);
}

/// Draws a solid circle centered at `[x, y]` with a given radius `r` and `color`.
///
/// This is not a perfect circle, but only a polygon approximation.
/// If this is an issue for you, consider using `draw_poly(x, y, 255, r, 0., color)` instead.
pub fn draw_circle(x: f32, y: f32, r: f32, color: Color) {
    draw_poly(x, y, 20, r, 0., color);
}

/// Draws a circle outline centered at `[x, y]` with a given radius, line `thickness` and `color`.
///
/// This is not a perfect circle, but only a polygon approximation.
/// If this is an issue for you, consider using `draw_poly_lines(x, y, 255, r, 0., thickness, color)` instead.
pub fn draw_circle_lines(x: f32, y: f32, r: f32, thickness: f32, color: Color) {
    draw_poly_lines(x, y, 30, r, 0., thickness, color);
}

/// Draws a solid ellipse centered at `[x, y]` with a given size `[w, h]`,
/// clockwise `rotation` (in degrees) and `color`.
pub fn draw_ellipse(x: f32, y: f32, w: f32, h: f32, rotation: f32, color: Color) {
    let sides = 20;
    let context = get_context();

    let mut vertices = Vec::<Vertex>::with_capacity(sides as usize + 2);
    let mut indices = Vec::<u16>::with_capacity(sides as usize * 3);

    let rot = rotation.to_radians();
    let sr = rot.sin();
    let cr = rot.cos();
    vertices.push(Vertex::new(x, y, 0., 0., 0., color));
    for i in 0..=sides {
        let rx = (i as f32 / sides as f32 * std::f32::consts::PI * 2.).cos();
        let ry = (i as f32 / sides as f32 * std::f32::consts::PI * 2.).sin();

        let px = w * rx;
        let py = h * ry;
        let rotated_x = px * cr - py * sr;
        let rotated_y = py * cr + px * sr;
        let vertex = Vertex::new(x + rotated_x, y + rotated_y, 0., rx, ry, color);

        vertices.push(vertex);

        if i != sides {
            indices.extend_from_slice(&[0, i as u16 + 1, i as u16 + 2]);
        }
    }

    context.gl.texture(None);
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(&vertices, &indices);
}

/// Draws an ellipse outline centered at `[x, y]` with a given size `[w, h]`,
/// clockwise `rotation` (in degrees), line `thickness` and `color`.
pub fn draw_ellipse_lines(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rotation: f32,
    thickness: f32,
    color: Color,
) {
    let sides = 20;

    let rot = rotation.to_radians();
    let sr = rot.sin();
    let cr = rot.cos();
    for i in 0..sides {
        let rx = (i as f32 / sides as f32 * std::f32::consts::PI * 2.).cos();
        let ry = (i as f32 / sides as f32 * std::f32::consts::PI * 2.).sin();
        let px = w * rx;
        let py = h * ry;
        let rotated_x = px * cr - py * sr;
        let rotated_y = py * cr + px * sr;

        let p0 = vec2(x + rotated_x, y + rotated_y);

        let rx = ((i + 1) as f32 / sides as f32 * std::f32::consts::PI * 2.).cos();
        let ry = ((i + 1) as f32 / sides as f32 * std::f32::consts::PI * 2.).sin();
        let px = w * rx;
        let py = h * ry;
        let rotated_x = px * cr - py * sr;
        let rotated_y = py * cr + px * sr;

        let p1 = vec2(x + rotated_x, y + rotated_y);

        draw_line(p0.x, p0.y, p1.x, p1.y, thickness, color);
    }
}

/// Draws a line between points `[x1, y1]` and `[x2, y2]` with a given `thickness` and `color`.
pub fn draw_line(x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: Color) {
    let context = get_context();
    let dx = x2 - x1;
    let dy = y2 - y1;

    // https://stackoverflow.com/questions/1243614/how-do-i-calculate-the-normal-vector-of-a-line-segment

    let nx = -dy;
    let ny = dx;

    let tlen = (nx * nx + ny * ny).sqrt() / (thickness * 0.5);
    if tlen < f32::EPSILON {
        return;
    }
    let tx = nx / tlen;
    let ty = ny / tlen;

    context.gl.texture(None);
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(
        &[
            Vertex::new(x1 + tx, y1 + ty, 0., 0., 0., color),
            Vertex::new(x1 - tx, y1 - ty, 0., 0., 0., color),
            Vertex::new(x2 + tx, y2 + ty, 0., 0., 0., color),
            Vertex::new(x2 - tx, y2 - ty, 0., 0., 0., color),
        ],
        &[0, 1, 2, 2, 1, 3],
    );
}

/// Draw arc from `rotation`(in degrees) to `arc + rotation` (`arc` in degrees),
/// centered at `[x, y]` with a given number of `sides`, `radius`, line `thickness`, and `color`.
pub fn draw_arc(
    x: f32,
    y: f32,
    sides: u8,
    radius: f32,
    rotation: f32,
    thickness: f32,
    arc: f32,
    color: Color,
) {
    let rot = rotation.to_radians();
    let part = arc.to_radians();

    let sides = (sides as f32 * part / std::f32::consts::TAU)
        .ceil()
        .max(1.0);
    let span = part / sides;
    let sides = sides as usize;

    let context = get_context();
    context.gl.texture(None);
    context.gl.draw_mode(DrawMode::Triangles);

    let mut verticies = Vec::<Vertex>::with_capacity(sides * 2);
    let mut indicies = Vec::<u16>::with_capacity(sides * 2);

    for i in 0..sides {
        let start_angle = i as f32 * span + rot;
        let end_angle = start_angle + span;

        indicies.extend([0, 1, 2, 2, 1, 3].map(|k| k + (verticies.len() as u16)));

        for (angle, radius) in [
            (start_angle, radius),
            (start_angle, radius + thickness),
            (end_angle, radius),
            (end_angle, radius + thickness),
        ] {
            let point = Vec2::new(x, y) + radius * Vec2::from_angle(angle);
            verticies.push(Vertex::new(point.x, point.y, 0., 0., 0., color));
        }
    }

    context.gl.geometry(&verticies, &indicies);
}

#[cfg(test)]
mod rounded_rectangle_tests {
    use super::*;
    use crate::color::colors::WHITE;

    fn cross(a: Vec2, b: Vec2, c: Vec2) -> f64 {
        let ab = (b - a).as_dvec2();
        let ac = (c - a).as_dvec2();
        ab.x * ac.y - ab.y * ac.x
    }

    fn points(vertices: &[Vertex; 37]) -> Vec<Vec2> {
        vertices.iter().map(|v| v.position.truncate()).collect()
    }

    fn area(vertices: &[Vertex; 37], indices: &[u16; 108]) -> f64 {
        let points = points(vertices);
        indices
            .chunks_exact(3)
            .map(|t| {
                cross(
                    points[t[0] as usize],
                    points[t[1] as usize],
                    points[t[2] as usize],
                ) * 0.5
            })
            .sum()
    }

    #[test]
    fn rounded_rectangle_rejects_invalid_geometry_without_a_window() {
        for args in [
            (0., 0., 0., 20., 4.),
            (0., 0., 20., 0., 4.),
            (0., 0., -1., 20., 4.),
            (0., 0., 20., -1., 4.),
            (f32::NAN, 0., 20., 20., 4.),
            (0., f32::INFINITY, 20., 20., 4.),
            (0., 0., f32::INFINITY, 20., 4.),
            (0., 0., 20., f32::NAN, 4.),
            (0., 0., 20., 20., f32::NAN),
            (0., 0., 20., 20., f32::INFINITY),
            (0., 0., 20., 20., f32::NEG_INFINITY),
            (f32::MAX, 0., f32::MAX, 20., 4.),
            (0., f32::MAX, 20., f32::MAX, 4.),
            (1e20, 0., 1., 20., 4.),
            (0., 1e20, 20., 1., 4.),
            (1e8, 0., 10., 10., 5.),
            (0., 1e8, 10., 10., 5.),
        ] {
            let (x, y, w, h, r) = args;
            assert!(rounded_rectangle_geometry(x, y, w, h, r, WHITE).is_none());
            // An invalid public call must return before acquiring GPU context.
            draw_rectangle_rounded(x, y, w, h, r, WHITE);
        }
    }

    #[test]
    fn rounded_rectangle_zero_and_negative_radii_are_square() {
        let (vertices, indices) = rounded_rectangle_geometry(-5., 3., 24., 18., 0., WHITE).unwrap();
        assert!((area(&vertices, &indices) - 24. * 18.).abs() < 1e-6);
        let (negative, _) = rounded_rectangle_geometry(-5., 3., 24., 18., -50., WHITE).unwrap();
        assert_eq!(points(&vertices), points(&negative));
        for point in points(&vertices).into_iter().skip(1) {
            assert!(point.x == -5. || point.x == 19.);
            assert!(point.y == 3. || point.y == 21.);
        }
    }

    #[test]
    fn rounded_rectangle_clamps_radius_for_capsules_and_circles() {
        for (w, h) in [(80., 20.), (20., 80.), (20., 20.)] {
            let r = f32::min(w, h) * 0.5;
            let (clamped, indices) = rounded_rectangle_geometry(0., 0., w, h, r, WHITE).unwrap();
            let (oversized, _) = rounded_rectangle_geometry(0., 0., w, h, f32::MAX, WHITE).unwrap();
            assert_eq!(points(&clamped), points(&oversized));
            // Exact area of four inscribed eight-segment quarter circles,
            // plus the remaining straight rectangular area.
            let expected = f64::from(w * h)
                - (4. - 16. * (std::f64::consts::PI / 16.).sin()) * f64::from(r * r);
            assert!((area(&clamped, &indices) - expected).abs() < 0.001);
        }
    }

    #[test]
    fn rounded_rectangle_fan_is_convex_bounded_and_nonoverlapping() {
        for (w, h, r) in [
            (80., 50., 8.),
            (20., 20., 10.),
            (80., 20., 99.),
            (20., 80., 99.),
            (80., 50., 0.),
        ] {
            let (vertices, indices) = rounded_rectangle_geometry(-7., 11., w, h, r, WHITE).unwrap();
            let points = points(&vertices);
            for p in &points {
                assert!(p.x >= -7. && p.x <= -7. + w && p.y >= 11. && p.y <= 11. + h);
            }
            for triangle in indices.chunks_exact(3) {
                assert!(
                    cross(
                        points[triangle[0] as usize],
                        points[triangle[1] as usize],
                        points[triangle[2] as usize]
                    ) >= 0.
                );
            }
            for i in 0..36 {
                assert!(
                    cross(
                        points[i + 1],
                        points[(i + 1) % 36 + 1],
                        points[(i + 2) % 36 + 1]
                    ) >= -0.0001
                );
            }
            // Offset sample grid avoids deliberately shared fan edges. No
            // sample may lie strictly inside more than one triangle.
            let mut covered = 0;
            for ix in 0..43 {
                for iy in 0..37 {
                    let p = vec2(
                        -7. + (ix as f32 + 0.31) / 43. * w,
                        11. + (iy as f32 + 0.23) / 37. * h,
                    );
                    let count = indices
                        .chunks_exact(3)
                        .filter(|t| {
                            let (a, b, c) = (
                                points[t[0] as usize],
                                points[t[1] as usize],
                                points[t[2] as usize],
                            );
                            cross(a, b, p) > 1e-5 && cross(b, c, p) > 1e-5 && cross(c, a, p) > 1e-5
                        })
                        .count();
                    assert!(count <= 1, "overlap at {p:?}");
                    covered += count;
                }
            }
            assert!(covered > 1000);
        }
    }

    #[test]
    fn rounded_rectangle_preserves_color_and_normalized_uvs() {
        let color = Color::new(0.2, 0.4, 0.6, 0.5);
        let (vertices, indices) =
            rounded_rectangle_geometry(20., -40., 100., 50., 12., color).unwrap();
        for vertex in vertices {
            assert_eq!(vertex.color, <[u8; 4]>::from(color));
            assert!(
                vertex.uv.x >= 0. && vertex.uv.x <= 1. && vertex.uv.y >= 0. && vertex.uv.y <= 1.
            );
            assert_eq!(vertex.position.z, 0.);
        }
        assert!(indices.iter().all(|i| *i < 37));
    }
}
