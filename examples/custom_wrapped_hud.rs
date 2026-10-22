//! Immediate drawing recipe: resize; wheel inside panel / Up / Down; Home / End.
#[path = "support/custom_wrapped_hud_layout.rs"]
mod layout;
use layout::{arrange, panel_limit, text_width, FONT_SCALE, FONT_SIZE, LINE_DISTANCE};
use macroquad::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

const NOTES: &str = "Dispatch notes\n\nCarry the parcel through the courtyard and keep the route clear. Resize this panel: its height follows the measured text until the viewport limit.\nLong ID: COURIERWITHOUTANYSPACESTOBREAKTHELINE1234567890\nHard breaks stay explicit.";

// Read-only browser/native replay diagnostics. All geometry uses logical pixels.
// 0..2 window w/h/DPR; 3..6 panel xywh; 7..10 text clip xywh;
// 11..13 measured width/height/offset; 14..15 baseline xy; 16 scroll;
// 17 max scroll; 18 line count; 19 requested panel height;
// 20..22 drawn width/height/offset; 23 font size; 24 scale; 25 line distance.
static VALUES: [AtomicU32; 26] = [const { AtomicU32::new(0) }; 26];
static STATUS: AtomicU32 = AtomicU32::new(0);
static FRAMES: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub extern "C" fn custom_wrapped_hud_status() -> u32 {
    STATUS.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn custom_wrapped_hud_frames() -> u32 {
    FRAMES.load(Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn custom_wrapped_hud_value(index: u32) -> f32 {
    VALUES
        .get(index as usize)
        .map_or(0., |value| f32::from_bits(value.load(Ordering::Relaxed)))
}

fn config() -> Conf {
    Conf {
        window_title: "Custom wrapped HUD".into(),
        window_width: 960,
        window_height: 640,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(config)]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        STATUS.store(2, Ordering::Relaxed);
        macroquad::logging::error!("CUSTOM_WRAPPED_HUD_FAIL: {}", info);
    }));
    // Embed the existing small fixture: native and WebGL need no asset request.
    let font = load_ttf_font_from_bytes(include_bytes!("ui_assets/MinimalPixel v2.ttf")).unwrap();
    // Existing wrap_text compares DPI-scaled glyph advances. Its width argument
    // therefore uses physical pixels; measured/drawn positions remain logical.
    let wrap = |text: &str, width| {
        wrap_text(
            text,
            Some(&font),
            FONT_SIZE,
            FONT_SCALE,
            width * screen_dpi_scale(),
        )
    };
    assert_eq!(wrap("First\n\nSecond", 10_000.), "First\n\nSecond");
    let word = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let broken = wrap(word, 80.);
    assert!(broken.contains('\n'));
    assert_eq!(broken.replace('\n', ""), word);
    for line in broken.lines() {
        assert!(measure_text(line, Some(&font), FONT_SIZE, FONT_SCALE).width <= 80.01);
    }
    let mut scroll = 0.;
    #[cfg(not(target_arch = "wasm32"))]
    let mut previous = None;
    loop {
        set_default_camera();
        clear_background(Color::from_rgba(18, 28, 45, 255));
        let size = vec2(screen_width(), screen_height());
        let limit = panel_limit(size);
        let width = text_width(limit);
        let wrapped = if width > 0. {
            wrap(NOTES, width)
        } else {
            String::new()
        };
        let measured = measure_multiline_text(
            &wrapped,
            Some(&font),
            FONT_SIZE,
            FONT_SCALE,
            Some(LINE_DISTANCE),
        );
        // The application owns scrolling; re-clamp against each resized layout.
        if arrange(limit, measured, scroll)
            .panel
            .contains(mouse_position_logical())
        {
            scroll -= mouse_wheel().1 * 28.;
        }
        scroll += (is_key_down(KeyCode::Down) as u8 as f32 - is_key_down(KeyCode::Up) as u8 as f32)
            * get_frame_time()
            * 160.;
        if is_key_pressed(KeyCode::Home) {
            scroll = 0.;
        }
        if is_key_pressed(KeyCode::End) {
            scroll = f32::MAX;
        }
        let panel = arrange(limit, measured, scroll);
        scroll = panel.scroll;
        let drawn = with_clip_rect(panel.panel, || {
            // Even this deliberately oversized background stays in the panel.
            draw_rectangle(
                panel.panel.x - 8.,
                panel.panel.y - 8.,
                panel.panel.w + 16.,
                panel.panel.h + 16.,
                Color::from_rgba(36, 58, 76, 255),
            );
            with_clip_rect(panel.clip, || {
                draw_multiline_text_ex(
                    &wrapped,
                    panel.baseline.x,
                    panel.baseline.y,
                    Some(LINE_DISTANCE),
                    TextParams {
                        font: Some(&font),
                        font_size: FONT_SIZE,
                        font_scale: FONT_SCALE,
                        color: WHITE,
                        ..Default::default()
                    },
                )
            })
        });
        // Same custom font, size, scale, unrotated aspect and line distance above.
        assert!((drawn.width - measured.width).abs() < 0.01);
        assert!((drawn.height - measured.height).abs() < 0.01);
        assert!((drawn.offset_y - measured.offset_y).abs() < 0.01);
        // Both scopes have ended: ordinary HUD drawing is restored.
        draw_text("Custom wrapped HUD", 16., 28., 25., WHITE);
        draw_text("Resize | wheel / arrows | Home / End", 16., 48., 16., WHITE);
        draw_text(
            "HUD restored outside the panel",
            16.,
            size.y - 12.,
            16.,
            YELLOW,
        );
        draw_rectangle(size.x - 24., 8., 12., 12., GREEN);
        draw_rectangle(size.x - 24., size.y - 24., 12., 12., GOLD);
        let values = [
            size.x,
            size.y,
            screen_dpi_scale(),
            panel.panel.x,
            panel.panel.y,
            panel.panel.w,
            panel.panel.h,
            panel.clip.x,
            panel.clip.y,
            panel.clip.w,
            panel.clip.h,
            measured.width,
            measured.height,
            measured.offset_y,
            panel.baseline.x,
            panel.baseline.y,
            scroll,
            panel.scroll_max,
            wrapped.lines().count() as f32,
            panel.requested_height,
            drawn.width,
            drawn.height,
            drawn.offset_y,
            FONT_SIZE as f32,
            FONT_SCALE,
            LINE_DISTANCE,
        ];
        for (target, value) in VALUES.iter().zip(values) {
            target.store(value.to_bits(), Ordering::Relaxed);
        }
        FRAMES.fetch_add(1, Ordering::Relaxed);
        STATUS.store(1, Ordering::Relaxed);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let state = (size.x.to_bits(), size.y.to_bits(), scroll.to_bits());
            if previous != Some(state) && std::env::var_os("MACROQUAD_WRAPPED_HUD_TRACE").is_some()
            {
                println!("CUSTOM_WRAPPED_HUD_STATE {{\"values\":{values:?}}}");
            }
            previous = Some(state);
        }
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
