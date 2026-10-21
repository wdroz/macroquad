//! Optional CPU clip sampling; no window, image decoding or global clock.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use glam::{Quat, Vec3};

use super::{load_buffers, GltfError};
use crate::gltf_draw::GltfLocalTransform;

/// Optional animation sidecar for the same default scene as [`super::GltfScene`].
///
/// Parsing loads buffers but never images or GPU resources. LINEAR and STEP
/// node TRS/morph-weight channels are supported; CUBICSPLINE is an explicit
/// error. Channels outside the selected scene are validated but omitted from
/// samples. Their key times still contribute to the source clip duration.
/// Skinning, animation extensions, blending and event tracks are not included.
///
/// ```
/// use macroquad::gltf::GltfAnimationData;
/// let data = GltfAnimationData::from_bytes(br#"{"asset":{"version":"2.0"}}"#).unwrap();
/// assert!(data.clips.is_empty());
/// ```
#[derive(Clone, Debug, Default)]
pub struct GltfAnimationData {
    /// Clips in document order, with shared immutable channel arrays.
    pub clips: Vec<GltfAnimationClip>,
}

/// An immutable clip with scene-local node indices and shared key arrays.
///
/// Non-targeted components of an animated TRS node use that node's authored
/// local TRS on every sample. Nodes absent from the clip are not changed.
/// Apply matching [`Self::with_up_axis`] conversion when the model/pose was
/// converted. Matrix-authored nodes cannot receive TRS channels; weight
/// channels on them remain valid.
///
/// ```
/// use macroquad::gltf::GltfAnimationData;
/// # let data = GltfAnimationData::default();
/// for clip in &data.clips { println!("{:?}: {} seconds", clip.name(), clip.duration()); }
/// ```
#[derive(Clone, Debug)]
pub struct GltfAnimationClip {
    inner: Arc<ClipData>,
    root_rotation: Quat,
}

#[derive(Debug)]
struct ClipData {
    name: Option<String>,
    duration: f32,
    transforms: Vec<(usize, GltfLocalTransform, bool)>,
    channels: Vec<Channel>,
}

#[derive(Debug)]
struct Channel {
    node: Option<usize>,
    property: gltf::animation::Property,
    step: bool,
    times: Vec<f32>,
    values: Vec<f32>,
    width: usize,
}

/// One owned sample; edits do not affect its clip or another player.
///
/// Transforms replace an animated node's full authored local TRS; weights use
/// one finite value per target. Node indices are scene-local, not glTF document
/// indices. Use `apply_pose`, `apply_morph` or `apply` with matching source
/// instances; the methods validate and stage changes before committing them.
///
/// ```
/// use macroquad::gltf::GltfAnimationSample;
/// let sample = GltfAnimationSample::default();
/// assert!(sample.transforms.is_empty() && sample.weights.is_empty());
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GltfAnimationSample {
    /// Full local transforms for nodes with any TRS channel, in scene order.
    pub transforms: Vec<(usize, GltfLocalTransform)>,
    /// Morph weights for nodes with a weight channel, in channel order.
    pub weights: Vec<(usize, Vec<f32>)>,
}

impl GltfAnimationData {
    /// Loads GLB or embedded-buffer glTF animation data without decoding images.
    ///
    /// Accessors validate format, count, finite values, ranges, alignment and
    /// sparse indices. Inputs are FLOAT SCALAR, strictly increasing and
    /// nonnegative. Outputs accept FLOAT, plus normalized BYTE/UBYTE/SHORT/
    /// USHORT for rotation and weight channels as allowed by glTF 2.0.
    ///
    /// ```
    /// use macroquad::gltf::GltfAnimationData;
    /// assert!(GltfAnimationData::from_bytes(b"bad glTF").is_err());
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GltfError> {
        let parse = |error| {
            GltfError::new(format!(
                "glTF animation document could not be parsed: {error}"
            ))
        };
        if bytes.starts_with(b"glTF") {
            let glb = gltf::binary::Glb::from_slice(bytes).map_err(parse)?;
            let parsed = gltf::Gltf::from_slice(&glb.json).map_err(parse)?;
            Self::from_document(&parsed.document, glb.bin.as_deref())
        } else {
            let parsed = gltf::Gltf::from_slice(bytes).map_err(parse)?;
            Self::from_document(&parsed.document, None)
        }
    }

    fn from_document(document: &gltf::Document, binary: Option<&[u8]>) -> Result<Self, GltfError> {
        let buffers = load_buffers(document, binary)?;
        let mut nodes = HashMap::new();
        let mut pending = document
            .default_scene()
            .or_else(|| document.scenes().next())
            .map(|scene| scene.nodes().map(|node| (node, true)).collect::<Vec<_>>())
            .unwrap_or_default();
        pending.reverse();
        while let Some((node, root)) = pending.pop() {
            let index = nodes.len();
            if nodes.insert(node.index(), (index, root)).is_some() {
                return Err(GltfError::new("glTF animation scene has a repeated node"));
            }
            pending.extend(
                node.children()
                    .map(|node| (node, false))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev(),
            );
        }
        let mut clips = Vec::new();
        for animation in document.animations() {
            let mut channels = Vec::new();
            let mut transforms = HashMap::new();
            let mut used = HashSet::new();
            let mut duration = 0_f32;
            for channel in animation.channels() {
                let context = format!(
                    "glTF animation {} channel {}",
                    animation.index(),
                    channel.index()
                );
                let target = channel.target();
                let node = target.node();
                let property = target.property();
                if !used.insert((node.index(), property as u8)) {
                    return Err(GltfError::new(format!(
                        "{context} repeats a node/property target"
                    )));
                }
                let sampler = channel.sampler();
                let step = match sampler.interpolation() {
                    gltf::animation::Interpolation::Linear => false,
                    gltf::animation::Interpolation::Step => true,
                    mode => {
                        return Err(GltfError::new(format!(
                            "{context} unsupported interpolation {mode:?} (CUBICSPLINE)"
                        )))
                    }
                };
                let input = sampler.input();
                let times = read_values(&input, 1, false, &buffers, &context)?;
                if times.iter().any(|&time| time < 0.)
                    || times.windows(2).any(|pair| pair[0] >= pair[1])
                {
                    return Err(GltfError::new(format!(
                        "{context} key times must be finite, nonnegative and strictly increasing"
                    )));
                }
                duration = duration.max(*times.last().unwrap());
                let width = match property {
                    gltf::animation::Property::Translation | gltf::animation::Property::Scale => 3,
                    gltf::animation::Property::Rotation => 4,
                    gltf::animation::Property::MorphTargetWeights => {
                        let mesh = node.mesh().ok_or_else(|| {
                            GltfError::new(format!("{context} weight target has no mesh"))
                        })?;
                        let mut counts = mesh
                            .primitives()
                            .map(|primitive| primitive.morph_targets().len());
                        let count = counts.next().unwrap_or(0);
                        if count == 0 || counts.any(|other| other != count) {
                            return Err(GltfError::new(format!("{context} weight target requires consistent nonzero morph target counts")));
                        }
                        count
                    }
                };
                let output = sampler.output();
                let weights = property == gltf::animation::Property::MorphTargetWeights;
                let normalized = weights || property == gltf::animation::Property::Rotation;
                let mut values = read_values(
                    &output,
                    if weights { 1 } else { width },
                    normalized,
                    &buffers,
                    &context,
                )?;
                if values.len()
                    != times.len().checked_mul(width).ok_or_else(|| {
                        GltfError::new(format!("{context} output count overflows"))
                    })?
                {
                    return Err(GltfError::new(format!(
                        "{context} output count does not match keys and target width"
                    )));
                }
                if property == gltf::animation::Property::Rotation {
                    for key in values.chunks_exact_mut(4) {
                        let rotation = Quat::from_array(key.try_into().unwrap());
                        // Normalized integer encodings introduce quantization error.
                        if (rotation.length_squared() - 1.).abs() > 0.02 {
                            return Err(GltfError::new(format!(
                                "{context} rotation keys must be unit quaternions"
                            )));
                        }
                        key.copy_from_slice(&rotation.normalize().to_array());
                    }
                }
                let scene_node = nodes.get(&node.index()).copied();
                if !weights {
                    let gltf::scene::Transform::Decomposed {
                        translation,
                        rotation,
                        scale,
                    } = node.transform()
                    else {
                        return Err(GltfError::new(format!(
                            "{context} TRS channels cannot target a matrix-authored node"
                        )));
                    };
                    if let Some((index, root)) = scene_node {
                        let local = GltfLocalTransform {
                            translation: Vec3::from_array(translation),
                            rotation: Quat::from_array(rotation),
                            scale: Vec3::from_array(scale),
                        };
                        if !local.translation.is_finite()
                            || !local.scale.is_finite()
                            || !local.rotation.is_finite()
                            || (local.rotation.length_squared() - 1.).abs() > 1e-4
                        {
                            return Err(GltfError::new(format!(
                                "{context} authored local TRS is invalid"
                            )));
                        }
                        transforms.insert(index, (local, root));
                    }
                }
                channels.push(Channel {
                    node: scene_node.map(|(index, _)| index),
                    property,
                    step,
                    times,
                    values,
                    width,
                });
            }
            let mut transforms = transforms
                .into_iter()
                .map(|(node, (local, root))| (node, local, root))
                .collect::<Vec<_>>();
            transforms.sort_by_key(|&(node, _, _)| node);
            clips.push(GltfAnimationClip {
                inner: Arc::new(ClipData {
                    name: animation.name().map(str::to_owned),
                    duration,
                    transforms,
                    channels,
                }),
                root_rotation: Quat::IDENTITY,
            });
        }
        Ok(Self { clips })
    }
}

impl GltfAnimationClip {
    /// The optional document-authored clip name.
    /// ```
    /// # fn inspect(clip: &macroquad::gltf::GltfAnimationClip) { let _ = clip.name(); }
    /// ```
    pub fn name(&self) -> Option<&str> {
        self.inner.name.as_deref()
    }

    /// Last key time across all channels, measured from clip time zero.
    /// ```
    /// # fn inspect(clip: &macroquad::gltf::GltfAnimationClip) { assert!(clip.duration() >= 0.); }
    /// ```
    pub fn duration(&self) -> f32 {
        self.inner.duration
    }

    /// Samples finite absolute clip seconds. Each channel clamps to its own
    /// first/last key; negative time also selects its first key. LINEAR rotation
    /// uses normalized shortest-path slerp. STEP switches exactly at each key.
    /// No global clock or instance mutation is involved.
    /// ```
    /// # fn sample(clip: &macroquad::gltf::GltfAnimationClip) {
    /// assert!(clip.sample(f32::NAN).is_err());
    /// let first = clip.sample(0.).unwrap();
    /// # }
    /// ```
    pub fn sample(&self, seconds: f32) -> Result<GltfAnimationSample, GltfError> {
        if !seconds.is_finite() {
            return Err(GltfError::new("glTF animation sample time must be finite"));
        }
        let mut sample = GltfAnimationSample {
            transforms: self
                .inner
                .transforms
                .iter()
                .map(|&(node, local, _)| (node, local))
                .collect(),
            weights: Vec::new(),
        };
        for channel in &self.inner.channels {
            let Some(node) = channel.node else {
                continue;
            };
            let upper = channel.times.partition_point(|&time| time <= seconds);
            let left = upper.saturating_sub(1);
            let right = upper.min(channel.times.len() - 1);
            let mix = if channel.step || left == right {
                0.
            } else {
                ((seconds as f64 - channel.times[left] as f64)
                    / (channel.times[right] as f64 - channel.times[left] as f64))
                    .clamp(0., 1.)
            };
            let a = &channel.values[left * channel.width..(left + 1) * channel.width];
            let b = &channel.values[right * channel.width..(right + 1) * channel.width];
            let values = a
                .iter()
                .zip(b)
                .map(|(&a, &b)| ((1. - mix) * a as f64 + mix * b as f64) as f32)
                .collect::<Vec<_>>();
            if channel.property == gltf::animation::Property::MorphTargetWeights {
                sample.weights.push((node, values));
            } else {
                let index = sample
                    .transforms
                    .binary_search_by_key(&node, |&(node, _)| node)
                    .unwrap();
                let local = &mut sample.transforms[index].1;
                match channel.property {
                    gltf::animation::Property::Translation => {
                        local.translation = Vec3::from_slice(&values)
                    }
                    gltf::animation::Property::Scale => local.scale = Vec3::from_slice(&values),
                    gltf::animation::Property::Rotation => {
                        let a = Quat::from_array(a.try_into().unwrap());
                        let b = Quat::from_array(b.try_into().unwrap());
                        local.rotation = if mix == 0. {
                            a
                        } else if mix == 1. {
                            b
                        } else {
                            a.slerp(b, mix as f32).normalize()
                        };
                    }
                    _ => unreachable!(),
                }
            }
        }
        if self.root_rotation != Quat::IDENTITY {
            for ((_, local), &(_, _, root)) in
                sample.transforms.iter_mut().zip(&self.inner.transforms)
            {
                if root {
                    local.translation = self.root_rotation * local.translation;
                    local.rotation = (self.root_rotation * local.rotation).normalize();
                }
            }
        }
        if sample.transforms.iter().any(|(_, local)| {
            !local.translation.is_finite()
                || !local.scale.is_finite()
                || !local.rotation.is_finite()
        }) {
            return Err(GltfError::new(
                "glTF animation up-axis conversion produced non-finite TRS",
            ));
        }
        Ok(sample)
    }

    /// Applies the draw model's Y-up conversion to animated scene roots.
    /// Children remain parent-local. Repeated calls compose rotations, matching
    /// the model/pose helpers; invalid axes leave a clone unchanged.
    /// ```
    /// # fn convert(clip: &macroquad::gltf::GltfAnimationClip) {
    /// let z_up = clip.with_up_axis(macroquad::math::Vec3::Z);
    /// # }
    /// ```
    pub fn with_up_axis(&self, up: Vec3) -> Self {
        let mut copy = self.clone();
        if let Some(rotation) = super::up_axis_rotation(up) {
            copy.root_rotation = (rotation * copy.root_rotation).normalize();
        }
        copy
    }
}

/// A small independent player driven exclusively by caller-supplied time.
///
/// [`Self::advance`] takes elapsed seconds, whereas [`Self::seek`] takes
/// absolute clip seconds. Pausing freezes advance but still permits seeking.
/// Nonlooping playback clamps at duration; looping uses `[0, duration)` and
/// zero-duration clips stay at zero. Reset rewinds without changing pause/loop.
/// Applying a returned sample to instances is a separate explicit operation.
///
/// ```
/// # fn play(clip: macroquad::gltf::GltfAnimationClip) {
/// let mut player = macroquad::gltf::GltfAnimationPlayer::new(clip);
/// player.set_looping(true);
/// let frame = player.advance(1. / 60.).unwrap();
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct GltfAnimationPlayer {
    clip: GltfAnimationClip,
    time: f32,
    paused: bool,
    looping: bool,
}

impl GltfAnimationPlayer {
    /// Creates a playing, nonlooping player at zero without sampling or uploads.
    /// ```
    /// # fn create(clip: macroquad::gltf::GltfAnimationClip) {
    /// let player = macroquad::gltf::GltfAnimationPlayer::new(clip);
    /// assert_eq!(player.time(), 0.);
    /// # }
    /// ```
    pub fn new(clip: GltfAnimationClip) -> Self {
        Self {
            clip,
            time: 0.,
            paused: false,
            looping: false,
        }
    }
    /// Reads the shared immutable clip.
    /// ```
    /// # fn inspect(player: &macroquad::gltf::GltfAnimationPlayer) { let _ = player.clip(); }
    /// ```
    pub fn clip(&self) -> &GltfAnimationClip {
        &self.clip
    }
    /// Current clip seconds after wrapping/clamping.
    /// ```
    /// # fn inspect(player: &macroquad::gltf::GltfAnimationPlayer) { assert!(player.time() >= 0.); }
    /// ```
    pub fn time(&self) -> f32 {
        self.time
    }
    /// Whether elapsed-time advancement is paused.
    /// ```
    /// # fn inspect(player: &macroquad::gltf::GltfAnimationPlayer) { let _ = player.is_paused(); }
    /// ```
    pub fn is_paused(&self) -> bool {
        self.paused
    }
    /// Changes pause state without changing the sampled time.
    /// ```
    /// # fn pause(player: &mut macroquad::gltf::GltfAnimationPlayer) { player.set_paused(true); }
    /// ```
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
    /// Whether time wraps at the last key.
    /// ```
    /// # fn inspect(player: &macroquad::gltf::GltfAnimationPlayer) { let _ = player.is_looping(); }
    /// ```
    pub fn is_looping(&self) -> bool {
        self.looping
    }
    /// Enables/disables wrapping. Enabling also wraps the current time.
    /// ```
    /// # fn loop_clip(player: &mut macroquad::gltf::GltfAnimationPlayer) { player.set_looping(true); }
    /// ```
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
        self.time = self.position(self.time as f64);
    }
    /// Rewinds to zero, preserving pause and loop settings. Apply `sample()` to
    /// restore the clip's first frame; this does not restore authored rest values.
    /// ```
    /// # fn rewind(player: &mut macroquad::gltf::GltfAnimationPlayer) { player.reset(); }
    /// ```
    pub fn reset(&mut self) {
        self.time = 0.;
    }
    /// Samples current time without advancing or changing pause state.
    /// ```
    /// # fn read(player: &macroquad::gltf::GltfAnimationPlayer) { let frame = player.sample().unwrap(); }
    /// ```
    pub fn sample(&self) -> Result<GltfAnimationSample, GltfError> {
        self.clip.sample(self.time)
    }
    /// Seeks finite nonnegative absolute seconds, wrapping/clamping as configured.
    /// Errors leave playback state unchanged. Seeking is allowed while paused.
    /// ```
    /// # fn seek(player: &mut macroquad::gltf::GltfAnimationPlayer) {
    /// let frame = player.seek(0.5).unwrap();
    /// assert!(player.seek(f32::INFINITY).is_err());
    /// # }
    /// ```
    pub fn seek(&mut self, seconds: f32) -> Result<GltfAnimationSample, GltfError> {
        if !seconds.is_finite() || seconds < 0. {
            return Err(GltfError::new(
                "glTF animation seek requires finite nonnegative seconds",
            ));
        }
        let time = self.position(seconds as f64);
        let sample = self.clip.sample(time)?;
        self.time = time;
        Ok(sample)
    }
    /// Advances by finite nonnegative elapsed seconds supplied by the caller.
    /// A paused player validates the delta but keeps its time. Large finite
    /// deltas are reduced in f64 so addition cannot overflow an f32 clock.
    /// ```
    /// # fn update(player: &mut macroquad::gltf::GltfAnimationPlayer) {
    /// let frame = player.advance(1. / 60.).unwrap();
    /// # }
    /// ```
    pub fn advance(&mut self, delta_seconds: f32) -> Result<GltfAnimationSample, GltfError> {
        if !delta_seconds.is_finite() || delta_seconds < 0. {
            return Err(GltfError::new(
                "glTF animation advance requires finite nonnegative elapsed seconds",
            ));
        }
        let time = if self.paused {
            self.time
        } else {
            self.position(self.time as f64 + delta_seconds as f64)
        };
        let sample = self.clip.sample(time)?;
        self.time = time;
        Ok(sample)
    }
    fn position(&self, seconds: f64) -> f32 {
        let duration = self.clip.duration() as f64;
        if duration == 0. {
            0.
        } else if self.looping {
            let time = seconds.rem_euclid(duration) as f32;
            if time >= self.clip.duration() {
                0.
            } else {
                time
            }
        } else {
            seconds.min(duration) as f32
        }
    }
}

fn read_values(
    accessor: &gltf::Accessor<'_>,
    width: usize,
    integer: bool,
    buffers: &[Cow<'_, [u8]>],
    context: &str,
) -> Result<Vec<f32>, GltfError> {
    use gltf::accessor::{DataType, Dimensions};
    let expected = match width {
        1 => Dimensions::Scalar,
        3 => Dimensions::Vec3,
        4 => Dimensions::Vec4,
        _ => unreachable!(),
    };
    let ty = accessor.data_type();
    let allowed = if ty == DataType::F32 {
        !accessor.normalized()
    } else {
        integer
            && accessor.normalized()
            && matches!(
                ty,
                DataType::I8 | DataType::U8 | DataType::I16 | DataType::U16
            )
    };
    if !allowed || accessor.dimensions() != expected || accessor.count() == 0 {
        return Err(GltfError::new(format!(
            "{context} accessor {} has unsupported format; expected {expected:?} FLOAT{}",
            accessor.index(),
            if integer {
                " or normalized BYTE/UBYTE/SHORT/USHORT"
            } else {
                ""
            }
        )));
    }
    let component = ty.size();
    let size = width * component;
    let count = accessor
        .count()
        .checked_mul(width)
        .ok_or_else(|| GltfError::new(format!("{context} accessor count overflows")))?;
    let mut result = if let Some(view) = accessor.view() {
        let bytes = super::morph::elements(
            &view,
            accessor.offset(),
            accessor.count(),
            size,
            component,
            buffers,
            context,
        )?;
        bytes
            .chunks(view.stride().unwrap_or(size))
            .flat_map(|element| {
                element[..size]
                    .chunks_exact(component)
                    .map(|bytes| number(bytes, ty))
            })
            .collect::<Vec<_>>()
    } else {
        let mut result = Vec::new();
        result
            .try_reserve_exact(count)
            .map_err(|_| GltfError::new(format!("{context} accessor allocation is too large")))?;
        result.resize(count, 0.);
        result
    };
    if let Some(sparse) = accessor.sparse() {
        if sparse.count() == 0 || sparse.count() > accessor.count() {
            return Err(GltfError::new(format!("{context} invalid sparse count")));
        }
        let indices = sparse.indices();
        let values = sparse.values();
        if indices.view().stride().is_some()
            || values.view().stride().is_some()
            || indices.view().target().is_some()
            || values.view().target().is_some()
        {
            return Err(GltfError::new(format!(
                "{context} sparse views cannot have byteStride or target"
            )));
        }
        let index_size = indices.index_type().size();
        let index_bytes = super::morph::elements(
            &indices.view(),
            indices.offset(),
            sparse.count(),
            index_size,
            index_size,
            buffers,
            context,
        )?;
        let value_bytes = super::morph::elements(
            &values.view(),
            values.offset(),
            sparse.count(),
            size,
            component,
            buffers,
            context,
        )?;
        let mut previous = None;
        for (bytes, values) in index_bytes
            .chunks_exact(index_size)
            .zip(value_bytes.chunks_exact(size))
        {
            let index = match index_size {
                1 => bytes[0] as usize,
                2 => u16::from_le_bytes(bytes.try_into().unwrap()) as usize,
                4 => u32::from_le_bytes(bytes.try_into().unwrap()) as usize,
                _ => unreachable!(),
            };
            if index >= accessor.count() || previous.is_some_and(|previous| previous >= index) {
                return Err(GltfError::new(format!(
                    "{context} sparse indices must increase strictly and stay in range"
                )));
            }
            for (destination, bytes) in result[index * width..(index + 1) * width]
                .iter_mut()
                .zip(values.chunks_exact(component))
            {
                *destination = number(bytes, ty);
            }
            previous = Some(index);
        }
    }
    if result.iter().any(|value| !value.is_finite()) {
        return Err(GltfError::new(format!(
            "{context} accessor values must be finite"
        )));
    }
    Ok(result)
}

fn number(bytes: &[u8], ty: gltf::accessor::DataType) -> f32 {
    use gltf::accessor::DataType;
    match ty {
        DataType::F32 => f32::from_le_bytes(bytes.try_into().unwrap()),
        DataType::I8 => ((bytes[0] as i8) as f32 / 127.).max(-1.),
        DataType::U8 => bytes[0] as f32 / 255.,
        DataType::I16 => (i16::from_le_bytes(bytes.try_into().unwrap()) as f32 / 32767.).max(-1.),
        DataType::U16 => u16::from_le_bytes(bytes.try_into().unwrap()) as f32 / 65535.,
        _ => unreachable!(),
    }
}
