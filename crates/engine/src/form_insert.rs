//! 양식 개체 만들기: check boxes, input boxes, combo boxes, radio buttons and
//! push buttons, inserted at the caret as characters (Hangul's default).
//!
//! Sizes, colours and property values are the ones Hangul wrote for a freshly
//! inserted object of each kind in RHWP's `samples/hwpx/form-01.hwpx`. The one
//! exception is `BorderType`: it names a border fill of the source document's
//! header, so a new object uses 0 (no border fill, also seen there) and keeps
//! `DrawFrame=1`.
use super::*;
use hwpcraft_doc::model::control::{FormObject, FormType};
use hwpcraft_doc::model::shape::{CommonObjAttr, HorzRelTo, TextWrap, VertRelTo};
use serde::Deserialize;
use std::collections::HashMap;

const MM: f64 = 7200.0 / 25.4;
const MAX_TEXT: usize = 256;
const MAX_ITEMS: usize = 64;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormSpec {
    /// check_box | edit | combo_box | radio_button | push_button
    kind: String,
    #[serde(default)]
    caption: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    checked: bool,
    #[serde(default)]
    items: Vec<String>,
    /// Radio buttons with the same group are exclusive.
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    width_mm: Option<f64>,
    #[serde(default)]
    height_mm: Option<f64>,
}

/// (type, default size in HWPUNIT, background 0x00BBGGRR, default caption)
fn defaults(kind: &str) -> Result<(FormType, (u32, u32), u32, &'static str)> {
    Ok(match kind {
        "check_box" => (FormType::CheckBox, (9921, 1984), 0x00FF_FFFF, "선택 상자"),
        "radio_button" => (
            FormType::RadioButton,
            (8504, 1984),
            0x00FF_FFFF,
            "라디오 단추",
        ),
        "push_button" => (FormType::PushButton, (7087, 1984), 0x00F0_F0F0, "명령 단추"),
        "edit" => (FormType::Edit, (7087, 1984), 0x00F0_F0F0, ""),
        "combo_box" => (FormType::ComboBox, (6058, 1450), 0x00F0_F0F0, ""),
        _ => bail!("kind는 check_box·edit·combo_box·radio_button·push_button입니다."),
    })
}

fn type_label(t: FormType) -> &'static str {
    match t {
        FormType::PushButton => "PushButton",
        FormType::CheckBox => "CheckBox",
        FormType::ComboBox => "ComboBox",
        FormType::RadioButton => "RadioButton",
        FormType::Edit => "Edit",
    }
}

fn check_text(label: &str, value: &str) -> Result<()> {
    if value.chars().count() > MAX_TEXT || value.chars().any(char::is_control) {
        bail!("{label}은(는) 제어 문자 없이 {MAX_TEXT}자 이하입니다.");
    }
    Ok(())
}

fn size(mm: Option<f64>, default: u32, label: &str) -> Result<u32> {
    match mm {
        None => Ok(default),
        Some(v) if v.is_finite() && (3.0..=200.0).contains(&v) => Ok((v * MM).round() as u32),
        Some(_) => bail!("{label}는 3–200 mm입니다."),
    }
}

/// The property bag Hangul writes for a new object of `t` (form-01.hwpx).
fn properties(
    t: FormType,
    tab_order: usize,
    group: &str,
    items: &[String],
) -> HashMap<String, String> {
    let mut p: HashMap<String, String> = [
        ("AutoSize", "0"),
        ("BorderType", "0"),
        ("CharShapeID", "0"),
        ("Command", ""),
        ("DrawFrame", "1"),
        ("Editable", "1"),
        ("FollowContext", "0"),
        ("GroupName", ""),
        ("OutMarginBottom", "0"),
        ("OutMarginLeft", "0"),
        ("OutMarginRight", "0"),
        ("OutMarginTop", "0"),
        ("PosAffectLSpacing", "0"),
        ("PosAllowOverlap", "1"),
        ("PosFlowWithText", "1"),
        ("PosHoldAnchorAndSO", "0"),
        ("PosHorzAlign", "LEFT"),
        ("PosHorzOffset", "0"),
        ("PosHorzRelTo", "COLUMN"),
        ("PosTreatAsChar", "1"),
        ("PosVertAlign", "TOP"),
        ("PosVertOffset", "0"),
        ("PosVertRelTo", "PARA"),
        ("Printable", "1"),
        ("SzHeightRelTo", "ABSOLUTE"),
        ("SzProtect", "0"),
        ("SzWidthRelTo", "ABSOLUTE"),
        ("TabStop", "1"),
        ("WordWrap", "0"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    p.insert("TabOrder".into(), tab_order.to_string());
    match t {
        FormType::CheckBox | FormType::RadioButton | FormType::PushButton => {
            p.insert("RadioGroupName".into(), group.to_owned());
            p.insert("TriState".into(), "0".into());
            let back = if t == FormType::PushButton {
                "TRANSPARENT"
            } else {
                "OPAQUE"
            };
            p.insert("BackStyle".into(), back.into());
        }
        FormType::Edit => {
            for (k, v) in [
                ("AlignText", "LEFT"),
                ("MaxLength", "2147483647"),
                ("MultiLine", "0"),
                ("Number", "0"),
                ("PasswordChar", ""),
                ("ReadOnly", "0"),
                ("ScrollBars", "NONE"),
                ("TabKeyBehavior", "NEXT_OBJECT"),
            ] {
                p.insert(k.into(), v.into());
            }
        }
        FormType::ComboBox => {
            p.insert("AutoSize".into(), "1".into());
            p.insert("EditEnable".into(), "1".into());
            p.insert("ListBoxRows".into(), "4".into());
            p.insert("ListBoxWidth".into(), "0".into());
            for (i, item) in items.iter().enumerate() {
                p.insert(format!("listItem{i}"), item.clone());
                p.insert(format!("listItemDisplay{i}"), String::new());
            }
        }
    }
    p
}

/// Every form object of the document (body and table cells).
fn all_forms(doc: &hwpcraft_doc::model::document::Document) -> Vec<&FormObject> {
    fn walk<'a>(list: &'a [Paragraph], out: &mut Vec<&'a FormObject>) {
        for c in list.iter().flat_map(|p| &p.controls) {
            match c {
                Control::Form(f) => out.push(f),
                Control::Table(t) => t.cells.iter().for_each(|cell| walk(&cell.paragraphs, out)),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for section in &doc.sections {
        walk(&section.paragraphs, &mut out);
    }
    out
}

fn paragraph_at(
    doc: &mut hwpcraft_doc::model::document::Document,
    at: Cursor,
) -> Result<&mut Paragraph> {
    let body = doc
        .sections
        .get_mut(at.section)
        .and_then(|s| s.paragraphs.get_mut(at.paragraph))
        .ok_or_else(|| anyhow!("문단을 찾지 못했습니다."))?;
    match at.cell {
        None => Ok(body),
        Some(c) => match body.controls.get_mut(c.control) {
            Some(Control::Table(t)) => t
                .cells
                .get_mut(c.index)
                .and_then(|cell| cell.paragraphs.get_mut(c.paragraph))
                .ok_or_else(|| anyhow!("셀 문단을 찾지 못했습니다.")),
            _ => bail!("표를 찾지 못했습니다."),
        },
    }
}

impl Session {
    /// Insert a form object at the caret as a character; returns its `at`.
    pub(crate) fn insert_form(&mut self, spec: FormSpec) -> Result<Value> {
        let (form_type, (dw, dh), back, default_caption) = defaults(&spec.kind)?;
        let caption = spec.caption.unwrap_or_else(|| default_caption.to_owned());
        let text = spec.text.unwrap_or_default();
        check_text("caption", &caption)?;
        check_text("text", &text)?;
        let group = spec.group.unwrap_or_default();
        check_text("group", &group)?;
        let width = size(spec.width_mm, dw, "너비")?;
        let height = size(spec.height_mm, dh, "높이")?;
        match form_type {
            FormType::ComboBox => {
                if spec.items.len() > MAX_ITEMS {
                    bail!("목록 항목은 {MAX_ITEMS}개 이하입니다.");
                }
                for item in &spec.items {
                    check_text("items", item)?;
                }
                if !text.is_empty() && !spec.items.contains(&text) {
                    bail!("콤보 상자의 text는 items 중 하나여야 합니다.");
                }
            }
            _ if !spec.items.is_empty() => bail!("items는 콤보 상자(combo_box) 전용입니다."),
            _ => {}
        }
        if spec.checked && !matches!(form_type, FormType::CheckBox | FormType::RadioButton) {
            bail!("checked는 선택 상자·라디오 단추 전용입니다.");
        }
        if !group.is_empty() && form_type != FormType::RadioButton {
            bail!("group은 라디오 단추 전용입니다.");
        }
        if !text.is_empty() && !matches!(form_type, FormType::Edit | FormType::ComboBox) {
            bail!("text는 입력 상자·콤보 상자 전용입니다(단추 글은 caption).");
        }
        if !caption.is_empty() && matches!(form_type, FormType::Edit | FormType::ComboBox) {
            bail!("입력 상자·콤보 상자에는 caption이 없습니다. text를 쓰세요.");
        }
        let existing = all_forms(self.core.document());
        let label = type_label(form_type);
        let name = match spec.name {
            Some(n) => {
                check_text("name", &n)?;
                if n.trim().is_empty() || existing.iter().any(|f| f.name == n) {
                    bail!("양식 개체 이름은 비어 있지 않고 문서 안에서 서로 달라야 합니다.");
                }
                n
            }
            None => {
                let used = existing
                    .iter()
                    .filter_map(|f| f.name.strip_prefix(label)?.parse::<usize>().ok())
                    .max()
                    .unwrap_or(0);
                format!("{label}{}", used + 1)
            }
        };
        let tab_order = existing.len() + 1;
        let form = FormObject {
            common: CommonObjAttr {
                treat_as_char: true,
                flow_with_text: true,
                vert_rel_to: VertRelTo::Para,
                horz_rel_to: HorzRelTo::Column,
                text_wrap: TextWrap::Square,
                width,
                height,
                ..Default::default()
            },
            form_type,
            name,
            caption,
            text,
            width,
            height,
            fore_color: 0,
            back_color: back,
            value: i32::from(spec.checked),
            enabled: true,
            properties: properties(form_type, tab_order, &group, &spec.items),
        };
        self.guard_protected()?;
        let mut at = None;
        self.transaction(|s| {
            s.delete_selection()?;
            // Body objects keep the caret after them behind a trailing space, like
            // pictures (`reserve_object_position`); a cell keeps the caret as is.
            let c = if s.cursor.cell.is_none() {
                s.reserve_object_position()?
            } else {
                s.cursor
            };
            let doc = s.core.document_mut();
            doc.sections[c.section].raw_stream = None;
            let para = paragraph_at(doc, c)?;
            let ci = crate::ext_format::open_control_slot(para, c.offset);
            para.align_ctrl_data_records();
            para.controls.insert(ci, Control::Form(Box::new(form)));
            para.ctrl_data_records.insert(ci, None);
            for range in &mut para.field_ranges {
                if range.control_idx >= ci {
                    range.control_idx += 1;
                }
            }
            // Extended inline control (same character code as pictures and equations).
            para.control_mask |= 1 << 11;
            para.has_para_text = true;
            para.invalidate_layout_inputs();
            s.core
                .relayout_paragraph_native(c.section, c.paragraph, c.cell.map(|t| t.control))
                .map_err(err)?;
            at = Some(json!({"section": c.section, "paragraph": c.paragraph, "control": ci, "cell": c.cell}));
            s.clear_selection();
            Ok(())
        })?;
        at.ok_or_else(|| anyhow!("양식 개체를 넣지 못했습니다."))
    }
}
