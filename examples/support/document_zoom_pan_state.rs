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

#[derive(Clone, Copy, Debug)]
struct TouchDrag {
    id: u64,
    drag: Drag,
}

#[derive(Clone, Copy, Debug)]
struct Pinch {
    ids: [u64; 2],
    center: Vec2,
    distance: f64,
}

// A subpixel/coincident pair has no useful scale baseline. Continue midpoint
// translation, then rebase when the fingers separate instead of dividing by 0.
const MIN_PINCH_DISTANCE: f64 = 1.;

fn pinch_distance(a: Vec2, b: Vec2) -> f64 {
    (a.x as f64 - b.x as f64).hypot(a.y as f64 - b.y as f64)
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
    pub touch_starts: u32,
    pub pinch_starts: u32,
    pub pinch_frames: u32,
    pub touch_transitions: u32,
    pub touch_cancels: u32,
    pub rejected_touch_starts: u32,
    pub touch_taps: u32,
    drag: Option<Drag>,
    touch_drag: Option<TouchDrag>,
    pinch: Option<Pinch>,
    mouse_blocked: bool,
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
            touch_starts: 0,
            pinch_starts: 0,
            pinch_frames: 0,
            touch_transitions: 0,
            touch_cancels: 0,
            rejected_touch_starts: 0,
            touch_taps: 0,
            drag: None,
            touch_drag: None,
            pinch: None,
            mouse_blocked: false,
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

    fn cancel_capture(&mut self) {
        self.drag = None;
        self.touch_drag = None;
        self.pinch = None;
    }

    pub fn fit(&mut self) {
        self.zoom = self.fit_zoom();
        self.pan = (self.size() - PAGE * self.zoom) * 0.5;
        self.clamp_pan();
        self.follow_fit = true;
        self.cancel_capture();
        self.anchor_error = 0.;
        self.anchor_clamped = false;
    }

    /// Reset the view to 100%, centered on the page; preserve click diagnostics.
    pub fn reset(&mut self) {
        self.zoom = 1.;
        self.pan = (self.size() - PAGE) * 0.5;
        self.clamp_pan();
        self.follow_fit = false;
        self.cancel_capture();
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
            self.cancel_capture();
        }
        self.resizes += 1;
        true
    }

    pub fn captured(&self) -> bool {
        self.drag.is_some() || self.touch_drag.is_some() || self.pinch.is_some()
    }

    pub fn touch_ids(&self) -> [Option<u64>; 2] {
        if let Some(pinch) = self.pinch {
            pinch.ids.map(Some)
        } else {
            [self.touch_drag.map(|drag| drag.id), None]
        }
    }

    pub fn touch_mode(&self) -> u32 {
        if self.pinch.is_some() {
            2
        } else {
            u32::from(self.touch_drag.is_some())
        }
    }

    /// Share this gate with mouse-operated HUD buttons. Touches, including
    /// terminal phases, suppress mouse input until a neutral mouse frame.
    pub fn mouse_allowed(&self) -> bool {
        !self.mouse_blocked
    }

    fn anchor_at(&mut self, page: Vec2, pointer: Vec2) {
        // Compute the desired pan/error in f64 so a finite captured contact far
        // outside the window cannot overflow the diagnostic length calculation.
        let x = pointer.x as f64 - self.viewport.x as f64 - page.x as f64 * self.zoom as f64;
        let y = pointer.y as f64 - self.viewport.y as f64 - page.y as f64 * self.zoom as f64;
        self.pan = vec2(x as f32, y as f32);
        self.clamp_pan();
        let error = (self.pan.x as f64 - x).hypot(self.pan.y as f64 - y);
        self.anchor_clamped = error > 0.001;
        self.anchor_error = error.min(f32::MAX as f64) as f32;
    }

    /// Only an in-viewport Started contact can acquire ownership. The input
    /// slice is a complete touches_logical() snapshot, whose order is unspecified.
    /// Extra/outside starts are ignored for their lifetime; they cannot steal a
    /// captured ID. Transitions rebase at current positions without a view jump.
    fn update_touches(&mut self, touches: &[Touch]) -> bool {
        let viewport = self.viewport;
        let active = |id| {
            touches.iter().find(|touch| {
                touch.id == id
                    && touch.position.is_finite()
                    && !matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled)
            })
        };
        let candidate = |owned: [Option<u64>; 2]| {
            touches
                .iter()
                .filter(|touch| {
                    touch.phase == TouchPhase::Started
                        && contains(viewport, touch.position)
                        && !owned.contains(&Some(touch.id))
                })
                .min_by_key(|touch| touch.id)
        };
        let mut changed = false;
        // Take the old ownership first so a missing/cancelled contact can never
        // leave a stale origin for a later contact with another ID.
        if let Some(pinch) = self.pinch.take() {
            let first = active(pinch.ids[0]);
            let second = active(pinch.ids[1]);
            if let (Some(a), Some(b)) = (first, second) {
                let center = a.position * 0.5 + b.position * 0.5;
                let distance = pinch_distance(a.position, b.position);
                let page = self.window_to_page(pinch.center);
                if page.is_finite() {
                    if pinch.distance > MIN_PINCH_DISTANCE && distance > MIN_PINCH_DISTANCE {
                        // f64 ratio avoids intermediate overflow for otherwise
                        // finite positions; clamp before conversion to f32.
                        self.zoom = ((self.zoom as f64 * distance / pinch.distance)
                            .clamp(self.fit_zoom().min(0.2) as f64, MAX_ZOOM as f64))
                            as f32;
                    }
                    if center != pinch.center || distance != pinch.distance {
                        self.anchor_at(page, center);
                        self.follow_fit = false;
                        self.pinch_frames += 1;
                        changed = true;
                    }
                    self.pinch = Some(Pinch {
                        center,
                        distance,
                        ..pinch
                    });
                } else {
                    self.touch_cancels += 1;
                    changed = true;
                }
            } else {
                // Terminal coordinates do not transform a pinch. Rebase the
                // survivor, discard tap eligibility, and require a fresh start
                // for the second finger. Missing contacts act like cancellation.
                self.touch_drag = first.or(second).map(|touch| TouchDrag {
                    id: touch.id,
                    drag: Drag {
                        start: touch.position,
                        last: touch.position,
                        moved: true,
                    },
                });
                self.touch_transitions += 1;
                if pinch.ids.iter().any(|id| {
                    !touches.iter().any(|t| {
                        t.id == *id && t.phase != TouchPhase::Cancelled && t.position.is_finite()
                    })
                }) {
                    self.touch_cancels += 1;
                }
                changed = true;
            }
        } else if let Some(mut owned) = self.touch_drag.take() {
            let contact = touches.iter().find(|touch| touch.id == owned.id);
            if let Some(touch) = contact
                .filter(|touch| touch.position.is_finite() && touch.phase != TouchPhase::Cancelled)
            {
                if touch.phase != TouchPhase::Ended {
                    if let Some(second) = candidate([Some(owned.id), None]) {
                        self.pinch = Some(Pinch {
                            ids: [owned.id, second.id],
                            center: touch.position * 0.5 + second.position * 0.5,
                            distance: pinch_distance(touch.position, second.position),
                        });
                        self.touch_starts += 1;
                        self.pinch_starts += 1;
                        self.touch_transitions += 1;
                        changed = true;
                    }
                }
                if self.pinch.is_none() {
                    let drag = &mut owned.drag;
                    if drag.moved || touch.position.distance(drag.start) > 3. {
                        let from = if drag.moved { drag.last } else { drag.start };
                        let delta = touch.position - from;
                        if delta != Vec2::ZERO {
                            self.pan += delta;
                            self.clamp_pan();
                            self.follow_fit = false;
                            self.drag_frames += 1;
                            changed = true;
                        }
                        drag.moved = true;
                    }
                    drag.last = touch.position;
                    if touch.phase == TouchPhase::Ended {
                        if !drag.moved && self.marker_hit(touch.position) {
                            self.selected = !self.selected;
                            self.hits += 1;
                            self.touch_taps += 1;
                        }
                        changed = true;
                    } else {
                        self.touch_drag = Some(owned);
                    }
                }
            } else {
                self.touch_cancels += 1;
                changed = true;
            }
        }
        if self.touch_mode() == 0 {
            if let Some(first) = candidate([None, None]) {
                self.touch_drag = Some(TouchDrag {
                    id: first.id,
                    drag: Drag {
                        start: first.position,
                        last: first.position,
                        moved: false,
                    },
                });
                self.touch_starts += 1;
                self.drag_starts += 1;
                changed = true;
                if let Some(second) = candidate([Some(first.id), None]) {
                    self.pinch = Some(Pinch {
                        ids: [first.id, second.id],
                        center: first.position * 0.5 + second.position * 0.5,
                        distance: pinch_distance(first.position, second.position),
                    });
                    self.touch_drag = None;
                    self.touch_starts += 1;
                    self.pinch_starts += 1;
                    self.touch_transitions += 1;
                }
            }
        }
        // A pinch owns both IDs, never an additional dormant drag.
        if self.pinch.is_some() {
            self.touch_drag = None;
        }
        for touch in touches
            .iter()
            .filter(|touch| touch.phase == TouchPhase::Started)
        {
            if !self.touch_ids().contains(&Some(touch.id)) {
                self.rejected_touch_starts += 1;
                changed = true;
            }
        }
        changed
    }

    /// The example also disables engine mouse simulation at startup. This gate
    /// prevents an already queued synthetic mouse release/held button or wheel
    /// from operating the page/HUD while touch owns the interaction.
    pub fn update_with_touches(&mut self, input: Input, touches: &[Touch]) -> bool {
        let mut changed = self.update_touches(touches);
        if !touches.is_empty() {
            changed |= self.drag.take().is_some() || !self.mouse_blocked;
            self.mouse_blocked = true;
        }
        if self.mouse_blocked {
            if touches.is_empty() && !input.pressed && !input.down && !input.released {
                self.mouse_blocked = false;
                changed = true;
            }
            return changed;
        }
        changed | self.update_mouse(input)
    }

    pub fn marker_hit(&self, pointer: Vec2) -> bool {
        contains(self.viewport, pointer) && contains(MARKER, self.window_to_page(pointer))
    }

    /// Wheel only affects the hovered viewport. Ctrl+vertical wheel zooms;
    /// Ctrl+horizontal-only wheel leaves the current view and drag unchanged.
    /// ordinary wheel pans by 40 logical pixels per reported wheel unit. A
    /// captured left drag may continue outside, while bounds still apply.
    pub fn update(&mut self, input: Input) -> bool {
        self.update_with_touches(input, &[])
    }

    fn update_mouse(&mut self, input: Input) -> bool {
        let mut changed = false;
        let inside = contains(self.viewport, input.pointer);
        if input.wheel.is_finite()
            && input.wheel != Vec2::ZERO
            && (!input.zoom_modifier || input.wheel.y != 0.)
        {
            changed = true;
            if inside {
                self.wheels += 1;
                if input.zoom_modifier {
                    let page = self.window_to_page(input.pointer);
                    self.zoom = (self.zoom * (input.wheel.y.clamp(-20., 20.) * 0.14).exp())
                        .clamp(self.fit_zoom().min(0.2), MAX_ZOOM);
                    self.anchor_at(page, input.pointer);
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
