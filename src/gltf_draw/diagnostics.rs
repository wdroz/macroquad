use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    mem::size_of,
    sync::{Arc, Weak},
};

use super::{GltfModel, RetainedGltfModel};
use crate::{
    models::{Mesh, RetainedMesh, Vertex},
    texture::{Texture2D, TextureHandle, TextureSlotGuarded},
};

/// Original encoded image dimensions, before the loader's upload size cap.
///
/// This is metadata only: decoded pixels and encoded bytes are not retained.
/// An image index is local to its source GLB; equal indices from different loads
/// do not identify a shared texture.
///
/// ```
/// use macroquad::gltf_draw::GltfSourceImage;
/// let image = GltfSourceImage { image_index: 0, name: None, width: 8, height: 4 };
/// assert_eq!(image.width * image.height, 32);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GltfSourceImage {
    /// Index in the original GLB's images array.
    pub image_index: usize,
    /// Optional image name in that GLB.
    pub name: Option<String>,
    /// Source image width in pixels, before resizing.
    pub width: u32,
    /// Source image height in pixels, before resizing.
    pub height: u32,
}

/// One unique texture referenced by the reported models.
///
/// Estimates use miniquad's texture format, dimensions, sample count and
/// `allocate_mipmaps` flag. A mip chain includes every level down to 1×1;
/// changing only the sampling filter does not remove its allocated storage.
/// Raw backend changes, including manually generated mipmaps without that flag,
/// cannot always be observed. Driver padding, compression and copies are unknown.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn inspect(model: &GltfModel) {
/// for texture in model.resource_report().textures {
///     println!("{}x{}: {} estimated bytes", texture.width, texture.height,
///         texture.estimated_gpu_bytes);
/// }
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GltfTextureResource {
    /// Current uploaded width in pixels.
    pub width: u32,
    /// Current uploaded height in pixels.
    pub height: u32,
    /// Estimated allocated levels, including the base level.
    pub mip_levels: u32,
    /// Estimated texture storage, including mip levels, samples and cube faces.
    pub estimated_gpu_bytes: u64,
    /// Source metadata when tracking was enabled when this texture loaded.
    /// Caller-created textures and textures loaded without tracking have `None`.
    pub source: Option<GltfSourceImage>,
}

/// Optional snapshot of the geometry and textures referenced by selected models.
///
/// CPU values count vertex/index vector storage, including unused capacity;
/// they exclude model metadata, allocator overhead, source bytes and decoder
/// scratch space. GPU values are payload estimates, not driver measurements or
/// total process memory. Renderer streaming buffers, queued draws after a model
/// is dropped, and textures used only by custom materials are outside this scope.
/// See [`crate::telemetry`] for renderer diagnostics.
///
/// The report retains no geometry or texture handles. Collect several models
/// together to count shared allocations once; adding independent reports can
/// double-count resources.
///
/// ```
/// use macroquad::gltf_draw::GltfResourceReport;
/// let report = GltfResourceReport::collect(&[], &[]);
/// assert_eq!(report.cpu_geometry_bytes, 0);
/// assert!(report.textures.is_empty());
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GltfResourceReport {
    /// Used vertex and index bytes, once per unique CPU allocation.
    pub cpu_geometry_bytes: u64,
    /// Allocated vertex and index capacity bytes, including used bytes.
    pub cpu_geometry_capacity_bytes: u64,
    /// Unique retained GPU vertex and index buffer objects.
    pub gpu_buffer_count: usize,
    /// Retained vertex and index payload bytes; excludes streaming buffers.
    pub estimated_gpu_buffer_bytes: u64,
    /// Sum of unique texture estimates in [`Self::textures`].
    pub estimated_gpu_texture_bytes: u64,
    /// Unique textures, in first-reference order across the supplied models.
    pub textures: Vec<GltfTextureResource>,
}

impl GltfResourceReport {
    /// Collects a snapshot, deduplicating across CPU and retained instances.
    ///
    /// Cloned CPU models have independent geometry allocations and shared
    /// textures. Retained clones share buffers and optional CPU geometry.
    /// Current public texture overrides are honored, and retained CPU models
    /// contribute all resources they still own even if draw parts were changed.
    /// An active graphics context is required when any supplied model has a
    /// texture. Untextured CPU/retained reports do not query the graphics backend.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn inspect(a: &RetainedGltfModel, b: &RetainedGltfModel) {
    /// let report = GltfResourceReport::collect(&[], &[a, b]);
    /// println!("{} unique GPU buffers", report.gpu_buffer_count);
    /// # }
    /// ```
    pub fn collect(models: &[&GltfModel], retained_models: &[&RetainedGltfModel]) -> Self {
        SOURCE_IMAGES.with(|registry| registry.borrow_mut().prune());
        let mut collector = Collector::default();
        for model in models {
            collector.model(model);
        }
        for model in retained_models {
            if let Some(cpu) = model.cpu_model() {
                collector.model(cpu);
            }
            for part in &model.parts {
                collector.retained_mesh(&part.mesh);
                collector.texture(part.emissive_texture.as_ref());
            }
        }
        collector.report
    }
}

impl GltfModel {
    /// Reports this model's current geometry and unique referenced textures.
    ///
    /// See [`GltfResourceReport::collect`] to deduplicate across instances.
    /// Texture inspection requires an active graphics context.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn inspect(model: &GltfModel) {
    /// println!("{} CPU geometry bytes", model.resource_report().cpu_geometry_bytes);
    /// # }
    /// ```
    pub fn resource_report(&self) -> GltfResourceReport {
        GltfResourceReport::collect(&[self], &[])
    }
}

impl RetainedGltfModel {
    /// Reports shared uploaded geometry, textures and optional retained CPU data.
    ///
    /// See [`GltfResourceReport::collect`] to deduplicate across instances.
    /// Texture inspection requires an active graphics context.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn inspect(model: &RetainedGltfModel) {
    /// println!("{} estimated GPU buffer bytes",
    ///     model.resource_report().estimated_gpu_buffer_bytes);
    /// # }
    /// ```
    pub fn resource_report(&self) -> GltfResourceReport {
        GltfResourceReport::collect(&[], &[self])
    }
}

/// Enables optional source-image metadata tracking for subsequent glTF loads.
///
/// Disabled by default. Call on the render thread before loading to include
/// original image names, indices and dimensions in [`GltfTextureResource::source`].
/// Disabling clears existing metadata; re-enabling does not recover earlier
/// source dimensions. Geometry/texture byte reports work without this tracking.
///
/// Tracking keeps only weak handles and small metadata, never decoded pixels or
/// live GPU resources. Dead entries are pruned on each tracked upload and report.
/// Reports are built only when explicitly requested; no per-frame walk is added.
///
/// ```no_run
/// use macroquad::prelude::*;
/// # fn load(bytes: &[u8]) -> Result<GltfModel, String> {
/// set_gltf_resource_diagnostics_enabled(true);
/// let model = GltfModel::from_bytes(bytes)?;
/// assert!(model.resource_report().textures.iter().all(|t| t.source.is_some()));
/// # Ok(model) }
/// ```
pub fn set_gltf_resource_diagnostics_enabled(enabled: bool) {
    SOURCE_IMAGES.with(|registry| {
        let mut registry = registry.borrow_mut();
        registry.enabled = enabled;
        if !enabled {
            registry.images.clear();
        }
    });
}

#[derive(Default)]
struct SourceRegistry {
    enabled: bool,
    images: HashMap<miniquad::TextureId, (Weak<TextureSlotGuarded>, GltfSourceImage)>,
}

impl SourceRegistry {
    fn prune(&mut self) {
        self.images
            .retain(|_, (texture, _)| texture.strong_count() > 0);
    }
}

thread_local! {
    static SOURCE_IMAGES: RefCell<SourceRegistry> = RefCell::new(SourceRegistry::default());
}

pub(super) fn record_source_image(
    texture: &Texture2D,
    image_index: usize,
    name: Option<&str>,
    (width, height): (u32, u32),
) {
    SOURCE_IMAGES.with(|registry| {
        let mut registry = registry.borrow_mut();
        if !registry.enabled {
            return;
        }
        if let TextureHandle::Managed(handle) = &texture.texture {
            registry.prune();
            registry.images.insert(
                texture.raw_miniquad_id(),
                (
                    Arc::downgrade(handle),
                    GltfSourceImage {
                        image_index,
                        name: name.map(str::to_owned),
                        width,
                        height,
                    },
                ),
            );
        }
    });
}

#[derive(Default)]
struct Collector {
    report: GltfResourceReport,
    cpu_allocations: HashSet<usize>,
    gpu_buffers: HashSet<usize>,
    textures: HashSet<miniquad::TextureId>,
}

impl Collector {
    fn model(&mut self, model: &GltfModel) {
        for part in &model.parts {
            self.mesh(&part.mesh);
            self.texture(part.emissive_texture.as_ref());
        }
    }

    fn vector<T>(&mut self, vector: &Vec<T>) {
        // Empty vectors with capacity still own storage; zero-capacity vectors
        // use a dangling sentinel that must not participate in deduplication.
        if vector.capacity() > 0 && self.cpu_allocations.insert(vector.as_ptr() as usize) {
            self.report.cpu_geometry_bytes += vector.len() as u64 * size_of::<T>() as u64;
            self.report.cpu_geometry_capacity_bytes +=
                vector.capacity() as u64 * size_of::<T>() as u64;
        }
    }

    fn mesh(&mut self, mesh: &Mesh) {
        self.vector(&mesh.vertices);
        self.vector(&mesh.indices);
        self.texture(mesh.texture.as_ref());
    }

    fn retained_mesh(&mut self, mesh: &RetainedMesh) {
        if self.gpu_buffers.insert(Arc::as_ptr(&mesh.buffers) as usize) {
            self.report.gpu_buffer_count += 2;
            self.report.estimated_gpu_buffer_bytes += mesh.buffers.vertices_count as u64
                * size_of::<Vertex>() as u64
                + mesh.buffers.indices_count as u64 * size_of::<u16>() as u64;
        }
        self.texture(mesh.texture.as_ref());
    }

    fn texture(&mut self, texture: Option<&Texture2D>) {
        let Some(texture) = texture else { return };
        let id = texture.raw_miniquad_id();
        if !self.textures.insert(id) {
            return;
        }
        let params = crate::get_quad_context().texture_params(id);
        let (mip_levels, estimated_gpu_bytes) = texture_storage(params);
        let source = SOURCE_IMAGES.with(|registry| {
            registry
                .borrow()
                .images
                .get(&id)
                .map(|(_, source)| source.clone())
        });
        self.report.estimated_gpu_texture_bytes = self
            .report
            .estimated_gpu_texture_bytes
            .saturating_add(estimated_gpu_bytes);
        self.report.textures.push(GltfTextureResource {
            width: params.width,
            height: params.height,
            mip_levels,
            estimated_gpu_bytes,
            source,
        });
    }
}

fn texture_storage(params: miniquad::TextureParams) -> (u32, u64) {
    let mut width = params.width;
    let mut height = params.height;
    if width == 0 || height == 0 {
        return (0, 0);
    }
    let bytes_per_pixel = params.format.size(1, 1) as u64;
    let faces = if params.kind == miniquad::TextureKind::CubeMap {
        6
    } else {
        1
    };
    let samples = params.sample_count.max(1) as u64;
    let mut levels = 0;
    let mut bytes = 0u64;
    loop {
        levels += 1;
        bytes = bytes.saturating_add(
            (width as u64)
                .saturating_mul(height as u64)
                .saturating_mul(bytes_per_pixel)
                .saturating_mul(faces)
                .saturating_mul(samples),
        );
        if !params.allocate_mipmaps || (width == 1 && height == 1) {
            break;
        }
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    (levels, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_vectors_count_capacity_once_even_when_empty() {
        let mut vertices = Vec::<Vertex>::with_capacity(12);
        vertices.resize(3, Vertex::new(0., 0., 0., 0., 0., crate::color::WHITE));
        let indices = Vec::<u16>::with_capacity(9);
        let mut collector = Collector::default();
        collector.vector(&vertices);
        collector.vector(&vertices);
        collector.vector(&indices);
        collector.vector(&Vec::<u16>::new());
        assert_eq!(
            collector.report.cpu_geometry_bytes,
            3 * size_of::<Vertex>() as u64
        );
        assert_eq!(
            collector.report.cpu_geometry_capacity_bytes,
            12 * size_of::<Vertex>() as u64 + 9 * size_of::<u16>() as u64
        );
        collector.vector(&vertices.clone());
        assert_eq!(
            collector.report.cpu_geometry_bytes,
            6 * size_of::<Vertex>() as u64
        );
    }

    #[test]
    fn estimates_cover_full_non_square_mip_chains_and_large_dimensions() {
        let mut params = miniquad::TextureParams {
            width: 8,
            height: 2,
            allocate_mipmaps: true,
            ..Default::default()
        };
        assert_eq!(texture_storage(params), (4, (16 + 4 + 2 + 1) * 4));
        params.width = 7;
        params.height = 3;
        assert_eq!(texture_storage(params), (3, (21 + 3 + 1) * 4));
        params.allocate_mipmaps = false;
        assert_eq!(texture_storage(params), (1, 21 * 4));
        params.width = u32::MAX;
        params.height = u32::MAX;
        assert_eq!(texture_storage(params), (1, u64::MAX));
        params.width = 0;
        assert_eq!(texture_storage(params), (0, 0));
    }
}
