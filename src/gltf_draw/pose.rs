//! Replacement world transforms and editable hierarchies for world-baked chunks.

use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use super::{retained::transformed_bounds, GltfDrawMesh, GltfModelTransform, RetainedGltfDrawMesh};
use crate::{gltf::GltfScene, math::Aabb, models::draw_mesh};

/// Rest world matrices indexed by the same scene-local node IDs as draw chunks.
///
/// Obtain this small sidecar through [`super::GltfModelLoader::take_pose`] to
/// reuse the loader's parse, or construct it from an existing CPU scene. Empty
/// nodes are included, so a spawn's authored world rotation remains available.
/// This metadata does not own geometry. [`Self::hierarchy`] creates independent
/// local transforms and propagates edits through descendants.
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
    rest: Arc<GltfPoseRest>,
}

#[derive(Debug)]
struct GltfPoseRest {
    rest_world: Vec<Mat4>,
    rest_local: Vec<Option<Mat4>>,
    parents: Vec<Option<usize>>,
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
        // GltfNode predates this sidecar and keeps decomposed TRS plus an exact
        // world matrix. Prefer authored TRS when it reproduces that world;
        // otherwise reconstruct the local matrix through the parent inverse.
        // A singular parent can lose information irretrievably. Mark that local
        // unavailable rather than silently replacing authored shear with TRS.
        let rest_local = scene
            .nodes
            .iter()
            .map(|node| {
                let Some(parent) = node.parent else {
                    return Some(node.world);
                };
                let parent = scene.nodes.get(parent)?;
                // Even apparently matching TRS may hide authored shear along
                // a collapsed parent axis. Without source locals we cannot
                // safely distinguish that case from a true TRS child.
                let inverse = affine_inverse(parent.world)?;
                let trs = Mat4::from_scale_rotation_translation(
                    node.scale,
                    node.rotation,
                    node.translation,
                );
                if parent.world * trs == node.world {
                    Some(trs)
                } else {
                    Some(inverse * node.world)
                }
            })
            .collect();
        Self {
            rest: Arc::new(GltfPoseRest {
                rest_world: scene.nodes.iter().map(|node| node.world).collect(),
                rest_local,
                parents: scene.nodes.iter().map(|node| node.parent).collect(),
            }),
        }
    }

    pub(super) fn from_document(scene: &GltfScene, document: &gltf::Document) -> Self {
        // Reuse the validated parse, in precisely the scene loader's preorder.
        // Unlike reconstruction from GltfScene, this preserves matrix locals
        // even under a singular parent, without adding fields to GltfNode.
        let mut pending = document
            .default_scene()
            .or_else(|| document.scenes().next())
            .map(|scene| scene.nodes().collect::<Vec<_>>())
            .unwrap_or_default();
        pending.reverse();
        let mut rest_local = Vec::with_capacity(scene.nodes.len());
        while let Some(node) = pending.pop() {
            rest_local.push(Some(crate::gltf::local_transform(&node).3));
            pending.extend(node.children().collect::<Vec<_>>().into_iter().rev());
        }
        debug_assert_eq!(rest_local.len(), scene.nodes.len());
        Self {
            rest: Arc::new(GltfPoseRest {
                rest_world: scene.nodes.iter().map(|node| node.world).collect(),
                rest_local,
                parents: scene.nodes.iter().map(|node| node.parent).collect(),
            }),
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
        self.rest.rest_world.get(node).copied()
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
            rest: Arc::new(GltfPoseRest {
                rest_world: self
                    .rest
                    .rest_world
                    .iter()
                    .map(|world| rotation * *world)
                    .collect(),
                rest_local: self
                    .rest
                    .rest_local
                    .iter()
                    .zip(&self.rest.parents)
                    .map(|(local, parent)| {
                        local.map(|local| {
                            if parent.is_none() {
                                rotation * local
                            } else {
                                local
                            }
                        })
                    })
                    .collect(),
                parents: self.rest.parents.clone(),
            }),
        }
    }

    /// Stable scene-local handle, using the node IDs stored on draw chunks.
    ///
    /// Handles remain valid across edits, reset and instance clones. Keep them
    /// with the same source model; handles do not identify a different document.
    /// ```
    /// use macroquad::prelude::*;
    /// # let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
    /// assert!(GltfModelPose::from_scene(&scene).handle(0).is_none());
    /// ```
    pub fn handle(&self, node: usize) -> Option<GltfNodeHandle> {
        self.rest.rest_world.get(node).map(|_| GltfNodeHandle(node))
    }

    /// Rest local matrix when available, including authored shear.
    ///
    /// Loader sidecars retain every source local. [`Self::from_scene`] can only
    /// reconstruct a non-TRS child through an invertible parent's inverse,
    /// which can introduce floating-point roundoff. A singular parent's lost
    /// information makes every child of that parent return `None`, as does a
    /// missing node. Even a matching decomposed TRS cannot prove that a
    /// singular parent did not erase source shear.
    /// ```
    /// use macroquad::prelude::*;
    /// # let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
    /// assert!(GltfModelPose::from_scene(&scene).rest_local(0).is_none());
    /// ```
    pub fn rest_local(&self, node: usize) -> Option<Mat4> {
        self.rest.rest_local.get(node).copied().flatten()
    }

    /// Creates independent editable locals/worlds sharing immutable rest data.
    ///
    /// Validates finite affine locals/worlds and preorder parent links. Singular
    /// locals are allowed; their world can be queried, but a singular rest world
    /// cannot provide [`GltfHierarchyPose::node_pose`] for world-baked drawing.
    /// Children of singular parents in [`Self::from_scene`] have unavailable
    /// local information and return an identified error; use a loader sidecar
    /// to retain exact source matrices.
    /// ```
    /// use macroquad::prelude::*;
    /// let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{}]}"#).unwrap();
    /// let rest = GltfModelPose::from_scene(&scene);
    /// let mut instance = rest.hierarchy().unwrap();
    /// let node = instance.handle(0).unwrap();
    /// instance.set_local(node, GltfLocalTransform { translation: Vec3::X, ..Default::default() }).unwrap();
    /// assert_eq!(instance.world(node).unwrap().w_axis.truncate(), Vec3::X);
    /// instance.reset();
    /// assert_eq!(instance.node_pose(node).unwrap().transform(), Mat4::IDENTITY);
    /// ```
    pub fn hierarchy(&self) -> Result<GltfHierarchyPose, String> {
        let mut local = Vec::with_capacity(self.rest.rest_world.len());
        let mut poses = Vec::with_capacity(self.rest.rest_world.len());
        for (node, &world) in self.rest.rest_world.iter().enumerate() {
            if self.rest.parents[node].is_some_and(|parent| parent >= node) {
                return Err(format!(
                    "glTF hierarchy node {node} parent must precede its child"
                ));
            }
            let matrix = self.rest_local(node).ok_or_else(|| format!("glTF hierarchy node {node} exact local matrix is unavailable under its singular parent; use the loader pose sidecar"))?;
            if !super::retained::is_affine(matrix) || !super::retained::is_affine(world) {
                return Err(format!(
                    "glTF hierarchy node {node} rest matrices must be finite and affine"
                ));
            }
            local.push(matrix);
            poses.push(GltfNodePose::new(node, world).ok());
        }
        Ok(GltfHierarchyPose {
            rest: self.rest.clone(),
            local,
            world: self.rest.rest_world.clone(),
            at_rest: vec![true; self.rest.rest_world.len()],
            poses,
        })
    }
}

fn affine_inverse(matrix: Mat4) -> Option<Mat4> {
    let mut inverse = GltfModelTransform::new(matrix)?.normal_matrix.transpose();
    // The validated input is affine; generic inversion can round this row.
    inverse.x_axis.w = 0.;
    inverse.y_axis.w = 0.;
    inverse.z_axis.w = 0.;
    inverse.w_axis.w = 1.;
    Some(inverse)
}

/// A stable index in one source scene, shared by all of its hierarchy instances.
///
/// ```
/// use macroquad::prelude::*;
/// # let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{}]}"#).unwrap();
/// assert_eq!(GltfModelPose::from_scene(&scene).handle(0).unwrap().index(), 0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GltfNodeHandle(usize);

impl GltfNodeHandle {
    /// Node index on [`GltfDrawMesh::node`] and [`GltfScene::nodes`].
    /// ```
    /// use macroquad::prelude::*;
    /// # fn index(handle: GltfNodeHandle) -> usize {
    /// handle.index()
    /// # }
    /// ```
    pub fn index(self) -> usize {
        self.0
    }
}

/// A local translation, unit quaternion rotation and signed scale.
///
/// Zero and negative scales are allowed. [`GltfHierarchyPose::set_local`]
/// validates finite fields and a quaternion whose squared length is within
/// `1e-4` of one, then normalizes it. Matrix-authored shear is retained until
/// explicitly replaced; use [`GltfHierarchyPose::set_local_matrix`] for shear.
/// ```
/// use macroquad::prelude::*;
/// let local = GltfLocalTransform { translation: Vec3::Y, ..Default::default() };
/// assert_eq!(local.matrix().w_axis.truncate(), Vec3::Y);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GltfLocalTransform {
    /// Translation relative to the parent (or scene axes for a root).
    pub translation: Vec3,
    /// Unit quaternion rotation relative to the parent.
    pub rotation: Quat,
    /// Signed scale on local axes; zero is permitted.
    pub scale: Vec3,
}

impl Default for GltfLocalTransform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl GltfLocalTransform {
    /// Composes `translation * rotation * scale` without validating fields.
    /// ```
    /// use macroquad::prelude::*;
    /// assert_eq!(GltfLocalTransform::default().matrix(), Mat4::IDENTITY);
    /// ```
    pub fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// An editable scene instance with independent locals and descendant worlds.
///
/// Create through [`GltfModelPose::hierarchy`]. Clones share immutable rest
/// metadata, and copy only pose state, never draw geometry. Edits recompute
/// descendant worlds immediately. Pass each part's [`Self::node_pose`] to the
/// existing posed bounds/draw/material APIs so bounds, culling and transparent
/// sorting use the same transform. Colliders, markers and animation are unchanged.
/// ```
/// use macroquad::prelude::*;
/// # let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
/// let first = GltfModelPose::from_scene(&scene).hierarchy().unwrap();
/// let mut second = first.clone();
/// second.reset();
/// ```
#[derive(Clone, Debug)]
pub struct GltfHierarchyPose {
    rest: Arc<GltfPoseRest>,
    local: Vec<Mat4>,
    world: Vec<Mat4>,
    at_rest: Vec<bool>,
    poses: Vec<Option<GltfNodePose>>,
}

impl GltfHierarchyPose {
    /// Looks up the stable handle associated with a chunk's node index.
    /// ```
    /// use macroquad::prelude::*;
    /// # fn find(instance: &GltfHierarchyPose) {
    /// assert!(instance.handle(usize::MAX).is_none());
    /// # }
    /// ```
    pub fn handle(&self, node: usize) -> Option<GltfNodeHandle> {
        self.world.get(node).map(|_| GltfNodeHandle(node))
    }

    fn checked(&self, node: GltfNodeHandle) -> Result<usize, String> {
        self.world
            .get(node.0)
            .map(|_| node.0)
            .ok_or_else(|| format!("glTF hierarchy node {} does not exist", node.0))
    }

    /// Reads the current local matrix, preserving any authored shear.
    /// ```
    /// use macroquad::prelude::*;
    /// # fn read(instance: &GltfHierarchyPose, node: GltfNodeHandle) {
    /// let local = instance.local(node).unwrap();
    /// # }
    /// ```
    pub fn local(&self, node: GltfNodeHandle) -> Result<Mat4, String> {
        Ok(self.local[self.checked(node)?])
    }

    /// Decomposes a non-singular TRS local; rejects shear or singular matrices.
    ///
    /// Reconstruction must match every matrix component within `1e-5` relative
    /// to that component's magnitude (with an absolute `1e-5` floor). Signed
    /// scale decomposition is not unique; the returned matrix is equivalent.
    /// Read [`Self::local`] for an exact matrix or explicitly replace the local
    /// with known TRS using [`Self::set_local`].
    /// ```
    /// use macroquad::prelude::*;
    /// # fn read(instance: &GltfHierarchyPose, node: GltfNodeHandle) {
    /// let trs = instance.local_trs(node);
    /// # }
    /// ```
    pub fn local_trs(&self, node: GltfNodeHandle) -> Result<GltfLocalTransform, String> {
        let matrix = self.local(node)?;
        if GltfModelTransform::new(matrix).is_none() {
            return Err(format!(
                "glTF hierarchy node {} local matrix is singular; read the exact matrix instead",
                node.0
            ));
        }
        let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
        let trs = GltfLocalTransform {
            translation,
            rotation,
            scale,
        };
        let rebuilt = trs.matrix();
        let matches = matrix
            .to_cols_array()
            .iter()
            .zip(rebuilt.to_cols_array())
            .all(|(&a, b)| (a - b).abs() <= 1e-5 * a.abs().max(1.));
        if !matches || !rotation.is_finite() || (rotation.length_squared() - 1.).abs() > 1e-4 {
            return Err(format!(
                "glTF hierarchy node {} local matrix contains shear; read the exact matrix instead",
                node.0
            ));
        }
        Ok(trs)
    }

    /// Reads the current world matrix before a drawing parent transform.
    /// ```
    /// use macroquad::prelude::*;
    /// # fn read(instance: &GltfHierarchyPose, node: GltfNodeHandle) {
    /// let world = instance.world(node).unwrap();
    /// # }
    /// ```
    pub fn world(&self, node: GltfNodeHandle) -> Result<Mat4, String> {
        Ok(self.world[self.checked(node)?])
    }

    /// Parent handle, or `None` for a scene root. Edits preserve these links.
    /// ```
    /// use macroquad::prelude::*;
    /// # fn read(instance: &GltfHierarchyPose, node: GltfNodeHandle) {
    /// let parent = instance.parent(node).unwrap();
    /// # }
    /// ```
    pub fn parent(&self, node: GltfNodeHandle) -> Result<Option<GltfNodeHandle>, String> {
        Ok(self.rest.parents[self.checked(node)?].map(GltfNodeHandle))
    }

    /// Captures this node's current pose for selected drawing and bounds.
    ///
    /// Captures are `Copy` and retain their current world after later edits.
    /// A singular rest world returns an error because baked geometry lost its
    /// local coordinates. It does not prevent hierarchy edits or world queries.
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn query(instance: &GltfHierarchyPose, part: &GltfDrawMesh) -> Result<(), String> {
    /// let node = instance.handle(part.node).unwrap();
    /// let pose = instance.node_pose(node)?;
    /// let bounds = part.posed_bounds(&pose, Mat4::IDENTITY)?;
    /// # Ok(()) }
    /// ```
    pub fn node_pose(&self, node: GltfNodeHandle) -> Result<GltfNodePose, String> {
        self.poses[self.checked(node)?].ok_or_else(|| format!("glTF hierarchy node {} rest world matrix is not invertible for world-baked drawing", node.0))
    }

    /// Replaces one local TRS and immediately recomputes descendants.
    ///
    /// Rejects non-finite values, invalid unit rotations and overflowing worlds
    /// or baked draw transforms atomically. Finite singular replacements are
    /// allowed, following [`GltfNodePose::set_world`]'s bounds/lit-draw rule.
    /// ```
    /// use macroquad::prelude::*;
    /// # fn move_node(instance: &mut GltfHierarchyPose, node: GltfNodeHandle) -> Result<(), String> {
    /// instance.set_local(node, GltfLocalTransform { translation: Vec3::Y, ..Default::default() })?;
    /// # Ok(()) }
    /// ```
    pub fn set_local(
        &mut self,
        node: GltfNodeHandle,
        mut local: GltfLocalTransform,
    ) -> Result<(), String> {
        self.checked(node)?;
        if !local.translation.is_finite()
            || !local.scale.is_finite()
            || !local.rotation.is_finite()
            || (local.rotation.length_squared() - 1.).abs() > 1e-4
        {
            return Err(format!(
                "glTF hierarchy node {} local TRS must be finite with a unit rotation",
                node.0
            ));
        }
        local.rotation = local.rotation.normalize();
        self.set_local_matrix(node, local.matrix())
    }

    /// Replaces a finite affine local matrix, including shear or zero scale.
    ///
    /// Descendant recomputation and draw-transform validation are atomic. No
    /// geometry changes, uploads or GPU access occur. Unrelated branches retain
    /// their exact worlds. Passing the rest local restores exact rest worlds
    /// when its ancestors and descendants also have rest locals.
    /// ```
    /// use macroquad::prelude::*;
    /// # fn move_node(instance: &mut GltfHierarchyPose, node: GltfNodeHandle) -> Result<(), String> {
    /// instance.set_local_matrix(node, Mat4::from_translation(Vec3::Y))?;
    /// # Ok(()) }
    /// ```
    pub fn set_local_matrix(&mut self, handle: GltfNodeHandle, matrix: Mat4) -> Result<(), String> {
        let node = self.checked(handle)?;
        if !super::retained::is_affine(matrix) {
            return Err(format!(
                "glTF hierarchy node {node} local matrix must be finite and affine"
            ));
        }
        if matrix == self.local[node] {
            return Ok(());
        }
        // Preorder means all affected ancestors are available before children.
        // Stage only pose state; failure leaves the whole instance unchanged.
        let mut pending: Vec<(usize, Mat4, bool, Option<GltfNodePose>)> = Vec::new();
        let mut affected = vec![None; self.world.len()];
        for index in node..self.world.len() {
            let parent = self.rest.parents[index];
            let parent_update = parent.and_then(|parent| affected[parent]);
            if index != node && parent_update.is_none() {
                continue;
            }
            let local = if index == node {
                matrix
            } else {
                self.local[index]
            };
            let parent_at_rest = parent
                .map(|parent| {
                    parent_update
                        .map(|position: usize| pending[position].2)
                        .unwrap_or(self.at_rest[parent])
                })
                .unwrap_or(true);
            let at_rest = parent_at_rest && Some(local) == self.rest.rest_local[index];
            let world = if at_rest {
                self.rest.rest_world[index]
            } else {
                let parent_world = parent
                    .map(|parent| {
                        parent_update
                            .map(|position| pending[position].1)
                            .unwrap_or(self.world[parent])
                    })
                    .unwrap_or(Mat4::IDENTITY);
                parent_world * local
            };
            if !super::retained::is_affine(world) {
                return Err(format!(
                    "glTF hierarchy node {index} recomputed world matrix must be finite and affine"
                ));
            }
            let mut pose = self.poses[index];
            if let Some(pose) = &mut pose {
                pose.set_world(world)?;
            }
            affected[index] = Some(pending.len());
            pending.push((index, world, at_rest, pose));
        }
        self.local[node] = matrix;
        for (index, world, at_rest, pose) in pending {
            self.world[index] = world;
            self.at_rest[index] = at_rest;
            self.poses[index] = pose;
        }
        Ok(())
    }

    /// Restores one node's authored local, preserving other local edits.
    ///
    /// Descendants recompute with their current locals. An edited ancestor can
    /// keep this node away from its rest world. Overflow errors are atomic.
    /// ```
    /// use macroquad::prelude::*;
    /// # fn reset_node(instance: &mut GltfHierarchyPose, node: GltfNodeHandle) -> Result<(), String> {
    /// instance.reset_local(node)?;
    /// # Ok(()) }
    /// ```
    pub fn reset_local(&mut self, node: GltfNodeHandle) -> Result<(), String> {
        let index = self.checked(node)?;
        self.set_local_matrix(node, self.rest.rest_local[index].unwrap())
    }

    /// Restores every exact source local/world and identity drawing transform.
    /// ```
    /// use macroquad::prelude::*;
    /// # fn reset(instance: &mut GltfHierarchyPose) {
    /// instance.reset();
    /// # }
    /// ```
    pub fn reset(&mut self) {
        for node in 0..self.world.len() {
            self.local[node] = self.rest.rest_local[node].unwrap();
            self.world[node] = self.rest.rest_world[node];
            self.at_rest[node] = true;
            if let Some(pose) = &mut self.poses[node] {
                pose.world = pose.rest_world;
                pose.transform = Mat4::IDENTITY;
            }
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

    fn source_pose(bytes: &[u8]) -> (GltfScene, GltfModelPose) {
        let scene = GltfScene::from_bytes(bytes).unwrap();
        let parsed = gltf::Gltf::from_slice(bytes).unwrap();
        let rest = GltfModelPose::from_document(&scene, &parsed.document);
        (scene, rest)
    }

    #[test]
    fn hierarchy_propagates_parent_and_child_edits_and_preserves_independent_rest() {
        let (scene, rest) = source_pose(
            br#"{
            "asset":{"version":"2.0"},"scenes":[{"nodes":[0,4]}],"nodes":[
                {"translation":[3,2,-1],"scale":[2,1,0.5],"children":[1,3]},
                {"translation":[1,0,0],"children":[2]},
                {"translation":[0,2,1]}, {"translation":[-2,0,0]},
                {"translation":[-5,0,0]}
            ]}"#,
        );
        let mut first = rest.hierarchy().unwrap();
        let second = first.clone();
        assert!(Arc::ptr_eq(&first.rest, &second.rest));
        let parent = first.handle(0).unwrap();
        let blind = first.handle(1).unwrap();
        let prop = first.handle(2).unwrap();
        assert_eq!(first.parent(parent).unwrap(), None);
        assert_eq!(first.parent(prop).unwrap(), Some(blind));
        let captured = first.node_pose(prop).unwrap();
        let parent_local = GltfLocalTransform {
            translation: vec3(-2., 1., 3.),
            rotation: Quat::from_rotation_y(0.7),
            scale: vec3(-2., 3., 0.4),
        };
        first.set_local(parent, parent_local).unwrap();
        let mut blind_local = first.local_trs(blind).unwrap();
        blind_local.rotation = Quat::from_rotation_z(0.9);
        first.set_local(blind, blind_local).unwrap();
        let expected = parent_local.matrix() * blind_local.matrix() * rest.rest_local(2).unwrap();
        assert!(first.world(prop).unwrap().abs_diff_eq(expected, 2e-5));
        assert!(first
            .node_pose(prop)
            .unwrap()
            .world()
            .abs_diff_eq(expected, 2e-5));
        // Nonuniform parent + rotated child produces a sheared world, not a
        // decomposed approximation, and siblings follow only their own chain.
        assert_eq!(
            first.world(first.handle(3).unwrap()).unwrap(),
            parent_local.matrix() * rest.rest_local(3).unwrap()
        );
        assert_eq!(
            first.world(first.handle(4).unwrap()).unwrap(),
            scene.nodes[4].world
        );
        assert_eq!(captured.world(), scene.nodes[2].world);
        for (index, node) in scene.nodes.iter().enumerate() {
            assert_eq!(
                second.world(second.handle(index).unwrap()).unwrap(),
                node.world
            );
            assert_eq!(rest.rest_world(index).unwrap(), node.world);
        }
        first.reset_local(parent).unwrap();
        assert_ne!(first.node_pose(prop).unwrap().transform(), Mat4::IDENTITY);
        first.reset_local(blind).unwrap();
        assert_eq!(first.node_pose(prop).unwrap().transform(), Mat4::IDENTITY);
        first.set_local(parent, parent_local).unwrap();
        first.reset();
        for index in 0..scene.nodes.len() {
            assert_eq!(
                first
                    .node_pose(first.handle(index).unwrap())
                    .unwrap()
                    .transform(),
                Mat4::IDENTITY
            );
            assert_eq!(
                first.local(first.handle(index).unwrap()).unwrap(),
                rest.rest_local(index).unwrap()
            );
        }
    }

    #[test]
    fn hierarchy_up_axis_rotates_roots_once_and_keeps_descendant_locals() {
        let (_, rest) = source_pose(br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[
            {"translation":[1,3,2],"rotation":[0,0,0.25881904510252074,0.9659258262890683],"children":[1]},
            {"translation":[1,2,3],"scale":[2,1,0.5]}
        ]}"#);
        for up in [Vec3::Y, Vec3::Z, -Vec3::Y, vec3(1., 2., 3.)] {
            let converted = rest.with_up_axis(up);
            assert_eq!(converted.rest_local(1), rest.rest_local(1));
            let basis = crate::gltf::up_axis_rotation(up)
                .map(Mat4::from_quat)
                .unwrap_or(Mat4::IDENTITY);
            assert_eq!(
                converted.rest_local(0).unwrap(),
                basis * rest.rest_local(0).unwrap()
            );
            let mut normal = rest.hierarchy().unwrap();
            let mut turned = converted.hierarchy().unwrap();
            let child = normal.handle(1).unwrap();
            let local = GltfLocalTransform {
                translation: vec3(-1., 4., 2.),
                rotation: Quat::from_rotation_x(0.5),
                scale: vec3(1., 2., 0.3),
            };
            normal.set_local(child, local).unwrap();
            turned.set_local(child, local).unwrap();
            assert!(turned
                .world(child)
                .unwrap()
                .abs_diff_eq(basis * normal.world(child).unwrap(), 2e-5));
            turned.reset();
            assert_eq!(
                turned.world(child).unwrap(),
                converted.rest_world(1).unwrap()
            );
            assert_eq!(turned.node_pose(child).unwrap().transform(), Mat4::IDENTITY);
            // Conversions compose only at roots, including repeated calls.
            let twice = converted.with_up_axis(Vec3::Z);
            assert_eq!(twice.rest_local(1), rest.rest_local(1));
        }
    }

    #[test]
    fn hierarchy_preserves_matrix_shear_and_reports_trs_and_singular_limits() {
        let (_, rest) = source_pose(
            br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[
            {"translation":[3,0,0],"children":[1]},
            {"matrix":[1,0,0,0,0.5,1,0,0,0,0,1,0,2,3,0,1]}
        ]}"#,
        );
        let mut instance = rest.hierarchy().unwrap();
        let root = instance.handle(0).unwrap();
        let child = instance.handle(1).unwrap();
        assert!(instance.local_trs(child).unwrap_err().contains("shear"));
        let source_local = instance.local(child).unwrap();
        instance
            .set_local(
                root,
                GltfLocalTransform {
                    rotation: Quat::from_rotation_z(0.4),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(instance.local(child).unwrap(), source_local);
        assert_eq!(
            instance.world(child).unwrap(),
            instance.world(root).unwrap() * source_local
        );
        instance
            .set_local_matrix(child, Mat4::from_scale(vec3(0., 1., 1.)))
            .unwrap();
        assert!(instance.local_trs(child).unwrap_err().contains("singular"));
        assert!(instance.node_pose(child).is_ok());
        assert!(GltfModelTransform::new(instance.node_pose(child).unwrap().transform()).is_none());
        instance.reset();
        assert_eq!(instance.local(child).unwrap(), source_local);
        assert_eq!(
            instance.node_pose(child).unwrap().transform(),
            Mat4::IDENTITY
        );
    }

    #[test]
    fn hierarchy_loader_retains_locals_hidden_by_singular_parent() {
        // The parent's collapsed X axis erases the child's shear. An inverse
        // or equality against decomposed TRS cannot recover that information.
        let (scene, rest) = source_pose(
            br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[
            {"scale":[0,1,1],"children":[1]},
            {"matrix":[1,0,0,0,0.5,1,0,0,0,0,1,0,2,3,0,1]}
        ]}"#,
        );
        let reconstructed = GltfModelPose::from_scene(&scene);
        assert!(reconstructed.rest_local(1).is_none());
        assert!(reconstructed.hierarchy().unwrap_err().contains("node 1"));
        let mut instance = rest.hierarchy().unwrap();
        let parent = instance.handle(0).unwrap();
        let child = instance.handle(1).unwrap();
        assert!(instance.node_pose(parent).is_err());
        assert!(instance.node_pose(child).is_err());
        assert_eq!(instance.local(child).unwrap().y_axis.x, 0.5);
        instance
            .set_local(parent, GltfLocalTransform::default())
            .unwrap();
        assert_eq!(instance.world(child).unwrap(), rest.rest_local(1).unwrap());
        // Rest baked coordinates are still unrecoverable even after uncollapse.
        assert!(instance.node_pose(child).is_err());
        instance.reset();
        assert_eq!(instance.world(child).unwrap(), scene.nodes[1].world);
    }

    #[test]
    fn hierarchy_rejects_invalid_edits_and_descendant_overflow_atomically() {
        let (_, rest) = source_pose(
            br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[
            {"children":[1]}, {"translation":[10,0,0]}
        ]}"#,
        );
        let mut instance = rest.hierarchy().unwrap();
        let root = instance.handle(0).unwrap();
        for invalid in [
            Mat4::NAN,
            Mat4::ZERO,
            Mat4::perspective_rh_gl(1., 1., 0.1, 10.),
            Mat4::from_scale(vec3(f32::MAX, 1., 1.)),
        ] {
            let before = instance.clone();
            assert!(instance.set_local_matrix(root, invalid).is_err());
            assert_eq!(instance.local, before.local);
            assert_eq!(instance.world, before.world);
            assert_eq!(instance.at_rest, before.at_rest);
            for index in 0..2 {
                assert_eq!(
                    instance.poses[index].unwrap().transform(),
                    before.poses[index].unwrap().transform()
                );
            }
        }
        for invalid in [
            Quat::from_xyzw(0., 0., 0., 0.),
            Quat::from_xyzw(0., 0., 0., 2.),
            Quat::from_xyzw(f32::NAN, 0., 0., 1.),
        ] {
            assert!(instance
                .set_local(
                    root,
                    GltfLocalTransform {
                        rotation: invalid,
                        ..Default::default()
                    }
                )
                .unwrap_err()
                .contains("node 0"));
        }
        assert!(instance
            .set_local(
                root,
                GltfLocalTransform {
                    translation: Vec3::NAN,
                    ..Default::default()
                }
            )
            .is_err());
        assert!(instance
            .set_local(
                root,
                GltfLocalTransform {
                    scale: Vec3::INFINITY,
                    ..Default::default()
                }
            )
            .is_err());
        let missing = GltfNodeHandle(5);
        assert!(instance.world(missing).is_err());
        assert!(instance.local(missing).is_err());
        assert!(instance.parent(missing).is_err());
        assert!(instance.node_pose(missing).is_err());
        assert!(instance.reset_local(missing).is_err());
        assert!(instance.set_local_matrix(missing, Mat4::IDENTITY).is_err());
        // A finite singular replacement is supported; it is not Mat4::ZERO,
        // whose homogeneous row makes it invalid as an affine transform.
        instance
            .set_local(
                root,
                GltfLocalTransform {
                    scale: vec3(0., 1., 1.),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(instance.node_pose(instance.handle(1).unwrap()).is_ok());
    }

    #[test]
    fn hierarchy_rejects_invalid_public_scene_parents_without_panicking() {
        let (mut scene, _) = source_pose(br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"children":[1]},{}]}"#);
        scene.nodes[0].parent = Some(1);
        assert!(GltfModelPose::from_scene(&scene)
            .hierarchy()
            .unwrap_err()
            .contains("parent"));
        scene.nodes[0].parent = Some(usize::MAX);
        assert!(GltfModelPose::from_scene(&scene).hierarchy().is_err());
        scene.nodes[0].parent = None;
        scene.nodes[1].parent = Some(1);
        assert!(GltfModelPose::from_scene(&scene)
            .hierarchy()
            .unwrap_err()
            .contains("parent"));
        scene.nodes[1].parent = Some(0);
        scene.nodes[0].world = Mat4::NAN;
        assert!(GltfModelPose::from_scene(&scene).hierarchy().is_err());
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
