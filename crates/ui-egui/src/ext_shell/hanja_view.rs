//! 보기 → 한자 발음 표시: on screen only, the Hangul reading (본음) of each hanja is drawn
//! small above it. The page SVG draws one `<text x y font-size>` per character; the
//! readings of single-hanja texts are added at the end of the page (outside the body's
//! clip, so the first line's readings show), centred on each hanja's advance. Texts under
//! a transform (rotated or vertical writing) are left alone. Printing and PDF are unchanged.
use std::fmt::Write as _;

/// Reading size and baseline, in parts of the hanja's font size.
const SIZE: f32 = 0.5;
const RISE: f32 = 0.95;

/// `svg` with the reading of each single-hanja `<text>` drawn above it.
pub(crate) fn hanja_readings_svg(svg: &str) -> String {
    let mut readings = String::new();
    // Open groups: whether each has a transform.
    let mut groups: Vec<bool> = Vec::new();
    let mut at = 0;
    while let Some(i) = svg[at..].find('<') {
        let start = at + i;
        let Some(end) = svg[start..].find('>').map(|j| start + j) else {
            break;
        };
        let tag = &svg[start..=end];
        at = end + 1;
        if tag.starts_with("<g ") || tag == "<g>" {
            groups.push(tag.contains("transform"));
        } else if tag.starts_with("</g") {
            groups.pop();
        } else if tag.starts_with("<text ") && !tag.ends_with("/>") {
            let Some(close) = svg[at..].find("</text>").map(|j| at + j) else {
                break;
            };
            let body = &svg[at..close];
            at = close + "</text>".len();
            if groups.iter().any(|t| *t) || tag.contains("transform") {
                continue;
            }
            if let Some(reading) = reading(body)
                && let (Some(x), Some(y), Some(size)) =
                    (number(tag, "x"), number(tag, "y"), number(tag, "font-size"))
            {
                let width = number(tag, "textLength").unwrap_or(size);
                let fill = text(tag, "fill").unwrap_or("#000000");
                let _ = write!(
                    readings,
                    r#"<text x="{}" y="{}" font-size="{}" text-anchor="middle" fill="{fill}" font-family="sans-serif">{reading}</text>"#,
                    x + width / 2.0,
                    y - size * RISE,
                    size * SIZE,
                );
            }
        }
    }
    match svg.rfind("</svg>") {
        Some(end) if !readings.is_empty() => format!("{}{readings}{}", &svg[..end], &svg[end..]),
        _ => svg.to_owned(),
    }
}

/// The reading of a text holding exactly one hanja.
fn reading(body: &str) -> Option<char> {
    let mut chars = body.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let hanja = matches!(u32::from(c), 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F);
    if !hanja {
        return None;
    }
    hwpcraft_engine::primary_reading(c)
}

fn text<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    tag[start..].split('"').next()
}

fn number(tag: &str, name: &str) -> Option<f32> {
    text(tag, name)?
        .parse()
        .ok()
        .filter(|v: &f32| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_goes_above_each_hanja_only() {
        let svg = concat!(
            r#"<svg><g clip-path="url(#body)">"#,
            r##"<text x="10" y="50" font-size="20" fill="#112233">韓</text>"##,
            r##"<text x="30" y="50" font-size="20" fill="#000000">한</text>"##,
            r##"<text x="50" y="50" font-size="20" fill="#000000">國語</text>"##,
            r#"<text x="90" y="50" font-size="20" textLength="10">國</text></g>"#,
            r#"<g transform="rotate(90)"><text x="70" y="50" font-size="20">國</text></g>"#,
            "</svg>",
        );
        let out = hanja_readings_svg(svg);
        let added = concat!(
            r##"<text x="20" y="31" font-size="10" text-anchor="middle" fill="#112233" font-family="sans-serif">한</text>"##,
            r##"<text x="95" y="31" font-size="10" text-anchor="middle" fill="#000000" font-family="sans-serif">국</text>"##,
        );
        // Outside the clipped group, after everything else; rotated texts and runs of
        // several characters get none.
        assert_eq!(out, svg.replace("</svg>", &format!("{added}</svg>")));
    }

    #[test]
    fn a_page_without_hanja_is_unchanged() {
        let svg = r#"<svg><text x="1" y="2" font-size="3">가</text><text x="1" y="2"/></svg>"#;
        assert_eq!(hanja_readings_svg(svg), svg);
    }
}
