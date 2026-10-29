//! Lossless PNG for `render_page`.
//!
//! The image crate's plain `PngEncoder::new` uses its fastest deflate level.
//! Rendered pages are mostly flat colour, so the default level is about
//! 2.5 times smaller for a few extra milliseconds. Fully opaque pages drop the
//! alpha channel; any transparent pixel keeps RGBA, so pixels never change.
use anyhow::{Result, ensure};
use image::{
    ExtendedColorType, ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};

pub(crate) fn encode(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    ensure!(
        rgba.len() == width as usize * height as usize * 4,
        "render buffer does not match {width}x{height} RGBA"
    );
    let mut png = Vec::new();
    let encoder =
        PngEncoder::new_with_quality(&mut png, CompressionType::Default, FilterType::Adaptive);
    let (pixels, _) = rgba.as_chunks::<4>();
    if pixels.iter().all(|[_, _, _, alpha]| *alpha == u8::MAX) {
        let rgb: Vec<u8> = pixels.iter().flat_map(|&[r, g, b, _]| [r, g, b]).collect();
        encoder.write_image(&rgb, width, height, ExtendedColorType::Rgb8)?;
    } else {
        encoder.write_image(rgba, width, height, ExtendedColorType::Rgba8)?;
    }
    Ok(png)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::encode;

    fn page(width: u32, height: u32, alpha: u8) -> Vec<u8> {
        let mut rgba = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let ink = (x / 7 + y / 5) % 3 == 0;
                let value = if ink { 30 } else { 255 };
                rgba.extend_from_slice(&[value, value, if ink { 90 } else { 255 }, u8::MAX]);
            }
        }
        rgba[3] = alpha;
        rgba
    }

    #[test]
    fn opaque_page_round_trips_as_rgb() {
        let rgba = page(120, 80, u8::MAX);
        let png = encode(&rgba, 120, 80).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!(decoded.color(), image::ColorType::Rgb8);
        assert_eq!(decoded.to_rgba8().into_raw(), rgba);
    }

    #[test]
    fn transparent_pixel_keeps_alpha() {
        let rgba = page(64, 48, 0);
        let png = encode(&rgba, 64, 48).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!(decoded.color(), image::ColorType::Rgba8);
        assert_eq!(decoded.to_rgba8().into_raw(), rgba);
    }

    #[test]
    fn smaller_than_fast_level() {
        use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
        let rgba = page(400, 560, u8::MAX);
        let mut fast = Vec::new();
        PngEncoder::new(&mut fast)
            .write_image(&rgba, 400, 560, ExtendedColorType::Rgba8)
            .unwrap();
        assert!(encode(&rgba, 400, 560).unwrap().len() < fast.len());
    }

    #[test]
    fn rejects_mismatched_buffer() {
        assert!(encode(&[0; 7], 1, 2).is_err());
    }
}
