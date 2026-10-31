//! Caption text of tables, pictures and drawing objects: the words after the
//! automatic number ("그림 1 설명"). RHWP has no caption text API, so the single
//! caption paragraph is rebuilt with consistent offsets.
use super::*;
use hwpcraft_doc::model::shape::{Caption, ShapeObject};

/// The caption a shape draws: its own field for groups, charts and OLE objects
/// when present, else the drawing attributes'.
fn shape_caption(shape: &ShapeObject) -> Option<&Caption> {
    match shape {
        ShapeObject::Picture(p) => p.caption.as_ref(),
        ShapeObject::Group(g) => g.caption.as_ref(),
        ShapeObject::Chart(c) if c.caption.is_some() => c.caption.as_ref(),
        ShapeObject::Ole(o) if o.caption.is_some() => o.caption.as_ref(),
        other => other.drawing().and_then(|d| d.caption.as_ref()),
    }
}

fn shape_caption_mut(shape: &mut ShapeObject) -> Option<&mut Caption> {
    let own = match &*shape {
        ShapeObject::Chart(c) => c.caption.is_some(),
        ShapeObject::Ole(o) => o.caption.is_some(),
        _ => false,
    };
    match (own, shape) {
        (_, ShapeObject::Picture(p)) => p.caption.as_mut(),
        (_, ShapeObject::Group(g)) => g.caption.as_mut(),
        (true, ShapeObject::Chart(c)) => c.caption.as_mut(),
        (true, ShapeObject::Ole(o)) => o.caption.as_mut(),
        (_, other) => other.drawing_mut().and_then(|d| d.caption.as_mut()),
    }
}

fn control_caption(control: &Control) -> Result<Option<&Caption>> {
    Ok(match control {
        Control::Picture(p) => p.caption.as_ref(),
        Control::Shape(s) => shape_caption(s),
        Control::Table(_) => bail!("표 캡션은 table.caption_text로 고칩니다."),
        _ => bail!("캡션을 둘 수 있는 그림·개체가 아닙니다."),
    })
}

/// The single caption paragraph and the character index of its number control.
pub(crate) fn number_paragraph(cap: &Caption) -> Result<(&Paragraph, usize)> {
    let [p] = cap.paragraphs.as_slice() else {
        bail!("여러 문단 캡션은 아직 고칠 수 없습니다.");
    };
    if p.controls.len() != 1
        || !matches!(p.controls[0], Control::AutoNumber(_))
        || !p.field_ranges.is_empty()
        || p.text.chars().any(char::is_control)
    {
        bail!("번호 외 개체가 있는 캡션은 고칠 수 없습니다.");
    }
    let number = p
        .control_text_positions()
        .first()
        .copied()
        .unwrap_or(0)
        .min(p.text.chars().count());
    Ok((p, number))
}

/// Text after the caption number; readers place it before or after the space.
pub(crate) fn text_after_number(cap: &Caption) -> Result<String> {
    let (p, number) = number_paragraph(cap)?;
    let after: String = p.text.chars().skip(number).collect();
    Ok(after.trim_start_matches(' ').to_owned())
}

/// Rebuild the caption paragraph as label + number control (8 code units) +
/// " " + text, the layout Hangul's own captions have when RHWP reads them.
pub(crate) fn rewrite(cap: &mut Caption, text: &str) -> Result<()> {
    if text.chars().count() > 1000 || text.chars().any(char::is_control) {
        bail!("캡션은 제어 문자 없이 1,000자 이하로 입력하세요.");
    }
    let (p, number) = number_paragraph(cap)?;
    let before: String = p.text.chars().take(number).collect();
    let label = before.split(' ').next().unwrap_or("").to_owned();
    let prefix = format!("{label} ");
    let suffix = format!(" {text}");
    let mut offsets = Vec::new();
    let mut at = 0u32;
    for ch in prefix.chars() {
        offsets.push(at);
        at += ch.len_utf16() as u32;
    }
    offsets.push(at);
    at += 8;
    for ch in suffix.chars() {
        offsets.push(at);
        at += ch.len_utf16() as u32;
    }
    let p = cap
        .paragraphs
        .first_mut()
        .ok_or_else(|| anyhow!("캡션 문단이 없습니다."))?;
    p.text = format!("{prefix} {suffix}");
    p.char_offsets = offsets;
    // Characters, the number control and the paragraph end.
    p.char_count = at + 1;
    Ok(())
}

impl Session {
    /// Text after the automatic number of a picture's or drawing object's caption.
    pub fn object_caption_text(&self, a: ObjectAddress) -> Result<String> {
        let cap = control_caption(self.object(a)?)?.ok_or_else(|| {
            anyhow!("캡션이 없는 개체입니다. object.properties hasCaption으로 먼저 넣으세요.")
        })?;
        text_after_number(cap)
    }

    /// Replace the words after the caption number as one undo step.
    pub fn set_object_caption_text(&mut self, a: ObjectAddress, text: &str) -> Result<()> {
        // Validate everything before the transaction so a refusal changes nothing.
        let mut probe = control_caption(self.object(a)?)?
            .ok_or_else(|| {
                anyhow!("캡션이 없는 개체입니다. object.properties hasCaption으로 먼저 넣으세요.")
            })?
            .clone();
        rewrite(&mut probe, text)?;
        self.guard_protected()?;
        self.transaction(|s| {
            let section = &mut s.core.document_mut().sections[a.section];
            section.raw_stream = None;
            let cap = section
                .paragraphs
                .get_mut(a.paragraph)
                .and_then(|p| p.controls.get_mut(a.control))
                .and_then(|c| match c {
                    Control::Picture(p) => p.caption.as_mut(),
                    Control::Shape(sh) => shape_caption_mut(sh),
                    _ => None,
                })
                .ok_or_else(|| anyhow!("캡션을 찾지 못했습니다."))?;
            *cap = probe;
            s.core
                .relayout_paragraph_native(a.section, a.paragraph, None)
                .map_err(err)?;
            Ok(())
        })
    }
}
