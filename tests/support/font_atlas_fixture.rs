//! Self-contained GPU regression shared by the native test and WASM example.
//! The flushed reference uses a separate atlas. Large, undrawn glyphs force
//! replacements; ordinary text must keep the texture and UVs it queued with.

use macroquad::prelude::*;

const FONT: &[u8] = include_bytes!("../../src/ProggyClean.ttf");
const COLUMN: usize = 128;
const HEIGHT: u32 = 64;

#[derive(Debug)]
pub struct Result {
    pub dpi: f32,
    pub lit_pixels: u32,
    pub checksum: u32,
}

fn font() -> Font {
    let mut font = load_ttf_font_from_bytes(FONT).unwrap();
    font.set_filter(FilterMode::Nearest);
    font
}

fn draw(font: &Font, column: usize) {
    draw_text_ex(
        "Ag",
        (column * COLUMN + 8) as f32,
        44.,
        TextParams {
            font: Some(font),
            font_size: 40,
            // ProggyClean has a half-pixel advance at this size. Doubling the
            // scale keeps glyph origins on integer pixels at DPI 1 and 2,
            // avoiding nearest-sampling ties as the atlas is repacked.
            font_scale: 2.,
            color: WHITE,
            ..Default::default()
        },
    );
}

fn read(target: &RenderTarget) -> Image {
    unsafe { get_internal_gl().flush() };
    target.texture.get_texture_data()
}

fn column(image: &Image, column: usize) -> Vec<u8> {
    image
        .bytes
        .chunks_exact(image.width as usize * 4)
        .flat_map(|row| {
            row[column * COLUMN * 4..(column + 1) * COLUMN * 4]
                .iter()
                .copied()
        })
        .collect()
}

fn expect(image: &Image, index: usize, reference: &[u8], stage: &str) {
    let actual = column(image, index);
    let differing = actual.iter().zip(reference).filter(|(a, b)| a != b).count();
    assert!(
        differing == 0,
        "{stage}, column {index}: {differing} differing bytes"
    );
}

pub async fn run() -> Result {
    // Unlike merely drawing a large logical font size, these physical cache
    // sizes force growth at both DPI 1 and DPI 2. Check the font metrics so a
    // future font edit cannot silently turn this into a no-growth test.
    let metrics = fontdue::Font::from_bytes(FONT, fontdue::FontSettings::default()).unwrap();
    let first = metrics.metrics('W', 1026.);
    let second = metrics.metrics('W', 2050.);
    assert!(first.height > 512 && first.height <= 1024, "{first:?}");
    assert!(second.height > 1024 && second.height <= 2048, "{second:?}");

    let target = render_target((COLUMN * 4) as u32, HEIGHT);
    let camera = Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., (COLUMN * 4) as f32, HEIGHT as f32))
    };
    set_camera(&camera);
    clear_background(BLACK);

    // Flush an independent, nonempty correctness reference before the font
    // under test has queued anything. Integer columns sample identical pixels
    // even on a high-DPI window; draw_text_ex still rasterizes at the real DPI.
    let reference_font = font();
    draw(&reference_font, 0);
    let reference = column(&read(&target), 0);
    let lit_pixels = reference.chunks(4).filter(|pixel| pixel[0] > 0).count() as u32;
    assert!(lit_pixels > 100, "reference glyphs were not rendered");
    let checksum = reference.iter().fold(2166136261u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(16777619)
    });

    let growing = font();
    draw(&growing, 1); // Pending geometry uses the initial 512x512 atlas.
    growing.populate_font_cache(&['W'], 1026);
    draw(&growing, 2); // Uploads the first replacement, without flushing.
    growing.populate_font_cache(&['W'], 2050);
    draw(&growing, 3); // Uploads a second replacement, still without flushing.
    drop(growing); // All three generations must survive until submission.
    let image = read(&target);
    for index in 0..4 {
        expect(
            &image,
            index,
            &reference,
            "multiple replacements and font drop before flush",
        );
    }
    next_frame().await; // Collect retired generations after their submission.

    // A surviving font must remain valid after collection. Also exercise a
    // short-lived font without growth and the explicit submit-and-trim path.
    set_camera(&camera);
    clear_background(BLACK);
    draw(&reference_font, 0);
    let short_lived = font();
    draw(&short_lived, 1);
    drop(short_lived);
    retire_unused_render_resources();
    let image = read(&target);
    expect(&image, 0, &reference, "live atlas after frame collection");
    expect(
        &image,
        1,
        &reference,
        "font drop before explicit retirement",
    );
    next_frame().await;

    // Cancelling pending draws must not leave stale references for collection
    // or later draws, even when the cancelled font had several generations.
    set_camera(&camera);
    clear_background(BLACK);
    let cancelled = font();
    draw(&cancelled, 0);
    cancelled.populate_font_cache(&['W'], 1026);
    draw(&cancelled, 1);
    drop(cancelled);
    clear_background(BLACK);
    next_frame().await;
    assert!(
        read(&target)
            .bytes
            .chunks(4)
            .all(|pixel| pixel == [0, 0, 0, 255]),
        "cancelled glyphs were submitted"
    );

    // Allocate a fresh atlas after both collection paths to exercise reused
    // backend texture slots, then compare both surviving and new fonts.
    set_camera(&camera);
    clear_background(BLACK);
    draw(&reference_font, 0);
    let fresh = font();
    draw(&fresh, 1);
    drop(fresh);
    let image = read(&target);
    expect(
        &image,
        0,
        &reference,
        "surviving font after cancellation collection",
    );
    expect(
        &image,
        1,
        &reference,
        "fresh atlas after cancellation collection",
    );
    next_frame().await;
    set_default_camera();

    Result {
        dpi: screen_dpi_scale(),
        lit_pixels,
        checksum,
    }
}
