//! Applying owned CPU animation samples to existing pose/morph APIs.

use crate::gltf::GltfAnimationSample;

use super::{GltfHierarchyPose, GltfMorphInstance};

impl GltfAnimationSample {
    /// Replaces animated nodes' full local TRS and propagates their descendants.
    ///
    /// Nodes outside this sample retain their current locals. Within an animated
    /// node, non-targeted properties use authored source values, not a prior
    /// application edit. Keep samples and instances with the same source and
    /// up-axis conversion. Cloned CPU pose state stages all edits atomically.
    /// No geometry evaluation, GPU upload or global clock access occurs.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// # fn update(sample: &GltfAnimationSample, pose: &mut GltfHierarchyPose) -> Result<(), String> {
    /// sample.apply_pose(pose)?;
    /// # Ok(()) }
    /// ```
    pub fn apply_pose(&self, pose: &mut GltfHierarchyPose) -> Result<(), String> {
        pose.set_animation_locals(&self.transforms)
    }

    /// Applies sampled weights using the existing CPU morph evaluator/uploads.
    ///
    /// Returns whether any node's weights changed. Identical finite weights
    /// reuse their geometry without evaluation or uploads. A clone captures
    /// existing immutable buffers while staging all weight changes; an error
    /// leaves the caller's weights, geometry and bounds unchanged. Earlier staged
    /// nodes may have uploaded temporary buffers before a later node fails;
    /// those follow the ordinary queued resource retirement boundary.
    /// The matching source document is required, as for all numeric node IDs.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn update(sample: &GltfAnimationSample, morph: &mut GltfMorphInstance) -> Result<(), String> {
    /// let changed = sample.apply_morph(morph)?;
    /// # Ok(()) }
    /// ```
    pub fn apply_morph(&self, morph: &mut GltfMorphInstance) -> Result<bool, String> {
        let mut staged = morph.clone();
        let mut changed = false;
        for (node, weights) in &self.weights {
            changed |= staged.set_weights(*node, weights)?;
        }
        *morph = staged;
        Ok(changed)
    }

    /// Applies transform and weight channels as one atomic instance update.
    ///
    /// Pass `None` only for a sample without weight channels. Invalid poses,
    /// missing morph instances, malformed weights and failed morph evaluation
    /// preserve both caller instances. Uploads remain synchronous and temporary
    /// buffers on a failed staged update follow [`Self::apply_morph`]'s rules.
    /// Playback time belongs to the player and is separate from this commit.
    /// Returns whether morph weights changed, not whether a transform changed;
    /// a successful transform-only sample returns `false`.
    /// Transform-only samples use the atomic pose update directly and leave
    /// any supplied morph instance untouched.
    /// If applying fails, a caller's preceding player `advance`/`seek` is not
    /// rolled back; keep a player clone when a clock checkpoint is needed.
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    /// # fn update(sample: &GltfAnimationSample, pose: &mut GltfHierarchyPose, morph: &mut GltfMorphInstance) -> Result<(), String> {
    /// sample.apply(pose, Some(morph))?;
    /// # Ok(()) }
    /// ```
    pub fn apply(
        &self,
        pose: &mut GltfHierarchyPose,
        morph: Option<&mut GltfMorphInstance>,
    ) -> Result<bool, String> {
        if morph.is_none() && !self.weights.is_empty() {
            return Err(
                "glTF animation sample has weight channels but no morph instance".to_owned(),
            );
        }
        if self.weights.is_empty() {
            self.apply_pose(pose)?;
            return Ok(false);
        }
        let mut staged_pose = pose.clone();
        self.apply_pose(&mut staged_pose)?;
        let changed = if let Some(morph) = morph {
            self.apply_morph(morph)?
        } else {
            false
        };
        *pose = staged_pose;
        Ok(changed)
    }
}
