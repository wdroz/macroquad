//! Checked sprite-sheet regions and anchored drawing.

use crate::{
    color::Color,
    math::{vec2, Rect, Vec2},
    texture::{draw_texture_ex, DrawTextureParams, Texture2D},
};

/// Why a sprite region or its drawing parameters could not be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteRegionError {
    /// Sheet dimensions must be finite and strictly positive.
    InvalidSheetSize,
    /// A source rectangle needs finite nonnegative coordinates and representable positive extents.
    InvalidSource,
    /// The complete source rectangle must fit inside the sheet.
    SourceOutOfBounds,
    /// Untrimmed dimensions must be positive and contain the complete trimmed image.
    InvalidTrim,
    /// A pivot must be finite; it may lie outside the untrimmed frame.
    InvalidPivot,
    /// Anchor, destination size, rotation or the resulting geometry is invalid.
    InvalidDraw,
    /// The supplied texture's dimensions differ from the region's sheet dimensions.
    TextureSizeMismatch,
}

impl std::fmt::Display for SpriteRegionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSheetSize => "sprite sheet dimensions must be finite and positive",
            Self::InvalidSource => {
                "sprite source must have nonnegative coordinates and positive dimensions"
            }
            Self::SourceOutOfBounds => "sprite source lies outside the sheet",
            Self::InvalidTrim => "sprite trim does not fit inside the untrimmed frame",
            Self::InvalidPivot => "sprite pivot must be finite",
            Self::InvalidDraw => "invalid sprite drawing parameters or resulting geometry",
            Self::TextureSizeMismatch => "sprite region and texture dimensions do not match",
        })
    }
}

impl std::error::Error for SpriteRegionError {}

/// Validated source pixels and optional trimming metadata for one sprite.
///
/// Source coordinates start at the sheet's top-left. `frame_size` describes the
/// original, untrimmed frame; `trim_offset` locates the source image in that frame.
/// Both use source pixels, independently of the active camera or window DPI.
/// Rotated atlas entries are not supported. Fractional source pixels are allowed.
///
/// The pivot is measured from the untrimmed frame's top-left and defaults to zero.
/// Drawing places that point at the caller's anchor; flips and rotation are about
/// that anchor. Frames with different trimming can therefore share one stable
/// pivot. A finite pivot outside the frame is allowed.
///
/// This small value owns no texture or GPU resource. Use the existing [`Texture2D`]
/// ownership/atlas path; [`Self::draw`] checks its dimensions before submitting.
/// Existing [`super::AnimationFrame`] and [`super::Animation`] literals are unchanged.
///
/// ```
/// use macroquad::experimental::animation::SpriteRegion;
/// use macroquad::prelude::*;
/// let frame = SpriteRegion::trimmed(
///     vec2(64.0, 32.0), Rect::new(0.0, 0.0, 12.0, 16.0),
///     vec2(20.0, 24.0), vec2(4.0, 8.0),
/// ).unwrap().with_pivot(vec2(10.0, 24.0)).unwrap();
/// assert_eq!(frame.trim_offset(), vec2(4.0, 8.0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteRegion {
    sheet_size: Vec2,
    source: Rect,
    frame_size: Vec2,
    trim_offset: Vec2,
    pivot: Vec2,
}

/// Transform a sprite's full untrimmed frame in the active drawing coordinates.
///
/// `dest_size` scales the full frame, including omitted transparent margins.
/// Its dimensions must be finite and positive; use flips for mirroring.
/// The default draws at its source-pixel size without rotation or mirroring.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpriteRegionDrawParams {
    /// Full untrimmed destination dimensions; `None` uses [`SpriteRegion::frame_size`].
    pub dest_size: Option<Vec2>,
    /// Rotation in radians around the anchor supplied to the draw call.
    pub rotation: f32,
    /// Mirror horizontally around the supplied anchor.
    pub flip_x: bool,
    /// Mirror vertically around the supplied anchor.
    pub flip_y: bool,
}

impl SpriteRegion {
    /// Validates an untrimmed source rectangle, initially pivoted at its top-left.
    ///
    /// Empty, negative, nonfinite or out-of-bounds rectangles return an error.
    /// No texture or graphics context is needed.
    pub fn new(sheet_size: Vec2, source: Rect) -> Result<Self, SpriteRegionError> {
        Self::trimmed(sheet_size, source, vec2(source.w, source.h), Vec2::ZERO)
    }

    /// Selects a zero-based cell in a grid with no spacing or margins.
    ///
    /// `cell_size` is in source pixels. Incomplete edge cells and zero-size cells
    /// are rejected. The sheet need not be an exact multiple of the cell size.
    /// Large indices are checked before converting the calculated origin to f32.
    /// Normal f32 rounding is allowed, but an origin rounded by half a cell or
    /// more (which could select another cell), or collapsed extents, is rejected.
    ///
    /// ```
    /// use macroquad::experimental::animation::SpriteRegion;
    /// use macroquad::prelude::*;
    /// let frame = SpriteRegion::from_grid(vec2(48.0, 32.0), vec2(16.0, 16.0), 2, 1).unwrap();
    /// assert_eq!(frame.source(), Rect::new(32.0, 16.0, 16.0, 16.0));
    /// ```
    pub fn from_grid(
        sheet_size: Vec2,
        cell_size: Vec2,
        column: u32,
        row: u32,
    ) -> Result<Self, SpriteRegionError> {
        if !positive_size(sheet_size) {
            return Err(SpriteRegionError::InvalidSheetSize);
        }
        if !positive_size(cell_size) {
            return Err(SpriteRegionError::InvalidSource);
        }
        let x = f64::from(cell_size.x) * f64::from(column);
        let y = f64::from(cell_size.y) * f64::from(row);
        if x + f64::from(cell_size.x) > f64::from(sheet_size.x)
            || y + f64::from(cell_size.y) > f64::from(sheet_size.y)
        {
            return Err(SpriteRegionError::SourceOutOfBounds);
        }
        if (f64::from(x as f32) - x).abs() >= f64::from(cell_size.x) * 0.5
            || (f64::from(y as f32) - y).abs() >= f64::from(cell_size.y) * 0.5
        {
            return Err(SpriteRegionError::InvalidSource);
        }
        Self::new(
            sheet_size,
            Rect::new(x as f32, y as f32, cell_size.x, cell_size.y),
        )
    }

    /// Validates a trimmed rectangle and its original frame dimensions/offset.
    ///
    /// The source is not rescaled within the original frame. Its width/height
    /// plus the nonnegative trim offset must fit inside `frame_size`.
    pub fn trimmed(
        sheet_size: Vec2,
        source: Rect,
        frame_size: Vec2,
        trim_offset: Vec2,
    ) -> Result<Self, SpriteRegionError> {
        if !positive_size(sheet_size) {
            return Err(SpriteRegionError::InvalidSheetSize);
        }
        if !nonnegative_point(vec2(source.x, source.y)) || !positive_size(vec2(source.w, source.h))
        {
            return Err(SpriteRegionError::InvalidSource);
        }
        if !fits(
            vec2(source.x, source.y),
            vec2(source.w, source.h),
            sheet_size,
        ) {
            return Err(SpriteRegionError::SourceOutOfBounds);
        }
        if source.x + source.w <= source.x || source.y + source.h <= source.y {
            return Err(SpriteRegionError::InvalidSource);
        }
        if !positive_size(frame_size)
            || !nonnegative_point(trim_offset)
            || !fits(trim_offset, vec2(source.w, source.h), frame_size)
            || trim_offset.x + source.w <= trim_offset.x
            || trim_offset.y + source.h <= trim_offset.y
        {
            return Err(SpriteRegionError::InvalidTrim);
        }
        Ok(Self {
            sheet_size,
            source,
            frame_size,
            trim_offset,
            pivot: Vec2::ZERO,
        })
    }

    /// Selects a finite pivot in untrimmed source pixels, including outside points.
    pub fn with_pivot(mut self, pivot: Vec2) -> Result<Self, SpriteRegionError> {
        if !pivot.is_finite() {
            return Err(SpriteRegionError::InvalidPivot);
        }
        self.pivot = pivot;
        Ok(self)
    }

    /// Original sheet dimensions in source pixels.
    pub const fn sheet_size(&self) -> Vec2 {
        self.sheet_size
    }

    /// Validated source rectangle in sheet pixels.
    pub const fn source(&self) -> Rect {
        self.source
    }

    /// Dimensions of the complete frame before transparent margins were removed.
    pub const fn frame_size(&self) -> Vec2 {
        self.frame_size
    }

    /// Source image's top-left position within the untrimmed frame.
    pub const fn trim_offset(&self) -> Vec2 {
        self.trim_offset
    }

    /// Anchor location measured from the untrimmed frame's top-left.
    pub const fn pivot(&self) -> Vec2 {
        self.pivot
    }

    /// Computes a position and ordinary texture parameters without a graphics context.
    ///
    /// `anchor` is in the active camera's drawing coordinates (logical pixels with
    /// the default camera). The returned position is the trimmed quad's unrotated
    /// top-left; rotation is left in the returned parameters for [`draw_texture_ex`].
    /// Its pivot is always `anchor`. Flips mirror the trim offset as well as texels,
    /// keeping the chosen visible pivot at the anchor across differently trimmed frames.
    ///
    /// Returns an error for nonfinite inputs, nonpositive destination sizes or
    /// unrepresentable resulting f32 geometry. The returned source remains in
    /// original sheet pixels, so ordinary texture atlas remapping is preserved.
    /// Unlike [`Self::draw`], this method cannot check the texture dimensions.
    ///
    /// This asymmetric trim moves across the anchor when mirrored; the source
    /// rectangle and anchor stay the same:
    ///
    /// ```
    /// use macroquad::experimental::animation::{SpriteRegion, SpriteRegionDrawParams};
    /// use macroquad::prelude::*;
    ///
    /// let frame = SpriteRegion::trimmed(
    ///     vec2(64.0, 32.0), Rect::new(24.0, 0.0, 8.0, 16.0),
    ///     vec2(24.0, 24.0), vec2(4.0, 8.0),
    /// ).unwrap().with_pivot(vec2(12.0, 24.0)).unwrap();
    /// let anchor = vec2(100.0, 160.0);
    /// let params = SpriteRegionDrawParams {
    ///     dest_size: Some(vec2(48.0, 48.0)), // Full frame, including missing margins.
    ///     ..Default::default()
    /// };
    /// let (left, normal) = frame.draw_params(anchor, params).unwrap();
    /// let (right, mirrored) = frame.draw_params(anchor, SpriteRegionDrawParams {
    ///     flip_x: true, ..params
    /// }).unwrap();
    /// assert_eq!(left, vec2(84.0, 128.0));
    /// assert_eq!(right, vec2(100.0, 128.0));
    /// assert_eq!(normal.dest_size, Some(vec2(16.0, 32.0)));
    /// assert_eq!(normal.source, mirrored.source);
    /// assert_eq!(mirrored.pivot, Some(anchor));
    /// ```
    pub fn draw_params(
        &self,
        anchor: Vec2,
        params: SpriteRegionDrawParams,
    ) -> Result<(Vec2, DrawTextureParams), SpriteRegionError> {
        let dest_size = params.dest_size.unwrap_or(self.frame_size);
        if !anchor.is_finite() || !positive_size(dest_size) || !params.rotation.is_finite() {
            return Err(SpriteRegionError::InvalidDraw);
        }
        let scale_x = f64::from(dest_size.x) / f64::from(self.frame_size.x);
        let scale_y = f64::from(dest_size.y) / f64::from(self.frame_size.y);
        let offset_x = if params.flip_x {
            f64::from(self.pivot.x) - f64::from(self.trim_offset.x) - f64::from(self.source.w)
        } else {
            f64::from(self.trim_offset.x) - f64::from(self.pivot.x)
        };
        let offset_y = if params.flip_y {
            f64::from(self.pivot.y) - f64::from(self.trim_offset.y) - f64::from(self.source.h)
        } else {
            f64::from(self.trim_offset.y) - f64::from(self.pivot.y)
        };
        let position = vec2(
            (f64::from(anchor.x) + offset_x * scale_x) as f32,
            (f64::from(anchor.y) + offset_y * scale_y) as f32,
        );
        let size = vec2(
            (f64::from(self.source.w) * scale_x) as f32,
            (f64::from(self.source.h) * scale_y) as f32,
        );
        if !position.is_finite() || !positive_size(size) {
            return Err(SpriteRegionError::InvalidDraw);
        }
        if position.x + size.x <= position.x || position.y + size.y <= position.y {
            return Err(SpriteRegionError::InvalidDraw);
        }
        // draw_texture_ex rotates in f32. Validate that arithmetic too, rather
        // than admitting an overflowing intermediate hidden by a finite f64 sum.
        let (sin, cos) = params.rotation.sin_cos();
        let (x, width) = if params.flip_x {
            (position.x + size.x, -size.x)
        } else {
            (position.x, size.x)
        };
        let (y, height) = if params.flip_y {
            (position.y + size.y, -size.y)
        } else {
            (position.y, size.y)
        };
        let mut rotated_corners = [Vec2::ZERO; 4];
        for (i, corner) in [
            vec2(x, y),
            vec2(x + width, y),
            vec2(x + width, y + height),
            vec2(x, y + height),
        ]
        .into_iter()
        .enumerate()
        {
            let relative = corner - anchor;
            let rotated = vec2(
                relative.x * cos - relative.y * sin,
                relative.x * sin + relative.y * cos,
            ) + anchor;
            if !relative.is_finite() || !rotated.is_finite() {
                return Err(SpriteRegionError::InvalidDraw);
            }
            rotated_corners[i] = rotated;
        }
        if (0..4).any(|i| rotated_corners[i] == rotated_corners[(i + 1) % 4]) {
            return Err(SpriteRegionError::InvalidDraw);
        }
        Ok((
            position,
            DrawTextureParams {
                source: Some(self.source),
                dest_size: Some(size),
                rotation: params.rotation,
                flip_x: params.flip_x,
                flip_y: params.flip_y,
                pivot: Some(anchor),
            },
        ))
    }

    /// Draws through [`draw_texture_ex`] with the existing texture/atlas lifetime rules.
    ///
    /// Dimensions must match [`Self::sheet_size`]; the texture stays caller-owned.
    /// No commands are submitted on validation failure. This call requires an
    /// initialized graphics context, just like ordinary texture drawing.
    ///
    /// ```no_run
    /// use macroquad::experimental::animation::{SpriteRegion, SpriteRegionDrawParams};
    /// use macroquad::prelude::*;
    ///
    /// // Create/load textures once, then reuse their checked regions each frame.
    /// let image = Image::gen_image_color(32, 16, SKYBLUE);
    /// let sheet = Texture2D::from_image(&image);
    /// let frame = SpriteRegion::from_grid(sheet.size(), vec2(16.0, 16.0), 1, 0)
    ///     .unwrap().with_pivot(vec2(8.0, 16.0)).unwrap();
    /// frame.draw(&sheet, vec2(100.0, 160.0), WHITE, SpriteRegionDrawParams {
    ///     dest_size: Some(vec2(32.0, 32.0)),
    ///     flip_x: true,
    ///     ..Default::default()
    /// }).unwrap();
    /// ```
    pub fn draw(
        &self,
        texture: &Texture2D,
        anchor: Vec2,
        color: Color,
        params: SpriteRegionDrawParams,
    ) -> Result<(), SpriteRegionError> {
        let (position, params) = self.draw_params(anchor, params)?;
        if texture.size() != self.sheet_size {
            return Err(SpriteRegionError::TextureSizeMismatch);
        }
        draw_texture_ex(texture, position.x, position.y, color, params);
        Ok(())
    }
}

fn positive_size(size: Vec2) -> bool {
    size.is_finite() && size.x > 0.0 && size.y > 0.0
}

fn nonnegative_point(point: Vec2) -> bool {
    point.is_finite() && point.x >= 0.0 && point.y >= 0.0
}

fn fits(offset: Vec2, size: Vec2, bounds: Vec2) -> bool {
    // Subtract before comparison so a tiny positive size cannot disappear in
    // addition to a very large offset, even in f64.
    f64::from(size.x) <= f64::from(bounds.x) - f64::from(offset.x)
        && f64::from(size.y) <= f64::from(bounds.y) - f64::from(offset.y)
        && f64::from(offset.x) <= f64::from(bounds.x) - f64::from(size.x)
        && f64::from(offset.y) <= f64::from(bounds.y) - f64::from(size.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> SpriteRegion {
        SpriteRegion::trimmed(
            vec2(64.0, 32.0),
            Rect::new(8.0, 4.0, 12.0, 16.0),
            vec2(20.0, 24.0),
            vec2(3.0, 6.0),
        )
        .unwrap()
        .with_pivot(vec2(10.0, 20.0))
        .unwrap()
    }

    #[test]
    fn explicit_and_grid_regions_validate_complete_bounds() {
        let region = SpriteRegion::from_grid(vec2(49.0, 34.0), vec2(16.0, 16.0), 2, 1).unwrap();
        assert_eq!(region.source(), Rect::new(32.0, 16.0, 16.0, 16.0));
        assert_eq!(region.frame_size(), vec2(16.0, 16.0));
        assert_eq!(region.pivot(), Vec2::ZERO);
        assert_eq!(region.trim_offset(), Vec2::ZERO);
        assert_eq!(
            SpriteRegion::from_grid(vec2(49.0, 34.0), vec2(16.0, 16.0), 3, 1),
            Err(SpriteRegionError::SourceOutOfBounds)
        );
        assert_eq!(
            SpriteRegion::from_grid(vec2(49.0, 34.0), vec2(16.0, 16.0), u32::MAX, 0),
            Err(SpriteRegionError::SourceOutOfBounds)
        );
        assert!(SpriteRegion::new(vec2(64.0, 32.0), Rect::new(60.0, 30.0, 4.0, 2.0)).is_ok());
        assert!(SpriteRegion::new(vec2(64.0, 32.0), Rect::new(63.75, 30.0, 0.5, 2.0)).is_err());
        assert!(SpriteRegion::new(vec2(64.0, 32.0), Rect::new(63.5, 30.5, 0.5, 1.5)).is_ok());
        assert!(SpriteRegion::from_grid(vec2(1.0, 1.0), vec2(0.1, 0.1), 3, 3).is_ok());
        assert_eq!(
            SpriteRegion::from_grid(vec2(32_000_000.0, 1.0), vec2(1.0, 1.0), 16_777_217, 0),
            Err(SpriteRegionError::InvalidSource)
        );
    }

    #[test]
    fn rejects_invalid_sheet_source_and_trim_values() {
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                SpriteRegion::new(vec2(bad, 32.0), Rect::new(0.0, 0.0, 1.0, 1.0)),
                Err(SpriteRegionError::InvalidSheetSize)
            );
            assert_eq!(
                SpriteRegion::new(vec2(32.0, 32.0), Rect::new(0.0, 0.0, 1.0, bad)),
                Err(SpriteRegionError::InvalidSource)
            );
            assert!(SpriteRegion::from_grid(vec2(32.0, 32.0), vec2(bad, 16.0), 0, 0).is_err());
            assert!(SpriteRegion::trimmed(
                vec2(32.0, 32.0),
                Rect::new(0.0, 0.0, 4.0, 4.0),
                vec2(8.0, bad),
                Vec2::ZERO
            )
            .is_err());
        }
        for offset in [
            vec2(-1.0, 0.0),
            vec2(0.0, f32::NAN),
            vec2(f32::INFINITY, 0.0),
        ] {
            assert!(
                SpriteRegion::new(vec2(32.0, 32.0), Rect::new(offset.x, offset.y, 4.0, 4.0))
                    .is_err()
            );
            assert!(SpriteRegion::trimmed(
                vec2(32.0, 32.0),
                Rect::new(0.0, 0.0, 4.0, 4.0),
                vec2(8.0, 8.0),
                offset
            )
            .is_err());
        }
        assert_eq!(
            SpriteRegion::trimmed(
                vec2(32.0, 32.0),
                Rect::new(0.0, 0.0, 4.0, 4.0),
                vec2(8.0, 8.0),
                vec2(4.25, 0.0)
            ),
            Err(SpriteRegionError::InvalidTrim)
        );
        assert!(SpriteRegion::trimmed(
            vec2(32.0, 32.0),
            Rect::new(0.0, 0.0, 4.0, 4.0),
            vec2(8.0, 8.0),
            vec2(4.0, 4.0)
        )
        .is_ok());
    }

    #[test]
    fn bounds_checks_do_not_round_away_a_positive_overhang() {
        let size = vec2(16_777_216.0, 32.0);
        assert_eq!(
            SpriteRegion::new(size, Rect::new(size.x, 0.0, 1.0, 1.0)),
            Err(SpriteRegionError::SourceOutOfBounds)
        );
        assert_eq!(
            SpriteRegion::trimmed(size, Rect::new(0.0, 0.0, 1.0, 1.0), size, vec2(size.x, 0.0)),
            Err(SpriteRegionError::InvalidTrim)
        );
        assert_eq!(
            SpriteRegion::new(vec2(f32::MAX, 1.0), Rect::new(f32::MAX, 0.0, 1.0, 1.0)),
            Err(SpriteRegionError::SourceOutOfBounds)
        );
        assert_eq!(
            SpriteRegion::trimmed(
                vec2(1.0, 1.0),
                Rect::new(0.0, 0.0, 1.0, 1.0),
                vec2(f32::MAX, 1.0),
                vec2(f32::MAX, 0.0)
            ),
            Err(SpriteRegionError::InvalidTrim)
        );
        assert_eq!(
            SpriteRegion::new(vec2(f32::MAX, 1.0), Rect::new(16_777_216.0, 0.0, 1.0, 1.0)),
            Err(SpriteRegionError::InvalidSource)
        );
    }

    #[test]
    fn pivot_is_finite_but_may_be_outside_frame() {
        assert!(frame().with_pivot(vec2(-10.0, 100.0)).is_ok());
        for pivot in [vec2(f32::NAN, 0.0), vec2(0.0, f32::INFINITY)] {
            assert_eq!(
                frame().with_pivot(pivot),
                Err(SpriteRegionError::InvalidPivot)
            );
        }
    }

    #[test]
    fn trim_offsets_and_destination_sizes_preserve_anchor_for_all_flips() {
        let frame = frame();
        let anchor = vec2(120.0, 80.0);
        for flip_x in [false, true] {
            for flip_y in [false, true] {
                let (position, params) = frame
                    .draw_params(
                        anchor,
                        SpriteRegionDrawParams {
                            dest_size: Some(vec2(40.0, 72.0)),
                            flip_x,
                            flip_y,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                assert_eq!(params.dest_size, Some(vec2(24.0, 48.0)));
                assert_eq!(params.source, Some(frame.source()));
                assert_eq!(params.pivot, Some(anchor));
                assert_eq!(position.x, if flip_x { 110.0 } else { 106.0 });
                assert_eq!(position.y, if flip_y { 74.0 } else { 38.0 });
                // Source point corresponding to the full-frame pivot lands on
                // the anchor, even though the quad's top-left changes on flips.
                let local_pivot = (frame.pivot() - frame.trim_offset()) * vec2(2.0, 3.0);
                let local_pivot = vec2(
                    if flip_x {
                        24.0 - local_pivot.x
                    } else {
                        local_pivot.x
                    },
                    if flip_y {
                        48.0 - local_pivot.y
                    } else {
                        local_pivot.y
                    },
                );
                assert_eq!(position + local_pivot, anchor);
            }
        }
    }

    #[test]
    fn differently_trimmed_frames_share_a_rotated_anchor() {
        let a = frame();
        let b = SpriteRegion::trimmed(
            a.sheet_size(),
            Rect::new(30.0, 0.0, 8.0, 10.0),
            a.frame_size(),
            vec2(7.0, 12.0),
        )
        .unwrap()
        .with_pivot(a.pivot())
        .unwrap();
        let anchor = vec2(-12.0, 300.0);
        for region in [a, b] {
            let (position, params) = region
                .draw_params(
                    anchor,
                    SpriteRegionDrawParams {
                        rotation: 1.234,
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(position + region.pivot() - region.trim_offset(), anchor);
            assert_eq!(params.pivot, Some(anchor));
            assert_eq!(params.rotation, 1.234);
        }
    }

    #[test]
    fn rejects_invalid_draws_and_overflowing_geometry() {
        let frame = frame();
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(frame
                .draw_params(
                    Vec2::ZERO,
                    SpriteRegionDrawParams {
                        dest_size: Some(vec2(bad, 24.0)),
                        ..Default::default()
                    }
                )
                .is_err());
        }
        assert!(frame
            .draw_params(vec2(f32::INFINITY, 0.0), Default::default())
            .is_err());
        assert!(frame
            .draw_params(
                Vec2::ZERO,
                SpriteRegionDrawParams {
                    rotation: f32::NAN,
                    ..Default::default()
                }
            )
            .is_err());
        assert!(frame
            .with_pivot(vec2(-f32::MAX, 0.0))
            .unwrap()
            .draw_params(vec2(f32::MAX, 0.0), Default::default())
            .is_err());
        assert!(frame
            .draw_params(vec2(f32::MAX, 0.0), Default::default())
            .is_err());
        // f32 division overflows, but f64 intermediate ratios preserve this
        // representable destination for a small source and frame alike.
        let tiny = SpriteRegion::new(vec2(1.0, 1.0), Rect::new(0.0, 0.0, 1e-30, 1e-30)).unwrap();
        let (_, params) = tiny
            .draw_params(
                Vec2::ZERO,
                SpriteRegionDrawParams {
                    dest_size: Some(vec2(1e10, 1e10)),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(params.dest_size, Some(vec2(1e10, 1e10)));
    }

    #[test]
    fn animation_struct_literals_remain_usable() {
        let _ = super::super::Animation {
            name: "idle".into(),
            row: 0,
            frames: 1,
            fps: 12,
        };
        let animation_frame = super::super::AnimationFrame {
            source_rect: frame().source(),
            dest_size: frame().frame_size(),
        };
        assert!(SpriteRegion::new(frame().sheet_size(), animation_frame.source_rect).is_ok());
    }
}
