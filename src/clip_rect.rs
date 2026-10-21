//! Exact physical clip bounds, rounded only after nested intersection.
use crate::math::Rect;

pub(crate) type Scissor = (i32, i32, i32, i32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ClipRect {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl ClipRect {
    const EMPTY: Self = Self {
        left: 0.,
        top: 0.,
        right: 0.,
        bottom: 0.,
    };

    pub(crate) fn from_rect(rect: Rect, scale: f32) -> Self {
        if ![rect.x, rect.y, rect.w, rect.h, scale]
            .iter()
            .all(|v| v.is_finite())
            || rect.w <= 0.
            || rect.h <= 0.
            || scale <= 0.
        {
            return Self::EMPTY;
        }
        let scale = f64::from(scale);
        Self {
            left: f64::from(rect.x) * scale,
            top: f64::from(rect.y) * scale,
            right: (f64::from(rect.x) + f64::from(rect.w)) * scale,
            bottom: (f64::from(rect.y) + f64::from(rect.h)) * scale,
        }
    }

    pub(crate) fn from_scissor((x, y, w, h): Scissor) -> Self {
        if w <= 0 || h <= 0 {
            return Self::EMPTY;
        }
        Self {
            left: f64::from(x),
            top: f64::from(y),
            right: f64::from(x) + f64::from(w),
            bottom: f64::from(y) + f64::from(h),
        }
    }

    pub(crate) fn intersect(self, other: Self) -> Self {
        let result = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        if result.right <= result.left || result.bottom <= result.top {
            Self::EMPTY
        } else {
            result
        }
    }

    /// Bounds are clamped to a valid surface before integer conversion.
    pub(crate) fn scissor(self) -> Scissor {
        if self.right <= self.left || self.bottom <= self.top {
            return (0, 0, 0, 0);
        }
        let left = self.left.floor() as i32;
        let top = self.top.floor() as i32;
        (
            left,
            top,
            self.right.ceil() as i32 - left,
            self.bottom.ceil() as i32 - top,
        )
    }
}

pub(crate) fn surface_clip(width: u32, height: u32, viewport: Option<Scissor>) -> ClipRect {
    let width = width.min(i32::MAX as u32) as i32;
    let height = height.min(i32::MAX as u32) as i32;
    let surface = ClipRect::from_scissor((0, 0, width, height));
    if let Some((x, y, w, h)) = viewport {
        if w <= 0 || h <= 0 {
            return ClipRect::EMPTY;
        }
        // A GL viewport is bottom-left based, while our clips are top-left.
        surface.intersect(ClipRect {
            left: f64::from(x),
            right: f64::from(x) + f64::from(w),
            top: f64::from(height) - f64::from(y) - f64::from(h),
            bottom: f64::from(height) - f64::from(y),
        })
    } else {
        surface
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: f32, y: f32, w: f32, h: f32, dpi: f32) -> ClipRect {
        ClipRect::from_rect(Rect::new(x, y, w, h), dpi)
    }

    #[test]
    fn logical_edges_round_outward_at_current_density() {
        assert_eq!(rect(1.25, 2.25, 3.5, 4.5, 1.).scissor(), (1, 2, 4, 5));
        assert_eq!(rect(1.25, 2.25, 3.5, 4.5, 2.).scissor(), (2, 4, 8, 10));
        assert_eq!(rect(1.25, 2.25, 3.5, 4.5, 1.5).scissor(), (1, 3, 7, 8));
    }

    #[test]
    fn nested_intersections_are_exact_before_rounding() {
        let outer = rect(0.1, 0.1, 0.1, 0.1, 1.);
        let disjoint = rect(0.3, 0.3, 0.1, 0.1, 1.);
        assert_eq!(outer.scissor(), disjoint.scissor());
        assert_eq!(outer.intersect(disjoint).scissor(), (0, 0, 0, 0));
        assert_eq!(
            outer.intersect(disjoint).intersect(outer).scissor(),
            (0, 0, 0, 0)
        );
        assert_eq!(
            rect(10., 10., 20., 20., 1.)
                .intersect(rect(20., 15., 20., 10., 1.))
                .scissor(),
            (20, 15, 10, 10)
        );
    }

    #[test]
    fn viewport_origin_and_bounds_use_physical_target_height() {
        assert_eq!(
            surface_clip(100, 80, Some((10, 5, 50, 20))).scissor(),
            (10, 55, 50, 20)
        );
        assert_eq!(
            surface_clip(100, 80, Some((-10, -5, 50, 20))).scissor(),
            (0, 65, 40, 15)
        );
        assert_eq!(
            surface_clip(100, 80, Some((90, 70, 50, 20))).scissor(),
            (90, 0, 10, 10)
        );
        assert_eq!(
            surface_clip(100, 80, Some((0, 0, 0, 20))).scissor(),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn empty_invalid_and_extreme_rectangles_fail_closed_or_clamp() {
        for invalid in [
            Rect::new(0., 0., 0., 10.),
            Rect::new(0., 0., 10., -1.),
            Rect::new(f32::NAN, 0., 1., 1.),
            Rect::new(0., f32::INFINITY, 1., 1.),
            Rect::new(0., 0., f32::INFINITY, 1.),
        ] {
            assert_eq!(ClipRect::from_rect(invalid, 1.).scissor(), (0, 0, 0, 0));
        }
        let surface = surface_clip(100, 80, None);
        assert_eq!(
            rect(-f32::MAX, -f32::MAX, f32::MAX, f32::MAX, 2.)
                .intersect(surface)
                .scissor(),
            (0, 0, 0, 0)
        );
        assert_eq!(
            rect(-1., -1., f32::MAX, f32::MAX, 2.)
                .intersect(surface)
                .scissor(),
            (0, 0, 100, 80)
        );
        assert_eq!(
            surface
                .intersect(ClipRect::from_scissor((90, 70, i32::MAX, i32::MAX)))
                .scissor(),
            (90, 70, 10, 10)
        );
    }
}
