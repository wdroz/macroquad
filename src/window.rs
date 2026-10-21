//! Window and associated to window rendering context related functions.

use crate::{get_context, get_quad_context};

use crate::color::Color;

// miniquad is re-exported for the use in combination with `get_internal_gl`
pub use miniquad;

pub use miniquad::conf::Conf;

/// Block execution until the next frame.
#[must_use = "use `next_frame().await` to advance to the next frame"]
pub fn next_frame() -> crate::exec::FrameFuture {
    crate::thread_assert::same_thread();
    crate::exec::FrameFuture::default()
}

/// Fill window background with solid color.
/// Note: even when "clear_background" is not called explicitly,
/// the screen will be cleared at the beginning of the frame.
pub fn clear_background(color: Color) {
    let context = get_context();

    context.gl.clear(get_quad_context(), color);
}

/// Draw immediate shapes, images and text inside a nested rectangular clip.
///
/// The rectangle has its origin at the **top-left of the whole drawing surface**,
/// X right and Y down. On the window its units are logical window pixels, using
/// the current [`screen_dpi_scale`]. On a camera's render target they are target
/// texture pixels, independent of window DPI. The camera's world transform does
/// not transform the clip. Select the camera, render target and viewport before
/// entering the scope, and keep them unchanged until the closure returns.
///
/// Clips intersect the surface, the active camera's physical GL viewport and
/// all enclosing clips. Nested intersections happen before rounding: minima
/// round down and maxima round up to physical pixel edges. A disjoint, empty,
/// nonpositive-size or nonfinite rectangle clips everything, including in
/// deeper scopes. Nothing is flushed just to change a clip; queued draws retain
/// their individual clips. The previous clip is restored when the closure
/// returns (also during native panic unwinding). The closure's result is returned.
///
/// Use synchronous drawing only: do not advance a frame inside the closure.
/// Deferred built-in UI widgets retain their own clipping and are drawn later,
/// outside this scope. Background clears and raw backend calls are not clipped.
///
/// ```no_run
/// use macroquad::prelude::*;
/// set_default_camera();
/// let value = with_clip_rect(Rect::new(20., 60., 280., 160.), || {
///     draw_rectangle(0., 0., 600., 400., WHITE);
///     draw_text("A scrolling document", 28., 90., 24., BLACK);
///     with_clip_rect(Rect::new(40., 80., 80., 40.), || {
///         draw_rectangle(0., 0., 600., 400., BLUE);
///     });
///     42
/// });
/// assert_eq!(value, 42);
/// draw_text("Unclipped HUD", 20., 30., 24., WHITE);
/// ```
pub fn with_clip_rect<R>(rect: crate::math::Rect, draw: impl FnOnce() -> R) -> R {
    use crate::clip_rect::{surface_clip, ClipRect, Scissor};

    struct RestoreClip {
        scissor: Option<Scissor>,
        scope: Option<ClipRect>,
    }
    impl Drop for RestoreClip {
        fn drop(&mut self) {
            let gl = &mut get_context().gl;
            gl.scissor(self.scissor);
            gl.set_clip_scope(self.scope);
        }
    }

    let context = get_context();
    let gl = &mut context.gl;
    let restore = RestoreClip {
        scissor: gl.scissor_state(),
        scope: gl.clip_scope(),
    };
    let (width, height, scale) = if let Some(pass) = gl.get_active_render_pass() {
        let backend = get_quad_context();
        let texture = backend.render_pass_texture(pass);
        let (width, height) = backend.texture_size(texture);
        (width, height, 1.)
    } else {
        let (width, height) = miniquad::window::screen_size();
        (width as u32, height as u32, screen_dpi_scale())
    };
    let mut clip = ClipRect::from_rect(rect, scale).intersect(surface_clip(
        width,
        height,
        gl.viewport_state(),
    ));
    if let Some(parent) = restore.scope {
        clip = clip.intersect(parent);
    }
    if let Some(previous) = restore.scissor {
        clip = clip.intersect(ClipRect::from_scissor(previous));
    }
    gl.set_clip_scope(Some(clip));
    gl.scissor(Some(clip.scissor()));
    let result = draw();
    drop(restore);
    result
}

#[doc(hidden)]
pub fn gl_set_drawcall_buffer_capacity(max_vertices: usize, max_indices: usize) {
    let context = get_context();
    context
        .gl
        .update_drawcall_capacity(get_quad_context(), max_vertices, max_indices);
}

/// Flushes queued geometry and releases unused renderer resources.
///
/// Use this at an infrequent scene-transition boundary, after
/// [`next_frame().await`](next_frame) has finished the old frame and before
/// loading the replacement scene. Switch away from the old camera/render target
/// and material before dropping them. Drop the old scene and release any
/// material texture bindings that still own its textures, then call this
/// function. Any geometry queued since the last frame is submitted first, so
/// dropping a mesh or texture used by that geometry does not cancel its draw.
/// Deferred UI work is rendered by `next_frame`, not by this operation.
///
/// Managed textures and retained mesh buffers are collected only after their
/// last owner is gone. Live textures, retained meshes, materials and render
/// targets remain valid; keep the active camera's render target and material
/// alive until switching away from them. Custom materials referenced by queued
/// draws must stay alive through this call, even after switching materials.
/// Weak and unmanaged handles do not own resources. Drawing state and
/// configured draw-call capacity limits are kept.
/// Users of raw internal geometry must rebind any texture they have released.
///
/// Cached immediate CPU geometry, draw-call storage and streaming GPU buffers
/// are released and recreated as needed on subsequent draws. This may cause
/// allocation work next frame, so do not call it every frame or for an ordinary
/// pause. Pausing and normal frame boundaries do not perform this trim. The
/// graphics driver may defer deletion; this is not a guarantee that process or
/// driver memory usage immediately decreases. Requires a live window and its
/// render thread, like the drawing functions.
///
/// ```no_run
/// # use macroquad::prelude::*;
/// # async fn unload_scene(old_scene: RetainedMesh) {
/// next_frame().await;
/// set_default_camera();
/// gl_use_default_material();
/// drop(old_scene);
/// retire_unused_render_resources();
/// // Load the next scene here; surviving shared handles can still be drawn.
/// # }
/// ```
pub fn retire_unused_render_resources() {
    let context = get_context();
    context.perform_render_passes();
    context.gl.trim_drawcall_buffers(get_quad_context());
    let quad_context = get_quad_context();
    if quad_context.info().backend == miniquad::Backend::OpenGl {
        // miniquad 0.4.11's GL texture deletion leaves texture bindings cached.
        // Clear them before deletion, as the ordinary end-of-frame path does,
        // so reused GL names cannot corrupt subsequent texture uploads. On GL,
        // commit_frame only clears bindings; Metal must not commit mid-frame.
        quad_context.commit_frame();
    }
    // Submission releases queued mesh/texture owners. Collect afterwards,
    // including textures whose last owner was a just-submitted draw.
    context.textures.garbage_collect(quad_context);
}

pub struct InternalGlContext<'a> {
    pub quad_context: &'a mut dyn miniquad::RenderingBackend,
    pub quad_gl: &'a mut crate::quad_gl::QuadGl,
}

impl<'a> InternalGlContext<'a> {
    /// Draw all the batched stuff and reset the internal state cache.
    /// May be helpful for combining macroquad's drawing with raw miniquad/opengl calls.
    pub fn flush(&mut self) {
        get_context().perform_render_passes();
    }
}

pub unsafe fn get_internal_gl<'a>() -> InternalGlContext<'a> {
    let context = get_context();

    InternalGlContext {
        quad_context: get_quad_context(),
        quad_gl: &mut context.gl,
    }
}

pub fn screen_width() -> f32 {
    let context = get_context();
    context.screen_width / screen_dpi_scale()
}

pub fn screen_height() -> f32 {
    let context = get_context();

    context.screen_height / screen_dpi_scale()
}

/// Current framebuffer pixels per logical window unit.
///
/// On the web this reads the host's current density, including browser zoom
/// and display changes, rather than miniquad's initial cached value.
pub fn screen_dpi_scale() -> f32 {
    #[cfg(target_arch = "wasm32")]
    let density = unsafe {
        extern "C" {
            fn dpi_scale() -> f32;
        }
        dpi_scale()
    };
    #[cfg(not(target_arch = "wasm32"))]
    let density = miniquad::window::dpi_scale();
    crate::input_coordinates::valid_density(density)
}

/// Converts a logical window position or displacement to framebuffer pixels.
///
/// Uses [`screen_dpi_scale`] now; both coordinate systems start at the window's
/// top-left, with X right and Y down. Does not apply camera or viewport transforms.
///
/// ```no_run
/// use macroquad::prelude::*;
/// let physical = logical_to_physical(vec2(100.0, 80.0));
/// ```
pub fn logical_to_physical(position: glam::Vec2) -> glam::Vec2 {
    crate::input_coordinates::to_physical(position, screen_dpi_scale())
}

/// Converts framebuffer pixels to logical window positions or displacements.
///
/// These are the units of [`screen_width`], [`screen_height`] and 2D drawing.
/// No camera or viewport transformation is applied.
///
/// ```no_run
/// use macroquad::prelude::*;
/// let logical = physical_to_logical(vec2(200.0, 160.0));
/// ```
pub fn physical_to_logical(position: glam::Vec2) -> glam::Vec2 {
    crate::input_coordinates::to_logical(position, screen_dpi_scale())
}

/// Request the window size to be the given value. This takes DPI into account.
///
/// Note that the OS might decide to give a different size. Additionally, the size in macroquad won't be updated until the next `next_frame().await`.
pub fn request_new_screen_size(width: f32, height: f32) {
    miniquad::window::set_window_size(
        (width * screen_dpi_scale()) as u32,
        (height * screen_dpi_scale()) as u32,
    );
    // We do not set the context.screen_width and context.screen_height here.
    // After `set_window_size` is called, EventHandlerFree::resize will be invoked, setting the size correctly.
    // Because the OS might decide to give a different screen dimension, setting the context.screen_* here would be confusing.
}

/// Toggle whether the window is fullscreen.
pub fn set_fullscreen(fullscreen: bool) {
    miniquad::window::set_fullscreen(fullscreen);
}

/// With `set_panic_handler` set to a handler code, macroquad will use
/// `std::panic::catch_unwind` on user code to catch some panics.
///
/// Sometimes it is nice to let player send a bug report with a screenshot of an
/// error. It is way easier to ask for a screenshot than ask to connect to a phone
/// with adb and post a log.
///
/// For this very case "set_panic_handler" exists.
/// ```ignore
/// set_panic_handler(|msg, backtrace| async move {
///     loop {
///         clear_background(RED);
///         ui::root_ui().label(None, &msg);
///         for line in backtrace.split('\n') {
///             root_ui().label(None, line);
///         }
///         next_frame().await;
///      }
/// });
/// ```
///
/// `set_panic_handler` acts as a second app entry-point, that will be used
/// after a panic in user code will happen. Macroquad will also try to catch some OS
/// panics, but not all of them - some compatibility bugs may end up crashing the app.
///
/// Without `set_panic_handler` macroquad will not use `catch_unwind` at all,
/// therefore `panic_handler` is completely optional.
/// NOTE: only with "backtrace" macroquad feature `backtrace` string will contain an
/// actual backtrace. Otherwise only panic location and message will be available.
/// NOTE: on android, even with "backtrace" nice backtrace is available only if the game is compiled with sdk >= 21.
/// To use sdk >= 21 add "min_sdk_version = 21" to Cargo.toml
pub fn set_panic_handler<T, F>(future: F)
where
    T: std::future::Future<Output = ()> + 'static,
    F: Fn(String, String) -> T + Send + Sync + 'static,
{
    std::panic::set_hook(Box::new(move |info| {
        let message = format!("{info:?}");
        #[cfg(feature = "backtrace")]
        let backtrace_string = format!("{:?}", backtrace::Backtrace::new());
        #[cfg(not(feature = "backtrace"))]
        let backtrace_string = format!("Macroquad compiled without \"backtrace\" feature");
        crate::logging::error!("{}", message);
        crate::logging::error!("{}", backtrace_string);

        crate::get_context().recovery_future = Some(Box::pin(future(message, backtrace_string)));
    }));

    crate::get_context().unwind = true;
}
