//! A small game loop composed entirely from public sprite, camera and UI APIs.
use crate::sheet;
use macroquad::camera::{render_target_to_window, window_to_render_target};
use macroquad::experimental::animation::{
    AnimatedSprite, Animation, SpriteRegion, SpriteRegionDrawParams,
};
use macroquad::prelude::*;
use macroquad::ui::{root_ui, widgets, Layout, LayoutSettings, Skin, Ui};

pub const PAUSE: u64 = 501;
pub const RESUME: u64 = 502;
pub const RESTART: u64 = 503;
pub const TARGET_SIZE: Vec2 = Vec2::new(320., 180.);
const ANCHOR: Vec2 = Vec2::new(190., 80.);
const SPRITE_SIZE: Vec2 = Vec2::new(48., 48.);
const LOG: &str = "Click the sprite to collect a pulse. The camera maps the pointer through the letterboxed target. Resize the window: its world position stays the same. Pause freezes the animation and world input. Tab and Shift+Tab move inside the menu; Enter or Space activates a button. Resume preserves the partial animation frame. Restart clears the score and rewinds while staying paused.";

pub fn config() -> Conf {
    Conf {
        window_title: "Sprites, picking and pause UI".into(),
        window_width: 960,
        window_height: 640,
        high_dpi: true,
        ..Default::default()
    }
}

pub struct Scene {
    camera: Camera2D,
    texture: Texture2D,
    skin: Skin,
    pub sprite: AnimatedSprite,
    pub paused: bool,
    pub hits: u32,
    pub restarts: u32,
    pub scroll: f32,
    // Logical window rectangles: target display, Pause, Resume, Restart, log, modal.
    pub rectangles: [Rect; 6],
}

impl Scene {
    pub fn new() -> Self {
        let target = render_target(TARGET_SIZE.x as u32, TARGET_SIZE.y as u32);
        target.texture.set_filter(FilterMode::Nearest);
        let ui = root_ui();
        let skin = Skin {
            margin: 0.,
            window_style: ui
                .style_builder()
                .color(Color::from_rgba(35, 46, 66, 255))
                .build(),
            label_style: ui.style_builder().font_size(18).text_color(WHITE).build(),
            button_style: ui
                .style_builder()
                .font_size(18)
                .color(Color::from_rgba(225, 232, 240, 255))
                .color_hovered(WHITE)
                .color_clicked(LIGHTGRAY)
                .build(),
            ..ui.default_skin()
        };
        Self {
            camera: Camera2D {
                rotation: 8.,
                render_target: Some(target),
                ..Camera2D::from_display_rect(Rect::new(0., 0., TARGET_SIZE.x, TARGET_SIZE.y))
            },
            texture: sheet::texture(),
            skin,
            sprite: AnimatedSprite::try_new(
                20,
                20,
                &[Animation {
                    name: "pulse".into(),
                    row: 0,
                    frames: 2,
                    fps: 2,
                }],
                true,
            )
            .unwrap(),
            paused: false,
            hits: 0,
            restarts: 0,
            scroll: 0.,
            rectangles: [Rect::default(); 6],
        }
    }

    fn layout(&mut self) {
        let available = vec2(
            (screen_width() - 32.).max(1.),
            (screen_height() - 220.).max(1.),
        );
        let size = TARGET_SIZE * (available.x / TARGET_SIZE.x).min(available.y / TARGET_SIZE.y);
        self.rectangles[0] = Rect::new(
            (screen_width() - size.x) * 0.5,
            94. + (available.y - size.y) * 0.5,
            size.x,
            size.y,
        );
        self.rectangles[4] = Rect::new(
            16.,
            (screen_height() - 104.).max(94.),
            (screen_width() - 32.).max(1.),
            68.,
        );
        let modal_size = vec2((screen_width() - 40.).clamp(1., 360.), 258.);
        self.rectangles[5] = Rect::new(
            (screen_width() - modal_size.x) * 0.5,
            (screen_height() - modal_size.y) * 0.5,
            modal_size.x,
            modal_size.y,
        );
    }

    #[allow(dead_code)] // Read-only diagnostic used by the native/WebGL check host.
    pub fn sprite_window_position(&self) -> Vec2 {
        render_target_to_window(
            self.camera.world_to_target(ANCHOR).unwrap(),
            TARGET_SIZE,
            self.rectangles[0],
            false,
            true,
        )
        .unwrap()
    }

    pub fn pick(&mut self, pointer: Vec2) {
        // Input/display use logical window units, the target uses physical pixels,
        // and the camera converts those pixels into world coordinates. The flip
        // matches draw_texture_ex below; it is not an extra world-axis inversion.
        let world = window_to_render_target(pointer, TARGET_SIZE, self.rectangles[0], false, true)
            .and_then(|point| self.camera.target_to_world(point));
        if !self.paused
            && world
                .is_some_and(|p| Rect::new(ANCHOR.x - 24., ANCHOR.y - 24., 48., 48.).contains(p))
        {
            self.hits += 1;
        }
    }

    fn button(ui: &mut Ui, rect: &mut Rect, id: u64, label: &str) -> bool {
        let position = ui.canvas().cursor();
        let size = vec2(ui.available_width(), 36.);
        *rect = Rect::new(position.x, position.y, size.x, size.y);
        widgets::Button::new(label)
            .id(id)
            .focus_color(YELLOW)
            .size(size)
            .ui(ui)
    }

    fn menu(&mut self) {
        let mut ui = root_ui();
        ui.push_skin(&self.skin);
        widgets::Window::new(
            510,
            vec2(16., 42.),
            vec2((screen_width() - 32.).max(1.), 44.),
        )
        .titlebar(false)
        .movable(false)
        .ui(&mut ui, |ui| {
            ui.set_layout(
                LayoutSettings::new(Layout::Horizontal)
                    .padding(RectOffset::new(8., 8., 4., 4.))
                    .gap(vec2(8., 0.)),
            );
            let label_width = (ui.available_width() - 110.).max(1.);
            widgets::Label::new(format!("Pulses: {}", self.hits))
                .size(vec2(label_width, 36.))
                .ui(ui);
            if Self::button(ui, &mut self.rectangles[1], PAUSE, "Pause") {
                self.paused = true;
            }
        });
        if self.paused {
            let rect = self.rectangles[5];
            ui.modal(511, rect.point(), rect.size(), |ui| {
                ui.set_layout(LayoutSettings::new(Layout::Vertical)
                    .padding(RectOffset::new(14., 14., 14., 14.)).gap(vec2(0., 10.)));
                widgets::Label::new("Paused").size(vec2(ui.available_width(), 26.)).ui(ui);
                let width = ui.available_width();
                widgets::Group::new(512, vec2(width, 96.)).layout(Layout::Vertical)
                    .content_layout(LayoutSettings::new(Layout::Vertical).gap(vec2(0., 2.)))
                    .ui(ui, |ui| {
                        let text = wrap_text("Tab / Shift+Tab select a button. Enter / Space activates. Resume keeps the animation fraction. Restart rewinds and clears pulses. Escape also resumes.", None, 18, 1., ui.available_width());
                        for line in text.lines() {
                            widgets::Label::new(line).size(vec2(ui.available_width(), 22.)).ui(ui);
                        }
                    });
                if Self::button(ui, &mut self.rectangles[2], RESUME, "Resume") {
                    self.paused = false;
                }
                if Self::button(ui, &mut self.rectangles[3], RESTART, "Restart (stay paused)") {
                    self.sprite.reset();
                    self.hits = 0;
                    self.restarts += 1;
                }
            });
        }
        ui.pop_skin();
    }

    pub fn frame(&mut self, delta_seconds: f64) {
        self.layout();
        let was_paused = self.paused;
        if is_key_pressed(KeyCode::Escape) {
            self.paused = !self.paused;
        }
        self.menu();
        // Suppress world input and elapsed time on both modal transition frames.
        // In particular, a Resume click must not also collect the sprite beneath it.
        let blocked = was_paused || self.paused;
        self.sprite.playing = !blocked;
        self.sprite.update_with_delta(delta_seconds).unwrap();
        if !blocked && is_mouse_button_pressed(MouseButton::Left) {
            self.pick(mouse_position_logical());
        }

        set_camera(&self.camera);
        clear_background(Color::from_rgba(31, 46, 63, 255));
        for x in (0..=320).step_by(40) {
            draw_line(
                x as f32,
                0.,
                x as f32,
                180.,
                1.,
                Color::from_rgba(48, 68, 87, 255),
            );
        }
        for y in (0..=180).step_by(40) {
            draw_line(
                0.,
                y as f32,
                320.,
                y as f32,
                1.,
                Color::from_rgba(48, 68, 87, 255),
            );
        }
        SpriteRegion::new(sheet::SHEET_SIZE, self.sprite.frame().source_rect)
            .unwrap()
            .with_pivot(sheet::PIVOT)
            .unwrap()
            .draw(
                &self.texture,
                ANCHOR,
                WHITE,
                SpriteRegionDrawParams {
                    dest_size: Some(SPRITE_SIZE),
                    ..Default::default()
                },
            )
            .unwrap();

        set_default_camera();
        clear_background(Color::from_rgba(15, 23, 35, 255));
        let display = self.rectangles[0];
        draw_texture_ex(
            &self.camera.render_target.as_ref().unwrap().texture,
            display.x,
            display.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(display.size()),
                flip_y: true,
                ..Default::default()
            },
        );
        draw_text("Click a pulse / Escape to pause", 16., 28., 23., WHITE);
        let log = self.rectangles[4];
        let wrapped = wrap_text(LOG, None, 17, 1., (log.w - 16.).max(1.));
        let max_scroll = (wrapped.lines().count() as f32 * 22. - log.h + 12.).max(0.);
        if !blocked && log.contains(mouse_position_logical()) {
            self.scroll -= mouse_wheel().1 * 24.;
        }
        self.scroll = self.scroll.clamp(0., max_scroll);
        draw_rectangle_rounded(
            log.x - 2.,
            log.y - 2.,
            log.w + 4.,
            log.h + 4.,
            6.,
            Color::from_rgba(47, 65, 85, 255),
        );
        with_clip_rect(log, || {
            draw_rectangle(
                log.x,
                log.y,
                log.w,
                log.h,
                Color::from_rgba(24, 35, 51, 255),
            );
            draw_multiline_text(
                &wrapped,
                log.x + 8.,
                log.y + 22. - self.scroll,
                17.,
                Some(22. / 17.),
                WHITE,
            );
        });
        // This footer is outside the clip; UI's deferred widgets have their own clip.
        draw_text(
            "Wheel over log / Tab for menu",
            16.,
            screen_height() - 12.,
            17.,
            YELLOW,
        );
        if self.paused {
            draw_rectangle(
                0.,
                0.,
                screen_width(),
                screen_height(),
                Color::new(0., 0., 0., 0.45),
            );
        }
    }
}
