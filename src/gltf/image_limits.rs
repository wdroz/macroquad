/// Optional per-image limits checked from source metadata before decoding.
///
/// Both limits default to [`None`], preserving the uncapped loaders. The byte
/// estimate is `width * height * 4` for one 8-bit RGBA image, before any upload
/// resizing. It is not a bound on decoder scratch memory, higher-bit-depth
/// intermediate buffers, conversion copies, encoded data, or all scene images
/// combined. Prepare oversized assets offline; `max_texture_size` only limits
/// the upload after decoding.
///
/// With either limit enabled, PNG, JPEG, and WebP dimensions are read without
/// decoding pixels. WebP canvas and embedded frame dimensions are all checked.
/// Invalid or unsupported metadata is rejected, as are RGBA estimates that
/// overflow or cannot fit in a single allocation on the current platform.
///
/// ```
/// use macroquad::gltf::GltfImageLimits;
///
/// let limits = GltfImageLimits {
///     max_source_pixels: Some(4096 * 4096),
///     max_decoded_bytes: Some(64 * 1024 * 1024),
/// };
/// assert_eq!(limits.max_decoded_bytes, Some(67_108_864));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GltfImageLimits {
    /// Maximum source width times height for each image. Zero rejects any image.
    pub max_source_pixels: Option<u64>,
    /// Maximum estimated 8-bit RGBA bytes for each source image, before resizing.
    /// Zero rejects any image. Decoder and conversion working memory is extra.
    pub max_decoded_bytes: Option<u64>,
}

pub(crate) fn check_image(data: &[u8], limits: GltfImageLimits) -> Result<(), String> {
    if limits.max_source_pixels.is_none() && limits.max_decoded_bytes.is_none() {
        return Ok(());
    }
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        if data.get(8..16) != Some(&b"\0\0\0\rIHDR"[..]) {
            return Err("invalid PNG IHDR metadata".into());
        }
        let dimensions = data.get(16..24).ok_or("truncated PNG dimensions")?;
        check_dimensions(
            u32::from_be_bytes(dimensions[..4].try_into().unwrap()),
            u32::from_be_bytes(dimensions[4..].try_into().unwrap()),
            limits,
            isize::MAX as u64,
        )
    } else if data.starts_with(b"\xff\xd8") {
        check_jpeg(data, limits)
    } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(&b"WEBP"[..]) {
        check_webp(data, limits)
    } else {
        Err("unsupported image metadata; image limits support PNG, JPEG, and WebP".into())
    }
}

fn check_dimensions(
    width: u32,
    height: u32,
    limits: GltfImageLimits,
    allocation_max: u64,
) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err(format!("invalid image dimensions {width}x{height}"));
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or("source pixel count overflow")?;
    if let Some(maximum) = limits.max_source_pixels {
        if pixels > maximum {
            return Err(format!(
                "source image {width}x{height} has {pixels} pixels, exceeding max_source_pixels {maximum}"
            ));
        }
    }
    let bytes = pixels.checked_mul(4).ok_or_else(|| {
        format!("estimated RGBA decoded-byte count overflows for {width}x{height}")
    })?;
    if let Some(maximum) = limits.max_decoded_bytes {
        if bytes > maximum {
            return Err(format!(
                "source image {width}x{height} needs an estimated {bytes} RGBA decoded bytes, exceeding max_decoded_bytes {maximum}"
            ));
        }
    }
    if bytes > allocation_max {
        return Err(format!(
            "estimated RGBA decoded-byte count {bytes} for {width}x{height} exceeds this platform's allocation limit {allocation_max}"
        ));
    }
    Ok(())
}

fn check_jpeg(data: &[u8], limits: GltfImageLimits) -> Result<(), String> {
    let mut rest = &data[2..];
    while !rest.is_empty() {
        if rest[0] != 0xff {
            return Err("invalid JPEG marker before dimensions".into());
        }
        while rest.first() == Some(&0xff) {
            rest = &rest[1..];
        }
        let (&marker, after_marker) = rest.split_first().ok_or("truncated JPEG marker")?;
        rest = after_marker;
        if matches!(marker, 0xda | 0xd9) {
            break;
        }
        if matches!(marker, 0x01 | 0xd0..=0xd8) {
            continue;
        }
        let length = rest.get(..2).ok_or("truncated JPEG segment length")?;
        let length = usize::from(u16::from_be_bytes(length.try_into().unwrap()));
        if length < 2 {
            return Err("invalid JPEG segment length".into());
        }
        let segment = rest.get(2..length).ok_or("truncated JPEG segment")?;
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let dimensions = segment.get(1..5).ok_or("truncated JPEG dimensions")?;
            return check_dimensions(
                u32::from(u16::from_be_bytes(dimensions[2..4].try_into().unwrap())),
                u32::from(u16::from_be_bytes(dimensions[..2].try_into().unwrap())),
                limits,
                isize::MAX as u64,
            );
        }
        rest = &rest[length..];
    }
    Err("JPEG has no frame dimensions".into())
}

// image 0.24's WebPDecoder::new decodes the image, even when its caller only
// asks for dimensions. Inspect container and frame headers directly instead.
fn check_webp(data: &[u8], limits: GltfImageLimits) -> Result<(), String> {
    let length = u64::from(u32::from_le_bytes(data[4..8].try_into().unwrap())) + 8;
    if length != data.len() as u64 {
        return Err("WebP RIFF length does not match the image payload".into());
    }
    let mut chunks = &data[12..];
    let (kind, payload) = next_webp_chunk(&mut chunks)?;
    match kind {
        b"VP8 " | b"VP8L" => check_webp_frame(kind, payload, limits, None),
        b"VP8X" => {
            let header = payload.get(..10).ok_or("truncated WebP VP8X dimensions")?;
            let canvas = (read_u24(&header[4..7]) + 1, read_u24(&header[7..10]) + 1);
            check_dimensions(canvas.0, canvas.1, limits, isize::MAX as u64)?;
            let mut found_frame = false;
            while !chunks.is_empty() {
                let (kind, payload) = next_webp_chunk(&mut chunks)?;
                match kind {
                    b"VP8 " | b"VP8L" => {
                        check_webp_frame(kind, payload, limits, Some(canvas))?;
                        found_frame = true;
                    }
                    b"ANMF" => {
                        let header = payload.get(..16).ok_or("truncated WebP ANMF dimensions")?;
                        let frame = (read_u24(&header[6..9]) + 1, read_u24(&header[9..12]) + 1);
                        check_dimensions(frame.0, frame.1, limits, isize::MAX as u64)?;
                        if read_u24(&header[..3]) * 2 + frame.0 > canvas.0
                            || read_u24(&header[3..6]) * 2 + frame.1 > canvas.1
                        {
                            return Err("WebP animation frame lies outside the canvas".into());
                        }
                        let mut frames = &payload[16..];
                        let mut found_nested_frame = false;
                        while !frames.is_empty() {
                            let (kind, payload) = next_webp_chunk(&mut frames)?;
                            match kind {
                                b"VP8 " | b"VP8L" => {
                                    check_webp_frame(kind, payload, limits, Some(frame))?;
                                    found_nested_frame = true;
                                }
                                b"ALPH" => {}
                                _ => return Err("unsupported WebP animation frame chunk".into()),
                            }
                        }
                        if !found_nested_frame {
                            return Err("WebP animation frame has no image dimensions".into());
                        }
                        found_frame = true;
                    }
                    b"ALPH" | b"ANIM" | b"ICCP" | b"EXIF" | b"XMP " => {}
                    // Unknown RIFF metadata has no image dimensions. The
                    // decoder remains responsible for its format validity.
                    _ => {}
                }
            }
            if found_frame {
                Ok(())
            } else {
                Err("WebP has no image frame dimensions".into())
            }
        }
        _ => Err("unsupported WebP image chunk".into()),
    }
}

fn next_webp_chunk<'a>(chunks: &mut &'a [u8]) -> Result<(&'a [u8], &'a [u8]), String> {
    let header = chunks.get(..8).ok_or("truncated WebP chunk header")?;
    let length = u32::from_le_bytes(header[4..8].try_into().unwrap());
    // u64 handles a u32::MAX chunk length plus its padding on 32-bit targets.
    let padded_end = 8 + u64::from(length) + u64::from(length & 1);
    if padded_end > chunks.len() as u64 {
        return Err("truncated WebP chunk payload".into());
    }
    let payload = &chunks[8..8 + length as usize];
    *chunks = &chunks[padded_end as usize..];
    Ok((&header[..4], payload))
}

fn check_webp_frame(
    kind: &[u8],
    payload: &[u8],
    limits: GltfImageLimits,
    expected: Option<(u32, u32)>,
) -> Result<(), String> {
    let (width, height) = if kind == b"VP8L" {
        let header = payload.get(..5).ok_or("truncated WebP VP8L dimensions")?;
        if header[0] != 0x2f {
            return Err("invalid WebP VP8L signature".into());
        }
        let bits = u32::from_le_bytes(header[1..5].try_into().unwrap());
        ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
    } else {
        let header = payload.get(..10).ok_or("truncated WebP VP8 dimensions")?;
        if header[0] & 1 != 0 || &header[3..6] != b"\x9d\x01\x2a" {
            return Err("invalid WebP VP8 keyframe header".into());
        }
        (
            u32::from(u16::from_le_bytes(header[6..8].try_into().unwrap()) & 0x3fff),
            u32::from(u16::from_le_bytes(header[8..10].try_into().unwrap()) & 0x3fff),
        )
    };
    check_dimensions(width, height, limits, isize::MAX as u64)?;
    if expected.is_some_and(|dimensions| dimensions != (width, height)) {
        return Err("WebP codec dimensions do not match the container frame".into());
    }
    Ok(())
}

fn read_u24(bytes: &[u8]) -> u32 {
    u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    fn limits() -> GltfImageLimits {
        GltfImageLimits {
            max_source_pixels: Some(16),
            max_decoded_bytes: Some(64),
        }
    }

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes
    }

    fn chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = kind.to_vec();
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
        bytes.resize(bytes.len().next_multiple_of(2), 0);
        bytes
    }

    fn webp(chunks: &[u8]) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend((4 + chunks.len() as u32).to_le_bytes());
        bytes.extend(b"WEBP");
        bytes.extend(chunks);
        bytes
    }

    fn lossless_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x2f];
        bytes.extend(((width - 1) | ((height - 1) << 14)).to_le_bytes());
        bytes
    }

    fn extended_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0; 4];
        bytes.extend(&(width - 1).to_le_bytes()[..3]);
        bytes.extend(&(height - 1).to_le_bytes()[..3]);
        bytes
    }

    #[test]
    fn rejects_header_only_large_images_before_attempting_pixel_decode() {
        let png = png_header(100_000, 100_000);
        let jpeg = b"\xff\xd8\xff\xc0\x00\x08\x08\xff\xff\xff\xff\x01";
        let vp8l = webp(&chunk(b"VP8L", &lossless_header(16_384, 16_384)));
        let mut vp8 = b"\0\0\0\x9d\x01\x2a".to_vec();
        vp8.extend(16_383u16.to_le_bytes());
        vp8.extend(16_383u16.to_le_bytes());
        let vp8 = webp(&chunk(b"VP8 ", &vp8));
        let vp8x = webp(&chunk(b"VP8X", &extended_header(1 << 24, 1 << 24)));
        for bytes in [&png[..], &jpeg[..], &vp8l[..], &vp8[..], &vp8x[..]] {
            assert!(
                check_image(bytes, limits())
                    .unwrap_err()
                    .contains("max_source_pixels"),
                "a metadata limit must reject before a missing pixel stream"
            );
        }
    }

    #[test]
    fn decoded_byte_limit_checks_original_rgba_size_and_inclusive_boundary() {
        let bytes = png_header(4, 4);
        assert!(check_image(&bytes, limits()).is_ok());
        let only_bytes = GltfImageLimits {
            max_source_pixels: None,
            max_decoded_bytes: Some(63),
        };
        assert!(check_image(&bytes, only_bytes)
            .unwrap_err()
            .contains("max_decoded_bytes 63"));
        assert!(check_image(&png_header(0, 4), limits()).is_err());
        assert!(check_image(
            &png_header(1, 1),
            GltfImageLimits {
                max_source_pixels: Some(0),
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn catches_rgba_u64_overflow_and_32_bit_allocation_overflow() {
        let unbounded = GltfImageLimits::default();
        assert!(check_dimensions(u32::MAX, u32::MAX, unbounded, u64::MAX)
            .unwrap_err()
            .contains("overflows"));
        assert!(check_dimensions(32_768, 32_768, unbounded, i32::MAX as u64)
            .unwrap_err()
            .contains("platform's allocation limit"));
        assert!(check_dimensions(32_767, 16_384, unbounded, i32::MAX as u64).is_ok());
    }

    #[test]
    fn extended_webp_checks_codec_dimensions_beyond_the_canvas() {
        let mut chunks = chunk(b"VP8X", &extended_header(1, 1));
        chunks.extend(chunk(b"VP8L", &lossless_header(16_384, 16_384)));
        assert!(check_image(&webp(&chunks), limits())
            .unwrap_err()
            .contains("max_source_pixels"));

        let mut frame = vec![0; 16]; // Declares a 1x1 animation frame.
        frame.extend(chunk(b"VP8L", &lossless_header(16_384, 16_384)));
        let mut chunks = chunk(b"VP8X", &extended_header(1, 1));
        chunks.extend(chunk(b"ANMF", &frame));
        assert!(check_image(&webp(&chunks), limits())
            .unwrap_err()
            .contains("max_source_pixels"));
    }

    #[test]
    fn validates_real_small_png_jpeg_and_webp_without_changing_pixels() {
        let rgba = [10, 20, 30, 255, 90, 80, 70, 255];
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&rgba, 2, 1, image::ColorType::Rgba8)
            .unwrap();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode(&rgba, 2, 1, image::ColorType::Rgba8)
            .unwrap();
        let mut webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut webp)
            .encode(&rgba, 2, 1, image::ColorType::Rgba8)
            .unwrap();
        for bytes in [&png, &jpeg, &webp] {
            let before = image::load_from_memory(bytes).unwrap().into_rgba8();
            check_image(bytes, limits()).unwrap();
            let after = image::load_from_memory(bytes).unwrap().into_rgba8();
            assert_eq!(before, after);
            assert_eq!(after.dimensions(), (2, 1));
        }
    }

    #[test]
    fn accepts_valid_small_extended_and_animated_webp() {
        let rgba = [10, 20, 30, 255, 90, 80, 70, 255];
        let mut encoded = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
            .encode(&rgba, 2, 1, image::ColorType::Rgba8)
            .unwrap();
        let mut chunks = chunk(b"VP8X", &extended_header(2, 1));
        chunks.extend(&encoded[12..]);
        let static_image = webp(&chunks);

        let mut canvas = extended_header(2, 1);
        canvas[0] = 2; // Animation flag.
        let mut chunks = chunk(b"VP8X", &canvas);
        chunks.extend(chunk(b"ANIM", &[0; 6]));
        let mut frame = vec![0; 16];
        frame[6] = 1; // Frame width minus one.
        frame[12] = 10; // Duration in milliseconds.
        frame.extend(&encoded[12..]);
        chunks.extend(chunk(b"ANMF", &frame));
        chunks.extend(chunk(b"ANMF", &frame));
        let animated_image = webp(&chunks);

        for bytes in [&static_image, &animated_image] {
            check_image(bytes, limits()).unwrap();
            let decoded = image::load_from_memory(bytes).unwrap().into_rgba8();
            assert_eq!(decoded.dimensions(), (2, 1));
            assert_eq!(decoded.as_raw(), &rgba);
        }
    }

    #[test]
    fn malformed_metadata_is_rejected_and_uncapped_check_is_a_noop() {
        for bytes in [
            b"bad".as_slice(),
            b"\xff\xd8\xff",
            b"\xff\xd8\xff\xe0\0\x01",
        ] {
            assert!(check_image(bytes, limits()).is_err());
            assert!(check_image(bytes, GltfImageLimits::default()).is_ok());
        }
        let mut truncated_chunk = b"VP8L".to_vec();
        truncated_chunk.extend(u32::MAX.to_le_bytes());
        assert!(check_image(&webp(&truncated_chunk), limits())
            .unwrap_err()
            .contains("truncated WebP chunk payload"));
        let bytes = webp(&chunk(b"VP8L", &lossless_header(1, 1)));
        for length in 0..bytes.len() {
            assert!(check_image(&bytes[..length], limits()).is_err());
        }
    }
}
