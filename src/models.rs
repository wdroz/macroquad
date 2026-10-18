//! 3D shapes and models, loading 3d models from files, drawing 3D primitives.
//!
//! Collision wireframes for the [`macroquad::math`](crate::math) shapes are
//! [`draw_aabb`], [`draw_ray`], [`draw_plane_wires`], [`draw_triangle_wires`],
//! [`draw_obb`], and [`draw_capsule_wires`]. [`draw_sphere`] takes a center and
//! a radius, the same fields as a [`Sphere`](crate::math::Sphere).

use crate::math::{Aabb, Capsule, Obb, Plane, Ray, Triangle};
use crate::{color::Color, get_context};

use crate::{quad_gl::DrawMode, texture::Texture2D};
use glam::{vec2, vec3, vec4, Quat, Vec2, Vec3, Vec4};

#[repr(C)]
#[derive(Clone, Debug, Copy)]
pub struct Vertex {
    pub position: Vec3,
    pub uv: Vec2,
    pub color: [u8; 4],
    /// Normal is not used by macroquad and is completely optional.
    /// Might be usefull for custom shaders.
    /// While normal is not used by macroquad, it is completely safe to use it
    /// to pass arbitary user data, hence Vec4.
    pub normal: Vec4,
}

impl Vertex {
    pub fn new(x: f32, y: f32, z: f32, u: f32, v: f32, color: Color) -> Vertex {
        Vertex {
            position: vec3(x, y, z),
            uv: vec2(u, v),
            color: color.into(),
            normal: vec4(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn new2(position: Vec3, uv: Vec2, color: Color) -> Vertex {
        Vertex {
            position,
            uv,
            color: color.into(),
            normal: vec4(0.0, 0.0, 0.0, 0.0),
        }
    }
}

#[derive(Clone)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub texture: Option<Texture2D>,
}

/// Immutable geometry uploaded once and shared by cheap handle clones.
///
/// Draw with [`draw_retained_mesh`]. The source [`Mesh`] may be changed or
/// dropped after upload; this handle keeps no CPU copy of its geometry.
/// Queued draws keep the buffers and the selected base texture alive. Once
/// all handles and queued draws are gone, buffers are deleted at the next
/// renderer flush (normally the end of the frame) or retained mesh upload.
/// Creation and drawing require an active macroquad context.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn example(mesh: &Mesh) -> Result<(), RetainedMeshError> {
/// let retained = RetainedMesh::from_mesh(mesh)?;
/// draw_retained_mesh(&retained, Mat4::IDENTITY);
/// draw_retained_mesh(&retained, Mat4::from_translation(Vec3::X));
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct RetainedMesh {
    pub(crate) buffers: std::sync::Arc<crate::quad_gl::RetainedMeshBuffers>,
    /// Base texture selected when this handle is drawn. Changing it does not
    /// reupload geometry or alter already queued draws or other handle clones.
    pub texture: Option<Texture2D>,
}

/// Invalid triangle geometry passed to [`RetainedMesh::from_mesh`].
///
/// Validation runs before any GPU allocation. Mesh indices are `u16`, so a
/// mesh can address at most 65,536 vertices. Split larger meshes into chunks.
///
/// ```
/// use macroquad::prelude::*;
/// let empty = Mesh { vertices: vec![], indices: vec![], texture: None };
/// assert!(matches!(RetainedMesh::from_mesh(&empty), Err(RetainedMeshError::EmptyMesh)));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedMeshError {
    /// At least one vertex and one triangle are required.
    EmptyMesh,
    /// More vertices than a `u16` index can address.
    TooManyVertices,
    /// More indices than the backend's signed draw count can represent.
    TooManyIndices,
    /// The index count is not a multiple of three.
    IncompleteTriangle,
    /// An index does not refer to a vertex in the source mesh.
    IndexOutOfBounds { index: u16, vertex_count: usize },
}

impl std::fmt::Display for RetainedMeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyMesh => f.write_str("retained mesh must contain vertices and triangles"),
            Self::TooManyVertices => f.write_str("retained mesh exceeds 65,536 vertices"),
            Self::TooManyIndices => f.write_str("retained mesh exceeds the backend index count"),
            Self::IncompleteTriangle => {
                f.write_str("retained mesh indices must form complete triangles")
            }
            Self::IndexOutOfBounds {
                index,
                vertex_count,
            } => {
                write!(
                    f,
                    "retained mesh index {index} is outside {vertex_count} vertices"
                )
            }
        }
    }
}

impl std::error::Error for RetainedMeshError {}

impl RetainedMesh {
    /// Uploads a triangle mesh into immutable vertex and index buffers.
    ///
    /// Returns an error for empty, incomplete, oversized, or out-of-range
    /// geometry. Unlike immediate batching, this never clamps to the configured
    /// streaming draw-call capacity. The source mesh and its texture are unchanged.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn example(mesh: &Mesh) -> Result<(), RetainedMeshError> {
    /// let retained = RetainedMesh::from_mesh(mesh)?;
    /// draw_retained_mesh(&retained, Mat4::IDENTITY);
    /// # Ok(()) }
    /// ```
    pub fn from_mesh(mesh: &Mesh) -> Result<Self, RetainedMeshError> {
        validate_retained_mesh(mesh)?;
        let context = get_context();
        let buffers = context.gl.upload_retained_mesh(
            &mut *context.quad_context,
            &mesh.vertices,
            &mesh.indices,
        );
        Ok(Self {
            buffers,
            texture: mesh.texture.clone(),
        })
    }
}

fn validate_retained_mesh(mesh: &Mesh) -> Result<(), RetainedMeshError> {
    if mesh.vertices.is_empty() || mesh.indices.is_empty() {
        return Err(RetainedMeshError::EmptyMesh);
    }
    if mesh.vertices.len() > u16::MAX as usize + 1 {
        return Err(RetainedMeshError::TooManyVertices);
    }
    if mesh.indices.len() > i32::MAX as usize {
        return Err(RetainedMeshError::TooManyIndices);
    }
    if !mesh.indices.chunks_exact(3).remainder().is_empty() {
        return Err(RetainedMeshError::IncompleteTriangle);
    }
    if let Some(&index) = mesh
        .indices
        .iter()
        .find(|&&i| i as usize >= mesh.vertices.len())
    {
        return Err(RetainedMeshError::IndexOutOfBounds {
            index,
            vertex_count: mesh.vertices.len(),
        });
    }
    Ok(())
}

/// Queues retained triangles with the current camera and material.
///
/// `transform` is multiplied after the current model matrix. Submission order,
/// viewport, scissor, render target, custom uniforms and texture bindings are
/// preserved alongside ordinary immediate draws. Geometry is never reuploaded
/// by drawing. Each call is a separate draw; this is not hardware instancing.
///
/// Custom shaders receive the composed `Model` matrix. As with [`draw_mesh`],
/// they are responsible for transforming normals and choosing culling behavior
/// for reflected transforms. CPU vertices are not rewritten.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn example(mesh: &RetainedMesh) {
/// draw_retained_mesh(mesh, Mat4::from_translation(vec3(2.0, 0.0, 0.0)));
/// # }
/// ```
pub fn draw_retained_mesh(mesh: &RetainedMesh, transform: glam::Mat4) {
    let context = get_context();
    context.gl.texture(mesh.texture.as_ref());
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry_retained(mesh, transform);
}

#[cfg(test)]
mod retained_mesh_tests {
    use super::*;

    fn triangle() -> Mesh {
        Mesh {
            vertices: vec![Vertex::new(0., 0., 0., 0., 0., crate::color::WHITE); 3],
            indices: vec![0, 1, 2],
            texture: None,
        }
    }

    #[test]
    fn retained_mesh_validates_before_gpu_allocation() {
        let mut mesh = triangle();
        assert_eq!(validate_retained_mesh(&mesh), Ok(()));
        mesh.indices.push(0);
        assert_eq!(
            validate_retained_mesh(&mesh),
            Err(RetainedMeshError::IncompleteTriangle)
        );
        mesh.indices = vec![0, 1, 3];
        assert!(matches!(
            RetainedMesh::from_mesh(&mesh),
            Err(RetainedMeshError::IndexOutOfBounds {
                index: 3,
                vertex_count: 3
            })
        ));
        mesh.indices.clear();
        assert!(matches!(
            RetainedMesh::from_mesh(&mesh),
            Err(RetainedMeshError::EmptyMesh)
        ));
        mesh.indices = vec![0, 1, u16::MAX];
        mesh.vertices.resize(65_536, mesh.vertices[0]);
        assert_eq!(validate_retained_mesh(&mesh), Ok(()));
        mesh.vertices.push(mesh.vertices[0]);
        assert!(matches!(
            RetainedMesh::from_mesh(&mesh),
            Err(RetainedMeshError::TooManyVertices)
        ));
    }
}

pub fn draw_mesh(mesh: &Mesh) {
    let context = get_context();

    context.gl.texture(mesh.texture.as_ref());
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(&mesh.vertices[..], &mesh.indices[..]);
}

fn draw_quad(vertices: [Vertex; 4]) {
    let context = get_context();
    let indices = [0, 1, 2, 0, 2, 3];
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(&vertices, &indices);
}

pub fn draw_line_3d(start: Vec3, end: Vec3, color: Color) {
    let context = get_context();
    let uv = vec2(0., 0.);
    let indices = [0, 1];

    let line = [Vertex::new2(start, uv, color), Vertex::new2(end, uv, color)];

    context.gl.texture(None);
    context.gl.draw_mode(DrawMode::Lines);
    context.gl.geometry(&line, &indices);
}

/// Draw a grid centered at (0, 0, 0)
pub fn draw_grid(slices: u32, spacing: f32, axes_color: Color, other_color: Color) {
    draw_grid_ex(
        slices,
        spacing,
        axes_color,
        other_color,
        vec3(0., 0., 0.),
        Quat::IDENTITY,
    );
}

/// Draw a rotated grid centered at a specified point
pub fn draw_grid_ex(
    slices: u32,
    spacing: f32,
    axes_color: Color,
    other_color: Color,
    center: Vec3,
    rotation: Quat,
) {
    let half_slices = (slices as i32) / 2;
    for i in -half_slices..half_slices + 1 {
        let color = if i == 0 { axes_color } else { other_color };

        let start = vec3(i as f32 * spacing, 0., -half_slices as f32 * spacing);
        let end = vec3(i as f32 * spacing, 0., half_slices as f32 * spacing);

        draw_line_3d(
            rotation.mul_vec3(start) + center,
            rotation.mul_vec3(end) + center,
            color,
        );

        let start = vec3(-half_slices as f32 * spacing, 0., i as f32 * spacing);
        let end = vec3(half_slices as f32 * spacing, 0., i as f32 * spacing);

        draw_line_3d(
            rotation.mul_vec3(start) + center,
            rotation.mul_vec3(end) + center,
            color,
        );
    }
}

/// Draws a quad in the XZ plane, centered at `center`.
///
/// `size.x` and `size.y` are half the width and depth: the quad extends
/// `size` away from `center` on X and Z, and Y stays at `center.y`. This is a
/// horizontal patch when up is +Y. It is not a [`Plane`](crate::math::Plane).
/// A plane that may be tilted is [`draw_plane_wires`], whose `size` is the
/// full side length.
///
/// # Example
///
/// ```no_run
/// # use macroquad::prelude::*;
/// draw_plane(Vec3::ZERO, vec2(2.0, 2.0), None, GREEN);
/// ```
pub fn draw_plane(center: Vec3, size: Vec2, texture: Option<&Texture2D>, color: Color) {
    let v1 = Vertex::new2(center + vec3(-size.x, 0., -size.y), vec2(0., 0.), color);
    let v2 = Vertex::new2(center + vec3(-size.x, 0., size.y), vec2(0., 1.), color);
    let v3 = Vertex::new2(center + vec3(size.x, 0., size.y), vec2(1., 1.), color);
    let v4 = Vertex::new2(center + vec3(size.x, 0., -size.y), vec2(1., 0.), color);

    {
        let context = get_context();
        context.gl.texture(texture);
    }
    draw_quad([v1, v2, v3, v4]);
}

/// Draw an affine (2D) parallelogram at given position, as two triangles.
///
/// The drawn parallelogram will have the vertices: `offset`, `offset + e1`, `offset + e2` and `offset + e1 + e2`
///
/// # Arguments
///
/// * `offset` - Offset of the first point from the origin
/// * `e1`, `e2` - Base vectors for the parallelogram
/// * `texture` - Optional [Texture2D] to apply, which will be streched on the entire shape (todo!
/// support custom uv values per vertex)
/// * `color` - The [Color] to draw the parallelogram
///
/// # Examples
///
/// Draw an axis aligned rectangle
/// ```no_run
/// # use macroquad::prelude::*;
/// draw_affine_parallelogram(Vec3::ZERO, 3. * Vec3::X, 5. * Vec3::Z, None, RED);
/// ```
pub fn draw_affine_parallelogram(
    offset: Vec3,
    e1: Vec3,
    e2: Vec3,
    texture: Option<&Texture2D>,
    color: Color,
) {
    let v1 = Vertex::new2(offset, vec2(0., 0.), color);
    let v2 = Vertex::new2(offset + e1, vec2(0., 1.), color);
    let v3 = Vertex::new2(offset + e1 + e2, vec2(1., 1.), color);
    let v4 = Vertex::new2(offset + e2, vec2(1., 0.), color);

    {
        let context = get_context();
        context.gl.texture(texture);
    }
    draw_quad([v1, v2, v3, v4]);
}

/// Draw an affine (3D) parallelepiped at given position, using six parallelograms.
///
/// The drawn parallelepiped will be built from the followwing parallelograms:
///
/// * `offset, offset + e1, offset + e2`
/// * `offset, offset + e2, offset + e3`
/// * `offset, offset + e1, offset + e3`
/// * `offset, offset + e1 + e2, offset + e1 + e3`
/// * `offset, offset + e2 + e1, offset + e2 + e3`
/// * `offset, offset + e3 + e1, offset + e3 + e2`
///
/// # Arguments
///
/// * `offset` - Offset of the first point from the origin
/// * `e1`, `e2`, `e3` - Base vectors for the parallelepiped
/// * `texture` - Optional [Texture2D] to apply, which will repeat on each face (todo!
/// support custom uv values per vertex, multiple textures?)
/// * `color` - The [Color] to draw the parallelepiped (todo! support color per face?)
///
/// # Examples
///
/// Draw an axis aligned cube
/// ```no_run
/// # use macroquad::prelude::*;
/// draw_affine_parallelepiped(Vec3::ZERO, 3. * Vec3::X, 2. * Vec3::Y, 5. * Vec3::Z, None, RED);
/// ```
pub fn draw_affine_parallelepiped(
    offset: Vec3,
    e1: Vec3,
    e2: Vec3,
    e3: Vec3,
    texture: Option<&Texture2D>,
    color: Color,
) {
    draw_affine_parallelogram(offset, e1, e2, texture, color);
    draw_affine_parallelogram(offset, e1, e3, texture, color);
    draw_affine_parallelogram(offset, e2, e3, texture, color);

    draw_affine_parallelogram(offset + e1, e2, e3, texture, color);
    draw_affine_parallelogram(offset + e2, e1, e3, texture, color);
    draw_affine_parallelogram(offset + e3, e1, e2, texture, color);
}

pub fn draw_cube(position: Vec3, size: Vec3, texture: Option<&Texture2D>, color: Color) {
    let context = get_context();
    context.gl.texture(texture);

    let (x, y, z) = (position.x, position.y, position.z);
    let (width, height, length) = (size.x, size.y, size.z);

    // Front face
    let bl_pos = vec3(x - width / 2., y - height / 2., z + length / 2.);
    let bl_uv = vec2(0., 0.);
    let br_pos = vec3(x + width / 2., y - height / 2., z + length / 2.);
    let br_uv = vec2(1., 0.);

    let tr_pos = vec3(x + width / 2., y + height / 2., z + length / 2.);
    let tr_uv = vec2(1., 1.);

    let tl_pos = vec3(x - width / 2., y + height / 2., z + length / 2.);
    let tl_uv = vec2(0., 1.);

    draw_quad([
        Vertex::new2(bl_pos, bl_uv, color),
        Vertex::new2(br_pos, br_uv, color),
        Vertex::new2(tr_pos, tr_uv, color),
        Vertex::new2(tl_pos, tl_uv, color),
    ]);

    // Back face
    let bl_pos = vec3(x - width / 2., y - height / 2., z - length / 2.);
    let bl_uv = vec2(0., 0.);
    let br_pos = vec3(x + width / 2., y - height / 2., z - length / 2.);
    let br_uv = vec2(1., 0.);

    let tr_pos = vec3(x + width / 2., y + height / 2., z - length / 2.);
    let tr_uv = vec2(1., 1.);

    let tl_pos = vec3(x - width / 2., y + height / 2., z - length / 2.);
    let tl_uv = vec2(0., 1.);

    draw_quad([
        Vertex::new2(bl_pos, bl_uv, color),
        Vertex::new2(br_pos, br_uv, color),
        Vertex::new2(tr_pos, tr_uv, color),
        Vertex::new2(tl_pos, tl_uv, color),
    ]);

    // Top face
    let bl_pos = vec3(x - width / 2., y + height / 2., z - length / 2.);
    let bl_uv = vec2(0., 1.);
    let br_pos = vec3(x - width / 2., y + height / 2., z + length / 2.);
    let br_uv = vec2(0., 0.);

    let tr_pos = vec3(x + width / 2., y + height / 2., z + length / 2.);
    let tr_uv = vec2(1., 0.);

    let tl_pos = vec3(x + width / 2., y + height / 2., z - length / 2.);
    let tl_uv = vec2(1., 1.);

    draw_quad([
        Vertex::new2(bl_pos, bl_uv, color),
        Vertex::new2(br_pos, br_uv, color),
        Vertex::new2(tr_pos, tr_uv, color),
        Vertex::new2(tl_pos, tl_uv, color),
    ]);

    // Bottom face
    let bl_pos = vec3(x - width / 2., y - height / 2., z - length / 2.);
    let bl_uv = vec2(0., 1.);
    let br_pos = vec3(x - width / 2., y - height / 2., z + length / 2.);
    let br_uv = vec2(0., 0.);

    let tr_pos = vec3(x + width / 2., y - height / 2., z + length / 2.);
    let tr_uv = vec2(1., 0.);

    let tl_pos = vec3(x + width / 2., y - height / 2., z - length / 2.);
    let tl_uv = vec2(1., 1.);

    draw_quad([
        Vertex::new2(bl_pos, bl_uv, color),
        Vertex::new2(br_pos, br_uv, color),
        Vertex::new2(tr_pos, tr_uv, color),
        Vertex::new2(tl_pos, tl_uv, color),
    ]);

    // Right face
    let bl_pos = vec3(x + width / 2., y - height / 2., z - length / 2.);
    let bl_uv = vec2(0., 1.);
    let br_pos = vec3(x + width / 2., y + height / 2., z - length / 2.);
    let br_uv = vec2(0., 0.);

    let tr_pos = vec3(x + width / 2., y + height / 2., z + length / 2.);
    let tr_uv = vec2(1., 0.);

    let tl_pos = vec3(x + width / 2., y - height / 2., z + length / 2.);
    let tl_uv = vec2(1., 1.);

    draw_quad([
        Vertex::new2(bl_pos, bl_uv, color),
        Vertex::new2(br_pos, br_uv, color),
        Vertex::new2(tr_pos, tr_uv, color),
        Vertex::new2(tl_pos, tl_uv, color),
    ]);

    // Left face
    let bl_pos = vec3(x - width / 2., y - height / 2., z - length / 2.);
    let bl_uv = vec2(0., 1.);
    let br_pos = vec3(x - width / 2., y + height / 2., z - length / 2.);
    let br_uv = vec2(0., 0.);

    let tr_pos = vec3(x - width / 2., y + height / 2., z + length / 2.);
    let tr_uv = vec2(1., 0.);

    let tl_pos = vec3(x - width / 2., y - height / 2., z + length / 2.);
    let tl_uv = vec2(1., 1.);

    draw_quad([
        Vertex::new2(bl_pos, bl_uv, color),
        Vertex::new2(br_pos, br_uv, color),
        Vertex::new2(tr_pos, tr_uv, color),
        Vertex::new2(tl_pos, tl_uv, color),
    ]);
}

pub fn draw_cube_wires(position: Vec3, size: Vec3, color: Color) {
    let (x, y, z) = (position.x, position.y, position.z);
    let (width, height, length) = (size.x, size.y, size.z);

    // Front Face

    // Bottom Line
    draw_line_3d(
        vec3(x - width / 2., y - height / 2., z + length / 2.),
        vec3(x + width / 2., y - height / 2., z + length / 2.),
        color,
    );

    // Left Line
    draw_line_3d(
        vec3(x + width / 2., y - height / 2., z + length / 2.),
        vec3(x + width / 2., y + height / 2., z + length / 2.),
        color,
    );

    // Top Line
    draw_line_3d(
        vec3(x + width / 2., y + height / 2., z + length / 2.),
        vec3(x - width / 2., y + height / 2., z + length / 2.),
        color,
    );

    // Right Line
    draw_line_3d(
        vec3(x - width / 2., y + height / 2., z + length / 2.),
        vec3(x - width / 2., y - height / 2., z + length / 2.),
        color,
    );

    // Back Face
    // Bottom Line
    draw_line_3d(
        vec3(x - width / 2., y - height / 2., z - length / 2.),
        vec3(x + width / 2., y - height / 2., z - length / 2.),
        color,
    );

    // Left Line
    draw_line_3d(
        vec3(x + width / 2., y - height / 2., z - length / 2.),
        vec3(x + width / 2., y + height / 2., z - length / 2.),
        color,
    );

    // Top Line
    draw_line_3d(
        vec3(x + width / 2., y + height / 2., z - length / 2.),
        vec3(x - width / 2., y + height / 2., z - length / 2.),
        color,
    );

    // Right Line
    draw_line_3d(
        vec3(x - width / 2., y + height / 2., z - length / 2.),
        vec3(x - width / 2., y - height / 2., z - length / 2.),
        color,
    );

    // Top Face
    // Left Line
    draw_line_3d(
        vec3(x - width / 2., y + height / 2., z + length / 2.),
        vec3(x - width / 2., y + height / 2., z - length / 2.),
        color,
    );

    // Right Line
    draw_line_3d(
        vec3(x + width / 2., y + height / 2., z + length / 2.),
        vec3(x + width / 2., y + height / 2., z - length / 2.),
        color,
    );

    // Bottom Face
    // Left Line
    draw_line_3d(
        vec3(x - width / 2., y - height / 2., z + length / 2.),
        vec3(x - width / 2., y - height / 2., z - length / 2.),
        color,
    );

    // Right Line
    draw_line_3d(
        vec3(x + width / 2., y - height / 2., z + length / 2.),
        vec3(x + width / 2., y - height / 2., z - length / 2.),
        color,
    );
}

#[derive(Debug, Clone)]
pub struct DrawSphereParams {
    pub rings: usize,
    pub slices: usize,
    pub draw_mode: DrawMode,
}

impl Default for DrawSphereParams {
    fn default() -> DrawSphereParams {
        DrawSphereParams {
            rings: 16,
            slices: 16,
            draw_mode: DrawMode::Triangles,
        }
    }
}

pub fn draw_sphere(center: Vec3, radius: f32, texture: Option<&Texture2D>, color: Color) {
    draw_sphere_ex(center, radius, texture, color, Default::default());
}

pub fn draw_sphere_wires(center: Vec3, radius: f32, texture: Option<&Texture2D>, color: Color) {
    let params = DrawSphereParams {
        draw_mode: DrawMode::Lines,
        ..Default::default()
    };
    draw_sphere_ex(center, radius, texture, color, params);
}

pub fn draw_sphere_ex(
    center: Vec3,
    radius: f32,
    texture: Option<&Texture2D>,
    color: Color,
    params: DrawSphereParams,
) {
    let context = get_context();

    let rings = params.rings;
    let slices = params.slices;

    let scale = vec3(radius, radius, radius);

    context.gl.texture(texture);
    context.gl.draw_mode(params.draw_mode);

    for i in 0..rings + 1 {
        for j in 0..slices {
            use std::f32::consts::PI;

            let pi34 = PI / 2. * 3.;
            let pi2 = PI * 2.;
            let i = i as f32;
            let j = j as f32;
            let rings: f32 = rings as _;
            let slices: f32 = slices as _;

            let v1 = vec3(
                (pi34 + (PI / (rings + 1.)) * i).cos() * (j * pi2 / slices).sin(),
                (pi34 + (PI / (rings + 1.)) * i).sin(),
                (pi34 + (PI / (rings + 1.)) * i).cos() * (j * pi2 / slices).cos(),
            );
            let uv1 = vec2(i / rings, j / slices);
            let v2 = vec3(
                (pi34 + (PI / (rings + 1.)) * (i + 1.)).cos() * ((j + 1.) * pi2 / slices).sin(),
                (pi34 + (PI / (rings + 1.)) * (i + 1.)).sin(),
                (pi34 + (PI / (rings + 1.)) * (i + 1.)).cos() * ((j + 1.) * pi2 / slices).cos(),
            );
            let uv2 = vec2((i + 1.) / rings, (j + 1.) / slices);
            let v3 = vec3(
                (pi34 + (PI / (rings + 1.)) * (i + 1.)).cos() * (j * pi2 / slices).sin(),
                (pi34 + (PI / (rings + 1.)) * (i + 1.)).sin(),
                (pi34 + (PI / (rings + 1.)) * (i + 1.)).cos() * (j * pi2 / slices).cos(),
            );
            let uv3 = vec2((i + 1.) / rings, j / slices);

            context.gl.geometry(
                &[
                    Vertex::new2(v1 * scale + center, uv1, color),
                    Vertex::new2(v2 * scale + center, uv2, color),
                    Vertex::new2(v3 * scale + center, uv3, color),
                ],
                &[0, 1, 2],
            );

            let v1 = vec3(
                (pi34 + (PI / (rings + 1.)) * i).cos() * (j * pi2 / slices).sin(),
                (pi34 + (PI / (rings + 1.)) * i).sin(),
                (pi34 + (PI / (rings + 1.)) * i).cos() * (j * pi2 / slices).cos(),
            );
            let uv1 = vec2(i / rings, j / slices);
            let v2 = vec3(
                (pi34 + (PI / (rings + 1.)) * (i)).cos() * ((j + 1.) * pi2 / slices).sin(),
                (pi34 + (PI / (rings + 1.)) * (i)).sin(),
                (pi34 + (PI / (rings + 1.)) * (i)).cos() * ((j + 1.) * pi2 / slices).cos(),
            );
            let uv2 = vec2(i / rings, (j + 1.) / slices);
            let v3 = vec3(
                (pi34 + (PI / (rings + 1.)) * (i + 1.)).cos() * ((j + 1.) * pi2 / slices).sin(),
                (pi34 + (PI / (rings + 1.)) * (i + 1.)).sin(),
                (pi34 + (PI / (rings + 1.)) * (i + 1.)).cos() * ((j + 1.) * pi2 / slices).cos(),
            );
            let uv3 = vec2((i + 1.) / rings, (j + 1.) / slices);

            context.gl.geometry(
                &[
                    Vertex::new2(v1 * scale + center, uv1, color),
                    Vertex::new2(v2 * scale + center, uv2, color),
                    Vertex::new2(v3 * scale + center, uv3, color),
                ],
                &[0, 1, 2],
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct DrawCylinderParams {
    pub sides: usize,
    pub draw_mode: DrawMode,
}

impl Default for DrawCylinderParams {
    fn default() -> DrawCylinderParams {
        DrawCylinderParams {
            sides: 16,
            draw_mode: DrawMode::Triangles,
        }
    }
}

pub fn draw_cylinder(
    position: Vec3,
    radius_top: f32,
    radius_bottom: f32,
    height: f32,
    texture: Option<&Texture2D>,
    color: Color,
) {
    draw_cylinder_ex(
        position,
        radius_top,
        radius_bottom,
        height,
        texture,
        color,
        Default::default(),
    );
}

pub fn draw_cylinder_wires(
    position: Vec3,
    radius_top: f32,
    radius_bottom: f32,
    height: f32,
    texture: Option<&Texture2D>,
    color: Color,
) {
    let params = DrawCylinderParams {
        draw_mode: DrawMode::Lines,
        ..Default::default()
    };
    draw_cylinder_ex(
        position,
        radius_top,
        radius_bottom,
        height,
        texture,
        color,
        params,
    );
}

//Note: can also be used to draw a cone by setting radius_top or radius_bottom to 0
pub fn draw_cylinder_ex(
    position: Vec3,
    radius_top: f32,
    radius_bottom: f32,
    height: f32,
    texture: Option<&Texture2D>,
    color: Color,
    params: DrawCylinderParams,
) {
    let context = get_context();

    let sides = params.sides;

    context.gl.texture(texture);
    context.gl.draw_mode(params.draw_mode);

    use std::f32::consts::PI;
    let angle_step = PI * 2.0 / sides as f32;
    //draw body
    for i in 0..sides + 1 {
        let i = i as f32;
        //bottom left
        let v1 = vec3(
            (i * angle_step).sin() * radius_bottom,
            0.0,
            (i * angle_step).cos() * radius_bottom,
        );
        //bottom right
        let v2 = vec3(
            ((i + 1.0) * angle_step).sin() * radius_bottom,
            0.0,
            ((i + 1.0) * angle_step).cos() * radius_bottom,
        );
        //top right
        let v3 = vec3(
            ((i + 1.0) * angle_step).sin() * radius_top,
            height,
            ((i + 1.0) * angle_step).cos() * radius_top,
        );

        context.gl.geometry(
            &[
                Vertex::new2(v1 + position, vec2(0.0, 0.0), color),
                Vertex::new2(v2 + position, vec2(1.0, 0.0), color),
                Vertex::new2(v3 + position, vec2(1.0, 1.0), color),
            ],
            &[0, 1, 2],
        );

        //top left
        let v1 = vec3(
            (i * angle_step).sin() * radius_top,
            height,
            (i * angle_step).cos() * radius_top,
        );
        //bottom left
        let v2 = vec3(
            (i * angle_step).sin() * radius_bottom,
            0.0,
            (i * angle_step).cos() * radius_bottom,
        );
        //top right
        let v3 = vec3(
            ((i + 1.0) * angle_step).sin() * radius_top,
            height,
            ((i + 1.0) * angle_step).cos() * radius_top,
        );

        context.gl.geometry(
            &[
                Vertex::new2(v1 + position, vec2(0.0, 0.0), color),
                Vertex::new2(v2 + position, vec2(1.0, 0.0), color),
                Vertex::new2(v3 + position, vec2(1.0, 1.0), color),
            ],
            &[0, 1, 2],
        );
    }

    //draw cap
    for i in 0..sides + 1 {
        let i = i as f32;
        let v1 = vec3(0.0, height, 0.0);
        let v2 = vec3(
            (i * angle_step).sin() * radius_top,
            height,
            (i * angle_step).cos() * radius_top,
        );
        let v3 = vec3(
            ((i + 1.0) * angle_step).sin() * radius_top,
            height,
            ((i + 1.0) * angle_step).cos() * radius_top,
        );

        context.gl.geometry(
            &[
                Vertex::new2(v1 + position, vec2(0.0, 0.0), color),
                Vertex::new2(v2 + position, vec2(1.0, 0.0), color),
                Vertex::new2(v3 + position, vec2(1.0, 1.0), color),
            ],
            &[0, 1, 2],
        );
    }

    //draw base
    for i in 0..sides + 1 {
        let i = i as f32;
        let v1 = vec3(0.0, 0.0, 0.0);
        let v2 = vec3(
            (i * angle_step).sin() * radius_bottom,
            0.0,
            (i * angle_step).cos() * radius_bottom,
        );
        let v3 = vec3(
            ((i + 1.0) * angle_step).sin() * radius_bottom,
            0.0,
            ((i + 1.0) * angle_step).cos() * radius_bottom,
        );

        context.gl.geometry(
            &[
                Vertex::new2(v1 + position, vec2(0.0, 0.0), color),
                Vertex::new2(v2 + position, vec2(1.0, 0.0), color),
                Vertex::new2(v3 + position, vec2(1.0, 1.0), color),
            ],
            &[0, 1, 2],
        );
    }
}

/// Wireframe of an [`Aabb`](crate::math::Aabb).
///
/// This is [`draw_cube_wires`] with [`Aabb::center`](crate::math::Aabb::center)
/// and [`Aabb::size`](crate::math::Aabb::size), so the outline matches
/// [`draw_cube`] and [`Aabb::from_center_size`](crate::math::Aabb::from_center_size).
/// An inverted box (`min` past `max` on an axis) still outlines that span,
/// because the wires are drawn from the center by half of `max - min`. Overlap
/// and ray queries miss until [`Aabb::normalized`](crate::math::Aabb::normalized).
/// A non-finite corner draws nothing.
///
/// # Example
///
/// ```no_run
/// # use macroquad::prelude::*;
/// let crate_ = Aabb::from_center_size(vec3(0.0, 0.5, 0.0), Vec3::ONE);
/// draw_aabb(crate_, GREEN);
/// ```
pub fn draw_aabb(aabb: Aabb, color: Color) {
    if !aabb.min.is_finite() || !aabb.max.is_finite() {
        return;
    }
    draw_cube_wires(aabb.center(), aabb.size(), color);
}

/// Draws `ray` from its origin to `ray.point_at(t)`.
///
/// `t` is the same parameter as [`RayHit::t`](crate::math::RayHit::t). A
/// negative `t` extends behind the origin. A non-finite origin, direction, or
/// `t` draws nothing.
///
/// # Example
///
/// ```no_run
/// # use macroquad::prelude::*;
/// let ray = Ray::new(Vec3::ZERO, Vec3::X);
/// draw_ray(ray, 4.0, YELLOW);
/// ```
pub fn draw_ray(ray: Ray, t: f32, color: Color) {
    let Some((start, end)) = ray_segment(ray, t) else {
        return;
    };
    draw_line_3d(start, end, color);
}

/// Wire square lying on `plane`, centered on the projection of `center`.
///
/// `size` is the full side length, the same convention as [`draw_cube`].
/// `center` is moved onto the plane with
/// [`Plane::closest_point`](crate::math::Plane::closest_point) after a
/// non-unit normal is normalized and `distance` is divided by that same
/// length, so the square sits on the geometric plane. A zero or non-finite
/// normal, a non-finite distance or center, or a non-positive `size` draws
/// nothing.
///
/// [`draw_plane`] is a different helper: an axis-aligned quad in the XZ plane
/// whose `size` is a half-extent and which can take a texture.
///
/// # Example
///
/// ```no_run
/// # use macroquad::prelude::*;
/// let ground = Plane::new(Vec3::Y, 0.0);
/// draw_plane_wires(ground, Vec3::ZERO, 4.0, GREEN);
/// ```
pub fn draw_plane_wires(plane: Plane, center: Vec3, size: f32, color: Color) {
    let Some(corners) = plane_wire_corners(plane, center, size) else {
        return;
    };
    for index in 0..4 {
        draw_line_3d(corners[index], corners[(index + 1) % 4], color);
    }
}

/// Three edges of `triangle`.
///
/// A non-finite corner draws nothing.
///
/// # Example
///
/// ```no_run
/// # use macroquad::prelude::*;
/// let triangle = Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y);
/// draw_triangle_wires(triangle, PURPLE);
/// ```
pub fn draw_triangle_wires(triangle: Triangle, color: Color) {
    let Some(edges) = triangle_edges(triangle) else {
        return;
    };
    for (start, end) in edges {
        draw_line_3d(start, end, color);
    }
}

/// Twelve edges of `obb`.
///
/// [`Obb::half_extents`](crate::math::Obb::half_extents) are half of the full
/// size [`draw_cube`] takes. A negative component is empty and draws nothing,
/// matching the overlap queries. A zero extent still draws the flat box. A
/// zero or non-finite rotation is drawn as [`Quat::IDENTITY`] so the center
/// stays visible; those queries miss. A non-finite center or extent draws
/// nothing.
///
/// # Example
///
/// ```no_run
/// # use macroquad::prelude::*;
/// let box_ = Obb::new(Vec3::ZERO, vec3(0.5, 0.5, 0.5), Quat::IDENTITY);
/// draw_obb(box_, BLUE);
/// ```
pub fn draw_obb(obb: Obb, color: Color) {
    let Some(corners) = obb_corners(obb) else {
        return;
    };
    for (start, end) in obb_edge_indices() {
        draw_line_3d(corners[start], corners[end], color);
    }
}

/// Wire capsule: sphere wires at the inner endpoints and four lines on the hull.
///
/// `a` and `b` are the inner segment. The side lines sit `radius` out from
/// that segment. The spheres are [`draw_sphere_wires`], so each cap extends
/// `radius` past its endpoint. A negative or non-finite radius draws nothing.
/// A zero radius draws the segment. Coincident endpoints draw one sphere.
///
/// # Example
///
/// ```no_run
/// # use macroquad::prelude::*;
/// let body = Capsule::new(vec3(0.0, 0.4, 0.0), vec3(0.0, 1.2, 0.0), 0.3);
/// draw_capsule_wires(body, ORANGE);
/// ```
pub fn draw_capsule_wires(capsule: Capsule, color: Color) {
    if !capsule.a.is_finite() || !capsule.b.is_finite() || !capsule.radius.is_finite() {
        return;
    }
    if capsule.radius < 0.0 {
        return;
    }
    if capsule.radius == 0.0 {
        draw_line_3d(capsule.a, capsule.b, color);
        return;
    }
    draw_sphere_wires(capsule.a, capsule.radius, None, color);
    if (capsule.b - capsule.a).length_squared() > 1e-8 {
        draw_sphere_wires(capsule.b, capsule.radius, None, color);
        if let Some(lines) = capsule_side_lines(capsule) {
            for (start, end) in lines {
                draw_line_3d(start, end, color);
            }
        }
    }
}

fn ray_segment(ray: Ray, t: f32) -> Option<(Vec3, Vec3)> {
    if !ray.origin.is_finite() || !ray.direction.is_finite() || !t.is_finite() {
        return None;
    }
    let end = ray.point_at(t);
    if !end.is_finite() {
        None
    } else {
        Some((ray.origin, end))
    }
}

/// Unit plane for the same geometric plane as `plane`.
///
/// A non-unit normal is normalized and `distance` is scaled by the same
/// factor. A zero or non-finite normal is `None`.
fn unit_plane(plane: Plane) -> Option<Plane> {
    if !plane.normal.is_finite() || !plane.distance.is_finite() {
        return None;
    }
    let length = plane.normal.length();
    if !length.is_finite() || length <= 1e-8 {
        return None;
    }
    Some(Plane::new(plane.normal / length, plane.distance / length))
}

fn plane_tangents(normal: Vec3) -> Option<(Vec3, Vec3)> {
    let helper = if normal.cross(Vec3::Y).length_squared() > 1e-8 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let tangent = normal.cross(helper);
    if tangent.length_squared() <= 1e-12 {
        return None;
    }
    let tangent = tangent.normalize();
    let bitangent = normal.cross(tangent).normalize();
    if tangent.is_finite() && bitangent.is_finite() {
        Some((tangent, bitangent))
    } else {
        None
    }
}

fn plane_wire_corners(plane: Plane, center: Vec3, size: f32) -> Option<[Vec3; 4]> {
    if !center.is_finite() || !size.is_finite() || size <= 0.0 {
        return None;
    }
    let plane = unit_plane(plane)?;
    let (tangent, bitangent) = plane_tangents(plane.normal)?;
    let center = plane.closest_point(center);
    if !center.is_finite() {
        return None;
    }
    let half = size * 0.5;
    let corners = [
        center + (tangent + bitangent) * half,
        center + (-tangent + bitangent) * half,
        center + (-tangent - bitangent) * half,
        center + (tangent - bitangent) * half,
    ];
    if corners.iter().any(|corner| !corner.is_finite()) {
        None
    } else {
        Some(corners)
    }
}

fn triangle_edges(triangle: Triangle) -> Option<[(Vec3, Vec3); 3]> {
    if triangle.a.is_finite() && triangle.b.is_finite() && triangle.c.is_finite() {
        Some([
            (triangle.a, triangle.b),
            (triangle.b, triangle.c),
            (triangle.c, triangle.a),
        ])
    } else {
        None
    }
}

fn unit_quat(rotation: Quat) -> Quat {
    let length_sq = rotation.length_squared();
    if length_sq.is_finite() && length_sq > 1e-12 {
        rotation * length_sq.sqrt().recip()
    } else {
        Quat::IDENTITY
    }
}

fn obb_corners(obb: Obb) -> Option<[Vec3; 8]> {
    if !obb.center.is_finite() || !obb.half_extents.is_finite() {
        return None;
    }
    // A negative extent is empty for every Obb query. Drawing the absolute
    // box would show a solid the queries reject.
    if obb.half_extents.x < 0.0 || obb.half_extents.y < 0.0 || obb.half_extents.z < 0.0 {
        return None;
    }
    let rotation = unit_quat(obb.rotation);
    let corners = std::array::from_fn(|index| {
        let local = vec3(
            if index & 1 == 0 {
                -obb.half_extents.x
            } else {
                obb.half_extents.x
            },
            if index & 2 == 0 {
                -obb.half_extents.y
            } else {
                obb.half_extents.y
            },
            if index & 4 == 0 {
                -obb.half_extents.z
            } else {
                obb.half_extents.z
            },
        );
        obb.center + rotation * local
    });
    if corners.iter().any(|corner| !corner.is_finite()) {
        None
    } else {
        Some(corners)
    }
}

/// Twelve edges of an 8-corner box. Index bit 0 is X, bit 1 is Y, bit 2 is Z.
fn obb_edge_indices() -> [(usize, usize); 12] {
    let mut edges = [(0, 0); 12];
    let mut count = 0;
    for index in 0..8 {
        if index & 1 == 0 {
            edges[count] = (index, index + 1);
            count += 1;
        }
        if index & 2 == 0 {
            edges[count] = (index, index + 2);
            count += 1;
        }
        if index & 4 == 0 {
            edges[count] = (index, index + 4);
            count += 1;
        }
    }
    edges
}

fn capsule_side_lines(capsule: Capsule) -> Option<[(Vec3, Vec3); 4]> {
    if !capsule.a.is_finite()
        || !capsule.b.is_finite()
        || !capsule.radius.is_finite()
        || capsule.radius <= 0.0
    {
        return None;
    }
    let axis = capsule.b - capsule.a;
    if !axis.is_finite() || axis.length_squared() <= 1e-8 {
        return None;
    }
    let (tangent, bitangent) = plane_tangents(axis.normalize())?;
    let radius = capsule.radius;
    let lines = [
        tangent * radius,
        -tangent * radius,
        bitangent * radius,
        -bitangent * radius,
    ]
    .map(|offset| (capsule.a + offset, capsule.b + offset));
    if lines
        .iter()
        .any(|(start, end)| !start.is_finite() || !end.is_finite())
    {
        None
    } else {
        Some(lines)
    }
}

#[cfg(test)]
mod debug_draw_tests {
    use super::*;
    use glam::Vec3;

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn ray_segment_scales_direction_and_rejects_non_finite() {
        let ray = Ray::new(vec3(1.0, 2.0, 3.0), vec3(0.0, 0.0, 2.0));
        let (start, end) = ray_segment(ray, 0.5).unwrap();
        assert!(near(start, ray.origin));
        assert!(near(end, vec3(1.0, 2.0, 4.0)));

        let (start, end) = ray_segment(ray, -1.0).unwrap();
        assert!(near(start, ray.origin));
        assert!(near(end, vec3(1.0, 2.0, 1.0)));

        assert!(ray_segment(ray, f32::NAN).is_none());
        assert!(ray_segment(Ray::new(Vec3::NAN, Vec3::X), 1.0).is_none());
    }

    #[test]
    fn plane_wire_is_a_square_on_the_plane() {
        let ground = Plane::new(Vec3::Y, 0.0);
        let corners = plane_wire_corners(ground, vec3(2.0, 5.0, -1.0), 2.0).unwrap();
        let centroid = (corners[0] + corners[1] + corners[2] + corners[3]) * 0.25;
        assert!(near(centroid, vec3(2.0, 0.0, -1.0)));
        for corner in corners {
            assert!(ground.signed_distance(corner).abs() < 1e-4);
        }
        for index in 0..4 {
            let edge = corners[(index + 1) % 4] - corners[index];
            assert!((edge.length() - 2.0).abs() < 1e-4);
        }
        let diagonal = corners[2] - corners[0];
        assert!((diagonal.length() - 2.0 * std::f32::consts::SQRT_2).abs() < 1e-4);
    }

    #[test]
    fn plane_wire_normalizes_a_non_unit_normal_and_rejects_a_bad_plane() {
        let raised = Plane::new(vec3(0.0, 2.0, 0.0), 4.0);
        let corners = plane_wire_corners(raised, vec3(0.0, 10.0, 0.0), 2.0).unwrap();
        for corner in corners {
            assert!((corner.y - 2.0).abs() < 1e-4);
        }
        assert!(plane_wire_corners(Plane::new(Vec3::ZERO, 0.0), Vec3::ZERO, 1.0).is_none());
        assert!(plane_wire_corners(Plane::new(Vec3::Y, 0.0), Vec3::ZERO, 0.0).is_none());
        assert!(plane_wire_corners(Plane::new(Vec3::Y, 0.0), Vec3::ZERO, -2.0).is_none());
        assert!(plane_wire_corners(Plane::new(Vec3::Y, f32::NAN), Vec3::ZERO, 1.0).is_none());
    }

    #[test]
    fn triangle_edges_follow_winding_and_reject_non_finite() {
        let triangle = Triangle::new(Vec3::ZERO, Vec3::X, Vec3::Y);
        let edges = triangle_edges(triangle).unwrap();
        assert!(near(edges[0].0, Vec3::ZERO) && near(edges[0].1, Vec3::X));
        assert!(near(edges[1].0, Vec3::X) && near(edges[1].1, Vec3::Y));
        assert!(near(edges[2].0, Vec3::Y) && near(edges[2].1, Vec3::ZERO));
        assert!(triangle_edges(Triangle::new(Vec3::NAN, Vec3::X, Vec3::Y)).is_none());
    }

    #[test]
    fn obb_corners_match_half_extents_and_a_yaw() {
        let identity = Obb::new(vec3(1.0, 2.0, 3.0), vec3(1.0, 2.0, 3.0), Quat::IDENTITY);
        let corners = obb_corners(identity).unwrap();
        assert!(corners.iter().any(|corner| near(*corner, Vec3::ZERO)));
        assert!(corners
            .iter()
            .any(|corner| near(*corner, vec3(2.0, 4.0, 6.0))));
        assert_eq!(obb_edge_indices().len(), 12);

        let yaw = Obb::new(
            Vec3::ZERO,
            Vec3::ONE,
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        );
        let yawed = obb_corners(yaw).unwrap();
        // +90° about Y sends local (x, y, z) to (z, y, -x).
        assert!(yawed
            .iter()
            .any(|corner| near(*corner, vec3(1.0, 1.0, -1.0))));
        assert!(yawed.iter().all(|corner| {
            (corner.x.abs() - 1.0).abs() < 1e-4
                && (corner.y.abs() - 1.0).abs() < 1e-4
                && (corner.z.abs() - 1.0).abs() < 1e-4
        }));
    }

    #[test]
    fn obb_draws_identity_for_a_bad_rotation_and_skips_a_bad_center() {
        let zero = obb_corners(Obb::new(
            Vec3::ZERO,
            Vec3::ONE,
            Quat::from_xyzw(0.0, 0.0, 0.0, 0.0),
        ))
        .unwrap();
        let identity = obb_corners(Obb::new(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY)).unwrap();
        for (left, right) in zero.iter().zip(identity.iter()) {
            assert!(near(*left, *right));
        }

        assert!(obb_corners(Obb::new(Vec3::ZERO, vec3(-1.0, 2.0, 3.0), Quat::IDENTITY)).is_none());
        let flat = obb_corners(Obb::new(Vec3::ZERO, vec3(1.0, 0.0, 1.0), Quat::IDENTITY)).unwrap();
        assert!(flat.iter().all(|corner| corner.y.abs() < 1e-4));

        assert!(obb_corners(Obb::new(Vec3::NAN, Vec3::ONE, Quat::IDENTITY)).is_none());
    }

    #[test]
    fn capsule_side_lines_sit_on_the_hull() {
        let capsule = Capsule::new(Vec3::ZERO, vec3(0.0, 2.0, 0.0), 0.5);
        let lines = capsule_side_lines(capsule).unwrap();
        for (start, end) in lines {
            let start_radius = (start.x * start.x + start.z * start.z).sqrt();
            let end_radius = (end.x * end.x + end.z * end.z).sqrt();
            assert!((start_radius - 0.5).abs() < 1e-4);
            assert!((end_radius - 0.5).abs() < 1e-4);
            assert!(start.y.abs() < 1e-4);
            assert!((end.y - 2.0).abs() < 1e-4);
            assert!(((end - start).dot(Vec3::Y) - 2.0).abs() < 1e-4);
        }
        assert!(capsule_side_lines(Capsule::new(Vec3::ZERO, Vec3::Y, -1.0)).is_none());
        assert!(capsule_side_lines(Capsule::new(Vec3::ZERO, Vec3::ZERO, 0.5)).is_none());
        assert!(capsule_side_lines(Capsule::new(Vec3::ZERO, Vec3::Y, 0.0)).is_none());
    }
}
