//! Asset-free document interaction using existing camera, clipping and input APIs.
//! Wheel scroll; Ctrl+wheel zoom; left drag/pick; F fit; R 100%; Esc exit.
//! Touch: one finger drag/tap; two fingers midpoint-anchored pinch/pan.
#[path = "support/document_zoom_pan_state.rs"]
mod state;
use macroquad::prelude::*;
use state::{Document, Input, FIT_BUTTON, MARKER, PAGE, RESET_BUTTON};
use std::sync::atomic::{AtomicU32, Ordering};

// Read-only diagnostics for native stdout and the same external WebGL replay.
// Values: width,height,dpi,viewport x/y/w/h,zoom,pan x/y,pointer page x/y,
// hits,wheels,zooms,drag starts/frames,fit,anchor error/clamped,resizes,
// inside/outside presses,selected,rejected wheels,page origin x/y,
// marker screen x/y/w/h,captured. Window positions, pan, marker rectangles and
// anchor error use logical pixels; pointer page values use generated-page units.
// Appended touch values: mode (0 idle/1 drag/2 pinch), owner IDs (-1 absent),
// touch starts,pinch starts/frames,transitions,cancels,mouse blocked,
// rejected touch starts,taps. IDs are diagnostic f32 values only; actual
// ownership always compares u64 IDs without a lossy conversion.
static VALUES: [AtomicU32; 43] = [const { AtomicU32::new(0) }; 43];
static STATUS: AtomicU32 = AtomicU32::new(0);
static FRAMES: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub extern "C" fn document_zoom_pan_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn document_zoom_pan_frames() -> u32 {
    FRAMES.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn document_zoom_pan_value(index: u32) -> f32 {
    VALUES
        .get(index as usize)
        .map_or(0., |v| f32::from_bits(v.load(Ordering::Relaxed)))
}

fn config() -> Conf {
    Conf {
        window_title: "Document zoom and pan".into(),
        window_width: 960,
        window_height: 640,
        high_dpi: true,
        ..Default::default()
    }
}

fn draw_page(document: &Document) {
    draw_rectangle(0., 0., PAGE.x, PAGE.y, Color::from_rgba(244, 238, 216, 255));
    draw_rectangle(0., 0., PAGE.x, 12., Color::from_rgba(181, 56, 78, 255));
    draw_rectangle(
        0.,
        PAGE.y - 12.,
        PAGE.x,
        12.,
        Color::from_rgba(31, 144, 159, 255),
    );
    draw_text("A tiny generated page", 36., 68., 34., BLACK);
    draw_text("Page units: 600 x 800", 36., 110., 24., DARKGRAY);
    draw_text(
        "Drag the paper; click the colored note.",
        36.,
        150.,
        22.,
        DARKGRAY,
    );
    for line in 0..16 {
        let y = 400. + line as f32 * 21.;
        draw_rectangle(
            36.,
            y,
            350. + (line % 4) as f32 * 42.,
            7.,
            Color::from_rgba(134, 146, 151, 255),
        );
    }
    draw_rectangle(
        MARKER.x,
        MARKER.y,
        MARKER.w,
        MARKER.h,
        if document.selected { ORANGE } else { SKYBLUE },
    );
    draw_text(
        if document.selected {
            "Selected note"
        } else {
            "Click this note"
        },
        MARKER.x + 14.,
        MARKER.y + 54.,
        26.,
        BLACK,
    );
    // These oversized lines demonstrate clipping at all four viewport edges.
    draw_rectangle(-80., PAGE.y * 0.5, PAGE.x + 160., 3., MAROON);
    draw_rectangle(PAGE.x * 0.5, -80., 3., PAGE.y + 160., MAROON);
}

fn draw(document: &Document, size: Vec2) {
    set_default_camera();
    clear_background(Color::from_rgba(19, 28, 42, 255));
    let v = document.viewport;
    draw_rectangle(
        v.x - 2.,
        v.y - 2.,
        v.w + 4.,
        v.h + 4.,
        Color::from_rgba(84, 110, 135, 255),
    );
    draw_rectangle(v.x, v.y, v.w, v.h, Color::from_rgba(31, 45, 62, 255));
    let camera = Camera2D::from_display_rect(document.camera_rect(size));
    set_camera(&camera);
    with_clip_rect(v, || draw_page(document));
    set_default_camera();
    draw_text("Document zoom / pan", 16., 28., 25., WHITE);
    draw_text("Wheel: scroll   Ctrl+wheel: zoom", 16., 49., 16., WHITE);
    draw_text("Drag / tap note; pinch to zoom", 16., 68., 16., WHITE);
    draw_text(
        format!(
            "Zoom {:.0}%  picked {}  {}",
            document.zoom * 100.,
            document.hits,
            if document.follow_fit { "FIT" } else { "MANUAL" }
        ),
        16.,
        88.,
        16.,
        YELLOW,
    );
    for (rect, text) in [(FIT_BUTTON, "Fit"), (RESET_BUTTON, "Reset 100% ")] {
        draw_rectangle(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            Color::from_rgba(51, 75, 99, 255),
        );
        draw_text(text, rect.x + 8., rect.y + 17., 16., WHITE);
    }
    draw_text(
        "HUD restored outside the viewport",
        16.,
        size.y - 12.,
        15.,
        WHITE,
    );
    // Solid color probes make clip restoration inspectable at DPR 1 and 2.
    draw_rectangle(size.x - 24., 8., 12., 12., GREEN);
    draw_rectangle(size.x - 24., size.y - 24., 12., 12., GOLD);
}

fn publish(document: &Document, size: Vec2, pointer: Vec2, log: bool) {
    let page = document.window_to_page(pointer);
    let origin = document.page_to_window(Vec2::ZERO);
    let marker = document.page_to_window(vec2(MARKER.x, MARKER.y));
    let v = document.viewport;
    let ids = document.touch_ids();
    let values = [
        size.x,
        size.y,
        screen_dpi_scale(),
        v.x,
        v.y,
        v.w,
        v.h,
        document.zoom,
        document.pan.x,
        document.pan.y,
        page.x,
        page.y,
        document.hits as f32,
        document.wheels as f32,
        document.zooms as f32,
        document.drag_starts as f32,
        document.drag_frames as f32,
        u32::from(document.follow_fit) as f32,
        document.anchor_error,
        u32::from(document.anchor_clamped) as f32,
        document.resizes as f32,
        document.inside_presses as f32,
        document.outside_presses as f32,
        u32::from(document.selected) as f32,
        document.rejected_wheels as f32,
        origin.x,
        origin.y,
        marker.x,
        marker.y,
        MARKER.w * document.zoom,
        MARKER.h * document.zoom,
        u32::from(document.captured()) as f32,
        document.touch_mode() as f32,
        ids[0].map_or(-1., |id| id as f32),
        ids[1].map_or(-1., |id| id as f32),
        document.touch_starts as f32,
        document.pinch_starts as f32,
        document.pinch_frames as f32,
        document.touch_transitions as f32,
        document.touch_cancels as f32,
        u32::from(!document.mouse_allowed()) as f32,
        document.rejected_touch_starts as f32,
        document.touch_taps as f32,
    ];
    for (target, value) in VALUES.iter().zip(values) {
        target.store(value.to_bits(), Ordering::Relaxed);
    }
    if log {
        macroquad::logging::info!(
            "DOCUMENT_ZOOM_PAN_STATE {{\"frame\":{},\"values\":{:?}}}",
            FRAMES.load(Ordering::Relaxed),
            values
        );
    }
}

#[macroquad::main(config)]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        STATUS.store(2, Ordering::Relaxed);
        macroquad::logging::error!("DOCUMENT_ZOOM_PAN_FAIL: {}", info);
    }));
    // Own touch IDs explicitly instead of also processing their synthesized
    // mouse events. The state gate also drains any queued mouse state and
    // applies to the HUD buttons, including the final touch frame.
    simulate_mouse_with_touch(false);
    let mut document = Document::new(state::viewport(vec2(screen_width(), screen_height())));
    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        let size = vec2(screen_width(), screen_height());
        let pointer = mouse_position_logical();
        let touches = touches_logical();
        let mut changed = document.resize(state::viewport(size));
        let input = Input {
            pointer,
            pressed: is_mouse_button_pressed(MouseButton::Left),
            down: is_mouse_button_down(MouseButton::Left),
            released: is_mouse_button_released(MouseButton::Left),
            wheel: mouse_wheel().into(),
            zoom_modifier: is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl),
        };
        changed |= if touches.is_empty() {
            document.update(input)
        } else {
            document.update_with_touches(input, &touches)
        };
        let mouse_press = document.mouse_allowed() && is_mouse_button_pressed(MouseButton::Left);
        let touch_press = |rect| {
            touches.iter().any(|touch| {
                touch.phase == TouchPhase::Started && state::contains(rect, touch.position)
            })
        };
        if is_key_pressed(KeyCode::F)
            || (mouse_press && state::contains(FIT_BUTTON, pointer))
            || touch_press(FIT_BUTTON)
        {
            document.fit();
            changed = true;
        }
        if is_key_pressed(KeyCode::R)
            || (mouse_press && state::contains(RESET_BUTTON, pointer))
            || touch_press(RESET_BUTTON)
        {
            document.reset();
            changed = true;
        }
        assert!(document.bounded(), "document view escaped its bounds");
        draw(&document, size);
        let frame = FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
        publish(
            &document,
            size,
            pointer,
            changed || frame == 1 || frame.is_multiple_of(120),
        );
        if frame == 1 {
            STATUS.store(1, Ordering::Relaxed);
            macroquad::logging::info!("DOCUMENT_ZOOM_PAN_READY");
        }
        next_frame().await;
    }
}
