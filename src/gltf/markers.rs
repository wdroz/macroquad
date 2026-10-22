//! Static markers and their metadata, without a retained CPU scene.

use glam::{Mat4, Quat, Vec3};

use super::{rotate_shape, up_axis_rotation, GltfCollider, GltfScene, GltfSpawn};

/// Owned imported markers with no geometry, decoded images or node hierarchy.
///
/// Opt into [`crate::gltf_draw::GltfModelLoader::new_with_markers`] to reuse its
/// existing scene parse, then call its once-only `take_markers` at completion.
/// Alternatively, copy this metadata from an existing scene with
/// [`Self::from_scene`]. Colliders and spawns keep their original preorder and
/// scene-local node indices, including duplicate names. Tags and rest world
/// matrices cover every loaded node, including empty or tag-only nodes; no
/// parent links, local transforms, mesh arrays or texture handles are retained.
///
/// This is a static snapshot. Model instance transforms, hierarchy edits and
/// animations do not move it. Keep it with the model from the same document
/// and apply the same [`Self::with_up_axis`] conversion to both results.
/// Numeric node indices alone cannot identify a node in a different document.
///
/// ```
/// use macroquad::prelude::*;
/// let scene = GltfScene::from_bytes(br#"{
///   "asset":{"version":"2.0"},"scenes":[{"nodes":[0,1]}],"nodes":[
///     {"name":"player_spawn","extras":{"mq_tag":"player"}},
///     {"name":"goal_trigger","extras":{"mq_shape":"sphere"}}
///   ]}"#).unwrap();
/// let markers = GltfMarkers::from_scene(&scene);
/// drop(scene);
/// assert_eq!(markers.marker_tag(markers.spawns[0].node), Some("player"));
/// assert_eq!(markers.trigger_colliders().count(), 1);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GltfMarkers {
    /// Imported colliders and triggers, in original node preorder.
    pub colliders: Vec<GltfCollider>,
    /// Imported spawn points, in original node preorder.
    pub spawns: Vec<GltfSpawn>,
    rest_world: Vec<Mat4>,
    tags: Vec<Option<String>>,
}

impl GltfMarkers {
    /// Copies markers, tags and world matrices without copying meshes or images.
    ///
    /// The source scene is unchanged. Node indices and checked metadata lookups
    /// match that scene, even when a complete caller-written scene literal has
    /// missing tag entries. This operation needs no window.
    pub fn from_scene(scene: &GltfScene) -> Self {
        Self {
            colliders: scene.colliders.clone(),
            spawns: scene.spawns.clone(),
            rest_world: scene.nodes.iter().map(|node| node.world).collect(),
            tags: scene.marker_tags.clone(),
        }
    }

    pub(crate) fn take_from_scene(scene: &mut GltfScene) -> Self {
        Self {
            colliders: std::mem::take(&mut scene.colliders),
            spawns: std::mem::take(&mut scene.spawns),
            rest_world: scene.nodes.iter().map(|node| node.world).collect(),
            tags: std::mem::take(&mut scene.marker_tags),
        }
    }

    /// Borrows solids (`trigger == false`) in their original stored order.
    ///
    /// Lazy and allocation-free, with the same original references and
    /// scene-local node indices as [`GltfScene::solid_colliders`]. Enumerating
    /// the filtered iterator gives filtered positions, not collider indices.
    pub fn solid_colliders(&self) -> impl Iterator<Item = &GltfCollider> {
        self.colliders.iter().filter(|collider| !collider.trigger)
    }

    /// Borrows triggers (`trigger == true`) in their original stored order.
    ///
    /// This allocation-free filter has no movement, event or response policy,
    /// matching [`GltfScene::trigger_colliders`].
    pub fn trigger_colliders(&self) -> impl Iterator<Item = &GltfCollider> {
        self.colliders.iter().filter(|collider| collider.trigger)
    }

    /// Looks up a string `mq_tag` using the original scene-local node index.
    ///
    /// Includes tag-only nodes; an invalid index or absent/non-string tag is
    /// `None`. Empty strings remain `Some("")`, as on [`GltfScene::marker_tag`].
    pub fn marker_tag(&self, node: usize) -> Option<&str> {
        self.rest_world.get(node)?;
        self.tags.get(node)?.as_deref()
    }

    /// Reads the stored world matrix at an original scene-local node index.
    ///
    /// Includes parent transforms and any [`Self::with_up_axis`] conversion.
    /// Singular or non-finite matrices are preserved exactly; a missing node
    /// returns `None`. This does not read or edit a separate hierarchy pose.
    /// Use [`Self::marker_rotation`] for checked facing instead of assuming a
    /// spawn's parent is unrotated; [`GltfSpawn::position`] stores only position.
    pub fn rest_world(&self, node: usize) -> Option<Mat4> {
        self.rest_world.get(node).copied()
    }

    /// Reads rest-world facing with exactly [`GltfScene::marker_rotation`] rules.
    ///
    /// Includes parents and up-axis conversion. Matrix decomposition assigns a
    /// reflection to X scale and approximates shear. Missing nodes, singular
    /// or non-finite worlds and unnormalizable rotations return `None`.
    pub fn marker_rotation(&self, node: usize) -> Option<Quat> {
        world_rotation(self.rest_world(node)?)
    }

    /// Rotates glTF +Y onto `up`, matching [`GltfScene::with_up_axis`].
    ///
    /// The returned snapshot rotates every stored world matrix, collider shape
    /// and spawn position. Names, tags, order and node indices are unchanged.
    /// A positive Y direction, zero or non-finite direction returns an equal
    /// clone. Apply the same conversion to the model and any pose sidecar;
    /// converting one result never changes another. No camera is modified.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let scene = GltfScene::from_bytes(br#"{"asset":{"version":"2.0"},
    ///   "scenes":[{"nodes":[0]}],"nodes":[{"name":"player_spawn",
    ///   "translation":[0,2,0]}]}"#).unwrap();
    /// let markers = GltfMarkers::from_scene(&scene).with_up_axis(Vec3::Z);
    /// assert!((markers.spawns[0].position - vec3(0., 0., 2.)).length() < 1e-5);
    /// ```
    pub fn with_up_axis(&self, up: Vec3) -> Self {
        let mut markers = self.clone();
        let Some(rotation) = up_axis_rotation(up) else {
            return markers;
        };
        let basis = Mat4::from_quat(rotation);
        for world in &mut markers.rest_world {
            *world = basis * *world;
        }
        for collider in &mut markers.colliders {
            collider.shape = rotate_shape(rotation, collider.shape);
        }
        for spawn in &mut markers.spawns {
            spawn.position = rotation * spawn.position;
        }
        markers
    }
}

pub(super) fn world_rotation(world: Mat4) -> Option<Quat> {
    let determinant = world.determinant();
    if !world.is_finite() || determinant == 0.0 {
        return None;
    }
    let (_, rotation, _) = world.to_scale_rotation_translation();
    let length = rotation.length();
    if rotation.is_finite() && length.is_finite() && length > 0.0 {
        Some(rotation / length)
    } else {
        None
    }
}
