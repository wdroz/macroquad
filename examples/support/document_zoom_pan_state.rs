//! Application policy in logical window pixels; no GPU/window state is needed.
use macroquad::prelude::*;

pub const PAGE: Vec2 = Vec2::new(600., 800.);
pub const MARKER: Rect = Rect::new(180., 250., 240., 100.);
pub const FIT_BUTTON: Rect = Rect::new(16., 96., 64., 24.);
pub const RESET_BUTTON: Rect = Rect::new(88., 96., 104., 24.);
pub const MAX_ZOOM: f32 = 4.;

pub fn viewport(size: Vec2) -> Rect {
    Rect::new(16., 128., (size.x - 32.).max(1.), (size.y - 164.).max(1.))
}

// Half-open edges match the visible rectangle and do not steal a neighboring
// control's boundary. Rect::contains itself includes its far edge.
pub fn contains(rect: Rect, p: Vec2) -> bool {
    p.is_finite()
        && p.x >= rect.x
        && p.y >= rect.y
        && p.x < rect.x + rect.w
        && p.y < rect.y + rect.h
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    start: Vec2,
    last: Vec2,
    moved: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub pointer: Vec2,
    pub pressed: bool,
    pub down: bool,
    pub released: bool,
    pub wheel: Vec2,
    pub zoom_modifier: bool,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub viewport: Rect,
    /// Page origin relative to the viewport origin, in logical window pixels.
    pub pan: Vec2,
    /// Logical window pixels per generated-page unit.
    pub zoom: f32,
    pub follow_fit: bool,
    pub selected: bool,
    pub hits: u32,
    pub wheels: u32,
    pub rejected_wheels: u32,
    pub zooms: u32,
    pub drag_starts: u32,
    pub drag_frames: u32,
    pub inside_presses: u32,
    pub outside_presses: u32,
    pub resizes: u32,
    pub anchor_error: f32,
    pub anchor_clamped: bool,
    drag: Option<Drag>,
}

impl Document {
    pub fn new(viewport: Rect) -> Self {
        let mut value = Self {
            viewport,
            pan: Vec2::ZERO,
            zoom: 1.,
            follow_fit: true,
            selected: false,
            hits: 0,
            wheels: 0,
            rejected_wheels: 0,
            zooms: 0,
            drag_starts: 0,
            drag_frames: 0,
            inside_presses: 0,
            outside_presses: 0,
            resizes: 0,
            anchor_error: 0.,
            anchor_clamped: false,
            drag: None,
        };
        value.fit();
        value
    }

    pub fn origin(&self) -> Vec2 {
        vec2(self.viewport.x, self.viewport.y)
    }

    pub fn size(&self) -> Vec2 {
        vec2(self.viewport.w, self.viewport.h)
    }

    pub fn page_to_window(&self, point: Vec2) -> Vec2 {
        self.origin() + self.pan + point * self.zoom
    }

    pub fn window_to_page(&self, point: Vec2) -> Vec2 {
        (point - self.origin() - self.pan) / self.zoom
    }

    pub fn fit_zoom(&self) -> f32 {
        (self.viewport.w / PAGE.x)
            .min(self.viewport.h / PAGE.y)
            .min(MAX_ZOOM)
    }

    fn clamp_pan(&mut self) {
        let extent = self.size() - PAGE * self.zoom;
        self.pan.x = if extent.x >= 0. {
            extent.x * 0.5
        } else {
            self.pan.x.clamp(extent.x, 0.)
        };
        self.pan.y = if extent.y >= 0. {
            extent.y * 0.5
        } else {
            self.pan.y.clamp(extent.y, 0.)
        };
    }

    pub fn fit(&mut self) {
        self.zoom = self.fit_zoom();
        self.pan = (self.size() - PAGE * self.zoom) * 0.5;
        self.clamp_pan();
        self.follow_fit = true;
        self.drag = None;
        self.anchor_error = 0.;
        self.anchor_clamped = false;
    }

    /// Reset the view to 100%, centered on the page; preserve click diagnostics.
    pub fn reset(&mut self) {
        self.zoom = 1.;
        self.pan = (self.size() - PAGE) * 0.5;
        self.clamp_pan();
        self.follow_fit = false;
        self.drag = None;
        self.anchor_error = 0.;
        self.anchor_clamped = false;
    }

    /// Fit follows resize. A manual view preserves the page point under the
    /// old viewport center, subject to the new bounds. Cancel captured drags.
    pub fn resize(&mut self, viewport: Rect) -> bool {
        if self.viewport == viewport {
            return false;
        }
        let center_page = self.window_to_page(self.origin() + self.size() * 0.5);
        self.viewport = viewport;
        if self.follow_fit {
            self.fit();
        } else {
            self.pan = self.size() * 0.5 - center_page * self.zoom;
            self.clamp_pan();
            self.drag = None;
        }
        self.resizes += 1;
        true
    }

    pub fn captured(&self) -> bool {
        self.drag.is_some()
    }

    pub fn marker_hit(&self, pointer: Vec2) -> bool {
        contains(self.viewport, pointer) && contains(MARKER, self.window_to_page(pointer))
    }

    /// Wheel only affects the hovered viewport. Ctrl+vertical wheel zooms;
    /// ordinary wheel pans by 40 logical pixels per reported wheel unit. A
    /// captured left drag may continue outside, while bounds still apply.
    pub fn update(&mut self, input: Input) -> bool {
        let mut changed = false;
        let inside = contains(self.viewport, input.pointer);
        if input.wheel.is_finite() && input.wheel != Vec2::ZERO {
            changed = true;
            if inside {
                self.wheels += 1;
                if input.zoom_modifier {
                    let page = self.window_to_page(input.pointer);
                    self.zoom = (self.zoom * (input.wheel.y.clamp(-20., 20.) * 0.14).exp())
                        .clamp(self.fit_zoom().min(0.2), MAX_ZOOM);
                    let ideal = input.pointer - self.origin() - page * self.zoom;
                    self.pan = ideal;
                    self.clamp_pan();
                    self.anchor_clamped = (self.pan - ideal).length() > 0.001;
                    self.anchor_error = (self.page_to_window(page) - input.pointer).length();
                    self.zooms += 1;
                    self.follow_fit = false;
                } else {
                    self.pan += input.wheel.clamp(Vec2::splat(-100.), Vec2::splat(100.)) * 40.;
                    self.clamp_pan();
                    self.follow_fit = false;
                }
                // Do not mix a stale drag origin with a wheel transformation.
                self.drag = None;
            } else {
                self.rejected_wheels += 1;
            }
        }
        if input.pressed {
            changed = true;
            if inside {
                self.inside_presses += 1;
                self.drag_starts += 1;
                self.drag = Some(Drag {
                    start: input.pointer,
                    last: input.pointer,
                    moved: false,
                });
            } else {
                self.outside_presses += 1;
                self.drag = None;
            }
        }
        if let Some(mut drag) = self.drag {
            if input.pointer.is_finite() {
                // Apply a final event position even if movement and release
                // arrive together between two rendered frames.
                if input.down || input.released {
                    let crossed = (input.pointer - drag.start).length() > 3.;
                    if drag.moved || crossed {
                        let from = if drag.moved { drag.last } else { drag.start };
                        let delta = input.pointer - from;
                        if delta != Vec2::ZERO {
                            self.pan += delta;
                            self.clamp_pan();
                            self.drag_frames += 1;
                            self.follow_fit = false;
                            changed = true;
                        }
                        drag.moved = true;
                    }
                    drag.last = input.pointer;
                    self.drag = Some(drag);
                }
                if input.released {
                    if !drag.moved
                        && (input.pointer - drag.start).length() <= 3.
                        && self.marker_hit(input.pointer)
                    {
                        self.selected = !self.selected;
                        self.hits += 1;
                    }
                    changed = true;
                }
            }
            if !input.down || input.released || !input.pointer.is_finite() {
                self.drag = None;
            }
        }
        changed
    }

    /// Whole-window camera rectangle in generated-page units. A negative
    /// height gives this window camera the page's top-left, Y-down convention.
    /// Select it before the logical clip and keep it unchanged in the scope.
    pub fn camera_rect(&self, window: Vec2) -> Rect {
        let corner = self.window_to_page(Vec2::ZERO);
        let height = window.y / self.zoom;
        Rect::new(corner.x, corner.y + height, window.x / self.zoom, -height)
    }

    pub fn bounded(&self) -> bool {
        let extent = self.size() - PAGE * self.zoom;
        let axis = |pan: f32, available: f32| {
            if available >= 0. {
                (pan - available * 0.5).abs() < 0.002
            } else {
                pan >= available - 0.002 && pan <= 0.002
            }
        };
        self.zoom.is_finite()
            && self.zoom > 0.
            && self.pan.is_finite()
            && axis(self.pan.x, extent.x)
            && axis(self.pan.y, extent.y)
    }
}
