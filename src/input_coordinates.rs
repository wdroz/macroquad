//! Pure bookkeeping for input stored in framebuffer pixels.

use glam::Vec2;

pub(crate) fn valid_density(density: f32) -> f32 {
    if density.is_finite() && density > 0.0 {
        density
    } else {
        1.0
    }
}

pub(crate) fn to_physical(position: Vec2, density: f32) -> Vec2 {
    position * valid_density(density)
}

pub(crate) fn to_logical(position: Vec2, density: f32) -> Vec2 {
    position / valid_density(density)
}

pub(crate) struct PointerCoordinates {
    size: Vec2,
    density: f32,
    previous: Option<Vec2>,
}

impl PointerCoordinates {
    pub(crate) fn new(size: Vec2, density: f32) -> Self {
        Self {
            size,
            density: valid_density(density),
            previous: None,
        }
    }

    /// Returns the ratio for keeping existing positions at the same logical
    /// location when density changes. Call before accepting new physical input.
    pub(crate) fn sync(&mut self, size: Vec2, density: f32) -> f32 {
        let density = valid_density(density);
        let ratio = density / self.density;
        if self.size != size || self.density != density {
            self.previous = None;
            self.size = size;
            self.density = density;
        }
        ratio
    }

    pub(crate) fn reset(&mut self) {
        self.previous = None;
    }

    pub(crate) fn finish_frame(&mut self, position: Vec2) {
        self.previous = Some(position);
    }

    pub(crate) fn delta(&self, position: Vec2) -> Vec2 {
        position - self.previous.unwrap_or(position)
    }

    pub(crate) fn legacy_delta(&self, position: Vec2) -> Vec2 {
        let delta = self.delta(position);
        if self.size.x > 0.0 && self.size.y > 0.0 {
            -delta / self.size * 2.0
        } else {
            Vec2::ZERO
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_and_touch_targets_round_trip_at_current_density() {
        let target = Vec2::new(140.25, 91.5);
        for density in [1.0, 1.25, 2.0, 3.0] {
            let physical = to_physical(target, density);
            assert_eq!(to_logical(physical, density), target);
            assert_eq!(physical, target * density);
        }
        assert_eq!(to_logical(target, 0.0), target);
        assert_eq!(to_physical(target, f32::NAN), target);
    }

    #[test]
    fn relative_units_and_legacy_sign_stay_distinct() {
        for density in [1.0, 2.0] {
            let mut state = PointerCoordinates::new(Vec2::new(800.0, 600.0) * density, density);
            let start = to_physical(Vec2::new(100.0, 80.0), density);
            let end = start + to_physical(Vec2::new(4.0, -3.0), density);
            assert_eq!(state.delta(start), Vec2::ZERO);
            state.finish_frame(start);
            assert_eq!(state.delta(end), Vec2::new(4.0, -3.0) * density);
            assert_eq!(to_logical(state.delta(end), density), Vec2::new(4.0, -3.0));
            assert_eq!(state.legacy_delta(end), Vec2::new(-0.01, 0.01));
            // Reading a delta does not consume it.
            assert_eq!(state.delta(end), state.delta(end));
            state.finish_frame(end);
            assert_eq!(state.delta(end), Vec2::ZERO);
        }
    }

    #[test]
    fn resize_and_density_changes_clear_history_without_camera_jumps() {
        let size = Vec2::new(800.0, 600.0);
        let mut state = PointerCoordinates::new(size, 1.0);
        let position = Vec2::new(200.0, 150.0);
        state.finish_frame(position);
        assert_eq!(state.sync(size * 1.5, 1.0), 1.0);
        assert_eq!(state.delta(position), Vec2::ZERO);
        assert_eq!(state.legacy_delta(position), Vec2::ZERO);
        state.finish_frame(position);
        let ratio = state.sync(size * 1.5, 2.0); // Density-only change, no resize.
        assert_eq!(ratio, 2.0);
        let position = position * ratio;
        assert_eq!(state.delta(position), Vec2::ZERO);
        assert_eq!(to_logical(position, 2.0), Vec2::new(200.0, 150.0));
        state.finish_frame(position);
        assert_eq!(state.sync(size * 1.5, 2.0), 1.0);
        assert_eq!(state.delta(position + Vec2::X), Vec2::X);
        state.reset();
        assert_eq!(state.delta(position + Vec2::X), Vec2::ZERO);
    }
}
