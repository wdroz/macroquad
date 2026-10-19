use std::sync::Arc;

use glam::{Mat4, Vec3, Vec4};

use super::{GltfModel, GltfModelStats, GltfSurface};
use crate::{
    math::{Aabb, Frustum},
    models::{draw_retained_mesh, validate_retained_mesh, RetainedMesh, RetainedMeshError},
    texture::Texture2D,
};

/// Transform information for a custom retained glTF material.
///
/// The default material only transforms positions. A lighting shader must use
/// `normalize((NormalTransform * vec4(normal.xyz, 0.0)).xyz)` with
/// [`Self::normal_matrix`] supplied as its `NormalTransform` uniform. Use the
/// built-in `Model` uniform for positions; it already includes the draw transform.
/// When [`Self::mirrored`] is true, a material that culls faces must use the
/// opposite `PipelineParams::front_face_order` (or disable face culling).
/// The loader has already corrected reflected *node* winding; this flag only
/// accounts for the additional instance transform. No indices are rewritten.
///
/// Pass the complete matrix, including a parent pushed through the low-level
/// model stack, when constructing this value. Material selection and uniforms
/// stay application controlled and are captured by each retained draw.
///
/// ```
/// use macroquad::prelude::*;
/// let pose = GltfModelTransform::new(Mat4::from_scale(vec3(-2., 1., 1.))).unwrap();
/// assert!(pose.mirrored);
/// let normal = pose.normal_matrix.transform_vector3(Vec3::X).normalize();
/// assert_eq!(normal, -Vec3::X);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct GltfModelTransform {
    /// Complete affine transform used for positions.
    pub matrix: Mat4,
    /// Inverse transpose of `matrix`. Transform normals with a zero W component.
    pub normal_matrix: Mat4,
    /// Whether this instance reverses the source triangle winding.
    pub mirrored: bool,
}

impl GltfModelTransform {
    /// Builds normal and winding information without a graphics context.
    ///
    /// Returns `None` for non-finite, projective, singular, or numerically
    /// non-invertible matrices. Drawing and bounds support singular affine
    /// transforms, but lighting normals for those transforms are undefined.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// assert!(GltfModelTransform::new(Mat4::IDENTITY).is_some());
    /// assert!(GltfModelTransform::new(Mat4::from_scale(Vec3::ZERO)).is_none());
    /// ```
    pub fn new(matrix: Mat4) -> Option<Self> {
        if !is_affine(matrix) {
            return None;
        }
        let determinant = matrix.determinant();
        if !determinant.is_finite() || determinant == 0.0 {
            return None;
        }
        let normal_matrix = matrix.inverse().transpose();
        if !normal_matrix.is_finite() {
            return None;
        }
        Some(Self {
            matrix,
            normal_matrix,
            mirrored: determinant < 0.0,
        })
    }
}

/// One uploaded glTF chunk and its material and node metadata.
///
/// Cloning shares the immutable geometry and textures. Bounds are in the baked
/// scene coordinates, before any instance transform, and are rebuilt from the
/// source vertices during upload. See [`GltfModelTransform`] for lighting and
/// reflected-instance material requirements.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn draw(part: &RetainedGltfDrawMesh) {
/// part.draw(Mat4::from_translation(Vec3::X));
/// # }
/// ```
#[derive(Clone)]
pub struct RetainedGltfDrawMesh {
    /// Shared immutable buffers and the selected base-color texture.
    pub mesh: RetainedMesh,
    /// Shared emissive image, for application shaders.
    pub emissive_texture: Option<Texture2D>,
    /// Material metadata; drawing does not apply lighting, alpha or sidedness.
    pub surface: GltfSurface,
    /// Index into the original CPU scene's node list.
    pub node: usize,
    /// Original node or mesh name.
    pub name: String,
    /// Original ancestor and node names joined by `/`.
    pub path: String,
    /// Center of the untransformed chunk bounds.
    pub center: Vec3,
    /// Minimum corner before the instance transform.
    pub bounds_min: Vec3,
    /// Maximum corner before the instance transform.
    pub bounds_max: Vec3,
}

impl RetainedGltfDrawMesh {
    /// Queues this chunk with the current material and camera.
    ///
    /// The model stack is multiplied by `transform`, as in
    /// [`draw_retained_mesh`]. Non-finite or projective composed transforms draw
    /// nothing. Normals and reflected face culling follow [`GltfModelTransform`].
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn draw(part: &RetainedGltfDrawMesh) {
    /// part.draw(Mat4::IDENTITY);
    /// # }
    /// ```
    pub fn draw(&self, transform: Mat4) {
        if is_affine(crate::get_context().gl.model_matrix() * transform) {
            draw_retained_mesh(&self.mesh, transform);
        }
    }

    /// Conservative world bounds after a complete affine transform.
    ///
    /// Transforms all eight corners, including for rotation, nonuniform scale,
    /// reflection, shear and zero scale. Returns `None` for invalid bounds,
    /// non-finite/projective transforms or overflow. This pure query does not
    /// read the model stack; include any parent in `transform` yourself.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn query(part: &RetainedGltfDrawMesh) {
    /// let bounds = part.world_bounds(Mat4::from_translation(Vec3::Y));
    /// assert!(bounds.is_some());
    /// # }
    /// ```
    pub fn world_bounds(&self, transform: Mat4) -> Option<Aabb> {
        transformed_bounds(self.bounds_min, self.bounds_max, transform)
    }
}

/// Uploaded chunks of a [`GltfModel`], reusable with per-draw transforms.
///
/// Upload consumes the CPU model and makes keeping its geometry an explicit
/// choice. Clones share GPU buffers, textures, and the optional CPU model;
/// cloning or drawing never uploads geometry. Metadata is independently cloned.
/// This is repeated retained drawing, not hardware instancing. Queued draws
/// keep their buffers and textures alive after the model is dropped.
///
/// Like [`GltfModel::draw`], drawing preserves part order and uses the current
/// camera and material. For lighting, alpha sorting and sidedness, inspect
/// `parts`, select your material, and call [`RetainedGltfDrawMesh::draw`].
/// [`GltfModelTransform`] defines the normal and reflected-winding contract.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn example() -> Result<(), String> {
/// let cpu = GltfModel::from_bytes(include_bytes!("../../examples/assets/textured_quad.glb"))?;
/// let model = RetainedGltfModel::from_model(cpu, false).map_err(|e| e.to_string())?;
/// model.draw(Mat4::IDENTITY);
/// model.clone().draw(Mat4::from_translation(Vec3::X));
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct RetainedGltfModel {
    /// Chunks in the same order as the source model.
    pub parts: Vec<RetainedGltfDrawMesh>,
    /// Original loader counts; these are not byte or performance measurements.
    pub stats: GltfModelStats,
    /// Combined untransformed minimum, or zero for an empty model.
    pub bounds_min: Vec3,
    /// Combined untransformed maximum, or zero for an empty model.
    pub bounds_max: Vec3,
    /// Names of the original root nodes.
    pub root_names: Vec<String>,
    pub(super) cpu_model: Option<Arc<GltfModel>>,
}

impl RetainedGltfModel {
    /// Uploads all chunks once, keeping CPU geometry only when explicitly asked.
    ///
    /// `retain_cpu_geometry: true` keeps the consumed model in a shared `Arc`,
    /// accessible through [`Self::cpu_model`]. `false` releases its vertices and
    /// indices after upload. Images are shared in both cases. An active graphics
    /// context is required for nonempty models. Every chunk is validated before
    /// any geometry is uploaded; errors follow [`RetainedMesh::from_mesh`].
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn upload(cpu: GltfModel) -> Result<(), RetainedMeshError> {
    /// let model = RetainedGltfModel::from_model(cpu, true)?;
    /// assert!(model.cpu_model().is_some());
    /// # Ok(()) }
    /// ```
    pub fn from_model(
        model: GltfModel,
        retain_cpu_geometry: bool,
    ) -> Result<Self, RetainedMeshError> {
        for part in &model.parts {
            validate_retained_mesh(&part.mesh)?;
        }
        let mut parts = Vec::with_capacity(model.parts.len());
        for part in &model.parts {
            // Rebuild after possible application edits to the CPU geometry.
            let bounds = Aabb::from_points(part.mesh.vertices.iter().map(|v| v.position)).unwrap();
            parts.push(RetainedGltfDrawMesh {
                mesh: RetainedMesh::from_mesh(&part.mesh)?,
                emissive_texture: part.emissive_texture.clone(),
                surface: part.surface.clone(),
                node: part.node,
                name: part.name.clone(),
                path: part.path.clone(),
                center: bounds.center(),
                bounds_min: bounds.min,
                bounds_max: bounds.max,
            });
        }
        let combined = parts
            .iter()
            .map(|p| Aabb::from_min_max(p.bounds_min, p.bounds_max))
            .reduce(|a, b| Aabb::from_min_max(a.min.min(b.min), a.max.max(b.max)));
        Ok(Self {
            bounds_min: combined.map_or(Vec3::ZERO, |b| b.min),
            bounds_max: combined.map_or(Vec3::ZERO, |b| b.max),
            stats: model.stats,
            root_names: model.root_names.clone(),
            parts,
            cpu_model: retain_cpu_geometry.then(|| Arc::new(model)),
        })
    }

    /// Source geometry, when retention was requested during upload.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn inspect(model: &RetainedGltfModel) {
    /// if let Some(cpu) = model.cpu_model() {
    ///     println!("{} CPU chunks retained", cpu.parts.len());
    /// }
    /// # }
    /// ```
    pub fn cpu_model(&self) -> Option<&GltfModel> {
        self.cpu_model.as_deref()
    }

    /// Queues every part in source order with this instance transform.
    ///
    /// Uses the current material/camera and no automatic material switching.
    /// See [`RetainedGltfDrawMesh::draw`] and [`GltfModelTransform`].
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn draw(model: &RetainedGltfModel) {
    /// model.draw(Mat4::from_translation(vec3(2., 0., 0.)));
    /// # }
    /// ```
    pub fn draw(&self, transform: Mat4) {
        for part in &self.parts {
            part.draw(transform);
        }
    }

    /// Draws chunks whose transformed bounds intersect the world-space frustum.
    ///
    /// Returns the number submitted. Culling includes the current model stack
    /// times `transform`, exactly as drawing does, and preserves source order.
    /// Invalid transforms/bounds are skipped. Conservative boxes may keep a
    /// chunk that is actually outside. This does not sort transparent parts.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn draw(model: &RetainedGltfModel, camera: &Camera3D) {
    /// if let Some(frustum) = camera.frustum() {
    ///     let submitted = model.draw_culled(Mat4::IDENTITY, &frustum);
    ///     assert!(submitted <= model.parts.len());
    /// }
    /// # }
    /// ```
    pub fn draw_culled(&self, transform: Mat4, frustum: &Frustum) -> usize {
        let composed = crate::get_context().gl.model_matrix() * transform;
        let mut submitted = 0;
        for part in &self.parts {
            if part
                .world_bounds(composed)
                .is_some_and(|bounds| frustum.intersects_aabb(&bounds))
            {
                part.draw(transform);
                submitted += 1;
            }
        }
        submitted
    }

    /// Conservative bounds for the complete instance, without a GPU context.
    ///
    /// Returns `None` for an empty model or an invalid transform/bound. Supply
    /// the full transform including any parent; see the part's `world_bounds`.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn query(model: &RetainedGltfModel) {
    /// let bounds = model.world_bounds(Mat4::IDENTITY);
    /// # }
    /// ```
    pub fn world_bounds(&self, transform: Mat4) -> Option<Aabb> {
        if self.parts.is_empty() {
            return None;
        }
        transformed_bounds(self.bounds_min, self.bounds_max, transform)
    }
}

pub(super) fn is_affine(matrix: Mat4) -> bool {
    matrix.is_finite() && matrix.row(3) == Vec4::W
}

pub(super) fn transformed_bounds(min: Vec3, max: Vec3, matrix: Mat4) -> Option<Aabb> {
    if !is_affine(matrix) || !min.is_finite() || !max.is_finite() || min.cmpgt(max).any() {
        return None;
    }
    let mut result = Aabb::from_min_max(Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for bits in 0..8 {
        let corner = Vec3::new(
            if bits & 1 == 0 { min.x } else { max.x },
            if bits & 2 == 0 { min.y } else { max.y },
            if bits & 4 == 0 { min.z } else { max.z },
        );
        let point = matrix.transform_point3(corner);
        if !point.is_finite() {
            return None;
        }
        result.min = result.min.min(point);
        result.max = result.max.max(point);
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{vec3, Quat};

    #[test]
    fn normal_transform_matches_baked_geometry_with_reflection_and_shear() {
        let mut matrix = Mat4::from_scale_rotation_translation(
            vec3(-2., 3., 0.5),
            Quat::from_rotation_y(0.7),
            vec3(4., -2., 8.),
        );
        matrix.y_axis += matrix.x_axis * 0.3;
        let pose = GltfModelTransform::new(matrix).unwrap();
        assert!(pose.mirrored);
        let a = vec3(0., 0., 0.);
        let b = vec3(1., 1., 0.);
        let c = vec3(0., 1., 1.);
        let normal = (b - a).cross(c - a).normalize();
        let world_normal = pose.normal_matrix.transform_vector3(normal).normalize();
        let edge_b = matrix.transform_point3(b) - matrix.transform_point3(a);
        let edge_c = matrix.transform_point3(c) - matrix.transform_point3(a);
        assert!(world_normal.dot(edge_b).abs() < 1e-5);
        assert!(world_normal.dot(edge_c).abs() < 1e-5);
        // Selecting opposite front-face order has the same orientation as swapping indices.
        assert!(edge_c.cross(edge_b).normalize().dot(world_normal) > 0.99999);
        assert!(!GltfModelTransform::new(Mat4::IDENTITY).unwrap().mirrored);
    }

    #[test]
    fn transformed_bounds_enclose_affine_positions_and_drive_culling() {
        let min = vec3(7., -0.5, -0.2);
        let max = vec3(9., 0.5, 0.2);
        let translate = Mat4::from_translation(vec3(-8., 0., 0.));
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY).unwrap();
        assert!(!frustum.intersects_aabb(&Aabb::from_min_max(min, max)));
        assert!(frustum.intersects_aabb(&transformed_bounds(min, max, translate).unwrap()));
        let mut transform = Mat4::from_scale_rotation_translation(
            vec3(-2., 0.5, 3.),
            Quat::from_rotation_z(0.8),
            vec3(2., 3., 4.),
        );
        transform.z_axis += transform.y_axis * 0.5;
        let bounds = transformed_bounds(min, max, transform).unwrap();
        for x in 0..=4 {
            for y in 0..=4 {
                for z in 0..=4 {
                    let p = min + (max - min) * vec3(x as f32, y as f32, z as f32) * 0.25;
                    assert!(bounds
                        .expanded(1e-5)
                        .contains(transform.transform_point3(p)));
                }
            }
        }
    }

    #[test]
    fn singular_bounds_work_while_invalid_normal_matrices_are_rejected() {
        let singular = Mat4::from_scale(vec3(1., 0., 2.));
        let bounds = transformed_bounds(-Vec3::ONE, Vec3::ONE, singular).unwrap();
        assert_eq!(bounds.min, vec3(-1., 0., -2.));
        assert_eq!(bounds.max, vec3(1., 0., 2.));
        assert!(GltfModelTransform::new(singular).is_none());
        for invalid in [
            Mat4::NAN,
            Mat4::ZERO,
            Mat4::perspective_rh_gl(1., 1., 0.1, 10.),
        ] {
            assert!(transformed_bounds(-Vec3::ONE, Vec3::ONE, invalid).is_none());
            assert!(GltfModelTransform::new(invalid).is_none());
        }
        assert!(transformed_bounds(Vec3::ONE, -Vec3::ONE, Mat4::IDENTITY).is_none());
        assert!(transformed_bounds(Vec3::ZERO, Vec3::NAN, Mat4::IDENTITY).is_none());
        assert!(transformed_bounds(
            Vec3::ZERO,
            Vec3::splat(f32::MAX),
            Mat4::from_scale(Vec3::splat(2.))
        )
        .is_none());
    }

    #[test]
    fn empty_model_cpu_retention_is_explicit_and_shared() {
        for retain in [false, true] {
            let model = GltfModel {
                parts: vec![],
                stats: GltfModelStats::default(),
                bounds_min: Vec3::ZERO,
                bounds_max: Vec3::ZERO,
                root_names: vec!["empty".into()],
            };
            let retained = RetainedGltfModel::from_model(model, retain).unwrap();
            let clone = retained.clone();
            assert_eq!(retained.cpu_model().is_some(), retain);
            if retain {
                assert!(Arc::ptr_eq(
                    retained.cpu_model.as_ref().unwrap(),
                    clone.cpu_model.as_ref().unwrap()
                ));
            }
            assert!(retained.world_bounds(Mat4::IDENTITY).is_none());
            assert_eq!(retained.root_names, ["empty"]);
        }
    }
}
