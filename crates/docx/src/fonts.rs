//! Installed font metrics: how tall Word's single line is for a face.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn database() -> &'static fontdb::Database {
    static DB: OnceLock<fontdb::Database> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db
    })
}

/// The single line height of installed face `family` as a share of the font size, as
/// Word lays it out: 1.3 times the Windows ascent and descent for East Asian faces
/// (바탕 1.3), the horizontal header's ascent, descent and line gap for others (Arial
/// 1.15). `None` when the face is not installed.
pub fn line_height(family: &str) -> Option<f32> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<f32>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(map) = cache.lock()
        && let Some(found) = map.get(family)
    {
        return *found;
    }
    let db = database();
    let found = db
        .query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            ..Default::default()
        })
        .and_then(|id| {
            db.with_face_data(id, |data, index| {
                let face = ttf_parser::Face::parse(data, index).ok()?;
                let upem = f32::from(face.units_per_em());
                if upem <= 0.0 {
                    return None;
                }
                let hhea = f32::from(face.ascender()) - f32::from(face.descender())
                    + f32::from(face.line_gap());
                let win = face.tables().os2.map(|os2| {
                    f32::from(os2.windows_ascender()).abs()
                        + f32::from(os2.windows_descender()).abs()
                });
                // East Asian faces get 1.3 times their Windows ascent and descent.
                let east_asian = face.glyph_index('\u{AC00}').is_some()
                    || face.glyph_index('\u{4E00}').is_some();
                let height = match (east_asian, win.filter(|w| *w > 0.0)) {
                    (true, Some(win)) => win * 1.3,
                    _ => Some(hhea)
                        .filter(|h| *h > 0.0)
                        .or(win)
                        .unwrap_or(upem * 1.2),
                };
                Some(height / upem)
            })
            .flatten()
        });
    if let Ok(mut map) = cache.lock() {
        map.insert(family.to_owned(), found);
    }
    found
}

/// Whether installed face `family` has Hangul syllables; `None` when it is not installed.
pub fn covers_hangul(family: &str) -> Option<bool> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<bool>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(map) = cache.lock()
        && let Some(found) = map.get(family)
    {
        return *found;
    }
    let db = database();
    let found = db
        .query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            ..Default::default()
        })
        .and_then(|id| {
            db.with_face_data(id, |data, index| {
                let face = ttf_parser::Face::parse(data, index).ok()?;
                Some(
                    face.glyph_index('\u{AC00}').is_some()
                        && face.glyph_index('\u{D55C}').is_some(),
                )
            })
            .flatten()
        });
    if let Ok(mut map) = cache.lock() {
        map.insert(family.to_owned(), found);
    }
    found
}

/// The face Word draws Hangul with for East Asian font `family`: itself when it has
/// Hangul (or is not installed here), else Word's Korean fallback, 바탕.
pub fn hangul_face(family: &str) -> String {
    match covers_hangul(family) {
        Some(false) if covers_hangul("바탕") == Some(true) => "바탕".to_owned(),
        _ => family.to_owned(),
    }
}
