//! Replacement world transforms for individually selected, world-baked chunks.

use glam::{Mat4, Vec3};

use super::{retained::transformed_bounds, GltfDrawMesh, GltfModelTransform, RetainedGltfDrawMesh};
use crate::{gltf::GltfScene, math::Aabb, models::draw_mesh};

/// Rest world matrices indexed by the same scene-local node IDs as draw chunks.
///
/// Obtain this small sidecar through [`super::GltfModelLoader::take_pose`] to
/// reuse the loader's parse, or construct it from an existing CPU scene. Empty
/// nodes are included, so a spawn's authored world rotation remains available.
/// This metadata does not own geometry or propagate edits through a hierarchy.
/// Keep it with the model loaded from the same document and up-axis conversion;
/// a numeric node ID alone cannot identify a node in a different document.
///
/// ```
/// use macroquad::prelude::*;
/// let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"translation":[3,0,0]}]}"#).unwrap();
/// let poses = GltfModelPose::from_scene(&scene);
/// let pose = poses.node(0).unwrap();
/// assert_eq!(pose.rest_world().transform_point3(Vec3::ZERO), vec3(3., 0., 0.));
/// ```
#[derive(Clone, Debug)]
pub struct GltfModelPose {
    rest_world: Vec<Mat4>,
}

impl GltfModelPose {
    /// Copies node matrices without copying mesh arrays or requiring a window.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
    /// assert!(GltfModelPose::from_scene(&scene).rest_world(0).is_none());
    /// ```
    pub fn from_scene(scene: &GltfScene) -> Self {
        Self {
            rest_world: scene.nodes.iter().map(|node| node.world).collect(),
        }
    }

    /// Reads a node's authored world matrix, including an empty or singular node.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// # let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
    /// let rest = GltfModelPose::from_scene(&scene);
    /// assert!(rest.rest_world(usize::MAX).is_none());
    /// ```
    pub fn rest_world(&self, node: usize) -> Option<Mat4> {
        self.rest_world.get(node).copied()
    }

    /// Creates one editable replacement pose, initially exactly at rest.
    ///
    /// Missing nodes and non-finite, projective or non-invertible rest matrices
    /// return an error naming the node. A singular baked mesh cannot in general
    /// recover its lost local coordinates, so it cannot receive a replacement.
    /// Such nodes remain loadable and drawable through the existing APIs.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// # let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
    /// assert!(GltfModelPose::from_scene(&scene).node(42).is_err());
    /// ```
    pub fn node(&self, node: usize) -> Result<GltfNodePose, String> {
        let rest = self
            .rest_world(node)
            .ok_or_else(|| format!("glTF pose node {node} does not exist"))?;
        GltfNodePose::new(node, rest)
    }

    /// Rotates rest matrices with [`super::GltfModel::with_up_axis`]'s rule.
    ///
    /// Apply this to the original sidecar with the same `up` as the original
    /// model, before creating editable poses. Repeated calls compose rotations,
    /// as for the model. Invalid/zero axes and positive Y leave a copy unchanged.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// # let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"translation":[0,2,0]}]}"#).unwrap();
    /// let rest = GltfModelPose::from_scene(&scene).with_up_axis(Vec3::Z);
    /// assert!(rest.rest_world(0).unwrap().transform_point3(Vec3::ZERO).abs_diff_eq(Vec3::Z * 2., 1e-5));
    /// ```
    pub fn with_up_axis(&self, up: Vec3) -> Self {
        let Some(rotation) = crate::gltf::up_axis_rotation(up) else {
            return self.clone();
        };
        let rotation = Mat4::from_quat(rotation);
        Self {
            rest_world: self
                .rest_world
                .iter()
                .map(|world| rotation * *world)
                .collect(),
        }
    }
}

/// One node's replacement world transform and its immutable authored rest pose.
///
/// glTF drawing stores positions in scene world space. This pose computes
/// `world * inverse(rest_world)` once when changed, allowing selected chunks
/// to reuse that geometry. Replacing `T(pivot) * R_rest` with
/// `T(new_position) * R_new` rotates around the authored node origin, including a non-origin
/// pivot. Assign a spawn node's full world matrix to retain its yaw.
///
/// This edits only selected chunks with the same node ID; descendants,
/// colliders, markers and other chunks receive no automatic updates. Use the
/// material renderer's posed submission for automatic lighting, reflected
/// winding and transparency sorting. Custom materials follow
/// [`GltfModelTransform`] using the parent matrix times [`Self::transform`].
///
/// ```
/// use macroquad::prelude::*;
/// let mut pose = GltfNodePose::new(7, Mat4::from_translation(vec3(5., 0., 0.))).unwrap();
/// pose.set_world(Mat4::from_translation(vec3(2., 0., 0.)) * Mat4::from_rotation_y(1.)).unwrap();
/// assert!(pose.transform().transform_point3(vec3(5., 0., 0.)).abs_diff_eq(vec3(2., 0., 0.), 1e-5));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct GltfNodePose {
    node: usize,
    rest_world: Mat4,
    inverse_rest: Mat4,
    world: Mat4,
    transform: Mat4,
}

impl GltfNodePose {
    /// Creates a pose with a finite, affine, invertible authored world matrix.
    ///
    /// Numerically non-invertible matrices return an identified error, as in
    /// [`GltfModelTransform::new`]. Initial drawing uses exact identity.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let pose = GltfNodePose::new(3, Mat4::IDENTITY).unwrap();
    /// assert_eq!(pose.transform(), Mat4::IDENTITY);
    /// ```
    pub fn new(node: usize, rest_world: Mat4) -> Result<Self, String> {
        let mut inverse_rest = GltfModelTransform::new(rest_world)
            .ok_or_else(|| {
                format!(
                    "glTF pose node {node} rest world matrix must be finite, affine and invertible"
                )
            })?
            .normal_matrix
            .transpose();
        // The input has already been checked as exactly affine. Generic f32
        // inversion can round the inverse's homogeneous W away from one;
        // restore its known affine row before composing replacements. Keep
        // strict projective-input validation unchanged everywhere else.
        inverse_rest.x_axis.w = 0.0;
        inverse_rest.y_axis.w = 0.0;
        inverse_rest.z_axis.w = 0.0;
        inverse_rest.w_axis.w = 1.0;
        Ok(Self {
            node,
            rest_world,
            inverse_rest,
            world: rest_world,
            transform: Mat4::IDENTITY,
        })
    }

    /// Creates a selected pose directly from an existing CPU scene.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// # let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
    /// assert!(GltfNodePose::from_scene(&scene, 0).is_err());
    /// ```
    pub fn from_scene(scene: &GltfScene, node: usize) -> Result<Self, String> {
        let rest = scene
            .nodes
            .get(node)
            .ok_or_else(|| format!("glTF pose node {node} does not exist"))?;
        Self::new(node, rest.world)
    }

    /// Scene-local node ID; all chunks of that node may use this pose.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// assert_eq!(GltfNodePose::new(7, Mat4::IDENTITY).unwrap().node(), 7);
    /// ```
    pub fn node(&self) -> usize {
        self.node
    }

    /// Immutable authored world matrix, before the replacement.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// assert_eq!(GltfNodePose::new(0, Mat4::IDENTITY).unwrap().rest_world(), Mat4::IDENTITY);
    /// ```
    pub fn rest_world(&self) -> Mat4 {
        self.rest_world
    }

    /// Current replacement world matrix, before any drawing parent transform.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// assert_eq!(GltfNodePose::new(0, Mat4::IDENTITY).unwrap().world(), Mat4::IDENTITY);
    /// ```
    pub fn world(&self) -> Mat4 {
        self.world
    }

    /// Replaces the node world matrix without allocating or changing geometry.
    ///
    /// Rejects non-finite/projective matrices and overflow in the additional
    /// transform; errors leave this pose unchanged. Finite singular replacements
    /// support bounds and unlit drawing. The material renderer skips ordinary
    /// lit singular transforms because their normal matrix is undefined.
    /// Assign [`Self::rest_world`] to reset to exact identity drawing.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let mut pose = GltfNodePose::new(0, Mat4::IDENTITY).unwrap();
    /// pose.set_world(Mat4::from_translation(Vec3::Y)).unwrap();
    /// assert_eq!(pose.world().w_axis.truncate(), Vec3::Y);
    /// ```
    pub fn set_world(&mut self, world: Mat4) -> Result<(), String> {
        let transform = if world == self.rest_world {
            Mat4::IDENTITY
        } else {
            world * self.inverse_rest
        };
        if !super::retained::is_affine(world) || !super::retained::is_affine(transform) {
            return Err(format!("glTF pose node {} replacement world matrix and additional transform must be finite and affine", self.node));
        }
        self.world = world;
        self.transform = transform;
        Ok(())
    }

    /// Additional matrix applied to world-baked vertices: `world * inverse(rest)`.
    ///
    /// Pass this to existing shaded-part submission methods to reuse their map
    /// attributes. Include the drawing parent when computing custom normal
    /// matrices or calling ordinary `world_bounds` queries.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let mut pose = GltfNodePose::new(0, Mat4::from_translation(Vec3::X)).unwrap();
    /// pose.set_world(Mat4::from_translation(Vec3::Y)).unwrap();
    /// assert_eq!(pose.transform().transform_point3(Vec3::X), Vec3::Y);
    /// ```
    pub fn transform(&self) -> Mat4 {
        self.transform
    }

    pub(super) fn for_node(&self, node: usize) -> Result<Mat4, String> {
        if node != self.node {
            return Err(format!(
                "glTF pose node {} does not match chunk node {node}",
                self.node
            ));
        }
        Ok(self.transform)
    }
}

impl GltfDrawMesh {
    /// Draws this selected chunk at a replacement pose with the current material.
    ///
    /// No vertex vectors are cloned. CPU streaming uploads still apply. Custom
    /// normals and culling follow [`GltfModelTransform`]; use the material
    /// renderer for automatic lit normals and reflected face selection. Errors
    /// for a different node or invalid composed parent append no draw.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn draw(part: &GltfDrawMesh, pose: &GltfNodePose) -> Result<(), String> {
    /// part.draw_posed(pose)?;
    /// # Ok(()) }
    /// ```
    pub fn draw_posed(&self, pose: &GltfNodePose) -> Result<(), String> {
        let transform = pose.for_node(self.node)?;
        let context = crate::get_context();
        if !super::retained::is_affine(context.gl.model_matrix() * transform) {
            return Err(format!(
                "glTF pose node {} composed drawing transform must be finite and affine",
                self.node
            ));
        }
        context.gl.push_model_matrix(transform);
        draw_mesh(&self.mesh);
        crate::get_context().gl.pop_model_matrix();
        Ok(())
    }

    /// Conservative posed bounds including an explicit drawing parent.
    ///
    /// Pure query, transforming all eight stored bound corners. Metadata must
    /// describe application-edited geometry. A mismatched node, invalid parent,
    /// bounds or overflow returns an error; no model stack is read.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn query(part: &GltfDrawMesh, pose: &GltfNodePose) -> Result<(), String> {
    /// let bounds = part.posed_bounds(pose, Mat4::IDENTITY)?;
    /// # Ok(()) }
    /// ```
    pub fn posed_bounds(&self, pose: &GltfNodePose, parent: Mat4) -> Result<Aabb, String> {
        posed_bounds(self.node, self.bounds_min, self.bounds_max, pose, parent)
    }
}

impl RetainedGltfDrawMesh {
    /// Draws one replacement pose using its existing shared GPU buffers.
    ///
    /// Uses the current material. Lighting and reflected culling follow
    /// [`GltfModelTransform`]; the material renderer's posed submission handles
    /// them automatically. A different node or invalid composed parent returns
    /// an error without drawing. No geometry is cloned or uploaded.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn draw(part: &RetainedGltfDrawMesh, pose: &GltfNodePose) -> Result<(), String> {
    /// part.draw_posed(pose)?;
    /// # Ok(()) }
    /// ```
    pub fn draw_posed(&self, pose: &GltfNodePose) -> Result<(), String> {
        let transform = pose.for_node(self.node)?;
        if !super::retained::is_affine(crate::get_context().gl.model_matrix() * transform) {
            return Err(format!(
                "glTF pose node {} composed drawing transform must be finite and affine",
                self.node
            ));
        }
        self.draw(transform);
        Ok(())
    }

    /// Conservative replacement-pose bounds, including the supplied parent.
    ///
    /// This pure query has the same validation as [`GltfDrawMesh::posed_bounds`].
    /// Use its result for world-space frustum queries and application culling.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn query(part: &RetainedGltfDrawMesh, pose: &GltfNodePose) -> Result<(), String> {
    /// let bounds = part.posed_bounds(pose, Mat4::IDENTITY)?;
    /// # Ok(()) }
    /// ```
    pub fn posed_bounds(&self, pose: &GltfNodePose, parent: Mat4) -> Result<Aabb, String> {
        posed_bounds(self.node, self.bounds_min, self.bounds_max, pose, parent)
    }
}

fn posed_bounds(
    node: usize,
    min: Vec3,
    max: Vec3,
    pose: &GltfNodePose,
    parent: Mat4,
) -> Result<Aabb, String> {
    let transform = pose.for_node(node)?;
    if super::retained::is_affine(parent) {
        if let Some(bounds) = transformed_bounds(min, max, parent * transform) {
            return Ok(bounds);
        }
    }
    Err(format!("glTF pose node {node} bounds and parent transform must be finite and affine without overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Frustum;
    use glam::{vec3, Quat};

    fn near(a: Vec3, b: Vec3) {
        assert!(a.abs_diff_eq(b, 2e-5), "{a:?} != {b:?}");
    }

    #[test]
    fn replacement_rotates_about_authored_non_origin_pivot_and_uses_spawn_yaw() {
        let scene = GltfScene::from_bytes(br#"{
            "asset":{"version":"2.0"},"scenes":[{"nodes":[0,2]}],"nodes":[
                {"translation":[3,2,-1],"rotation":[0,0,0.7071067811865476,0.7071067811865476],"children":[1]},
                {"name":"raider","translation":[2,0,0],"scale":[2,1,0.5]},
                {"name":"player_spawn","translation":[-2,1,4],"rotation":[0,0.3420201433256687,0,0.9396926207859084]}
            ]}"#).unwrap();
        let rest = GltfModelPose::from_scene(&scene);
        let mut pose = rest.node(1).unwrap();
        let pivot = scene.nodes[1].world.transform_point3(Vec3::ZERO);
        near(pivot, vec3(3., 4., -1.));
        let spawn = scene.nodes[scene.spawns[0].node].world;
        pose.set_world(spawn).unwrap();
        near(pose.transform().transform_point3(pivot), vec3(-2., 1., 4.));
        let local_tip = vec3(0.25, 0., 0.);
        let baked_tip = scene.nodes[1].world.transform_point3(local_tip);
        let moved_tip = pose.transform().transform_point3(baked_tip);
        near(moved_tip, spawn.transform_point3(local_tip));
        let expected_yaw = Mat4::from_quat(Quat::from_rotation_y(40_f32.to_radians()));
        near(
            moved_tip - scene.spawns[0].position,
            expected_yaw.transform_vector3(local_tip),
        );
        // Only selected node metadata changed; ancestors, scene markers and rest stay authored.
        assert_eq!(pose.rest_world(), scene.nodes[1].world);
        assert_eq!(rest.rest_world(1), Some(scene.nodes[1].world));
        pose.set_world(pose.rest_world()).unwrap();
        assert_eq!(pose.transform(), Mat4::IDENTITY);
    }

    #[test]
    fn replacement_normals_and_winding_compose_rest_and_instance_reflections() {
        let local_normal = vec3(1., 2., 3.).normalize();
        let parent = Mat4::from_scale_rotation_translation(
            vec3(-0.5, 2., 1.),
            Quat::from_rotation_x(0.35),
            vec3(0., 1., -3.),
        );
        for rest_sign in [-1., 1.] {
            for world_sign in [-1., 1.] {
                let mut rest = Mat4::from_scale_rotation_translation(
                    vec3(rest_sign * 2., 1., 0.5),
                    Quat::from_rotation_z(0.7),
                    vec3(3., 4., -1.),
                );
                rest.y_axis += rest.x_axis * 0.3;
                let world = Mat4::from_scale_rotation_translation(
                    vec3(world_sign, 0.75, 3.),
                    Quat::from_rotation_y(0.9),
                    vec3(-2., 1., 5.),
                );
                let mut pose = GltfNodePose::new(5, rest).unwrap();
                pose.set_world(world).unwrap();
                let baked_normal = rest
                    .inverse()
                    .transpose()
                    .transform_vector3(local_normal)
                    .normalize();
                let moved = GltfModelTransform::new(parent * pose.transform()).unwrap();
                let expected = GltfModelTransform::new(parent * world).unwrap();
                near(
                    moved
                        .normal_matrix
                        .transform_vector3(baked_normal)
                        .normalize(),
                    expected
                        .normal_matrix
                        .transform_vector3(local_normal)
                        .normalize(),
                );
                // Loader corrected rest winding already; additional mirroring gives final orientation.
                let rest_mirror = rest.determinant() < 0.;
                assert_eq!(rest_mirror ^ moved.mirrored, expected.mirrored);
            }
        }
    }

    #[test]
    fn posed_bounds_follow_parent_shear_reflection_culling_and_singular_replacement() {
        let rest = Mat4::from_translation(vec3(8., 0., 0.));
        let mut pose = GltfNodePose::new(1, rest).unwrap();
        let min = vec3(7.8, -0.2, -0.2);
        let max = vec3(8.2, 0.2, 0.2);
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY).unwrap();
        assert!(
            !frustum.intersects_aabb(&posed_bounds(1, min, max, &pose, Mat4::IDENTITY).unwrap())
        );
        pose.set_world(Mat4::IDENTITY).unwrap();
        assert!(frustum.intersects_aabb(&posed_bounds(1, min, max, &pose, Mat4::IDENTITY).unwrap()));
        let mut parent = Mat4::from_scale_rotation_translation(
            vec3(-1., 2., 1.),
            Quat::from_rotation_y(0.7),
            vec3(1., -2., 0.),
        );
        parent.z_axis += parent.y_axis * 0.25;
        let bounds = posed_bounds(1, min, max, &pose, parent).unwrap();
        for x in 0..=2 {
            for y in 0..=2 {
                for z in 0..=2 {
                    let baked = min + (max - min) * vec3(x as f32, y as f32, z as f32) * 0.5;
                    assert!(bounds
                        .expanded(1e-5)
                        .contains((parent * pose.transform()).transform_point3(baked)));
                }
            }
        }
        pose.set_world(Mat4::from_scale(vec3(0., 1., 1.))).unwrap();
        let flat = posed_bounds(1, min, max, &pose, Mat4::IDENTITY).unwrap();
        assert_eq!(flat.min.x, 0.);
        assert_eq!(flat.max.x, 0.);
        assert!(GltfModelTransform::new(pose.transform()).is_none());
        assert!(posed_bounds(2, min, max, &pose, parent)
            .unwrap_err()
            .contains("node 2"));
        assert!(posed_bounds(1, max, min, &pose, parent).is_err());
        assert!(posed_bounds(1, min, max, &pose, Mat4::NAN).is_err());
        assert!(posed_bounds(1, min, max, &pose, Mat4::ZERO).is_err());
        assert!(posed_bounds(
            1,
            Vec3::splat(f32::MAX),
            Vec3::splat(f32::MAX),
            &pose,
            parent
        )
        .is_err());
    }

    #[test]
    fn valid_affine_rest_inverse_roundoff_keeps_replacements_affine() {
        let angle = 0.527;
        let ordinary_trs = Mat4::from_scale_rotation_translation(
            vec3(1.700527, 0.8, 0.4),
            Quat::from_rotation_z(angle)
                * Quat::from_rotation_x(0.31)
                * Quat::from_rotation_y(0.1581),
            vec3(3., 2., 1.),
        );
        let sheared = Mat4::from_cols_array(&[
            1.0135943,
            1.3057554,
            -0.4005783,
            0.,
            -0.005645871,
            1.2314442,
            0.023482725,
            0.,
            0.1540282,
            -0.0063322843,
            0.36910054,
            0.,
            3.,
            2.,
            1.,
            1.,
        ]);
        // Generic f32 inversion can produce homogeneous W = 0.99999994 for
        // either exact affine source, even though the inverse is affine.
        for rest in [ordinary_trs, sheared] {
            assert_eq!(rest.row(3), glam::Vec4::W);
            let mut pose = GltfNodePose::new(6, rest).unwrap();
            let replacement = Mat4::from_translation(Vec3::Y);
            pose.set_world(replacement).unwrap();
            assert_eq!(pose.transform().row(3), glam::Vec4::W);
            for local in [Vec3::ZERO, Vec3::X, vec3(-0.3, 0.7, 1.2)] {
                near(
                    pose.transform()
                        .transform_point3(rest.transform_point3(local)),
                    replacement.transform_point3(local),
                );
            }
            pose.set_world(pose.rest_world()).unwrap();
            assert_eq!(pose.transform(), Mat4::IDENTITY);
        }
    }

    #[test]
    fn invalid_pose_updates_are_atomic_and_identify_the_node() {
        for invalid in [
            Mat4::NAN,
            Mat4::ZERO,
            Mat4::from_scale(Vec3::ZERO),
            Mat4::perspective_rh_gl(1., 1., 0.1, 10.),
        ] {
            assert!(GltfNodePose::new(9, invalid)
                .unwrap_err()
                .contains("node 9"));
        }
        let mut pose = GltfNodePose::new(9, Mat4::from_translation(Vec3::X)).unwrap();
        pose.set_world(Mat4::from_translation(Vec3::Z)).unwrap();
        let before = pose;
        for invalid in [
            Mat4::NAN,
            Mat4::ZERO,
            Mat4::perspective_rh_gl(1., 1., 0.1, 10.),
        ] {
            assert!(pose.set_world(invalid).unwrap_err().contains("node 9"));
            assert_eq!(pose.world(), before.world());
            assert_eq!(pose.transform(), before.transform());
        }
        assert!(pose.for_node(3).unwrap_err().contains("chunk node 3"));
        let mut large = GltfNodePose::new(9, Mat4::from_scale(vec3(0.01, 1., 1.))).unwrap();
        assert!(large
            .set_world(Mat4::from_scale(vec3(f32::MAX, 1., 1.)))
            .is_err());
        assert_eq!(large.transform(), Mat4::IDENTITY);
    }

    #[test]
    fn rest_sidecar_matches_scene_up_axis_and_preserves_singular_node_metadata() {
        let scene = GltfScene::from_bytes(
            br#"{
            "asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[
                {"translation":[0,3,0],"children":[1]},
                {"translation":[1,2,3],"scale":[0,1,1]}
            ]}"#,
        )
        .unwrap();
        let original = GltfModelPose::from_scene(&scene);
        assert!(original.node(1).unwrap_err().contains("node 1"));
        assert_eq!(original.rest_world(1), Some(scene.nodes[1].world));
        for up in [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            -Vec3::Y,
            vec3(1., 2., 3.),
            Vec3::ZERO,
            Vec3::NAN,
        ] {
            let turned = original.with_up_axis(up);
            let expected = scene.with_up_axis(up);
            for (node, expected) in expected.nodes.iter().enumerate() {
                assert!(turned
                    .rest_world(node)
                    .unwrap()
                    .abs_diff_eq(expected.world, 1e-5));
            }
            assert!(turned.node(0).is_ok());
            assert!(turned.node(1).is_err());
        }
        assert!(original.node(2).unwrap_err().contains("node 2"));
    }
}
