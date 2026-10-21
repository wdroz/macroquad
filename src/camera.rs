//! 2D and 3D camera.
//!
//! [`Camera2D::target_to_world`] converts physical drawing-surface pixels into
//! world positions. For an offscreen texture displayed in a logical window
//! rectangle, first use [`window_to_render_target`]; use the inverse helpers
//! to place window-space overlays. The legacy `screen_to_world` and
//! `world_to_screen` methods keep their window-size convention.
//!
//! [`Camera3D::screen_to_ray`] unprojects a screen point into a world
//! [`Ray`]. [`Camera3D::frustum`] extracts the six inward
//! planes of that same view-projection.

use crate::{
    get_context,
    math::{Frustum, Ray, Rect},
    prelude::RenderPass,
    texture::RenderTarget,
    window::{screen_height, screen_width},
};
use glam::{vec2, vec3, Mat4, Vec2, Vec3, Vec4};

pub trait Camera {
    fn matrix(&self) -> Mat4;
    fn depth_enabled(&self) -> bool;
    fn render_pass(&self) -> Option<RenderPass>;
    fn viewport(&self) -> Option<(i32, i32, i32, i32)>;
}

#[derive(Debug)]
pub struct Camera2D {
    /// Rotation in degrees.
    pub rotation: f32,
    /// Scaling, should be (1.0, 1.0) by default.
    pub zoom: Vec2,
    /// Rotation and zoom origin.
    pub target: Vec2,
    /// Displacement from target.
    pub offset: Vec2,

    /// If "render_target" is set - camera will render to texture.
    ///
    /// Otherwise to the screen.
    pub render_target: Option<RenderTarget>,

    /// Physical GL viewport `(x, y, width, height)`, with a bottom-left origin.
    ///
    /// None means the whole framebuffer or render-target texture. Window DPI
    /// does not scale these values; convert logical viewport dimensions first.
    ///
    /// Viewport do not affect camera space, just the render position on the screen.
    ///
    /// Useful for things like splitscreen.
    pub viewport: Option<(i32, i32, i32, i32)>,
}

impl Camera2D {
    /// Will make camera space equals given rect.
    pub fn from_display_rect(rect: Rect) -> Camera2D {
        let target = vec2(rect.x + rect.w / 2., rect.y + rect.h / 2.);

        Camera2D {
            target,
            zoom: vec2(1. / rect.w * 2., -1. / rect.h * 2.),
            offset: vec2(0., 0.),
            rotation: 0.,

            render_target: None,
            viewport: None,
        }
    }
}

impl Default for Camera2D {
    fn default() -> Camera2D {
        Camera2D {
            zoom: vec2(1., 1.),
            offset: vec2(0., 0.),
            target: vec2(0., 0.),
            rotation: 0.,

            render_target: None,
            viewport: None,
        }
    }
}

impl Camera for Camera2D {
    fn matrix(&self) -> Mat4 {
        self.matrix_for_target(self.render_target.is_some())
    }

    fn depth_enabled(&self) -> bool {
        false
    }

    fn render_pass(&self) -> Option<RenderPass> {
        self.render_target.as_ref().map(|rt| rt.render_pass.clone())
    }

    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        self.viewport
    }
}

impl Camera2D {
    fn matrix_for_target(&self, render_to_texture: bool) -> Mat4 {
        // gleaned from https://github.com/raysan5/raylib/blob/master/src/core.c#L1528

        // The camera in world-space is set by
        //   1. Move it to target
        //   2. Rotate by -rotation and scale by (1/zoom)
        //      When setting higher scale, it's more intuitive for the world to become bigger (= camera become smaller),
        //      not for the camera getting bigger, hence the invert. Same deal with rotation.
        //   3. Move it by (-offset);
        //      Offset defines target transform relative to screen, but since we're effectively "moving" screen (camera)
        //      we need to do it into opposite direction (inverse transform)

        // Having camera transform in world-space, inverse of it gives the modelview transform.
        // Since (A*B*C)' = C'*B'*A', the modelview is
        //   1. Move to offset
        //   2. Rotate and Scale
        //   3. Move by -target
        let mat_origin = Mat4::from_translation(vec3(-self.target.x, -self.target.y, 0.0));
        let mat_rotation = Mat4::from_axis_angle(vec3(0.0, 0.0, 1.0), self.rotation.to_radians());
        let invert_y = if render_to_texture { 1.0 } else { -1.0 };
        let mat_scale = Mat4::from_scale(vec3(self.zoom.x, self.zoom.y * invert_y, 1.0));
        let mat_translation = Mat4::from_translation(vec3(self.offset.x, self.offset.y, 0.0));

        mat_translation * ((mat_scale * mat_rotation) * mat_origin)
    }

    /// Returns the screen space position for a 2d camera world space position.
    ///
    /// Screen position in window space - from (0, 0) to (screen_width, screen_height()).
    /// When a viewport is set, returns coordinates within that viewport's screen region.
    ///
    /// This legacy method uses window dimensions even with a render target.
    /// Use [`Self::world_to_target`] for physical target pixels and a physical viewport.
    pub fn world_to_screen(&self, point: Vec2) -> Vec2 {
        let dims = self
            .viewport()
            .map(|(vx, vy, vw, vh)| Rect {
                x: vx as f32,
                y: screen_height() - (vy + vh) as f32,
                w: vw as f32,
                h: vh as f32,
            })
            .unwrap_or(Rect {
                x: 0.0,
                y: 0.0,
                w: screen_width(),
                h: screen_height(),
            });

        let mat = self.matrix();
        let transform = mat.transform_point3(vec3(point.x, point.y, 0.));

        vec2(
            (transform.x / 2. + 0.5) * dims.w + dims.x,
            (0.5 - transform.y / 2.) * dims.h + dims.y,
        )
    }

    /// Returns the world space position for a 2d camera screen space position.
    ///
    /// Point is a screen space position, often mouse x and y.
    ///
    /// This legacy method uses window dimensions even with a render target.
    /// Use [`Self::target_to_world`] for physical target pixels and a physical viewport.
    pub fn screen_to_world(&self, point: Vec2) -> Vec2 {
        let dims = self
            .viewport()
            .map(|(vx, vy, vw, vh)| Rect {
                x: vx as f32,
                y: screen_height() - (vy + vh) as f32,
                w: vw as f32,
                h: vh as f32,
            })
            .unwrap_or(Rect {
                x: 0.0,
                y: 0.0,
                w: screen_width(),
                h: screen_height(),
            });

        let point = vec2(
            (point.x - dims.x) / dims.w * 2. - 1.,
            1. - (point.y - dims.y) / dims.h * 2.,
        );
        let inv_mat = self.matrix().inverse();
        let transform = inv_mat.transform_point3(vec3(point.x, point.y, 0.));

        vec2(transform.x, transform.y)
    }

    /// Projects a world point into physical pixels of this camera's drawing surface.
    ///
    /// Coordinates start at the **top left**, with x right and y down. A selected
    /// [`Self::render_target`] supplies its texture dimensions; otherwise the live
    /// window's physical framebuffer dimensions are used. The camera need not be
    /// active. [`Self::viewport`] is always the physical GL viewport, with a
    /// **bottom-left** origin; it affects pixel placement, not the camera matrix.
    ///
    /// These are framebuffer coordinates, not texture UV/source coordinates and
    /// not logical window pixels. On the window, use
    /// [`physical_to_logical`](crate::window::physical_to_logical) for logical
    /// overlays. For a displayed render-target texture, use
    /// [`render_target_to_window`] with the same rectangle and flips as its draw.
    ///
    /// Points outside the surface or viewport are extrapolated, not clipped.
    /// Returns `None` without a live drawing surface, or for nonfinite input,
    /// invalid dimensions/viewport, a singular or unrepresentable projection,
    /// or nonfinite output.
    /// Legacy [`Self::world_to_screen`] retains its window-space behavior.
    pub fn world_to_target(&self, point: Vec2) -> Option<Vec2> {
        self.world_to_target_with_size(point, self.target_pixel_size()?)
    }

    /// Unprojects physical, top-left drawing-surface pixels into world coordinates.
    ///
    /// This is the inverse of [`Self::world_to_target`], including its physical
    /// bottom-left viewport, render-target Y orientation and extrapolation rules.
    /// For window input, pass [`mouse_position_physical`](crate::input::mouse_position_physical),
    /// or convert logical input with [`logical_to_physical`](crate::window::logical_to_physical).
    /// For a render target displayed in the window, map logical input through
    /// [`window_to_render_target`] first. Window DPI does not scale texture pixels.
    /// Invalid/unavailable surfaces, singular cameras and nonfinite values return
    /// `None`. Legacy [`Self::screen_to_world`] remains unchanged.
    ///
    /// Direct window picking uses physical input; the HUD returns to logical units:
    ///
    /// ```no_run
    /// use macroquad::prelude::*;
    ///
    /// let camera = Camera2D::from_display_rect(Rect::new(0.0, 0.0, 320.0, 180.0));
    /// if let Some(world) = camera.target_to_world(mouse_position_physical()) {
    ///     set_camera(&camera);
    ///     draw_circle(world.x, world.y, 3.0, RED);
    /// }
    /// set_default_camera();
    /// if let Some(pixel) = camera.world_to_target(vec2(160.0, 90.0)) {
    ///     let label = physical_to_logical(pixel);
    ///     draw_text("Center", label.x, label.y, 20.0, BLACK);
    /// }
    /// ```
    pub fn target_to_world(&self, point: Vec2) -> Option<Vec2> {
        self.target_to_world_with_size(point, self.target_pixel_size()?)
    }

    /// Projects using an explicit physical drawing-surface size, without a window lookup.
    ///
    /// This has the coordinate and validity rules of [`Self::world_to_target`].
    /// `target_size` replaces only surface dimensions, **not** the camera matrix:
    /// the presence of [`Self::render_target`] still determines its Y orientation.
    /// A camera without a texture can therefore be used in headless tests with
    /// its normal window-camera orientation. Use actual framebuffer dimensions
    /// (logical size times DPI) for a window, or texture dimensions offscreen.
    ///
    /// ```
    /// use macroquad::prelude::*;
    /// let camera = Camera2D {
    ///     viewport: Some((20, 30, 200, 100)),
    ///     ..Default::default()
    /// };
    /// let size = vec2(640.0, 360.0);
    /// let pixels = camera.world_to_target_with_size(Vec2::ZERO, size).unwrap();
    /// assert_eq!(pixels, vec2(120.0, 280.0));
    /// let world = camera.target_to_world_with_size(pixels, size).unwrap();
    /// assert!(world.length() < 0.0001);
    /// ```
    pub fn world_to_target_with_size(&self, point: Vec2, target_size: Vec2) -> Option<Vec2> {
        project_target(self.matrix(), self.viewport, point, target_size)
    }

    /// Inverse of [`Self::world_to_target_with_size`] for an explicit physical size.
    ///
    /// No window or texture-size lookup occurs. The camera's render-target flag
    /// still selects its matrix orientation; outside points are extrapolated.
    /// Invalid input, a singular camera or a nonfinite result returns `None`.
    pub fn target_to_world_with_size(&self, point: Vec2, target_size: Vec2) -> Option<Vec2> {
        unproject_target(self.matrix(), self.viewport, point, target_size)
    }

    fn target_pixel_size(&self) -> Option<Vec2> {
        let size = if let Some(target) = &self.render_target {
            (target.texture.width(), target.texture.height())
        } else {
            crate::try_screen_size()?;
            let context = get_context();
            (context.screen_width, context.screen_height)
        };
        positive_size(size).map(|(width, height)| vec2(width, height))
    }
}

/// Maps logical window input onto a displayed render target's physical pixels.
///
/// `display` is the axis-aligned, positive-size logical window rectangle in which
/// the **entire** render-target texture was drawn. `target_size` is the texture's
/// pixel size. `flip_x` and `flip_y` must match [`DrawTextureParams`](crate::texture::DrawTextureParams).
/// Source cropping, rotation and a transformed display camera are not represented;
/// map those yourself before using this helper. A normal default-camera draw
/// with a positive `dest_size` is supported. Supply the inner displayed rectangle
/// after letterboxing so input in the bars returns `None`.
///
/// Output starts at the target framebuffer's top left, y down, suitable for
/// [`Camera2D::target_to_world`]. A raw render-target texture has reversed Y
/// relative to these physical pixels: `flip_y = true` gives a direct Y mapping,
/// while `false` reverses it. This correction is independent of the camera zoom.
/// No DPI factor is applied: logical input and the logical display rectangle
/// already share a space; texture dimensions stay fixed at any window DPI.
///
/// Rectangle boundaries are included. Outside, empty, nonfinite or unrepresentable
/// inputs return `None`. This pure helper performs no GPU or window operations.
///
/// ```
/// use macroquad::prelude::*;
/// let display = Rect::new(40.0, 70.0, 640.0, 360.0);
/// let size = vec2(320.0, 180.0);
/// let pixels = window_to_render_target(vec2(200.0, 160.0), size, display, false, true);
/// assert_eq!(pixels, Some(vec2(80.0, 45.0)));
/// assert!(window_to_render_target(vec2(10.0, 160.0), size, display, false, true).is_none());
/// ```
pub fn window_to_render_target(
    point: Vec2,
    target_size: Vec2,
    display: Rect,
    flip_x: bool,
    flip_y: bool,
) -> Option<Vec2> {
    positive_size((target_size.x, target_size.y))?;
    let (min, max) = display_bounds(display)?;
    if !inside_bounds(point, min, max) {
        return None;
    }
    // Subtraction at a fractional far edge can round above width/height even
    // though the point passed the inclusive bounds check.
    let mut normalized = ((point - min) / vec2(display.w, display.h)).clamp(Vec2::ZERO, Vec2::ONE);
    if flip_x {
        normalized.x = 1.0 - normalized.x;
    }
    if !flip_y {
        normalized.y = 1.0 - normalized.y;
    }
    finite_vec2(normalized * target_size)
}

/// Maps physical render-target pixels onto its logical window display rectangle.
///
/// Inverse of [`window_to_render_target`], with the same full-texture,
/// axis-aligned display and flip conventions. Use it for logical window overlays
/// at positions returned by [`Camera2D::world_to_target`]. Points outside the
/// texture return `None`, including world projections outside its bounds.
/// Boundaries are included; invalid dimensions/rectangles and nonfinite values
/// return `None`. This pure helper does not depend on the window DPI.
///
/// ```
/// use macroquad::prelude::*;
///
/// let target_size = vec2(320.0, 180.0);
/// let display = Rect::new(40.0, 70.0, 640.0, 360.0);
/// // These flags match the full texture's draw_texture_ex call.
/// let marker = render_target_to_window(
///     vec2(80.0, 45.0), target_size, display, true, true,
/// ).unwrap();
/// assert_eq!(marker, vec2(520.0, 160.0));
/// assert_eq!(
///     window_to_render_target(marker, target_size, display, true, true),
///     Some(vec2(80.0, 45.0)),
/// );
/// ```
pub fn render_target_to_window(
    point: Vec2,
    target_size: Vec2,
    display: Rect,
    flip_x: bool,
    flip_y: bool,
) -> Option<Vec2> {
    positive_size((target_size.x, target_size.y))?;
    let (min, _) = display_bounds(display)?;
    if !inside_bounds(point, Vec2::ZERO, target_size) {
        return None;
    }
    let mut normalized = point / target_size;
    if flip_x {
        normalized.x = 1.0 - normalized.x;
    }
    if !flip_y {
        normalized.y = 1.0 - normalized.y;
    }
    finite_vec2(min + normalized * vec2(display.w, display.h))
}

fn display_bounds(rect: Rect) -> Option<(Vec2, Vec2)> {
    positive_size((rect.w, rect.h))?;
    let min = vec2(rect.x, rect.y);
    let max = min + vec2(rect.w, rect.h);
    if min.is_finite() && max.is_finite() && max.x > min.x && max.y > min.y {
        Some((min, max))
    } else {
        None
    }
}

fn inside_bounds(point: Vec2, min: Vec2, max: Vec2) -> bool {
    point.is_finite()
        && point.x >= min.x
        && point.x <= max.x
        && point.y >= min.y
        && point.y <= max.y
}

fn finite_vec2(point: Vec2) -> Option<Vec2> {
    point.is_finite().then_some(point)
}

fn target_projection_rect(
    matrix: Mat4,
    viewport: Option<(i32, i32, i32, i32)>,
    size: Vec2,
) -> Option<Rect> {
    positive_size((size.x, size.y))?;
    let determinant = matrix.determinant();
    if !matrix.is_finite() || !determinant.is_finite() || determinant == 0.0 {
        return None;
    }
    let rect = picking_rect(viewport, size.x, size.y)?;
    display_bounds(rect)?;
    Some(rect)
}

fn project_target(
    matrix: Mat4,
    viewport: Option<(i32, i32, i32, i32)>,
    point: Vec2,
    size: Vec2,
) -> Option<Vec2> {
    let rect = target_projection_rect(matrix, viewport, size)?;
    if !point.is_finite() {
        return None;
    }
    let ndc = matrix.transform_point3(point.extend(0.0));
    finite_vec2(vec2(
        (ndc.x * 0.5 + 0.5) * rect.w + rect.x,
        (0.5 - ndc.y * 0.5) * rect.h + rect.y,
    ))
}

fn unproject_target(
    matrix: Mat4,
    viewport: Option<(i32, i32, i32, i32)>,
    point: Vec2,
    size: Vec2,
) -> Option<Vec2> {
    let rect = target_projection_rect(matrix, viewport, size)?;
    if !point.is_finite() {
        return None;
    }
    let inverse = matrix.inverse();
    let ndc = vec3(
        (point.x - rect.x) / rect.w * 2.0 - 1.0,
        1.0 - (point.y - rect.y) / rect.h * 2.0,
        0.0,
    );
    finite_vec2(inverse.transform_point3(ndc).truncate())
}

#[derive(Debug, Clone, Copy)]
pub enum Projection {
    Perspective,
    Orthographics,
}

#[derive(Debug)]
pub struct Camera3D {
    /// Camera position.
    pub position: Vec3,
    /// Camera target it looks-at.
    pub target: Vec3,
    /// Camera up vector (rotation over its axis).
    pub up: Vec3,
    /// Camera field-of-view aperture in Y (radians) for perspective.
    ///
    /// For [`Projection::Orthographics`] this is the full vertical size of the
    /// view (`top = fovy / 2`), the same value [`Camera::matrix`] uses. It is
    /// not a width: the horizontal size is `fovy * aspect`.
    pub fovy: f32,
    /// Screen aspect ratio.
    ///
    /// By default aspect is calculated with screen_width() / screen_height() on each frame.
    pub aspect: Option<f32>,
    /// Camera projection type, perspective or orthographics.
    pub projection: Projection,

    /// If "render_target" is set - camera will render to texture.
    ///
    /// Otherwise to the screen.
    pub render_target: Option<RenderTarget>,

    /// Part of the screen to render to.
    ///
    /// None means the whole screen.
    ///
    /// Viewport do not affect camera space, just the render position on the screen.
    ///
    /// Useful for things like splitscreen.
    pub viewport: Option<(i32, i32, i32, i32)>,

    /// Camera near plane
    pub z_near: f32,
    /// Camera far plane
    pub z_far: f32,
}

impl Default for Camera3D {
    fn default() -> Camera3D {
        Camera3D {
            position: vec3(0., -10., 0.),
            target: vec3(0., 0., 0.),
            aspect: None,
            up: vec3(0., 0., 1.),
            fovy: 45.0_f32.to_radians(),
            projection: Projection::Perspective,
            render_target: None,
            viewport: None,
            z_near: 0.01,
            z_far: 10000.0,
        }
    }
}

impl Camera for Camera3D {
    fn matrix(&self) -> Mat4 {
        let (width, height) = if let Some(rt) = &self.render_target {
            (rt.texture.width(), rt.texture.height())
        } else {
            (screen_width(), screen_height())
        };
        let aspect = self.aspect.unwrap_or(width / height);
        self.projection_view(aspect)
    }

    fn depth_enabled(&self) -> bool {
        true
    }

    fn render_pass(&self) -> Option<RenderPass> {
        self.render_target.as_ref().map(|rt| rt.render_pass.clone())
    }

    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        self.viewport
    }
}

impl Camera3D {
    /// View-projection used by [`Camera::matrix`] and [`Self::screen_to_ray`].
    ///
    /// `aspect` is `width / height` of the framebuffer when [`Camera3D::aspect`]
    /// is `None`.
    fn projection_view(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.position, self.target, self.up);
        match self.projection {
            Projection::Perspective => {
                Mat4::perspective_rh_gl(self.fovy, aspect, self.z_near, self.z_far) * view
            }
            Projection::Orthographics => {
                let top = self.fovy / 2.0;
                let right = top * aspect;
                Mat4::orthographic_rh_gl(-right, right, -top, top, self.z_near, self.z_far) * view
            }
        }
    }

    /// Unprojects `point` into a world [`Ray`] through the camera's view-projection.
    ///
    /// `point` is in pixels of the surface this camera draws to, origin at the
    /// top left, y down. A window uses logical pixels, the same space as
    /// [`mouse_position`](crate::input::mouse_position),
    /// [`mouse_position_logical`](crate::input::mouse_position_logical), and
    /// [`Camera2D::world_to_screen`].
    /// [`mouse_position_physical`](crate::input::mouse_position_physical) is
    /// framebuffer pixels and is the wrong scale when the display density is
    /// not 1. A render target uses that texture's pixel size. The center of
    /// the surface is `(x + width / 2, y + height / 2)`.
    ///
    /// The view-projection is the one [`Camera::matrix`] builds. [`Self::aspect`],
    /// when set, wins. Otherwise the aspect is that surface's width over its
    /// height. The surface is the render target's texture when
    /// [`Self::render_target`] is set, and the logical window otherwise. A viewport is
    /// the GL viewport `(x, y, width, height)` with its origin at the bottom
    /// left of that surface; it is converted to top-left pixels the same
    /// way as [`Camera2D::world_to_screen`]. The viewport does not change the
    /// projection, only which pixels the point is measured in.
    ///
    /// Perspective rays start at [`Self::position`]. Orthographic rays start on
    /// the near plane. `direction` is a unit vector, so a later hit's `t` is a
    /// distance from `origin`. [`Projection::Orthographics`] uses `fovy` as the
    /// full vertical size, the same value `matrix` uses.
    ///
    /// The result is an unbounded ray, not a near-to-far segment. Shape queries
    /// can report hits beyond the far plane (and, for perspective, before the
    /// near plane). Use [`Self::frustum`] separately when visibility matters.
    ///
    /// A synthetic camera does not need a window: set `aspect` and a viewport.
    /// With no window, the framebuffer is the viewport's far corner
    /// (`x + width`, `y + height`), so the viewport touches the top of that
    /// surface. A point, viewport, or camera that cannot be unprojected returns
    /// a ray at `position` with a zero direction.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::camera::{Camera3D, Projection};
    /// use macroquad::math::{vec2, vec3, Sphere, Vec3};
    ///
    /// let camera = Camera3D {
    ///     position: vec3(0.0, 0.0, 5.0),
    ///     target: Vec3::ZERO,
    ///     up: Vec3::Y,
    ///     fovy: 90.0_f32.to_radians(),
    ///     aspect: Some(1.0),
    ///     projection: Projection::Perspective,
    ///     viewport: Some((0, 0, 200, 200)),
    ///     ..Default::default()
    /// };
    /// let ray = camera.screen_to_ray(vec2(100.0, 100.0));
    /// assert_eq!(ray.origin, camera.position);
    /// assert!(ray.direction.dot(-Vec3::Z) > 0.999);
    /// let target = Sphere::new(Vec3::ZERO, 1.0);
    /// let hit = target.ray_intersect(ray.origin, ray.direction).unwrap();
    /// assert!((hit.t - 4.0).abs() < 1e-3);
    /// ```
    pub fn screen_to_ray(&self, point: Vec2) -> Ray {
        let miss = Ray::new(self.position, Vec3::ZERO);
        if !self.position.is_finite() || !point.x.is_finite() || !point.y.is_finite() {
            return miss;
        }
        let Some((fb_width, fb_height)) = self.framebuffer_size() else {
            return miss;
        };
        let Some(rect) = picking_rect(self.viewport, fb_width, fb_height) else {
            return miss;
        };
        let aspect = self.aspect.unwrap_or(fb_width / fb_height);
        if !aspect.is_finite() || aspect.abs() <= 1e-8 {
            return miss;
        }
        let inverse = self.projection_view(aspect).inverse();
        let ndc_x = (point.x - rect.x) / rect.w * 2.0 - 1.0;
        let ndc_y = 1.0 - (point.y - rect.y) / rect.h * 2.0;
        let Some(far) = unproject(&inverse, ndc_x, ndc_y, 1.0) else {
            return miss;
        };
        match self.projection {
            Projection::Perspective => {
                let direction = (far - self.position).normalize_or_zero();
                if direction == Vec3::ZERO {
                    miss
                } else {
                    Ray::new(self.position, direction)
                }
            }
            Projection::Orthographics => {
                let Some(near) = unproject(&inverse, ndc_x, ndc_y, -1.0) else {
                    return miss;
                };
                let direction = (far - near).normalize_or_zero();
                if direction == Vec3::ZERO {
                    miss
                } else {
                    Ray::new(near, direction)
                }
            }
        }
    }

    /// Six inward planes of this camera's view frustum.
    ///
    /// The matrix is the one [`Camera::matrix`] builds: [`Self::aspect`] wins
    /// when it is set, otherwise the aspect is the framebuffer width over its
    /// height. That framebuffer is the render target when [`Self::render_target`]
    /// is set, and the window otherwise. With neither, a positive viewport
    /// supplies the size, same as [`Self::screen_to_ray`]. The viewport does
    /// not change the projection.
    ///
    /// Planes point into the volume. See [`Frustum`] for which tests are exact
    /// and which can count a shape that only reaches a plane past a corner.
    /// Returns [`None`] when the aspect is missing or unusable, or when the
    /// view-projection has no finite planes (a zero look direction, an up axis
    /// parallel to it, or a non-finite field of view or clip distance).
    ///
    /// A synthetic camera does not need a window when `aspect` is set.
    /// Extract the frustum again after changing the camera. Creating it does
    /// not enable automatic culling for draw calls.
    ///
    /// # Example
    ///
    /// ```
    /// use macroquad::camera::Camera3D;
    /// use macroquad::math::{vec3, Vec3};
    ///
    /// let camera = Camera3D {
    ///     position: vec3(0.0, 0.0, 5.0),
    ///     target: Vec3::ZERO,
    ///     up: Vec3::Y,
    ///     aspect: Some(1.0),
    ///     z_near: 0.1,
    ///     z_far: 20.0,
    ///     ..Default::default()
    /// };
    /// let frustum = camera.frustum().unwrap();
    /// assert!(frustum.contains_point(Vec3::ZERO));
    /// assert!(!frustum.contains_point(camera.position));
    /// ```
    pub fn frustum(&self) -> Option<Frustum> {
        let aspect = if let Some(aspect) = self.aspect {
            aspect
        } else {
            let (width, height) = self.framebuffer_size()?;
            width / height
        };
        if !aspect.is_finite() || aspect.abs() <= 1e-8 {
            return None;
        }
        if !self.position.is_finite()
            || !self.target.is_finite()
            || !self.up.is_finite()
            || !self.fovy.is_finite()
            || !self.z_near.is_finite()
            || !self.z_far.is_finite()
        {
            return None;
        }
        let matrix = self.projection_view(aspect);
        Frustum::from_view_projection(matrix)
    }

    /// Pixel size of the surface this camera draws into.
    ///
    /// A render target uses its texture. Otherwise a live window uses the
    /// screen, matching [`Camera2D::world_to_screen`]. With neither, a viewport
    /// stands in for a framebuffer whose top-right corner is the viewport's
    /// far corner, which is enough for a unit test.
    fn framebuffer_size(&self) -> Option<(f32, f32)> {
        if let Some(target) = &self.render_target {
            return positive_size((target.texture.width(), target.texture.height()));
        }
        if let Some(size) = crate::try_screen_size() {
            return positive_size(size);
        }
        let (vx, vy, vw, vh) = self.viewport?;
        if vw <= 0 || vh <= 0 {
            return None;
        }
        positive_size((vx as f32 + vw as f32, vy as f32 + vh as f32))
    }
}

/// Top-left pixel rect of `viewport` inside a framebuffer of size `(fb_w, fb_h)`.
///
/// `viewport` is the GL viewport, origin at the bottom left. The y conversion
/// matches [`Camera2D::world_to_screen`]: `top = framebuffer_height - (y + height)`.
/// `None` means the whole framebuffer.
fn picking_rect(viewport: Option<(i32, i32, i32, i32)>, fb_w: f32, fb_h: f32) -> Option<Rect> {
    let rect = if let Some((vx, vy, vw, vh)) = viewport {
        if vw <= 0 || vh <= 0 {
            return None;
        }
        let top = vy as i64 + vh as i64;
        Rect {
            x: vx as f32,
            y: fb_h - top as f32,
            w: vw as f32,
            h: vh as f32,
        }
    } else {
        Rect {
            x: 0.0,
            y: 0.0,
            w: fb_w,
            h: fb_h,
        }
    };
    if rect.w > 0.0
        && rect.h > 0.0
        && rect.x.is_finite()
        && rect.y.is_finite()
        && rect.w.is_finite()
        && rect.h.is_finite()
    {
        Some(rect)
    } else {
        None
    }
}

fn positive_size((width, height): (f32, f32)) -> Option<(f32, f32)> {
    if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 {
        Some((width, height))
    } else {
        None
    }
}

fn unproject(inverse: &Mat4, ndc_x: f32, ndc_y: f32, ndc_z: f32) -> Option<Vec3> {
    let hit = *inverse * Vec4::new(ndc_x, ndc_y, ndc_z, 1.0);
    if !hit.is_finite() || hit.w.abs() <= 1e-8 {
        return None;
    }
    let world = hit.truncate() / hit.w;
    if world.is_finite() {
        Some(world)
    } else {
        None
    }
}

/// Set active 2D or 3D camera.
pub fn set_camera(camera: &dyn Camera) {
    let context = get_context();

    // flush previous camera draw calls
    context.perform_render_passes();

    context
        .gl
        .render_pass(camera.render_pass().map(|rt| rt.raw_miniquad_id()));

    context.gl.viewport(camera.viewport());
    context.gl.depth_test(camera.depth_enabled());
    context.camera_matrix = Some(camera.matrix());
}

/// Reset default 2D camera mode.
pub fn set_default_camera() {
    let context = get_context();

    // flush previous camera draw calls
    context.perform_render_passes();

    context.gl.render_pass(None);
    context.gl.viewport(None);
    context.gl.depth_test(false);
    context.camera_matrix = None;
}

pub(crate) struct CameraState {
    render_pass: Option<miniquad::RenderPass>,
    depth_test: bool,
    matrix: Option<Mat4>,
}

pub fn push_camera_state() {
    let context = get_context();

    let camera_state = CameraState {
        render_pass: context.gl.get_active_render_pass(),
        depth_test: context.gl.is_depth_test_enabled(),
        matrix: context.camera_matrix,
    };
    context.camera_stack.push(camera_state);
}

pub fn pop_camera_state() {
    let context = get_context();

    if let Some(camera_state) = context.camera_stack.pop() {
        context.perform_render_passes();

        context.gl.render_pass(camera_state.render_pass);
        context.gl.depth_test(camera_state.depth_test);
        context.camera_matrix = camera_state.matrix;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{vec3, Aabb, Vec3};

    #[test]
    fn camera2d_target_projection_respects_both_surface_orientations() {
        let camera = Camera2D {
            rotation: 31.0,
            zoom: vec2(0.025, -0.04),
            target: vec2(17.0, -11.0),
            offset: vec2(0.13, -0.21),
            viewport: Some((19, 23, 241, 127)),
            ..Default::default()
        };
        let size = vec2(320.0, 180.0);
        let point = vec2(21.0, -2.0);
        let rotation = (camera.rotation as f64).to_radians();
        let dx = (point.x - camera.target.x) as f64;
        let dy = (point.y - camera.target.y) as f64;
        let rx = rotation.cos() * dx - rotation.sin() * dy;
        let ry = rotation.sin() * dx + rotation.cos() * dy;
        for texture in [false, true] {
            let matrix = camera.matrix_for_target(texture);
            let ndc_x = rx * camera.zoom.x as f64 + camera.offset.x as f64;
            let ndc_y = ry * camera.zoom.y as f64 * if texture { 1.0 } else { -1.0 }
                + camera.offset.y as f64;
            let expected = vec2(
                ((ndc_x * 0.5 + 0.5) * 241.0 + 19.0) as f32,
                ((0.5 - ndc_y * 0.5) * 127.0 + 30.0) as f32,
            );
            let pixels = project_target(matrix, camera.viewport, point, size).unwrap();
            assert!((pixels - expected).length() < 0.0001);
            let restored = unproject_target(matrix, camera.viewport, pixels, size).unwrap();
            assert!((restored - point).length() < 0.0001);
            for flip_x in [false, true] {
                for flip_y in [false, true] {
                    let display = Rect::new(50.0, 40.0, 640.0, 360.0);
                    let screen =
                        render_target_to_window(pixels, size, display, flip_x, flip_y).unwrap();
                    let target =
                        window_to_render_target(screen, size, display, flip_x, flip_y).unwrap();
                    let world = unproject_target(matrix, camera.viewport, target, size).unwrap();
                    assert!((world - point).length() < 0.0001);
                }
            }
        }
    }

    fn assert_vec3_close(left: Vec3, right: Vec3) {
        let delta = left - right;
        assert!(
            delta.x.abs() < 1e-4 && delta.y.abs() < 1e-4 && delta.z.abs() < 1e-4,
            "left {left:?} right {right:?}"
        );
    }

    fn perspective_down_z() -> Camera3D {
        Camera3D {
            position: vec3(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fovy: 90.0_f32.to_radians(),
            aspect: Some(1.0),
            projection: Projection::Perspective,
            viewport: Some((0, 0, 200, 200)),
            z_near: 0.25,
            z_far: 40.0,
            ..Default::default()
        }
    }

    #[test]
    fn perspective_center_ray_looks_down_the_axis_without_a_window() {
        let camera = perspective_down_z();
        let ray = camera.screen_to_ray(vec2(100.0, 100.0));
        assert_eq!(ray.origin, camera.position);
        assert_vec3_close(ray.direction, -Vec3::Z);
        assert!((ray.direction.length() - 1.0).abs() < 1e-5);

        let wall = Aabb::from_center_size(vec3(0.0, 0.0, 1.0), Vec3::ONE);
        let hit = wall
            .ray_intersect(ray.origin, ray.direction)
            .expect("center ray meets the wall");
        assert_vec3_close(hit.point, vec3(0.0, 0.0, 1.5));
        assert!((hit.t - 3.5).abs() < 1e-4);
        assert_vec3_close(hit.normal, Vec3::Z);
    }

    #[test]
    fn perspective_edges_follow_the_frustum_and_y_is_down() {
        let camera = perspective_down_z();
        // 90° vertical fov, aspect 1: the frustum edge is 45° off the axis.
        let edge = 1.0 / 2.0_f32.sqrt();

        let top = camera.screen_to_ray(vec2(100.0, 0.0));
        assert_eq!(top.origin, camera.position);
        assert_vec3_close(top.direction, vec3(0.0, edge, -edge));

        let bottom = camera.screen_to_ray(vec2(100.0, 200.0));
        assert_vec3_close(bottom.direction, vec3(0.0, -edge, -edge));

        let left = camera.screen_to_ray(vec2(0.0, 100.0));
        assert_vec3_close(left.direction, vec3(-edge, 0.0, -edge));

        let corner = camera.screen_to_ray(vec2(0.0, 0.0));
        let corner_dir = vec3(-1.0, 1.0, -1.0).normalize();
        assert_vec3_close(corner.direction, corner_dir);
    }

    #[test]
    fn explicit_aspect_wins_over_a_square_viewport() {
        let mut camera = perspective_down_z();
        camera.aspect = Some(2.0);
        let right = camera.screen_to_ray(vec2(200.0, 100.0));
        let expected = vec3(2.0, 0.0, -1.0).normalize();
        assert_vec3_close(right.direction, expected);
    }

    #[test]
    fn missing_aspect_uses_the_viewport_size_when_there_is_no_window() {
        let mut camera = perspective_down_z();
        camera.aspect = None;
        camera.viewport = Some((0, 0, 200, 100));
        let right = camera.screen_to_ray(vec2(200.0, 50.0));
        let expected = vec3(2.0, 0.0, -1.0).normalize();
        assert_vec3_close(right.direction, expected);
    }

    #[test]
    fn z_up_camera_maps_screen_up_onto_that_axis() {
        let camera = Camera3D {
            position: vec3(0.0, -5.0, 0.0),
            target: Vec3::ZERO,
            up: Vec3::Z,
            fovy: 90.0_f32.to_radians(),
            aspect: Some(1.0),
            projection: Projection::Perspective,
            viewport: Some((0, 0, 80, 80)),
            z_near: 0.1,
            z_far: 30.0,
            ..Default::default()
        };
        let center = camera.screen_to_ray(vec2(40.0, 40.0));
        assert_eq!(center.origin, camera.position);
        assert_vec3_close(center.direction, Vec3::Y);

        let top = camera.screen_to_ray(vec2(40.0, 0.0));
        assert_vec3_close(top.direction, vec3(0.0, 1.0, 1.0).normalize());
    }

    #[test]
    fn viewport_offset_keeps_the_center_pixel_on_the_look_axis() {
        let mut camera = perspective_down_z();
        // Synthetic framebuffer is 120×100. The GL viewport sits 20px above
        // the bottom and 40px from the left, so its top-left pixel is (40, 0).
        camera.viewport = Some((40, 20, 80, 80));
        let center = camera.screen_to_ray(vec2(80.0, 40.0));
        assert_vec3_close(center.direction, -Vec3::Z);

        let left = camera.screen_to_ray(vec2(40.0, 40.0));
        let edge = 1.0 / 2.0_f32.sqrt();
        assert_vec3_close(left.direction, vec3(-edge, 0.0, -edge));
    }

    #[test]
    fn picking_rect_flips_a_gl_viewport_within_a_taller_framebuffer() {
        let rect = picking_rect(Some((10, 20, 30, 40)), 200.0, 100.0).unwrap();
        assert_eq!(
            rect,
            Rect {
                x: 10.0,
                y: 40.0,
                w: 30.0,
                h: 40.0,
            }
        );
        assert!(picking_rect(Some((0, 0, 0, 10)), 100.0, 100.0).is_none());
        let full = picking_rect(None, 320.0, 180.0).unwrap();
        assert_eq!(full.w, 320.0);
        assert_eq!(full.h, 180.0);
    }

    #[test]
    fn orthographic_rays_are_parallel_and_start_on_the_near_plane() {
        let camera = Camera3D {
            position: vec3(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fovy: 4.0,
            aspect: Some(1.0),
            projection: Projection::Orthographics,
            viewport: Some((0, 0, 100, 100)),
            z_near: 1.0,
            z_far: 50.0,
            ..Default::default()
        };
        let center = camera.screen_to_ray(vec2(50.0, 50.0));
        let top = camera.screen_to_ray(vec2(50.0, 0.0));
        let left = camera.screen_to_ray(vec2(0.0, 50.0));

        assert_vec3_close(center.direction, -Vec3::Z);
        assert_vec3_close(top.direction, center.direction);
        assert_vec3_close(left.direction, center.direction);
        assert_vec3_close(center.origin, vec3(0.0, 0.0, 4.0));
        assert_vec3_close(top.origin, vec3(0.0, 2.0, 4.0));
        assert_vec3_close(left.origin, vec3(-2.0, 0.0, 4.0));

        let wall = Aabb::from_center_size(Vec3::ZERO, Vec3::ONE);
        let hit = wall
            .ray_intersect(center.origin, center.direction)
            .expect("ortho center ray meets the wall");
        assert_vec3_close(hit.point, vec3(0.0, 0.0, 0.5));
    }

    #[test]
    fn a_bad_camera_or_point_returns_a_zero_direction() {
        let mut camera = perspective_down_z();
        let nan = camera.screen_to_ray(vec2(f32::NAN, 10.0));
        assert_eq!(nan.origin, camera.position);
        assert_eq!(nan.direction, Vec3::ZERO);

        camera.viewport = Some((0, 0, 0, 10));
        let empty = camera.screen_to_ray(vec2(0.0, 0.0));
        assert_eq!(empty.direction, Vec3::ZERO);

        camera.viewport = Some((0, 0, 200, 200));
        camera.target = camera.position;
        let collapsed = camera.screen_to_ray(vec2(100.0, 100.0));
        assert_eq!(collapsed.origin, camera.position);
        assert_eq!(collapsed.direction, Vec3::ZERO);

        camera = perspective_down_z();
        camera.viewport = None;
        let no_surface = camera.screen_to_ray(vec2(100.0, 100.0));
        assert_eq!(no_surface.direction, Vec3::ZERO);
    }

    #[test]
    fn perspective_frustum_matches_fov_and_clip_planes_without_a_window() {
        let camera = perspective_down_z();
        let frustum = camera.frustum().expect("aspect is set");
        for plane in [frustum.left, frustum.right, frustum.bottom, frustum.top] {
            assert!(
                plane.signed_distance(camera.position).abs() < 1e-4,
                "side plane should pass through the camera, got {}",
                plane.signed_distance(camera.position)
            );
        }
        let near_point = vec3(0.0, 0.0, 5.0 - camera.z_near);
        let far_point = vec3(0.0, 0.0, 5.0 - camera.z_far);
        assert!(frustum.near.signed_distance(near_point).abs() < 1e-3);
        assert!(frustum.far.signed_distance(far_point).abs() < 1e-2);
        assert!(frustum.contains_point(near_point));
        assert!(frustum.contains_point(Vec3::ZERO));
        assert!(!frustum.contains_point(camera.position));

        let edge = vec3(camera.z_near, 0.0, 5.0 - camera.z_near);
        assert!(frustum.right.signed_distance(edge).abs() < 1e-3);
        assert!(frustum.contains_point(edge));
        assert!(!frustum.contains_point(edge + Vec3::X * 0.05));

        let inside = Aabb::from_center_size(vec3(0.0, 0.0, 1.0), vec3(0.4, 0.4, 0.4));
        assert!(frustum.contains_aabb(&inside));
        let behind = Aabb::from_center_size(vec3(0.0, 0.0, 6.0), vec3(0.2, 0.2, 0.2));
        assert!(!frustum.intersects_aabb(&behind));
    }

    #[test]
    fn orthographic_frustum_is_a_box_in_front_of_the_camera() {
        let camera = Camera3D {
            position: vec3(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fovy: 4.0,
            aspect: Some(2.0),
            projection: Projection::Orthographics,
            z_near: 1.0,
            z_far: 20.0,
            ..Default::default()
        };
        let frustum = camera.frustum().unwrap();
        assert!(frustum.contains_point(vec3(0.0, 0.0, 0.0)));
        assert!(frustum.near.signed_distance(vec3(0.0, 0.0, 4.0)).abs() < 1e-3);
        assert!(frustum.right.signed_distance(vec3(4.0, 0.0, 0.0)).abs() < 1e-3);
        assert!(frustum.top.signed_distance(vec3(0.0, 2.0, 0.0)).abs() < 1e-3);
        assert!(!frustum.contains_point(vec3(4.2, 0.0, 0.0)));
        assert!(!frustum.contains_point(vec3(0.0, 0.0, 4.2)));
        let touching = crate::math::Sphere::new(vec3(4.2, 0.0, 0.0), 0.2);
        assert!(frustum.intersects_sphere(&touching));
        assert!(!frustum.contains_sphere(&touching));
    }

    #[test]
    fn z_up_frustum_looks_along_that_camera() {
        let camera = Camera3D {
            position: vec3(0.0, -5.0, 1.0),
            target: vec3(0.0, 0.0, 1.0),
            up: Vec3::Z,
            fovy: 90.0_f32.to_radians(),
            aspect: Some(1.0),
            z_near: 1.0,
            z_far: 8.0,
            ..Default::default()
        };
        let frustum = camera.frustum().unwrap();
        assert!(frustum.contains_point(vec3(0.0, -2.0, 1.0)));
        assert!(!frustum.contains_point(vec3(0.0, -4.5, 1.0)));
        assert!(frustum.near.signed_distance(vec3(0.0, -4.0, 1.0)).abs() < 1e-3);
        assert!(frustum.far.signed_distance(vec3(0.0, 3.0, 1.0)).abs() < 1e-2);
        let side = vec3(2.0, -3.0, 1.0);
        assert!(frustum.contains_point(side));
        assert!(!frustum.contains_point(side + Vec3::X));
        let above = vec3(0.0, -3.0, 3.0);
        assert!(frustum.contains_point(above));
        assert!(!frustum.contains_point(above + Vec3::Z));
    }

    #[test]
    fn a_missing_aspect_uses_the_viewport_and_a_bad_camera_is_none() {
        let mut camera = perspective_down_z();
        camera.aspect = None;
        camera.viewport = Some((0, 0, 200, 100));
        let wide = camera.frustum().unwrap();
        // 90° vertical fov, aspect 2: at the near plane the half width is 0.5.
        let right = vec3(0.5, 0.0, 5.0 - camera.z_near);
        assert!(wide.right.signed_distance(right).abs() < 1e-3);

        camera.viewport = None;
        assert!(camera.frustum().is_none());

        camera = perspective_down_z();
        camera.target = camera.position;
        assert!(camera.frustum().is_none());

        camera = perspective_down_z();
        camera.up = -Vec3::Z;
        assert!(camera.frustum().is_none());

        camera = perspective_down_z();
        camera.aspect = Some(0.0);
        assert!(camera.frustum().is_none());

        camera.aspect = Some(f32::NAN);
        assert!(camera.frustum().is_none());
    }
}
