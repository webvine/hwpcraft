//! Unit and colour conversions between HWP and WordCraft (points, RGB).
use wordcraft_doc::Rgb;

/// HWPUNIT (1/7200 inch) to points.
pub fn hu_pt(hu: i64) -> f32 {
    hu as f32 / 100.0
}

/// Paragraph-shape lengths are stored at twice HWPUNIT.
pub fn para_pt(value: i32) -> f32 {
    value as f32 / 200.0
}

/// HWP COLORREF (0x00BBGGRR) as RGB; `None` when it is "no colour".
pub fn rgb(color: u32) -> Option<Rgb> {
    (color & 0xFF00_0000 == 0).then_some(Rgb(
        (color & 0xFF) as u8,
        ((color >> 8) & 0xFF) as u8,
        ((color >> 16) & 0xFF) as u8,
    ))
}

/// `#RRGGBB` for HWPX.
pub fn hex(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2)
}

/// HWP border widths (index 0–15) in millimetres.
pub const BORDER_MM: [f32; 16] = [
    0.1, 0.12, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0,
];

/// HWP border width index to points.
pub fn border_pt(width: u8) -> f32 {
    BORDER_MM[usize::from(width).min(BORDER_MM.len() - 1)] * 72.0 / 25.4
}

/// Points to the nearest HWP border width index.
pub fn border_index(pt: f32) -> usize {
    let mm = pt * 25.4 / 72.0;
    let mut best = 0;
    for (i, w) in BORDER_MM.iter().enumerate() {
        if (w - mm).abs() < (BORDER_MM[best] - mm).abs() {
            best = i;
        }
    }
    best
}

/// HWP language slot of a character (0 Hangul, 1 Latin, 2 Hanja, 3 Japanese, 4 other,
/// 5 symbol, 6 user), or `None` for spaces and punctuation, which follow their neighbour.
/// The ranges follow RHWP's `detect_lang_category`.
pub fn lang_slot(c: char) -> Option<usize> {
    let cp = c as u32;
    Some(match cp {
        0x318D => 6,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF | 0xA960..=0xA97F | 0xD7B0..=0xD7FF => {
            0
        }
        0x0041..=0x005A | 0x0061..=0x007A | 0x0030..=0x0039 | 0x00C0..=0x024F | 0x1E00..=0x1EFF => {
            1
        }
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x20000..=0x2A6DF => 2,
        0x3040..=0x309F | 0x30A0..=0x30FF | 0x31F0..=0x31FF => 3,
        0x2190..=0x23FF | 0x2500..=0x27BF | 0x2460..=0x24FF | 0x3000..=0x303F => 5,
        0x0370..=0x03FF | 0x0400..=0x04FF | 0x0590..=0x06FF | 0x0E00..=0x0E7F => 4,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_bgr() {
        assert_eq!(rgb(0x0056_3412), Some(Rgb(0x12, 0x34, 0x56)));
        assert_eq!(rgb(0xFFFF_FFFF), None);
    }

    #[test]
    fn border_widths_round_trip() {
        for i in 0..16u8 {
            assert_eq!(border_index(border_pt(i)), usize::from(i));
        }
    }
}
