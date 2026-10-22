use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
    rc::Rc,
    sync::Arc,
};

use super::*;
use crate::gltf::GltfMarkers;
use crate::{math::Aabb, models::validate_retained_mesh};

/// Current operation in a pollable glTF load.
///
/// Each stage has its own work-unit count in [`GltfLoadProgress`].
/// ```
/// use macroquad::prelude::*;
/// let loader = GltfModelLoader::new(&[], GltfLoadSettings::default()).unwrap();
/// assert_eq!(loader.progress().stage, GltfLoadStage::Parse);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GltfLoadStage {
    /// Parse and validate the document and construct the CPU scene once.
    Parse,
    /// Check one consumed source image's metadata against optional limits.
    PreflightImages,
    /// Decode, resize and upload one texture, including its mipmaps.
    LoadTextures,
    /// Convert one node's primitive to triangle-safe CPU mesh chunks.
    ConvertMeshes,
    /// Validate one CPU chunk before any retained geometry upload.
    ValidateMeshes,
    /// Upload one retained mesh chunk.
    UploadMeshes,
    /// Construction succeeded. The result can be taken once.
    Complete,
    /// Cancellation released the handle's owned resources.
    Cancelled,
    /// An error released the handle's owned resources.
    Failed,
}

/// Work completed in the current stage, not elapsed time or overall percentage.
///
/// Counts reset when the stage changes. A poll completing the final unit leaves
/// `completed == total`; the next poll advances to the next stage without doing
/// its work. Empty stages are observable with both counts zero. Terminal stages
/// also have zero counts. This allows a caller to display and cancel every stage.
/// ```
/// use macroquad::prelude::*;
/// let loader = GltfModelLoader::new(&[], GltfLoadSettings::default()).unwrap();
/// assert_eq!(loader.progress().completed, 0);
/// assert_eq!(loader.progress().total, 1);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GltfLoadProgress {
    /// Operation that the next poll will continue or finish.
    pub stage: GltfLoadStage,
    /// Units already completed in this stage.
    pub completed: usize,
    /// Total units in this stage; zero for an empty or terminal stage.
    pub total: usize,
}

impl GltfLoadProgress {
    fn new(stage: GltfLoadStage, total: usize) -> Self {
        Self {
            stage,
            completed: 0,
            total,
        }
    }
}

struct TextureJob {
    texture: usize,
    context: String,
    /// Base-color or emissive. Shading-only textures stay out of model stats.
    model_texture: bool,
}

#[derive(Clone, Copy)]
struct PrimitiveJob {
    node: usize,
    mesh: usize,
    primitive: usize,
}

struct Prepared<'a> {
    scene: GltfScene,
    document: gltf::Document,
    binary: Cow<'a, [u8]>,
    source_meshes: Vec<usize>,
    texture_jobs: Vec<TextureJob>,
    image_jobs: Vec<usize>,
    primitive_jobs: Vec<PrimitiveJob>,
    mesh_uses: Vec<usize>,
    names: Vec<String>,
    paths: Vec<String>,
    textures: HashMap<usize, Texture2D>,
    material_parts: Vec<GltfPartMaterialMaps>,
    model: GltfModel,
    morph: Option<super::morph::MorphLoadData>,
}

impl<'a> Prepared<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self, String> {
        let (scene, document, binary) = prepare_glb(bytes)?;
        let source_meshes = crate::gltf::used_meshes(&document);
        let mut texture_jobs: Vec<TextureJob> = Vec::new();
        let mut image_jobs = Vec::new();
        let mut seen_textures = HashMap::<usize, usize>::new();
        let mut seen_images = HashSet::new();
        for mesh in &source_meshes {
            for primitive in mesh.primitives() {
                let material = primitive.material();
                let pbr = material.pbr_metallic_roughness();
                let slots = [
                    (
                        "base-color",
                        pbr.base_color_texture().map(|info| info.texture()),
                        true,
                    ),
                    (
                        "emissive",
                        material.emissive_texture().map(|info| info.texture()),
                        true,
                    ),
                    (
                        "normal",
                        material.normal_texture().map(|info| info.texture()),
                        false,
                    ),
                    (
                        "metallic-roughness",
                        pbr.metallic_roughness_texture().map(|info| info.texture()),
                        false,
                    ),
                ];
                for (slot, texture, model_texture) in slots {
                    let Some(texture) = texture else {
                        continue;
                    };
                    if let Some(job) = seen_textures.get(&texture.index()).copied() {
                        if model_texture {
                            texture_jobs[job].model_texture = true;
                        }
                        continue;
                    }
                    let image = texture.source();
                    if seen_images.insert(image.index()) {
                        image_jobs.push(texture_jobs.len());
                    }
                    seen_textures.insert(texture.index(), texture_jobs.len());
                    texture_jobs.push(TextureJob {
                        texture: texture.index(),
                        context: crate::gltf::image_context(&image, &material, slot),
                        model_texture,
                    });
                }
            }
        }
        let source_meshes = source_meshes.iter().map(|m| m.index()).collect();
        let mut primitive_jobs = Vec::new();
        let mut mesh_uses = vec![0; scene.meshes.len()];
        let mut names = Vec::with_capacity(scene.nodes.len());
        let mut paths: Vec<String> = Vec::with_capacity(scene.nodes.len());
        let mut root_names = Vec::new();
        for (node_index, node) in scene.nodes.iter().enumerate() {
            let mesh_name = node
                .mesh
                .and_then(|index| scene.meshes[index].name.as_deref());
            let name = node
                .name
                .as_deref()
                .or(mesh_name)
                .unwrap_or("unnamed")
                .to_string();
            let path = node.parent.map_or_else(
                || name.clone(),
                |parent| format!("{}/{}", paths[parent], name),
            );
            if node.parent.is_none() {
                root_names.push(name.clone());
            }
            names.push(name);
            paths.push(path);
            if let Some(mesh) = node.mesh {
                mesh_uses[mesh] += 1;
                for primitive in 0..scene.meshes[mesh].primitives.len() {
                    primitive_jobs.push(PrimitiveJob {
                        node: node_index,
                        mesh,
                        primitive,
                    });
                }
            }
        }
        let model = GltfModel {
            parts: Vec::new(),
            stats: GltfModelStats {
                nodes: scene.nodes.len(),
                meshes: scene.meshes.len(),
                ..Default::default()
            },
            bounds_min: Vec3::splat(f32::INFINITY),
            bounds_max: Vec3::splat(f32::NEG_INFINITY),
            root_names,
        };
        Ok(Self {
            scene,
            document,
            binary,
            source_meshes,
            texture_jobs,
            image_jobs,
            primitive_jobs,
            mesh_uses,
            names,
            paths,
            textures: HashMap::new(),
            material_parts: Vec::new(),
            model,
            morph: None,
        })
    }

    fn enable_morph(&mut self) -> Result<(), String> {
        let data = crate::gltf::GltfMorphData::from_document(&self.document, Some(&self.binary))
            .map_err(|error| error.to_string())?;
        let indices = self
            .scene
            .meshes
            .iter()
            .map(|mesh| {
                mesh.primitives
                    .iter()
                    .map(|primitive| primitive.indices.clone())
                    .collect()
            })
            .collect();
        self.morph = Some(super::morph::MorphLoadData {
            data,
            indices,
            chunks: Vec::new(),
        });
        Ok(())
    }

    fn preflight(&self, index: usize, limits: GltfImageLimits) -> Result<(), String> {
        // Retain the legacy path when no source limit was requested.
        if limits == GltfImageLimits::default() {
            return Ok(());
        }
        let job = &self.texture_jobs[self.image_jobs[index]];
        let texture = self.document.textures().nth(job.texture).unwrap();
        let data = embedded_image_bytes(&texture.source(), &self.binary)
            .map_err(|error| format!("{}: {error}", job.context))?;
        crate::gltf::image_limits::check_image(data, limits)
            .map_err(|error| format!("{}: {error}", job.context))
    }

    fn texture(&mut self, index: usize, maximum: Option<u16>) -> Result<(), String> {
        let job = &self.texture_jobs[index];
        let texture = self.document.textures().nth(job.texture).unwrap();
        load_texture(texture, &self.binary, &mut self.textures, maximum)
            .map_err(|error| format!("{}: {error}", job.context))?;
        Ok(())
    }

    fn primitive(&mut self, index: usize, limits: GltfMeshLimits) -> Result<(), String> {
        let job = self.primitive_jobs[index];
        let world = self.scene.nodes[job.node].world;
        let map_data = {
            let source = self
                .document
                .meshes()
                .nth(self.source_meshes[job.mesh])
                .unwrap()
                .primitives()
                .nth(job.primitive)
                .unwrap();
            let binary = self.binary.as_ref();
            crate::gltf::material_maps::resolve_primitive_maps(&source, &|index| {
                if index == 0 {
                    Some(binary)
                } else {
                    None
                }
            })?
        };
        let tangent_frame = crate::gltf::material_maps::world_tangents(&map_data.tangents, world);
        let normal_texture = map_data
            .normal
            .as_ref()
            .map(|map| self.textures[&map.texture_index].clone());
        let metallic_texture = map_data
            .metallic_roughness
            .as_ref()
            .map(|map| self.textures[&map.texture_index].clone());
        let primitive = &self.scene.meshes[job.mesh].primitives[job.primitive];
        let source = self
            .document
            .meshes()
            .nth(self.source_meshes[job.mesh])
            .unwrap()
            .primitives()
            .nth(job.primitive)
            .unwrap();
        let material = source.material();
        let pbr = material.pbr_metallic_roughness();
        let texture = pbr
            .base_color_texture()
            .map(|info| self.textures[&info.texture().index()].clone());
        let emissive_texture = material
            .emissive_texture()
            .map(|info| self.textures[&info.texture().index()].clone());
        let strength = material
            .extension_value("KHR_materials_emissive_strength")
            .and_then(|v| v.get("emissiveStrength"))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32;
        let mut emissive = material.emissive_factor();
        for channel in &mut emissive {
            *channel *= strength;
        }
        let surface = GltfSurface {
            name: material.name().map(str::to_string),
            base_color: pbr.base_color_factor(),
            emissive,
            metallic: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            transmission: material
                .extension_value("KHR_materials_transmission")
                .and_then(|v| v.get("transmissionFactor"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32,
            alpha_mode: match material.alpha_mode() {
                gltf::material::AlphaMode::Opaque => GltfAlphaMode::Opaque,
                gltf::material::AlphaMode::Mask => GltfAlphaMode::Mask,
                gltf::material::AlphaMode::Blend => GltfAlphaMode::Blend,
            },
            alpha_cutoff: material.alpha_cutoff().unwrap_or(0.5),
            double_sided: material.double_sided(),
            unlit: material.extension_value("KHR_materials_unlit").is_some(),
        };
        let mut vertices = world_vertices(primitive, world);
        if let Some(info) = pbr.base_color_texture() {
            apply_material_uvs(&mut vertices, &source, &self.binary, &info)?;
        }
        if let Some(info) = material.emissive_texture() {
            let mut emissive_vertices = vertices.clone();
            apply_material_uvs(&mut emissive_vertices, &source, &self.binary, &info)?;
            if pbr.base_color_texture().is_some() {
                validate_shared_uvs(&vertices, &emissive_vertices)?;
            } else {
                vertices = emissive_vertices;
            }
        }
        let indices = winding_indices(&primitive.indices, world);
        for (vertices, indices, sources) in split_triangles(&vertices, &indices, limits)? {
            if let Some(morph) = &mut self.morph {
                morph.chunks.push(super::morph::MorphChunk {
                    node: job.node,
                    mesh: job.mesh,
                    primitive: job.primitive,
                    sources: sources.clone(),
                });
            }
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for vertex in &vertices {
                min = min.min(vertex.position);
                max = max.max(vertex.position);
            }
            self.model.bounds_min = self.model.bounds_min.min(min);
            self.model.bounds_max = self.model.bounds_max.max(max);
            self.model.stats.triangles += indices.len() / 3;
            let chunk_uvs = |uvs: &[Vec2]| {
                sources
                    .iter()
                    .map(|index| uvs[*index as usize])
                    .collect::<Vec<_>>()
            };
            let normal = map_data.normal.as_ref().map(|map| GltfDrawMaterialMap {
                texture: normal_texture.clone().unwrap(),
                tex_coord: map.tex_coord,
                transform: map.transform,
                strength: map.strength,
                uvs: chunk_uvs(&map.uvs),
            });
            let metallic_roughness =
                map_data
                    .metallic_roughness
                    .as_ref()
                    .map(|map| GltfDrawMaterialMap {
                        texture: metallic_texture.clone().unwrap(),
                        tex_coord: map.tex_coord,
                        transform: map.transform,
                        strength: map.strength,
                        uvs: chunk_uvs(&map.uvs),
                    });
            let tangents = if tangent_frame.is_empty() {
                Vec::new()
            } else {
                sources
                    .iter()
                    .map(|index| tangent_frame[*index as usize])
                    .collect()
            };
            self.material_parts.push(GltfPartMaterialMaps {
                normal,
                metallic_roughness,
                tangents,
                material_index: map_data.material_index,
                material_name: map_data.material_name.clone(),
            });
            self.model.parts.push(GltfDrawMesh {
                mesh: Mesh {
                    vertices,
                    indices,
                    texture: texture.clone(),
                },
                emissive_texture: emissive_texture.clone(),
                surface: surface.clone(),
                node: job.node,
                name: self.names[job.node].clone(),
                path: self.paths[job.node].clone(),
                center: (min + max) * 0.5,
                bounds_min: min,
                bounds_max: max,
            });
        }
        if job.primitive + 1 == self.scene.meshes[job.mesh].primitives.len() {
            release_mesh_after_instance(
                &mut self.scene.meshes[job.mesh],
                &mut self.mesh_uses[job.mesh],
            );
        }
        Ok(())
    }

    fn finish(
        mut self,
        markers_requested: bool,
    ) -> (
        GltfModel,
        GltfModelMaterialMaps,
        GltfModelPose,
        Option<GltfMarkers>,
        Option<super::morph::MorphLoadData>,
    ) {
        let pose = GltfModelPose::from_document(&self.scene, &self.document);
        let markers = markers_requested.then(|| GltfMarkers::take_from_scene(&mut self.scene));
        if self.model.parts.is_empty() {
            self.model.bounds_min = Vec3::ZERO;
            self.model.bounds_max = Vec3::ZERO;
        }
        self.model.stats.draw_calls = self.model.parts.len();
        self.model.stats.textures = self
            .texture_jobs
            .iter()
            .filter(|job| job.model_texture)
            .count();
        let material_maps = GltfModelMaterialMaps {
            parts: self.material_parts,
        };
        (self.model, material_maps, pose, markers, self.morph)
    }
}

/// Optional frame-by-frame construction of a textured [`GltfModel`].
///
/// The input GLB stays borrowed until this handle is dropped. Parsing builds one
/// validated document and CPU scene; conversion and uploads reuse them. Each
/// [`Self::poll`] performs at most one stage transition or work unit. Call it on
/// the render thread and await [`crate::window::next_frame`] between calls to
/// advance a loading screen. The handle is neither `Send` nor `Sync`.
///
/// This is cooperative scheduling, **not a strict frame-time budget**. Parsing
/// and CPU scene construction are one synchronous unit. A primitive conversion,
/// one image decode/resize, and one GPU texture upload including mip generation
/// are also indivisible and may stall. Worker decoding is not implemented.
/// All consumed textures are uploaded before primitive conversion starts; source
/// mesh arrays stay alive until their last instance has been converted.
///
/// Cancellation, errors and drop release partial CPU data and managed texture
/// handles. Actual GPU deletion follows the normal frame collection, or an
/// explicit [`crate::window::retire_unused_render_resources`] after dropping
/// unwanted owners. These operations do not retire other live resources.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # async fn example(bytes: &[u8]) -> Result<GltfModel, String> {
/// let mut load = GltfModelLoader::new(bytes, GltfLoadSettings::default())?;
/// while load.poll()?.stage != GltfLoadStage::Complete {
///     clear_background(BLACK);
///     draw_text(&format!("{:?}", load.progress()), 20., 40., 24., WHITE);
///     next_frame().await;
/// }
/// # Ok(load.take_model().unwrap()) }
/// ```
/// A loader cannot move to a worker thread, even before its first GPU upload:
/// ```compile_fail
/// use macroquad::prelude::*;
/// let load = GltfModelLoader::new(&[], GltfLoadSettings::default()).unwrap();
/// std::thread::spawn(move || drop(load));
/// ```
pub struct GltfModelLoader<'a> {
    bytes: &'a [u8],
    settings: GltfLoadSettings,
    progress: GltfLoadProgress,
    prepared: Option<Prepared<'a>>,
    result: Option<GltfModel>,
    material_maps: Option<GltfModelMaterialMaps>,
    pose: Option<GltfModelPose>,
    markers_requested: bool,
    markers: Option<GltfMarkers>,
    morph_requested: bool,
    morph: Option<super::morph::MorphLoadData>,
    morph_pose: Option<GltfModelPose>,
    error: Option<String>,
    _render_thread: PhantomData<Rc<()>>,
}

impl<'a> GltfModelLoader<'a> {
    /// Creates a handle without parsing, decoding, or uploading.
    ///
    /// Invalid mesh limits or a zero texture-size cap are rejected immediately.
    /// ```
    /// use macroquad::prelude::*;
    /// let load = GltfModelLoader::new(&[], GltfLoadSettings::default()).unwrap();
    /// assert_eq!(load.progress().stage, GltfLoadStage::Parse);
    /// ```
    pub fn new(bytes: &'a [u8], settings: GltfLoadSettings) -> Result<Self, String> {
        validate_limits(settings.options.mesh_limits)?;
        if settings.options.max_texture_size == Some(0) {
            return Err("max_texture_size must be greater than zero".into());
        }
        Ok(Self {
            bytes,
            settings,
            progress: GltfLoadProgress::new(GltfLoadStage::Parse, 1),
            prepared: None,
            result: None,
            material_maps: None,
            pose: None,
            markers_requested: false,
            markers: None,
            morph_requested: false,
            morph: None,
            morph_pose: None,
            error: None,
            _render_thread: PhantomData,
        })
    }

    /// Creates a loader that also returns an owned static marker sidecar.
    ///
    /// This reuses the ordinary parse and retains no extra geometry or images
    /// in [`GltfMarkers`]. Take it once with [`Self::take_markers`] after
    /// completion. Ordinary [`Self::new`] does not retain a marker result.
    /// To combine markers with [`Self::new_with_morph`], call
    /// [`Self::enable_markers`] before the first poll instead.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # async fn load(bytes: &[u8]) -> Result<GltfMarkers, String> {
    /// let mut load = GltfModelLoader::new_with_markers(bytes, GltfLoadSettings::default())?;
    /// while load.poll()?.stage != GltfLoadStage::Complete { next_frame().await; }
    /// let markers = load.take_markers().unwrap();
    /// let model = load.take_model().unwrap();
    /// # Ok(markers) }
    /// ```
    pub fn new_with_markers(bytes: &'a [u8], settings: GltfLoadSettings) -> Result<Self, String> {
        let mut loader = Self::new(bytes, settings)?;
        loader.enable_markers()?;
        Ok(loader)
    }

    /// Requests static markers before the first poll, including on a morph load.
    ///
    /// Repeating this before parsing is harmless. Once polling has begun, this
    /// returns an error without changing the loader or any existing results.
    /// Markers remain at rest when morphs or a hierarchy pose change.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let mut load = GltfModelLoader::new_with_morph(&[], GltfLoadSettings::default()).unwrap();
    /// load.enable_markers().unwrap();
    /// assert!(load.take_markers().is_none());
    /// ```
    pub fn enable_markers(&mut self) -> Result<(), String> {
        if self.progress.stage != GltfLoadStage::Parse || self.progress.completed != 0 {
            return Err("glTF markers must be enabled before the first poll".into());
        }
        self.markers_requested = true;
        Ok(())
    }

    /// Opts into validated CPU morph inputs and exact chunk-to-source mapping.
    ///
    /// Parsing reuses the ordinary loader's document and embedded binary data.
    /// Normal loading progress still covers texture/conversion work only; call
    /// [`Self::take_morph_model`] at completion to evaluate defaults and upload
    /// retained geometry synchronously. Ordinary [`Self::new`] has no morph
    /// array or mapping retention. Taking the ordinary CPU model remains valid
    /// and preserves its existing unmorphed behavior.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # async fn load(bytes: &[u8]) -> Result<GltfMorphModel, String> {
    /// let mut loader = GltfModelLoader::new_with_morph(bytes, GltfLoadSettings::default())?;
    /// while loader.poll()?.stage != GltfLoadStage::Complete { next_frame().await; }
    /// # Ok(loader.take_morph_model()?.unwrap()) }
    /// ```
    pub fn new_with_morph(bytes: &'a [u8], settings: GltfLoadSettings) -> Result<Self, String> {
        let mut loader = Self::new(bytes, settings)?;
        loader.morph_requested = true;
        Ok(loader)
    }

    /// Reads progress without performing any work.
    /// ```
    /// use macroquad::prelude::*;
    /// let load = GltfModelLoader::new(&[], GltfLoadSettings::default()).unwrap();
    /// assert_eq!(load.progress().total, 1);
    /// ```
    pub fn progress(&self) -> GltfLoadProgress {
        self.progress
    }

    /// Performs at most one work unit on the render thread.
    ///
    /// Completed polls are harmless. A failure clears partial resources and
    /// every later poll returns the same error. A cancelled poll returns
    /// `"glTF loading cancelled"`. This never awaits or presents a frame itself.
    /// ```
    /// use macroquad::prelude::*;
    /// let mut load = GltfModelLoader::new(b"invalid", GltfLoadSettings::default()).unwrap();
    /// assert!(load.poll().is_err());
    /// assert_eq!(load.progress().stage, GltfLoadStage::Failed);
    /// ```
    pub fn poll(&mut self) -> Result<GltfLoadProgress, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if let Err(error) = self.step() {
            self.prepared = None;
            self.result = None;
            self.material_maps = None;
            self.pose = None;
            self.markers = None;
            self.morph = None;
            self.morph_pose = None;
            self.progress = GltfLoadProgress::new(GltfLoadStage::Failed, 0);
            self.error = Some(error.clone());
            return Err(error);
        }
        Ok(self.progress)
    }

    fn step(&mut self) -> Result<(), String> {
        use GltfLoadStage::*;
        if self.progress.stage == Complete {
            return Ok(());
        }
        if self.progress.stage == Parse && self.progress.completed == 0 {
            let mut prepared = Prepared::new(self.bytes)?;
            if self.morph_requested {
                prepared.enable_morph()?;
            }
            self.prepared = Some(prepared);
            self.progress.completed = 1;
            return Ok(());
        }
        if self.progress.completed == self.progress.total {
            let prepared = self.prepared.as_ref().unwrap();
            self.progress = match self.progress.stage {
                Parse => GltfLoadProgress::new(PreflightImages, prepared.image_jobs.len()),
                PreflightImages => GltfLoadProgress::new(LoadTextures, prepared.texture_jobs.len()),
                LoadTextures => GltfLoadProgress::new(ConvertMeshes, prepared.primitive_jobs.len()),
                ConvertMeshes => {
                    let (model, maps, pose, markers, morph) =
                        self.prepared.take().unwrap().finish(self.markers_requested);
                    self.result = Some(model);
                    self.material_maps = Some(maps);
                    self.morph_pose = self.morph_requested.then(|| pose.clone());
                    self.pose = Some(pose);
                    self.markers = markers;
                    self.morph = morph;
                    GltfLoadProgress::new(Complete, 0)
                }
                _ => unreachable!(),
            };
            return Ok(());
        }
        let prepared = self.prepared.as_mut().unwrap();
        match self.progress.stage {
            PreflightImages => {
                prepared.preflight(self.progress.completed, self.settings.image_limits)?
            }
            LoadTextures => prepared.texture(
                self.progress.completed,
                self.settings.options.max_texture_size,
            )?,
            ConvertMeshes => {
                prepared.primitive(self.progress.completed, self.settings.options.mesh_limits)?
            }
            _ => unreachable!(),
        }
        self.progress.completed += 1;
        Ok(())
    }

    /// Releases partial work or an untaken result. Repeated cancellation is safe.
    ///
    /// Failure remains `Failed`; cancellation after taking a model is a no-op.
    /// A model already returned by [`Self::take_model`] remains valid. This
    /// existing no-op also leaves any untaken sidecars available. Otherwise,
    /// cancellation releases all untaken results; an already taken marker,
    /// pose or material-map result remains independently owned by the caller.
    /// ```
    /// use macroquad::prelude::*;
    /// let mut load = GltfModelLoader::new(&[], GltfLoadSettings::default()).unwrap();
    /// load.cancel();
    /// assert_eq!(load.progress().stage, GltfLoadStage::Cancelled);
    /// ```
    pub fn cancel(&mut self) {
        if self.progress.stage == GltfLoadStage::Failed
            || (self.progress.stage == GltfLoadStage::Complete && self.result.is_none())
        {
            return;
        }
        self.prepared = None;
        self.result = None;
        self.material_maps = None;
        self.pose = None;
        self.markers = None;
        self.morph = None;
        self.morph_pose = None;
        self.error = Some("glTF loading cancelled".into());
        self.progress = GltfLoadProgress::new(GltfLoadStage::Cancelled, 0);
    }

    /// Takes the completed model once; pending, cancelled and failed loads return `None`.
    /// ```
    /// use macroquad::prelude::*;
    /// let mut load = GltfModelLoader::new(&[], GltfLoadSettings::default()).unwrap();
    /// assert!(load.take_model().is_none());
    /// ```
    pub fn take_model(&mut self) -> Option<GltfModel> {
        self.result.take()
    }

    /// Takes the normal and metallic-roughness maps once loading has finished.
    ///
    /// [`Self::take_model`] leaves these in place. Dropping the loader releases
    /// an untaken set, including shading textures that are not also stored on
    /// the model. Pending, cancelled, and failed loads return [`None`]. The
    /// material renderer samples these maps through
    /// [`GltfMaterialFrame::submit_shaded`].
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let mut load = GltfModelLoader::new(&[], GltfLoadSettings::default()).unwrap();
    /// assert!(load.take_material_maps().is_none());
    /// ```
    pub fn take_material_maps(&mut self) -> Option<GltfModelMaterialMaps> {
        self.material_maps.take()
    }

    /// Takes the scene's rest world matrices once loading has completed.
    ///
    /// Reuses the existing parse and includes empty/spawn nodes. The model and
    /// material-map take methods leave this sidecar in place. Pending, cancelled
    /// and failed loads return `None`; dropping the loader releases an untaken
    /// sidecar. Apply the same up-axis conversion as the corresponding model.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let mut load = GltfModelLoader::new(&[], GltfLoadSettings::default()).unwrap();
    /// assert!(load.take_pose().is_none());
    /// ```
    pub fn take_pose(&mut self) -> Option<GltfModelPose> {
        self.pose.take()
    }

    /// Takes an opted-in static marker sidecar once after successful completion.
    ///
    /// Pending, cancelled, failed and non-opted-in loads return `None`. The
    /// model, material-map, pose and morph takes leave this sidecar in place,
    /// and taking markers leaves those results in place. No `GltfScene`, mesh
    /// arrays, decoded images or GPU handles are returned or retained by it.
    /// Its rest-world shapes, facing and tags use the same scene-local node
    /// indices as the corresponding model and standalone scene loader.
    ///
    /// Dropping a loader releases its untaken sidecar. Cancellation does so too
    /// while the loader still owns its completed model; after a model take,
    /// cancellation keeps its existing no-op behavior and markers remain
    /// available. A taken sidecar owns its data independently of subsequent
    /// cancellation/drop, input bytes and other results. Apply the same
    /// [`GltfMarkers::with_up_axis`] conversion to it as to the model and pose.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// # use std::borrow::Cow;
    /// # let json = br#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],
    /// #   "nodes":[{"name":"player_spawn","extras":{"mq_tag":"player"}}]}"#;
    /// # let bytes = ::gltf::binary::Glb {
    /// #     header: ::gltf::binary::Header { magic: *b"glTF", version: 2, length: 0 },
    /// #     json: Cow::Borrowed(json), bin: Some(Cow::Borrowed(&[])),
    /// # }.to_vec().unwrap();
    /// // This marker-only GLB needs no texture uploads or graphics context.
    /// let mut load = GltfModelLoader::new_with_markers(&bytes, GltfLoadSettings::default()).unwrap();
    /// assert!(load.take_markers().is_none()); // pending
    /// while load.poll().unwrap().stage != GltfLoadStage::Complete {}
    /// let markers = load.take_markers().unwrap();
    /// assert!(load.take_markers().is_none()); // once only
    /// let model = load.take_model().unwrap();
    /// let pose = load.take_pose().unwrap();
    /// let maps = load.take_material_maps().unwrap();
    /// drop((load, model, pose, maps));
    /// drop(bytes);
    /// assert_eq!(markers.marker_tag(markers.spawns[0].node), Some("player"));
    /// ```
    pub fn take_markers(&mut self) -> Option<GltfMarkers> {
        self.markers.take()
    }

    /// Takes an opted-in load as a shared source for independent morph instances.
    ///
    /// Requires [`Self::new_with_morph`]. This consumes the untaken CPU model and
    /// private morph sidecar, evaluates node defaults, validates every chunk,
    /// then synchronously uploads all initial retained buffers. It may stall;
    /// uploads are not divided among the preceding polls. Source images and
    /// geometry errors follow ordinary loading and the CPU morph evaluator.
    /// Existing public draw-model structs and ordinary take methods are unchanged.
    ///
    /// Pending, failed, cancelled or already-taken models return `Ok(None)`.
    /// `take_pose` may be called before or afterwards. Taking maps is allowed,
    /// but their tangent
    /// geometry is not updated by morphs and must not be used as a deformed
    /// shading sidecar. Errors release consumed resources through ordinary
    /// retained/texture retirement.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn finish(loader: &mut GltfModelLoader<'_>) -> Result<(), String> {
    /// if let Some(source) = loader.take_morph_model()? {
    ///     source.instantiate().model().draw(Mat4::IDENTITY);
    /// }
    /// # Ok(()) }
    /// ```
    pub fn take_morph_model(&mut self) -> Result<Option<GltfMorphModel>, String> {
        if !self.morph_requested {
            return Err("glTF morph loading requires new_with_morph".into());
        }
        if self.result.is_none() {
            return Ok(None);
        }
        let pose = self
            .morph_pose
            .take()
            .ok_or("glTF morph pose was already taken")?;
        let model = self.result.take().unwrap();
        let input = self
            .morph
            .take()
            .ok_or("glTF morph source was already taken")?;
        GltfMorphModel::from_loaded(model, pose, input).map(Some)
    }
}

/// Pollable retained upload of an already loaded or edited [`GltfModel`].
///
/// All chunks are validated before the first upload. Each poll validates or
/// uploads at most one chunk; a chunk's buffer uploads are indivisible. No GLB
/// is reparsed and textures are shared. With `retain_cpu_geometry: false`, each
/// uploaded chunk's CPU arrays are released immediately. The source model and
/// any partial GPU resources are released on error, cancellation or drop;
/// actual GPU collection follows the same boundaries as [`GltfModelLoader`].
///
/// Construct, poll, cancel and drop on the render thread. This handle is neither
/// `Send` nor `Sync`. Neither polling nor validation presents a frame itself.
/// ```no_run
/// use macroquad::prelude::*;
/// # async fn example(model: GltfModel) -> Result<RetainedGltfModel, String> {
/// let mut upload = RetainedGltfModelLoader::new(model, false);
/// while upload.poll()?.stage != GltfLoadStage::Complete { next_frame().await; }
/// # Ok(upload.take_model().unwrap()) }
/// ```
/// Upload handles cannot move to worker threads:
/// ```compile_fail
/// use macroquad::prelude::*;
/// fn move_to_worker(upload: RetainedGltfModelLoader) {
///     std::thread::spawn(move || drop(upload));
/// }
/// ```
pub struct RetainedGltfModelLoader {
    source: Option<GltfModel>,
    retain_cpu_geometry: bool,
    parts: Vec<RetainedGltfDrawMesh>,
    progress: GltfLoadProgress,
    result: Option<RetainedGltfModel>,
    error: Option<String>,
    _render_thread: PhantomData<Rc<()>>,
}

impl RetainedGltfModelLoader {
    /// Takes ownership without validating or uploading. CPU retention is explicit.
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn example(model: GltfModel) {
    /// let upload = RetainedGltfModelLoader::new(model, false);
    /// assert_eq!(upload.progress().stage, GltfLoadStage::ValidateMeshes);
    /// # }
    /// ```
    pub fn new(model: GltfModel, retain_cpu_geometry: bool) -> Self {
        let total = model.parts.len();
        Self {
            source: Some(model),
            retain_cpu_geometry,
            parts: Vec::new(),
            progress: GltfLoadProgress::new(GltfLoadStage::ValidateMeshes, total),
            result: None,
            error: None,
            _render_thread: PhantomData,
        }
    }

    /// Reads stage-local progress without doing work.
    /// ```no_run
    /// # use macroquad::prelude::*;
    /// # fn example(upload: &RetainedGltfModelLoader) {
    /// let progress = upload.progress();
    /// assert!(progress.completed <= progress.total);
    /// # }
    /// ```
    pub fn progress(&self) -> GltfLoadProgress {
        self.progress
    }

    /// Validates or uploads at most one chunk on the render thread.
    ///
    /// Completed polls are harmless. Errors release owned resources and repeat
    /// unchanged on later polls. Cancellation returns `"glTF loading cancelled"`.
    /// ```no_run
    /// # use macroquad::prelude::*;
    /// # fn example(upload: &mut RetainedGltfModelLoader) -> Result<(), String> {
    /// let progress = upload.poll()?;
    /// # Ok(()) }
    /// ```
    pub fn poll(&mut self) -> Result<GltfLoadProgress, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if let Err(error) = self.step() {
            self.source = None;
            self.parts = Vec::new();
            self.result = None;
            self.error = Some(error.clone());
            self.progress = GltfLoadProgress::new(GltfLoadStage::Failed, 0);
            return Err(error);
        }
        Ok(self.progress)
    }

    fn step(&mut self) -> Result<(), String> {
        use GltfLoadStage::*;
        if self.progress.stage == Complete {
            return Ok(());
        }
        if self.progress.completed == self.progress.total {
            if self.progress.stage == ValidateMeshes {
                self.progress = GltfLoadProgress::new(UploadMeshes, self.progress.total);
            } else {
                let model = self.source.take().unwrap();
                let combined = self
                    .parts
                    .iter()
                    .map(|p| Aabb::from_min_max(p.bounds_min, p.bounds_max))
                    .reduce(|a, b| Aabb::from_min_max(a.min.min(b.min), a.max.max(b.max)));
                self.result = Some(RetainedGltfModel {
                    parts: std::mem::take(&mut self.parts),
                    stats: model.stats,
                    bounds_min: combined.map_or(Vec3::ZERO, |b| b.min),
                    bounds_max: combined.map_or(Vec3::ZERO, |b| b.max),
                    root_names: model.root_names.clone(),
                    cpu_model: self.retain_cpu_geometry.then(|| Arc::new(model)),
                });
                self.progress = GltfLoadProgress::new(Complete, 0);
            }
            return Ok(());
        }
        let part = &mut self.source.as_mut().unwrap().parts[self.progress.completed];
        if self.progress.stage == ValidateMeshes {
            validate_retained_mesh(&part.mesh).map_err(|e| e.to_string())?;
        } else {
            let bounds = Aabb::from_points(part.mesh.vertices.iter().map(|v| v.position)).unwrap();
            self.parts.push(RetainedGltfDrawMesh {
                mesh: crate::models::RetainedMesh::from_mesh(&part.mesh)
                    .map_err(|e| e.to_string())?,
                emissive_texture: part.emissive_texture.clone(),
                surface: part.surface.clone(),
                node: part.node,
                name: part.name.clone(),
                path: part.path.clone(),
                center: bounds.center(),
                bounds_min: bounds.min,
                bounds_max: bounds.max,
            });
            if !self.retain_cpu_geometry {
                part.mesh.vertices = Vec::new();
                part.mesh.indices = Vec::new();
            }
        }
        self.progress.completed += 1;
        Ok(())
    }

    /// Releases partial uploads or an untaken result; already-taken models survive.
    ///
    /// Repeated cancellation is safe and a failure remains `Failed`.
    /// ```no_run
    /// # use macroquad::prelude::*;
    /// # fn example(upload: &mut RetainedGltfModelLoader) {
    /// upload.cancel();
    /// # }
    /// ```
    pub fn cancel(&mut self) {
        if self.progress.stage == GltfLoadStage::Failed
            || (self.progress.stage == GltfLoadStage::Complete && self.result.is_none())
        {
            return;
        }
        self.source = None;
        self.parts = Vec::new();
        self.result = None;
        self.error = Some("glTF loading cancelled".into());
        self.progress = GltfLoadProgress::new(GltfLoadStage::Cancelled, 0);
    }

    /// Takes the completed retained model once. Returns `None` before success.
    /// ```no_run
    /// # use macroquad::prelude::*;
    /// # fn example(upload: &mut RetainedGltfModelLoader) {
    /// if let Some(model) = upload.take_model() { model.draw(Mat4::IDENTITY); }
    /// # }
    /// ```
    pub fn take_model(&mut self) -> Option<RetainedGltfModel> {
        self.result.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let source = gltf::binary::Glb::from_slice(include_bytes!(
            "../../examples/assets/textured_quad.glb"
        ))
        .unwrap();
        let mut json: serde_json::Value = serde_json::from_slice(&source.json).unwrap();
        edit(&mut json);
        gltf::binary::Glb {
            header: source.header,
            json: Cow::Owned(serde_json::to_vec(&json).unwrap()),
            bin: source.bin,
        }
        .to_vec()
        .unwrap()
    }

    fn untextured_fixture() -> Vec<u8> {
        fixture(|json| {
            json["materials"][0]["pbrMetallicRoughness"]
                .as_object_mut()
                .unwrap()
                .remove("baseColorTexture");
        })
    }

    fn untextured_model() -> GltfModel {
        GltfModel::from_bytes(&untextured_fixture()).unwrap()
    }

    #[test]
    fn rest_pose_sidecar_survives_independent_takes_and_keeps_empty_node_yaw() {
        let bytes = fixture(|json| {
            json["materials"][0]["pbrMetallicRoughness"]
                .as_object_mut()
                .unwrap()
                .remove("baseColorTexture");
            json["nodes"][0]["translation"] = serde_json::json!([3., 2., 1.]);
            json["nodes"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({
                    "name": "player_spawn", "translation": [-2., 1., 4.],
                    "rotation": [0., 0.3420201433256687, 0., 0.9396926207859084]
                }));
            json["scenes"][0]["nodes"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!(1));
        });
        let mut load = GltfModelLoader::new(&bytes, GltfLoadSettings::default()).unwrap();
        assert!(load.take_pose().is_none());
        while load.poll().unwrap().stage != GltfLoadStage::Complete {}
        let model = load.take_model().unwrap();
        assert!(load.take_material_maps().is_some());
        let poses = load.take_pose().unwrap();
        assert!(load.take_pose().is_none());
        drop(load);
        assert_eq!(
            poses
                .rest_world(model.parts[0].node)
                .unwrap()
                .w_axis
                .truncate(),
            Vec3::new(3., 2., 1.)
        );
        let mut pose = poses.node(model.parts[0].node).unwrap();
        let spawn = poses.rest_world(1).unwrap();
        pose.set_world(spawn).unwrap();
        assert!(pose
            .transform()
            .transform_point3(Vec3::new(3., 2., 1.))
            .abs_diff_eq(Vec3::new(-2., 1., 4.), 1e-5));
        assert!(spawn.transform_vector3(Vec3::X).abs_diff_eq(
            Vec3::new(40_f32.to_radians().cos(), 0., -40_f32.to_radians().sin()),
            1e-5
        ));
        let (_, sync_poses) = GltfModel::from_bytes_with_pose(&bytes).unwrap();
        assert_eq!(sync_poses.rest_world(1), Some(spawn));
    }

    #[test]
    fn polling_preserves_borrowed_binary_and_exposes_each_stage() {
        let bytes = untextured_fixture();
        let original = gltf::binary::Glb::from_slice(&bytes).unwrap();
        let mut load = GltfModelLoader::new(&bytes, GltfLoadSettings::default()).unwrap();
        assert!(load.prepared.is_none());
        assert!(load.take_model().is_none());
        assert_eq!(
            load.poll().unwrap(),
            GltfLoadProgress {
                stage: GltfLoadStage::Parse,
                completed: 1,
                total: 1
            }
        );
        let prepared = load.prepared.as_ref().unwrap();
        assert!(matches!(prepared.binary, Cow::Borrowed(_)));
        assert_eq!(
            prepared.binary.as_ptr(),
            original.bin.as_ref().unwrap().as_ptr()
        );
        assert_eq!(
            load.poll().unwrap(),
            GltfLoadProgress::new(GltfLoadStage::PreflightImages, 0)
        );
        assert_eq!(
            load.poll().unwrap(),
            GltfLoadProgress::new(GltfLoadStage::LoadTextures, 0)
        );
        assert_eq!(
            load.poll().unwrap(),
            GltfLoadProgress::new(GltfLoadStage::ConvertMeshes, 1)
        );
        assert_eq!(load.poll().unwrap().completed, 1);
        assert_eq!(load.poll().unwrap().stage, GltfLoadStage::Complete);
        assert!(load.prepared.is_none());
        let model = load.take_model().unwrap();
        assert_eq!(model.stats.triangles, 2);
        assert_eq!(model.stats.textures, 0);
        assert!(load.take_model().is_none());
        load.cancel();
        assert_eq!(load.poll().unwrap().stage, GltfLoadStage::Complete);
        assert_eq!(model.parts.len(), 1);
    }

    #[test]
    fn cancellation_releases_cpu_work_before_during_and_after_conversion() {
        let bytes = untextured_fixture();
        let mut trace = vec![];
        let mut reference = GltfModelLoader::new(&bytes, GltfLoadSettings::default()).unwrap();
        loop {
            trace.push(reference.progress());
            if reference.progress().stage == GltfLoadStage::Complete {
                break;
            }
            reference.poll().unwrap();
        }
        for (polls, expected) in trace.into_iter().enumerate() {
            let mut load = GltfModelLoader::new(&bytes, GltfLoadSettings::default()).unwrap();
            for _ in 0..polls {
                load.poll().unwrap();
            }
            assert_eq!(load.progress(), expected);
            load.cancel();
            assert!(load.prepared.is_none());
            assert!(load.take_model().is_none());
            assert!(load.take_pose().is_none());
            assert_eq!(load.progress().stage, GltfLoadStage::Cancelled);
            let first = load.poll().unwrap_err();
            load.cancel();
            assert_eq!(load.poll().unwrap_err(), first);
        }
    }

    #[test]
    fn parse_failure_is_terminal_and_cleanup_is_idempotent() {
        let mut load = GltfModelLoader::new(b"broken GLB", GltfLoadSettings::default()).unwrap();
        let error = load.poll().unwrap_err();
        assert_eq!(load.progress().stage, GltfLoadStage::Failed);
        assert!(load.prepared.is_none());
        load.cancel();
        assert_eq!(load.poll().unwrap_err(), error);
        assert_eq!(load.progress().stage, GltfLoadStage::Failed);
        assert!(load.take_model().is_none());
        assert!(load.take_pose().is_none());
    }

    #[test]
    fn invalid_image_failure_releases_prepared_scene_without_a_gpu() {
        let bytes = fixture(|json| {
            // A geometry view is valid glTF metadata but is not a decodable image.
            json["images"][0]["bufferView"] = serde_json::json!(0);
        });
        let mut load = GltfModelLoader::new(&bytes, GltfLoadSettings::default()).unwrap();
        while load.progress().stage != GltfLoadStage::LoadTextures {
            load.poll().unwrap();
        }
        let error = load.poll().unwrap_err();
        assert!(error.contains("image 0"), "{error}");
        assert!(error.contains("material"), "{error}");
        assert_eq!(load.progress().stage, GltfLoadStage::Failed);
        assert!(load.prepared.is_none());
        assert!(load.take_model().is_none());
        assert_eq!(load.poll().unwrap_err(), error);
    }

    #[test]
    fn limited_images_are_preflighted_before_any_texture_upload() {
        let bytes = fixture(|json| {
            json["images"][0]["bufferView"] = serde_json::json!(0);
        });
        let settings = GltfLoadOptions::default().with_image_limits(GltfImageLimits {
            max_source_pixels: Some(1024),
            max_decoded_bytes: None,
        });
        let mut load = GltfModelLoader::new(&bytes, settings).unwrap();
        while load.progress().stage != GltfLoadStage::PreflightImages {
            load.poll().unwrap();
        }
        assert!(load.prepared.as_ref().unwrap().textures.is_empty());
        assert!(load.poll().is_err());
        assert_eq!(load.progress().stage, GltfLoadStage::Failed);
        assert!(load.prepared.is_none());
    }

    #[test]
    fn instance_conversion_reuses_first_used_source_mesh_and_keeps_it_until_last_use() {
        let bytes = fixture(|json| {
            json["materials"][0]["pbrMetallicRoughness"]
                .as_object_mut()
                .unwrap()
                .remove("baseColorTexture");
            let mesh = json["meshes"][0].clone();
            json["meshes"].as_array_mut().unwrap().push(mesh);
            json["nodes"][0]["mesh"] = serde_json::json!(1);
            json["nodes"][0]["name"] = serde_json::json!("first");
            let mut second = json["nodes"][0].clone();
            second["name"] = serde_json::json!("second");
            second["translation"] = serde_json::json!([3, 0, 0]);
            json["nodes"].as_array_mut().unwrap().push(second);
            json["scenes"][0]["nodes"] = serde_json::json!([0, 1]);
        });
        let mut load = GltfModelLoader::new(&bytes, GltfLoadSettings::default()).unwrap();
        while load.progress().stage != GltfLoadStage::ConvertMeshes {
            load.poll().unwrap();
        }
        assert_eq!(load.progress().total, 2);
        assert_eq!(load.prepared.as_ref().unwrap().source_meshes, vec![1]);
        load.poll().unwrap();
        assert!(!load.prepared.as_ref().unwrap().scene.meshes[0]
            .primitives
            .is_empty());
        load.poll().unwrap();
        assert!(load.prepared.as_ref().unwrap().scene.meshes[0]
            .primitives
            .is_empty());
        load.poll().unwrap();
        let model = load.take_model().unwrap();
        assert_eq!(model.stats.meshes, 1);
        assert_eq!(model.stats.triangles, 4);
        assert_eq!(model.root_names, ["first", "second"]);
        for (a, b) in model.parts[0]
            .mesh
            .vertices
            .iter()
            .zip(&model.parts[1].mesh.vertices)
        {
            assert!((b.position - a.position - Vec3::X * 3.).length() < 0.00001);
        }
    }

    #[test]
    fn retained_validation_finishes_before_upload_and_drops_invalid_input() {
        let mut model = untextured_model();
        model.parts.push(model.parts[0].clone());
        model.parts[1].mesh.indices.push(0);
        let mut upload = RetainedGltfModelLoader::new(model, false);
        assert_eq!(upload.poll().unwrap().completed, 1);
        let error = upload.poll().unwrap_err();
        assert_eq!(upload.progress().stage, GltfLoadStage::Failed);
        assert!(upload.source.is_none());
        assert!(upload.parts.is_empty());
        upload.cancel();
        assert_eq!(upload.poll().unwrap_err(), error);
        assert!(upload.take_model().is_none());
    }

    #[test]
    fn retained_cancellation_can_stop_before_validation_or_upload_without_a_gpu() {
        for polls in 0..=2 {
            let mut upload = RetainedGltfModelLoader::new(untextured_model(), true);
            for _ in 0..polls {
                upload.poll().unwrap();
            }
            if polls == 2 {
                assert_eq!(upload.progress().stage, GltfLoadStage::UploadMeshes);
            }
            upload.cancel();
            assert!(upload.source.is_none());
            assert!(upload.take_model().is_none());
            assert_eq!(upload.progress().stage, GltfLoadStage::Cancelled);
            assert!(upload.poll().is_err());
        }
    }

    #[test]
    fn empty_retained_completion_keeps_explicit_cpu_ownership_and_take_survives_cancel() {
        let model = GltfModel {
            parts: vec![],
            stats: GltfModelStats::default(),
            bounds_min: Vec3::ZERO,
            bounds_max: Vec3::ZERO,
            root_names: vec!["empty".into()],
        };
        for keep in [false, true] {
            let mut upload = RetainedGltfModelLoader::new(model.clone(), keep);
            assert_eq!(upload.poll().unwrap().stage, GltfLoadStage::UploadMeshes);
            assert_eq!(upload.poll().unwrap().stage, GltfLoadStage::Complete);
            let completed = upload.take_model().unwrap();
            assert_eq!(completed.cpu_model().is_some(), keep);
            assert_eq!(completed.root_names, ["empty"]);
            assert!(upload.take_model().is_none());
            upload.cancel();
            assert_eq!(upload.poll().unwrap().stage, GltfLoadStage::Complete);
            assert_eq!(completed.bounds_min, Vec3::ZERO);
            assert_eq!(completed.bounds_max, Vec3::ZERO);
        }
    }
}
