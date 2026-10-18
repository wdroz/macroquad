use crate::{get_context, get_quad_context, time::get_time};

use std::collections::HashMap;

static mut PROFILER: Option<Profiler> = None;

fn get_profiler() -> &'static mut Profiler {
    unsafe {
        PROFILER.get_or_insert_with(|| Profiler {
            frame: Frame::new(),
            queries: HashMap::new(),
            active_query: None,
            prev_frame: Frame::new(),
            enabled: false,
            enable_request: None,
            capture_request: false,
            capture: false,
            drawcalls: vec![],
            strings: vec![],
        })
    }
}

#[derive(Debug, Clone)]
pub struct Zone {
    pub name: String,
    pub start_time: f64,
    pub duration: f64,
    pub children: Vec<Zone>,

    parent: *mut Zone,
}

impl Zone {
    fn clone(&self, parent: *mut Zone) -> Zone {
        Zone {
            name: self.name.clone(),
            start_time: self.start_time,
            duration: self.duration,
            children: self
                .children
                .iter()
                .map(|zone| zone.clone(self as *const _ as *mut _))
                .collect(),
            parent,
        }
    }
}

pub struct ZoneGuard {
    _marker: (),
}

impl ZoneGuard {
    pub fn new(name: &str) -> ZoneGuard {
        begin_zone(name);
        ZoneGuard { _marker: () }
    }
}

impl Drop for ZoneGuard {
    fn drop(&mut self) {
        end_zone();
    }
}

pub fn enable() {
    get_profiler().enable_request = Some(true);
}

pub fn disable() {
    get_profiler().enable_request = Some(false);
}

pub fn is_enabled() -> bool {
    get_profiler().enabled
}

pub fn begin_zone(name: &str) {
    if get_profiler().enabled {
        get_profiler().begin_zone(name);
    }
}

pub fn end_zone() {
    if get_profiler().enabled {
        get_profiler().end_zone();
    }
}

pub fn begin_gpu_query(name: &str) {
    get_profiler().begin_gpu_query(name);
}

pub fn end_gpu_query() {
    get_profiler().end_gpu_query();
}

/// Workaround to stop gl capture on debug rendering
#[doc(hidden)]
pub fn pause_gl_capture() {
    if get_profiler().capture {
        crate::get_context().gl.capture(false);
    }
}

/// Workaround to stop gl capture on debug rendering
pub fn resume_gl_capture() {
    if get_profiler().capture {
        crate::get_context().gl.capture(false);
    }
}

pub(crate) fn reset() {
    let profiler = get_profiler();

    assert!(
        get_profiler().frame.active_zone.is_null(),
        "New frame started with unpaired begin/end zones."
    );

    profiler.frame.full_frame_time = crate::time::get_frame_time();

    std::mem::swap(&mut profiler.prev_frame, &mut profiler.frame);
    profiler.frame = Frame::new();

    if let Some(enable) = profiler.enable_request.take() {
        profiler.enabled = enable;
    }

    if profiler.capture {
        profiler.capture = false;
        crate::get_context().gl.capture(false);
    }

    if profiler.capture_request {
        profiler.drawcalls.clear();
        profiler.capture = true;
        crate::get_context().gl.capture(true);
        profiler.capture_request = false;
    }
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub full_frame_time: f32,
    pub zones: Vec<Zone>,
    active_zone: *mut Zone,
}

impl Frame {
    const fn new() -> Frame {
        Frame {
            full_frame_time: 0.0,
            zones: vec![],
            active_zone: std::ptr::null_mut(),
        }
    }

    pub fn try_clone(&self) -> Option<Frame> {
        if self.active_zone.is_null() == false {
            return None;
        }

        Some(Frame {
            full_frame_time: self.full_frame_time,
            zones: self
                .zones
                .iter()
                .map(|zone| zone.clone(std::ptr::null_mut()))
                .collect(),
            active_zone: std::ptr::null_mut(),
        })
    }
}

pub fn frame() -> Frame {
    get_profiler().prev_frame.clone()
}

pub fn gpu_queries() -> Vec<(String, u64)> {
    get_profiler()
        .queries
        .iter()
        .map(|(name, query)| (name.to_owned(), query.value))
        .collect()
}

pub fn sample_gpu_queries() {
    for query in &mut get_profiler().queries {
        query.1.force_resume = true;
    }
}

struct Profiler {
    frame: Frame,
    prev_frame: Frame,
    queries: HashMap<String, GpuQuery>,
    active_query: Option<String>,
    enabled: bool,
    capture_request: bool,
    capture: bool,
    enable_request: Option<bool>,
    drawcalls: Vec<DrawCallTelemetry>,
    strings: Vec<String>,
}

impl Profiler {
    fn begin_gpu_query(&mut self, name: &str) {
        assert!(
            self.active_query.is_none(),
            "Only one active query is allowed by OpenGL"
        );

        let name = name.to_string();
        let query = self
            .queries
            .entry(name.clone())
            .or_insert_with(|| GpuQuery {
                query: miniquad::graphics::ElapsedQuery::new(),
                in_progress: false,
                value: 0,
                force_resume: false,
            });
        self.active_query = Some(name);
        if query.force_resume {
            query.in_progress = true;
            query.query.begin_query();
        }
    }

    fn end_gpu_query(&mut self) {
        let name = self
            .active_query
            .take()
            .expect("End query without begin query");
        let query = self.queries.get_mut(&name).unwrap();
        if query.in_progress {
            query.force_resume = false;
            query.in_progress = false;
            query.query.end_query();
        }
        if query.query.is_available() {
            query.value = query.query.get_result();
        }
    }

    fn begin_zone(&mut self, name: &str) {
        let zones = if self.frame.active_zone.is_null() {
            &mut self.frame.zones
        } else {
            unsafe { &mut (&mut *self.frame.active_zone).children }
        };

        zones.push(Zone {
            name: name.to_string(),
            start_time: get_time(),
            duration: 0.0,
            parent: self.frame.active_zone,
            children: vec![],
        });
        self.frame.active_zone = zones.last_mut().unwrap() as _;
    }

    fn end_zone(&mut self) {
        assert!(
            self.frame.active_zone.is_null() == false,
            "end_zone called without begin_zone"
        );

        let start_time = unsafe { (&mut *self.frame.active_zone).start_time };
        let duration = get_time() - start_time;

        unsafe { (&mut *self.frame.active_zone).duration = duration };
        self.frame.active_zone = unsafe { (&mut *self.frame.active_zone).parent };
    }
}

pub struct GpuQuery {
    pub query: miniquad::graphics::ElapsedQuery,
    pub in_progress: bool,
    pub value: u64,
    pub force_resume: bool,
}

pub fn scene_allocated_memory() -> usize {
    use crate::experimental::scene;

    scene::allocated_memory()
}

/// ```skip
/// {
///    let _t = telemetry::LogTimeGuard::new("Atlas build time");
///     mq::texture::build_textures_atlas();
/// }
/// ```
/// Will add "Time query: Atlas build time, 0.5s" string to
/// `telemetry::strings()`
pub struct LogTimeGuard<'a> {
    name: &'a str,
    start_time: f64,
}

impl<'a> LogTimeGuard<'a> {
    pub fn new(name: &'a str) -> LogTimeGuard<'a> {
        LogTimeGuard {
            name,
            start_time: get_time(),
        }
    }
}

impl<'a> Drop for LogTimeGuard<'a> {
    fn drop(&mut self) {
        log_string(&format!(
            "Time query: {}, {:.1}s",
            self.name,
            get_time() - self.start_time
        ));
    }
}

pub fn log_string(string: &str) {
    get_profiler().strings.push(string.to_owned());
}

pub fn drawcalls() -> Vec<DrawCallTelemetry> {
    get_profiler().drawcalls.clone()
}

pub fn strings() -> Vec<String> {
    get_profiler().strings.clone()
}

/// Note that coroutines pre-allocate 56Kb, so 56000 as a result doesnt mean a leak
pub fn coroutines_allocated_memory() -> usize {
    get_context().coroutines_context.allocated_memory()
}

pub fn active_coroutines_count() -> usize {
    get_context().coroutines_context.active_coroutines_count()
}

pub fn capture_frame() {
    get_profiler().capture_request = true;
}

#[derive(Clone, Debug)]
pub struct DrawCallTelemetry {
    pub indices_count: usize,
    pub texture: miniquad::TextureId,
}

pub(crate) fn track_drawcall(
    pipeline: &miniquad::Pipeline,
    bindings: &miniquad::Bindings,
    indices_count: usize,
) {
    let texture = get_quad_context().new_render_texture(miniquad::TextureParams {
        width: 128,
        height: 128,
        ..Default::default()
    });

    let pass = Some(get_quad_context().new_render_pass(texture, None));
    get_quad_context().begin_pass(pass, miniquad::PassAction::clear_color(0.4, 0.8, 0.4, 1.));
    get_quad_context().apply_pipeline(pipeline);
    get_quad_context().apply_bindings(bindings);
    get_quad_context().draw(0, indices_count as _, 1);
    get_quad_context().end_render_pass();

    get_profiler().drawcalls.push(DrawCallTelemetry {
        indices_count,
        texture,
    });
}

pub fn textures_count() -> usize {
    get_context().textures.len()
}

/// Geometry work submitted while renderer diagnostics are enabled.
///
/// Counts accumulate across frames until [`reset_renderer_diagnostics`]. Each
/// vertex or index buffer upload is one operation; allocating empty streaming
/// capacity is not an upload. These are submitted backend calls, not GPU timings
/// or application draw-function calls (immediate geometry may be batched).
///
/// ```no_run
/// use macroquad::telemetry;
/// telemetry::set_renderer_diagnostics_enabled(true);
/// telemetry::reset_renderer_diagnostics();
/// // Draw and finish a frame before reading submitted work.
/// let counts = telemetry::renderer_diagnostics().counters;
/// println!("{} draws, {} streamed bytes", counts.draw_calls, counts.streaming_buffer_upload_bytes);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RendererCounters {
    /// Total backend draws, including the auxiliary draws below.
    pub draw_calls: u64,
    /// Backend draws using batched immediate geometry.
    pub immediate_draw_calls: u64,
    /// Backend draws using retained geometry. Instances remain separate draws.
    pub retained_draw_calls: u64,
    /// Screen-texture copies and telemetry capture draws submitted by the renderer.
    /// Direct calls through the internal backend API are not observed.
    pub auxiliary_draw_calls: u64,
    /// Vertex/index buffer updates for immediate geometry (two per batch).
    pub streaming_buffer_uploads: u64,
    /// Geometry bytes passed to those immediate buffer updates.
    pub streaming_buffer_upload_bytes: u64,
    /// Initialized immutable vertex/index buffers (two per retained mesh).
    pub retained_buffer_uploads: u64,
    /// Geometry bytes passed when creating retained buffers.
    pub retained_buffer_upload_bytes: u64,
}

/// An on-demand snapshot of renderer geometry storage and optional work counts.
///
/// CPU values cover the immediate batching vectors, with initialized bytes
/// separate from allocated capacity. GPU byte values are requested buffer sizes,
/// not measured driver allocation or total process memory. They exclude textures,
/// shaders, render targets, snapshot-copy buffers, driver alignment/overhead, and
/// model-owned CPU geometry. glTF models expose their own resource reports.
/// Shared retained buffers count once regardless of handles or queued instances.
/// Buffers awaiting the next ordinary flush/upload collection are still included.
///
/// Reading a report neither flushes draws nor collects resources. Resource sizes
/// can be queried with counting disabled; only [`RendererCounters`] require
/// [`set_renderer_diagnostics_enabled`]. All functions require a live window.
///
/// ```no_run
/// let report = macroquad::telemetry::renderer_diagnostics();
/// println!("{} retained bytes, {} queued batches",
///     report.estimated_retained_gpu_buffer_bytes, report.queued_draw_calls);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RendererDiagnostics {
    /// Whether future renderer operations will update `counters`.
    pub counters_enabled: bool,
    /// Work since the last explicit reset, recorded only while enabled.
    pub counters: RendererCounters,
    /// Currently queued batches and retained draws, not yet submitted.
    pub queued_draw_calls: usize,
    /// Initialized vertex bytes in the immediate CPU batching vector.
    pub cpu_vertex_bytes: usize,
    /// Allocated vertex capacity in bytes, retained across normal frame clears.
    pub cpu_vertex_capacity_bytes: usize,
    /// Initialized index bytes in the immediate CPU batching vector.
    pub cpu_index_bytes: usize,
    /// Allocated index capacity in bytes, retained across normal frame clears.
    pub cpu_index_capacity_bytes: usize,
    /// Unique retained vertex/index buffer pairs owned by the renderer.
    pub retained_mesh_count: usize,
    /// Number of retained GPU buffers (two per retained mesh).
    pub retained_gpu_buffer_count: usize,
    /// Requested retained GPU buffer bytes; excludes driver overhead.
    pub estimated_retained_gpu_buffer_bytes: usize,
    /// Number of reusable immediate GPU buffers, including idle slots.
    pub streaming_gpu_buffer_count: usize,
    /// Requested immediate GPU buffer capacity bytes; excludes driver overhead.
    pub estimated_streaming_gpu_buffer_capacity_bytes: usize,
}

/// Enables or pauses renderer operation counting, initially disabled.
///
/// This takes effect immediately and does not reset existing counts. Queued draws
/// count only if enabled when they are submitted; uploads count at upload time.
/// Diagnostic resource snapshots remain available while counting is disabled.
///
/// ```no_run
/// macroquad::telemetry::set_renderer_diagnostics_enabled(true);
/// ```
pub fn set_renderer_diagnostics_enabled(enabled: bool) {
    get_context().gl.set_diagnostics_enabled(enabled);
}

/// Resets cumulative renderer operation counts without changing enablement.
///
/// Does not flush, discard queued draws, or release resources. Reset before the
/// measured work, then finish/flush that work before reading the report.
///
/// ```no_run
/// macroquad::telemetry::reset_renderer_diagnostics();
/// ```
pub fn reset_renderer_diagnostics() {
    get_context().gl.reset_diagnostics();
}

/// Reads renderer resources and cumulative operation counts without a flush.
///
/// See [`RendererDiagnostics`] for scope and byte-estimation limits.
///
/// ```no_run
/// let report = macroquad::telemetry::renderer_diagnostics();
/// assert!(report.cpu_vertex_capacity_bytes >= report.cpu_vertex_bytes);
/// ```
pub fn renderer_diagnostics() -> RendererDiagnostics {
    let context = get_context();
    context.gl.diagnostics(&mut *context.quad_context)
}
