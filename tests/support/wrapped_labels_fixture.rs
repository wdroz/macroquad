//! Shared public-API native/WebGL fixture. No external assets are loaded.
use macroquad::{
    prelude::*,
    ui::{root_ui, widgets, InputHandler, Layout, LayoutSettings},
};

const DESCRIPTION: &str = "Pause and settings: wrapped labels reserve their measured height in this compact menu. Resize the window, then scroll to Apply settings.\nHard breaks stay explicit, including this second paragraph.";

#[derive(Default)]
pub struct Menu {
    pub clicks: u32,
    pub footer_clicks: u32,
    pub last: Rect,
    pub height: f32,
    pub width: f32,
    pub scroll_max: f32,
    pub scroll_y: f32,
}

impl Menu {
    pub fn draw(&mut self) {
        set_default_camera();
        clear_background(Color::from_rgba(18, 28, 45, 255));
        let mut ui = root_ui();
        let mut skin = ui.default_skin();
        skin.margin = 0.;
        skin.window_style = ui.style_builder().build();
        skin.label_style = ui
            .style_builder()
            .font_size(16)
            .text_color(WHITE)
            .margin(RectOffset::new(6., 10., 4., 8.))
            .build();
        skin.group_style = ui
            .style_builder()
            .color(Color::from_rgba(30, 44, 62, 255))
            .build();
        ui.push_skin(&skin);
        let outer = vec2(
            (screen_width() - 24.).max(1.),
            (screen_height() - 72.).max(1.),
        );
        widgets::Group::new(macroquad::hash!("wrapped-label-menu"), outer)
            .position(vec2(12., 12.))
            .content_layout(
                LayoutSettings::new(Layout::Vertical)
                    .padding(RectOffset::new(12., 12., 10., 12.))
                    .gap(vec2(8., 7.)),
            )
            .ui(&mut ui, |ui| {
                self.width = ui.available_width();
                let description = widgets::Label::new(DESCRIPTION)
                    .wrap(self.width)
                    .multiline(20.);
                self.height = description.measure(ui).y;
                let start = ui.canvas().cursor().y;
                description.ui(ui);
                assert!((ui.canvas().cursor().y - start - self.height - 7.).abs() < 0.02);
                for index in 0..14 {
                    widgets::Button::new(format!("Setting {}: normal", index + 1))
                        .size(vec2(ui.available_width(), 30.))
                        .ui(ui);
                }
                let position = ui.canvas().cursor();
                self.last = Rect::new(position.x, position.y, ui.available_width(), 34.);
                if widgets::Button::new("Apply settings")
                    .size(self.last.size())
                    .ui(ui)
                {
                    self.clicks += 1;
                }
                self.scroll_max = ui.scroll_max().y.max(0.);
                self.scroll_y = ui.scroll().y;
            });
        if widgets::Button::new(format!("Unclipped footer | applies {}", self.clicks))
            .position(vec2(12., screen_height() - 44.))
            .size(vec2((screen_width() - 24.).max(1.), 32.))
            .ui(&mut ui)
        {
            self.footer_clicks += 1;
        }
        ui.pop_skin();
    }
}

fn check_measurement() {
    let mut ui = root_ui();
    for custom in [false, true] {
        let mut skin = ui.default_skin();
        let builder = ui
            .style_builder()
            .font_size(24)
            .text_color(WHITE)
            .margin(RectOffset::new(3., 7., 5., 11.))
            .background_margin(RectOffset::new(2., 4., 6., 8.));
        skin.label_style = if custom {
            builder
                .font(include_bytes!("../../examples/DancingScriptRegular.ttf"))
                .unwrap()
                .build()
        } else {
            builder.build()
        };
        ui.push_skin(&skin);
        let size = |ui: &mut macroquad::ui::Ui, text: &str, width: f32| {
            widgets::Label::new(text).wrap(width).measure(ui)
        };
        let empty = size(&mut ui, "", 100.);
        assert_eq!(empty, vec2(100., 54.));
        assert_eq!(size(&mut ui, "\r\n\n", 100.).y, empty.y + 48.);
        assert_eq!(
            size(&mut ui, "WWW", 16.1).y,
            size(&mut ui, "W\nW\nW", 200.).y,
            "each overwide glyph must occupy exactly one line, custom={custom}"
        );
        for width in [0., -1., f32::NAN, f32::INFINITY] {
            assert_eq!(size(&mut ui, "text", width), Vec2::ZERO);
        }
        assert_eq!(size(&mut ui, "text", 10.), vec2(10., 54.));
        // Invalid wrap widths promise zero allocation, including no implicit
        // gap or change to the position of the next row/column control.
        for (index, layout) in [Layout::Vertical, Layout::Horizontal]
            .into_iter()
            .enumerate()
        {
            widgets::Group::new(
                macroquad::hash!("wrapped-invalid-width", custom, index),
                vec2(300., 100.),
            )
            .position(vec2(12., 12.))
            .content_layout(LayoutSettings::new(layout).gap(vec2(9., 7.)))
            .ui(&mut ui, |ui| {
                let start = ui.canvas().cursor();
                for width in [0., -1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                    widgets::Label::new("invisible")
                        .wrap(width)
                        .size(vec2(999., 999.))
                        .ui(ui);
                    assert_eq!(ui.canvas().cursor(), start, "invalid width {width}");
                }
                widgets::Label::new("").wrap(80.).ui(ui);
                assert_ne!(
                    ui.canvas().cursor(),
                    start,
                    "valid empty text reserves a line"
                );
            });
        }
        let minimum = widgets::Label::new("wrapped")
            .size(vec2(999., 180.))
            .wrap(150.)
            .measure(&mut ui);
        assert_eq!(minimum, vec2(150., 180.));
        let legacy = widgets::Label::new("legacy\nlabel").measure(&mut ui);
        assert_eq!(
            widgets::Label::new("legacy\nlabel")
                .multiline(900.)
                .measure(&mut ui),
            legacy
        );
        let wide = size(&mut ui, DESCRIPTION, 600.);
        let narrow = size(&mut ui, DESCRIPTION, 100.);
        assert!(narrow.y > wide.y);
        // A row consumes the complete wrapped width, while measured height
        // contributes to its scroll extent. This uses the actual skin font.
        widgets::Group::new(macroquad::hash!("wrapped-row", custom), vec2(300., 70.))
            .position(vec2(12., 12.))
            .content_layout(LayoutSettings::new(Layout::Horizontal))
            .ui(&mut ui, |ui| {
                let start = ui.canvas().cursor();
                widgets::Label::new(DESCRIPTION).wrap(100.).ui(ui);
                assert!((ui.canvas().cursor().x - start.x - 100.).abs() < 0.01);
            });
        ui.pop_skin();
    }
}

/// Checks a large custom font whose origin is left of a clipped group. The
/// visible right end must survive the legacy painter's approximate pre-cull.
async fn check_clipped_ink() {
    let (width, height) = miniquad::window::screen_size();
    let target = render_target(width as u32, height as u32);
    for _ in 0..3 {
        set_camera(&Camera2D {
            render_target: Some(target.clone()),
            ..Camera2D::from_display_rect(Rect::new(0., 0., screen_width(), screen_height()))
        });
        clear_background(BLACK);
        {
            let mut ui = root_ui();
            let mut skin = ui.default_skin();
            skin.margin = 0.;
            skin.window_style = ui.style_builder().build();
            skin.group_style = ui.style_builder().color(BLANK).build();
            skin.label_style = ui
                .style_builder()
                .font_size(48)
                .text_color(WHITE)
                .font(include_bytes!("../../examples/DancingScriptRegular.ttf"))
                .unwrap()
                .build();
            ui.push_skin(&skin);
            widgets::Group::new(macroquad::hash!("wrapped-clipped-ink"), vec2(220., 90.))
                .position(vec2(80., 30.))
                .ui(&mut ui, |ui| {
                    widgets::Label::new("MMMMMMMMMMMMMMMM")
                        .wrap(800.)
                        .position(vec2(-250., 0.))
                        .ui(ui);
                });
            // Label clipping must restore the parent's clip for subsequent drawing.
            ui.canvas()
                .rect(Rect::new(12., 140., 32., 20.), None, Some(RED));
            ui.pop_skin();
        }
        next_frame().await;
    }
    let image = target.texture.get_texture_data();
    set_default_camera();
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(directory) = std::env::var("MACROQUAD_LAYOUT_CAPTURE_DIR") {
        image.export_png(&format!("{directory}/wrapped-clipped-ink.png"));
    }
    let dpi = screen_dpi_scale();
    let mut white = 0;
    for y in 0..image.height as usize {
        for x in 0..image.width as usize {
            let pixel = &image.bytes[(y * image.width as usize + x) * 4..][..4];
            if pixel[0] > 180 && pixel[1] > 180 && pixel[2] > 180 {
                let point = vec2(x as f32 / dpi, (image.height as usize - 1 - y) as f32 / dpi);
                assert!(
                    point.x >= 79. && point.x <= 301. && point.y >= 29. && point.y <= 121.,
                    "wrapped ink escaped group clip at {point:?}, dpi={dpi}"
                );
                white += 1;
            }
        }
    }
    assert!(
        white > 60,
        "visible large-font suffix must not be pre-culled, dpi={dpi}, pixels={white}"
    );
    let x = (24. * dpi) as usize;
    let y = image.height as usize - 1 - (150. * dpi) as usize;
    let pixel = &image.bytes[(y * image.width as usize + x) * 4..][..4];
    assert!(
        pixel[0] > 200 && pixel[1] < 70 && pixel[2] < 80,
        "parent clip was not restored: {pixel:?}"
    );
}

/// Compare overlapping real glyphs against independently positioned text. The
/// reference's atlas uses the UI atlas's nearest filter so its coverage can be
/// compared pixel for pixel, including the negative left bearing of this font.
async fn check_tight_spacing_ink() {
    let (width, height) = miniquad::window::screen_size();
    let target = render_target(width as u32, height as u32);
    let camera = Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., screen_width(), screen_height()))
    };
    let dpi = screen_dpi_scale();
    let position = vec2(20., 20.);
    let font_size = 48;
    let spacing = 2.;
    let lines = ["g", "É"];
    for custom in [false, true] {
        let bytes: &[u8] = if custom {
            include_bytes!("../../examples/DancingScriptRegular.ttf")
        } else {
            include_bytes!("../../src/ProggyClean.ttf")
        };
        let raw_font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).unwrap();
        let raster_size = (font_size as f32 * dpi).ceil();
        let metrics = lines.map(|line| raw_font.metrics(line.chars().next().unwrap(), raster_size));
        let baseline = metrics.iter().fold(
            raw_font
                .horizontal_line_metrics(raster_size)
                .unwrap()
                .ascent
                / dpi,
            |baseline, metrics| baseline.max((metrics.height as f32 + metrics.ymin as f32) / dpi),
        );
        let mut reference_font = load_ttf_font_from_bytes(bytes).unwrap();
        reference_font.set_filter(FilterMode::Nearest);
        let skin = {
            let ui = root_ui();
            let mut skin = ui.default_skin();
            skin.margin = 0.;
            let builder = ui.style_builder().font_size(font_size).text_color(WHITE);
            skin.label_style = if custom {
                builder.font(bytes).unwrap().build()
            } else {
                builder.build()
            };
            skin
        };
        let mut measured = Vec2::ZERO;
        for _ in 0..2 {
            set_camera(&camera);
            clear_background(BLACK);
            {
                let mut ui = root_ui();
                ui.push_skin(&skin);
                let label = widgets::Label::new("g\nÉ")
                    .wrap(200.)
                    .multiline(spacing)
                    .position(position);
                measured = label.measure(&mut ui);
                label.ui(&mut ui);
                ui.pop_skin();
            }
            next_frame().await;
        }
        let actual = target.texture.get_texture_data();
        // Check the allocation against the un-clipped raster metrics as well:
        // matching two identically short clips must not hide missing ink.
        for (index, metrics) in metrics.iter().enumerate() {
            let line_baseline = baseline + index as f32 * spacing;
            let top = line_baseline - (metrics.height as f32 + metrics.ymin as f32) / dpi;
            let bottom = line_baseline - metrics.ymin as f32 / dpi;
            assert!(top >= -0.001 && bottom <= measured.y + 0.001);
        }
        assert!(measured.y >= font_size as f32 + spacing);
        // Deferred UI clips truncate physical coordinates. Immediate clips on
        // a render target already use target pixels, independently of DPI.
        let clip = Rect::new(
            (position.x * dpi).trunc(),
            (position.y * dpi).trunc(),
            (measured.x * dpi).trunc(),
            (measured.y * dpi).trunc(),
        );
        for _ in 0..2 {
            set_camera(&camera);
            clear_background(BLACK);
            with_clip_rect(clip, || {
                for (index, line) in lines.iter().enumerate() {
                    draw_text_ex(
                        line,
                        position.x,
                        position.y + baseline + index as f32 * spacing,
                        TextParams {
                            font: Some(&reference_font),
                            font_size,
                            color: WHITE,
                            ..Default::default()
                        },
                    );
                }
            });
            next_frame().await;
        }
        let expected = target.texture.get_texture_data();
        set_default_camera();
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(directory) = std::env::var("MACROQUAD_LAYOUT_CAPTURE_DIR") {
            actual.export_png(&format!("{directory}/wrapped-tight-{custom}.png"));
            expected.export_png(&format!("{directory}/wrapped-tight-reference-{custom}.png"));
        }
        let mut visible = 0;
        let mut changed = 0;
        for (index, (actual, expected)) in actual
            .bytes
            .chunks_exact(4)
            .zip(expected.bytes.chunks_exact(4))
            .enumerate()
        {
            if actual[0] > 10 {
                visible += 1;
                let x = index % width as usize;
                let y = height as usize - 1 - index / width as usize;
                assert!(
                    x as f32 >= clip.x
                        && (x as f32) < clip.right()
                        && y as f32 >= clip.y
                        && (y as f32) < clip.bottom(),
                    "tight-spaced ink escaped allocation, custom={custom}, dpi={dpi}"
                );
            }
            // Allow one channel level for backend rounding, while comparing
            // every pixel so misplaced baselines and missing descenders fail.
            if actual
                .iter()
                .zip(expected)
                .any(|(actual, expected)| actual.abs_diff(*expected) > 1)
            {
                changed += 1;
            }
        }
        assert!(visible > 100, "tight-spaced text must be visible");
        assert_eq!(
            changed, 0,
            "tight-spaced label differs from reference, custom={custom}, dpi={dpi}"
        );
    }
}

pub async fn run() -> Menu {
    check_measurement();
    next_frame().await;
    check_clipped_ink().await;
    check_tight_spacing_ink().await;
    let mut menu = Menu::default();
    for _ in 0..5 {
        menu.draw();
        next_frame().await;
    }
    assert!(menu.scroll_max > 0.);
    let footer = (screen_width() * 0.5, screen_height() - 28.);
    root_ui().mouse_move(footer);
    root_ui().mouse_down(footer);
    menu.draw();
    root_ui().mouse_up(footer);
    next_frame().await;
    assert_eq!(menu.footer_clicks, 1);
    for _ in 0..4 {
        root_ui().mouse_move((screen_width() * 0.5, screen_height() * 0.5));
        root_ui().mouse_wheel(0., 10_000.);
        menu.draw();
        next_frame().await;
    }
    assert!(menu.last.y >= 12. && menu.last.bottom() <= screen_height() - 60. + 0.1);
    let point = (
        menu.last.x + menu.last.w * 0.5,
        menu.last.y + menu.last.h * 0.5,
    );
    root_ui().mouse_move(point);
    root_ui().mouse_down(point);
    menu.draw();
    root_ui().mouse_up(point);
    next_frame().await;
    assert_eq!(menu.clicks, 1);
    // Leave the live fixture at the top for real DOM wheel/click checks.
    root_ui().mouse_move((100., 100.));
    root_ui().mouse_wheel(0., -10_000.);
    for _ in 0..3 {
        menu.draw();
        next_frame().await;
    }
    macroquad::logging::info!("WRAPPED_LABELS_PASS");
    menu
}
