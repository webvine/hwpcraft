//! 양식 개체 속성: what Hangul's 양식 개체 속성 dialog edits, read with
//! `form.properties.get`, changed with `form.properties` and given to `form.insert`.
//!
//! The set is what RHWP keeps through HWP and HWPX: name, caption or text, checked
//! state, enabled, printable, frame (DrawFrame), size, text and background colours,
//! the font (the character shape CharShapeID names), combo box items and direct
//! input, the radio group, and the input box's read-only, multi-line, numbers-only,
//! maximum length and password character. Left out: the border fill (BorderType
//! names a header border fill that RHWP does not draw) and text alignment (RHWP
//! writes the HWPX names into HWP, where Hangul wants numbers).
//!
//! Combo box items stay in HWPX only. An HWP file has no field for them: Hangul fills
//! a combo box at open from the document script (`Name.InsertString("…", i);` that
//! the author wrote), which RHWP reads but does not write, and HwpCraft does not
//! write executable script. So an HWP save that would lose items is refused
//! ([`Session::combo_items_blocking_save`]) unless the caller asks to drop them
//! ([`Session::save_dropping_combo_items`]). A combo box the script fills (Hangul's own
//! HWP files, or HWPX with `Scripts/sourceScripts`) keeps the script's list in HWP, as
//! a Hangul save does, and is not refused until HwpCraft sets other items for it.
use super::*;
use crate::ext_object::FormAt;
use hwpcraft_doc::model::control::{FormObject, FormType};
use hwpcraft_doc::model::style::CharShapeMods;
use serde::Deserialize;
use serde_json::Map;
use std::collections::HashMap;
use std::path::Path;

pub(crate) const MM: f64 = 7200.0 / 25.4;
const MAX_TEXT: usize = 256;
const MAX_ITEMS: usize = 64;

/// Changed properties; None leaves a property as it is.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormProps {
    pub(crate) name: Option<String>,
    /// 단추·선택 상자·라디오 단추 글.
    pub(crate) caption: Option<String>,
    /// 입력 상자·콤보 상자 내용.
    pub(crate) text: Option<String>,
    pub(crate) checked: Option<bool>,
    enabled: Option<bool>,
    printable: Option<bool>,
    /// 테두리(입체 틀) 그리기.
    frame: Option<bool>,
    pub(crate) width_mm: Option<f64>,
    pub(crate) height_mm: Option<f64>,
    /// "#rrggbb"
    fore_color: Option<String>,
    back_color: Option<String>,
    font_face: Option<String>,
    /// pt
    font_size: Option<f64>,
    pub(crate) items: Option<Vec<String>>,
    /// 콤보 상자에 직접 입력 허용.
    edit_enable: Option<bool>,
    /// Radio buttons with the same group are exclusive.
    pub(crate) group: Option<String>,
    read_only: Option<bool>,
    multi_line: Option<bool>,
    number_only: Option<bool>,
    max_length: Option<u32>,
    /// One character, or "" for plain input.
    password_char: Option<String>,
}

fn kind_name(t: FormType) -> &'static str {
    match t {
        FormType::CheckBox => "check_box",
        FormType::RadioButton => "radio_button",
        FormType::PushButton => "push_button",
        FormType::Edit => "edit",
        FormType::ComboBox => "combo_box",
    }
}

pub(crate) fn check_text(label: &str, value: &str) -> Result<()> {
    if value.chars().count() > MAX_TEXT || value.chars().any(char::is_control) {
        bail!("{label}은(는) 제어 문자 없이 {MAX_TEXT}자 이하입니다.");
    }
    Ok(())
}

fn size_mm(v: f64, label: &str) -> Result<f64> {
    if !(v.is_finite() && (3.0..=200.0).contains(&v)) {
        bail!("{label}는 3–200 mm입니다.");
    }
    Ok(v)
}

/// HWPUNIT → mm, rounded to 0.01 as reported.
fn mm_of(hu: u32) -> f64 {
    (f64::from(hu) / MM * 100.0).round() / 100.0
}

/// The size for `mm`, keeping `current` when `mm` is the size reported for it.
fn hu_of(mm: f64, current: u32) -> u32 {
    if (mm_of(current) - mm).abs() < 0.005 {
        current
    } else {
        (mm * MM).round() as u32
    }
}

/// "#rrggbb" → 0x00BBGGRR.
fn colorref(hex: &str) -> Result<u32> {
    let h = hex
        .strip_prefix('#')
        .filter(|h| h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| anyhow!("색은 #rrggbb 형식입니다: {hex}"))?;
    let v = u32::from_str_radix(h, 16)?;
    Ok(((v & 0xff) << 16) | (v & 0xff00) | ((v >> 16) & 0xff))
}

fn hex_of(c: u32) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        c & 0xff,
        (c >> 8) & 0xff,
        (c >> 16) & 0xff
    )
}

fn flag(f: &FormObject, key: &str, default: bool) -> bool {
    f.properties
        .get(key)
        .map_or(default, |v| v == "1" || v.eq_ignore_ascii_case("true"))
}

fn set_flag(f: &mut FormObject, key: &str, on: bool) {
    f.properties
        .insert(key.into(), if on { "1" } else { "0" }.into());
}

/// Combo box items as the model keeps them (`listItem0`, `listItem1`, …).
pub(crate) fn list_items(f: &FormObject) -> Vec<String> {
    (0..)
        .map_while(|i| f.properties.get(&format!("listItem{i}")).cloned())
        .collect()
}

impl FormProps {
    /// Refuse what `t` does not take or values out of range, before anything changes.
    pub(crate) fn validate(&self, t: FormType) -> Result<()> {
        let button = matches!(
            t,
            FormType::PushButton | FormType::CheckBox | FormType::RadioButton
        );
        if let Some(c) = &self.caption {
            check_text("caption", c)?;
            if !c.is_empty() && !button {
                bail!("입력 상자·콤보 상자에는 caption이 없습니다. text를 쓰세요.");
            }
        }
        if let Some(text) = &self.text {
            check_text("text", text)?;
            if !text.is_empty() && button {
                bail!("text는 입력 상자·콤보 상자 전용입니다(단추 글은 caption).");
            }
        }
        if self.checked == Some(true) && !matches!(t, FormType::CheckBox | FormType::RadioButton) {
            bail!("checked는 선택 상자·라디오 단추 전용입니다.");
        }
        if let Some(g) = &self.group {
            check_text("group", g)?;
            if !g.is_empty() && t != FormType::RadioButton {
                bail!("group은 라디오 단추 전용입니다.");
            }
        }
        if let Some(items) = &self.items {
            if !items.is_empty() && t != FormType::ComboBox {
                bail!("items는 콤보 상자(combo_box) 전용입니다.");
            }
            if items.len() > MAX_ITEMS {
                bail!("목록 항목은 {MAX_ITEMS}개 이하입니다.");
            }
            for item in items {
                check_text("items", item)?;
            }
        }
        if self.edit_enable.is_some() && t != FormType::ComboBox {
            bail!("edit_enable은 콤보 상자 전용입니다.");
        }
        let edit_only = [
            self.read_only.is_some(),
            self.multi_line.is_some(),
            self.number_only.is_some(),
            self.max_length.is_some(),
            self.password_char.is_some(),
        ];
        if edit_only.contains(&true) && t != FormType::Edit {
            bail!(
                "read_only·multi_line·number_only·max_length·password_char는 입력 상자 전용입니다."
            );
        }
        if let Some(n) = self.max_length
            && !(1..=i32::MAX as u32).contains(&n)
        {
            bail!("max_length는 1–2147483647입니다.");
        }
        if let Some(p) = &self.password_char
            && (p.chars().count() > 1 || p.chars().any(char::is_control))
        {
            bail!("password_char는 글자 하나이거나 빈 값입니다.");
        }
        if let Some(n) = &self.name {
            check_text("name", n)?;
            if n.trim().is_empty() {
                bail!("양식 개체 이름은 비어 있지 않고 문서 안에서 서로 달라야 합니다.");
            }
        }
        if let Some(v) = self.width_mm {
            size_mm(v, "너비")?;
        }
        if let Some(v) = self.height_mm {
            size_mm(v, "높이")?;
        }
        for c in [&self.fore_color, &self.back_color].into_iter().flatten() {
            colorref(c)?;
        }
        let mut font = Map::new();
        if let Some(face) = &self.font_face {
            font.insert("fontFamily".into(), json!(face));
        }
        if let Some(pt) = self.font_size {
            if !pt.is_finite() {
                bail!("font_size는 1–4096 pt입니다.");
            }
            font.insert("fontSize".into(), json!((pt * 100.0).round() as i64));
        }
        crate::formatting::validate_format(&Value::Object(font), true)
            .map_err(|e| anyhow!("글꼴: {e}"))?;
        Ok(())
    }

    fn font(&self) -> bool {
        self.font_face.is_some() || self.font_size.is_some()
    }

    /// Change `f` (already validated); `char_shape` is the shape for a new font.
    fn apply(&self, f: &mut FormObject, char_shape: Option<u32>) {
        if let Some(n) = &self.name {
            f.name = n.clone();
        }
        if let Some(c) = &self.caption {
            f.caption = c.clone();
        }
        if let Some(t) = &self.text {
            f.text = t.clone();
        }
        if let Some(c) = self.checked {
            f.value = i32::from(c);
        }
        if let Some(e) = self.enabled {
            f.enabled = e;
        }
        if let Some(p) = self.printable {
            set_flag(f, "Printable", p);
        }
        if let Some(d) = self.frame {
            set_flag(f, "DrawFrame", d);
        }
        if let Some(mm) = self.width_mm {
            f.width = hu_of(mm, f.width);
            f.common.width = f.width;
        }
        if let Some(mm) = self.height_mm {
            f.height = hu_of(mm, f.height);
            f.common.height = f.height;
        }
        if let Some(c) = &self.fore_color {
            f.fore_color = colorref(c).unwrap_or(f.fore_color);
        }
        if let Some(c) = &self.back_color {
            f.back_color = colorref(c).unwrap_or(f.back_color);
        }
        if let Some(id) = char_shape {
            f.properties.insert("CharShapeID".into(), id.to_string());
            // A form that follows the surrounding text ignores its own shape.
            set_flag(f, "FollowContext", false);
        }
        if let Some(items) = &self.items {
            f.properties.retain(|k, _| !k.starts_with("listItem"));
            for (i, item) in items.iter().enumerate() {
                f.properties.insert(format!("listItem{i}"), item.clone());
                f.properties
                    .insert(format!("listItemDisplay{i}"), String::new());
            }
        }
        if let Some(e) = self.edit_enable {
            set_flag(f, "EditEnable", e);
        }
        if let Some(g) = &self.group {
            f.properties.insert("RadioGroupName".into(), g.clone());
        }
        for (key, value) in [
            ("ReadOnly", self.read_only),
            ("MultiLine", self.multi_line),
            ("Number", self.number_only),
        ] {
            if let Some(v) = value {
                set_flag(f, key, v);
            }
        }
        if let Some(n) = self.max_length {
            f.properties.insert("MaxLength".into(), n.to_string());
        }
        if let Some(p) = &self.password_char {
            f.properties.insert("PasswordChar".into(), p.clone());
        }
    }
}

/// Body or cell paragraph of a form object (a cursor names it).
fn cursor_of(at: &FormAt) -> Cursor {
    Cursor {
        section: at.section,
        paragraph: at.paragraph,
        offset: 0,
        cell: at.cell,
    }
}

/// Where every form object of the document is: (section, paragraph, control, cell).
fn form_places(doc: &hwpcraft_doc::model::document::Document) -> Vec<FormAt> {
    let mut out = Vec::new();
    for (si, section) in doc.sections.iter().enumerate() {
        for (pi, p) in section.paragraphs.iter().enumerate() {
            for (ci, c) in p.controls.iter().enumerate() {
                match c {
                    Control::Form(_) => out.push(FormAt {
                        section: si,
                        paragraph: pi,
                        control: ci,
                        cell: None,
                    }),
                    Control::Table(t) => {
                        for (index, cell) in t.cells.iter().enumerate() {
                            for (cp, para) in cell.paragraphs.iter().enumerate() {
                                for (fi, fc) in para.controls.iter().enumerate() {
                                    if matches!(fc, Control::Form(_)) {
                                        out.push(FormAt {
                                            section: si,
                                            paragraph: pi,
                                            control: fi,
                                            cell: Some(Cell {
                                                control: ci,
                                                index,
                                                paragraph: cp,
                                            }),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

impl Session {
    pub(crate) fn form_mut(&mut self, at: &FormAt) -> Result<&mut FormObject> {
        let para = crate::form_insert::paragraph_at(self.core.document_mut(), cursor_of(at))?;
        match para.controls.get_mut(at.control) {
            Some(Control::Form(f)) => Ok(f),
            _ => bail!("양식 개체를 찾지 못했습니다."),
        }
    }

    /// The character shape for the font `props` asks for, based on `base`.
    /// Changes the document (fonts, shapes): call inside a transaction.
    fn form_char_shape(&mut self, base: u32, props: &FormProps) -> Result<u32> {
        let mut mods = CharShapeMods::default();
        if let Some(face) = &props.font_face {
            let v = self.character_parameters_from(json!({"fontFamily": face}), None)?;
            let ids: Vec<u16> = v["fontIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_u64().and_then(|v| u16::try_from(v).ok()))
                .collect();
            mods.font_ids = Some(
                ids.try_into()
                    .map_err(|_| anyhow!("글꼴을 등록하지 못했습니다."))?,
            );
        }
        if let Some(pt) = props.font_size {
            mods.base_size = Some((pt * 100.0).round() as i32);
        }
        Ok(self
            .core
            .document_mut()
            .find_or_create_char_shape(base, &mods))
    }

    /// Apply validated `props` to the form at `at`, unchecking the other radio buttons
    /// of its group when it ends up checked, and lay out what changed.
    /// Call inside a transaction.
    pub(crate) fn apply_form_props(&mut self, at: &FormAt, props: &FormProps) -> Result<()> {
        let char_shape = if props.font() {
            let base = self
                .form_at(at)?
                .properties
                .get("CharShapeID")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            Some(self.form_char_shape(base, props)?)
        } else {
            None
        };
        let f = self.form_mut(at)?;
        props.apply(f, char_shape);
        let edited =
            (props.items.is_some() && f.form_type == FormType::ComboBox).then(|| f.name.clone());
        let group = (f.form_type == FormType::RadioButton && f.value == 1)
            .then(|| f.properties.get("RadioGroupName").cloned())
            .flatten()
            .filter(|g| !g.is_empty() && (props.checked == Some(true) || props.group.is_some()));
        let mut touched = vec![(at.section, at.paragraph, at.cell)];
        if let Some(group) = group {
            for other in form_places(self.core.document()) {
                if (other.section, other.paragraph, other.control, other.cell)
                    == (at.section, at.paragraph, at.control, at.cell)
                {
                    continue;
                }
                let f = self.form_mut(&other)?;
                if f.form_type == FormType::RadioButton
                    && f.value != 0
                    && f.properties.get("RadioGroupName") == Some(&group)
                {
                    f.value = 0;
                    touched.push((other.section, other.paragraph, other.cell));
                }
            }
        }
        touched.dedup();
        if let Some(name) = edited {
            self.edited_combo_items.insert(name);
        }
        for (section, paragraph, cell) in touched {
            let c = Cursor {
                section,
                paragraph,
                offset: 0,
                cell,
            };
            crate::form_insert::paragraph_at(self.core.document_mut(), c)?
                .invalidate_layout_inputs();
            self.core
                .relayout_paragraph_native(section, paragraph, cell.map(|t| t.control))
                .map_err(err)?;
        }
        Ok(())
    }

    /// 양식 개체 속성 (form.properties.get): the keys `form.properties` takes.
    pub(crate) fn form_properties(&self, at: &FormAt) -> Result<Value> {
        let f = self.form_at(at)?;
        let doc = self.core.document();
        let shape_id: usize = f
            .properties
            .get("CharShapeID")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let shape = doc.doc_info.char_shapes.get(shape_id);
        let face = shape
            .and_then(|cs| {
                doc.doc_info
                    .font_faces
                    .first()
                    .and_then(|faces| faces.get(cs.font_ids[0] as usize))
            })
            .map(|font| font.name.clone())
            .unwrap_or_default();
        let size = shape.map_or(10.0, |cs| f64::from(cs.base_size) / 100.0);
        let mut v = json!({
            "at": {"section": at.section, "paragraph": at.paragraph, "control": at.control,
                "cell": at.cell},
            "kind": kind_name(f.form_type),
            "name": f.name,
            "enabled": f.enabled,
            "printable": flag(f, "Printable", true),
            "frame": flag(f, "DrawFrame", true),
            "width_mm": mm_of(f.width),
            "height_mm": mm_of(f.height),
            "fore_color": hex_of(f.fore_color),
            "back_color": hex_of(f.back_color),
            "font_face": face,
            "font_size": size,
        });
        match f.form_type {
            FormType::PushButton | FormType::CheckBox | FormType::RadioButton => {
                v["caption"] = json!(f.caption);
            }
            FormType::Edit | FormType::ComboBox => v["text"] = json!(f.text),
        }
        match f.form_type {
            FormType::CheckBox => v["checked"] = json!(f.value == 1),
            FormType::RadioButton => {
                v["checked"] = json!(f.value == 1);
                v["group"] = json!(
                    f.properties
                        .get("RadioGroupName")
                        .cloned()
                        .unwrap_or_default()
                );
            }
            FormType::ComboBox => {
                let mut items = list_items(f);
                if items.is_empty() && at.cell.is_none() {
                    // Hangul's HWP keeps the list in the document script.
                    let raw = self
                        .core
                        .get_form_object_info_native(at.section, at.paragraph, at.control)
                        .map_err(err)?;
                    let info: Value = serde_json::from_str(&raw)?;
                    items = info["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|i| i.as_str().map(str::to_owned))
                        .collect();
                }
                v["items"] = json!(items);
                v["edit_enable"] = json!(flag(f, "EditEnable", true));
            }
            FormType::Edit => {
                v["read_only"] = json!(flag(f, "ReadOnly", false));
                v["multi_line"] = json!(flag(f, "MultiLine", false));
                v["number_only"] = json!(flag(f, "Number", false));
                v["max_length"] = json!(
                    f.properties
                        .get("MaxLength")
                        .and_then(|v| v.parse::<u32>().ok())
                        .unwrap_or(i32::MAX as u32)
                );
                v["password_char"] = json!(
                    f.properties
                        .get("PasswordChar")
                        .cloned()
                        .unwrap_or_default()
                );
            }
            FormType::PushButton => {}
        }
        Ok(v)
    }

    /// 양식 개체 속성 고치기 (form.properties): `at` plus the keys to change, one undo step.
    pub(crate) fn set_form_properties(&mut self, params: Value) -> Result<Value> {
        let Value::Object(mut map) = params else {
            bail!("form.properties 매개 변수는 JSON 객체입니다.");
        };
        let at: FormAt = serde_json::from_value(
            map.remove("at")
                .ok_or_else(|| anyhow!("at(양식 개체 위치)이 필요합니다."))?,
        )?;
        let kind = map.remove("kind");
        let props: FormProps = serde_json::from_value(Value::Object(map))?;
        let f = self.form_at(&at)?;
        let t = f.form_type;
        if let Some(k) = kind
            && k != kind_name(t)
        {
            bail!("양식 개체 종류는 바꿀 수 없습니다({}).", kind_name(t));
        }
        props.validate(t)?;
        if let Some(n) = &props.name
            && *n != f.name
            && crate::form_insert::all_forms(self.core.document())
                .iter()
                .any(|o| o.name == *n)
        {
            bail!("양식 개체 이름은 비어 있지 않고 문서 안에서 서로 달라야 합니다.");
        }
        self.transaction(|s| s.apply_form_props(&at, &props))?;
        self.form_properties(&at)
    }

    /// form.get for an object in a table cell (RHWP's query reads body objects only):
    /// the same keys from the model.
    pub(crate) fn form_info(&self, at: &FormAt) -> Result<Value> {
        let f = self.form_at(at)?;
        Ok(json!({
            "ok": true,
            "formType": crate::ext_object::form_type_name(f.form_type),
            "name": f.name,
            "value": f.value,
            "text": f.text,
            "caption": f.caption,
            "enabled": f.enabled,
            "width": f.width,
            "height": f.height,
            "foreColor": f.fore_color,
            "backColor": f.back_color,
            "properties": f.properties,
            "items": list_items(f),
        }))
    }
}

/// The document script Hangul runs at open (HWP `Scripts/DefaultJScript`), as text.
fn document_script(doc: &hwpcraft_doc::model::document::Document) -> String {
    let Some((_, data)) = doc
        .extra_streams
        .iter()
        .find(|(path, _)| path.trim_start_matches('/') == "Scripts/DefaultJScript")
    else {
        return String::new();
    };
    let Ok(raw) = hwpcraft_doc::decompress_hwp_stream(data, 16 << 20) else {
        return String::new();
    };
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16_lossy(&units)
}

/// The items `name.InsertString("text", index);` lines give a combo box, in index
/// order: what RHWP lists for the combo box after an HWP is opened.
fn script_items(script: &str, name: &str) -> Vec<String> {
    let prefix = format!("{name}.InsertString(");
    let mut items: Vec<(usize, String)> = script
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix(&prefix)?.strip_prefix('"')?;
            let (text, after) = rest.split_once('"')?;
            let index: String = after
                .trim_start()
                .strip_prefix(',')?
                .trim()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            Some((index.parse().ok()?, text.to_owned()))
        })
        .collect();
    items.sort_by_key(|(index, _)| *index);
    items.into_iter().map(|(_, text)| text).collect()
}

fn is_hwp(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("hwp"))
}

impl Session {
    /// Combo boxes whose items an HWP file would not keep: items in the model where the
    /// document script fills none for that name, or other ones than HwpCraft set.
    fn combo_items_lost(&self, path: &Path) -> Vec<FormAt> {
        if !is_hwp(path) {
            return Vec::new();
        }
        let doc = self.core.document();
        let mut script = None;
        form_places(doc)
            .into_iter()
            .filter(|at| {
                let Ok(f) = self.form_at(at) else {
                    return false;
                };
                let items = list_items(f);
                if f.form_type != FormType::ComboBox || items.is_empty() {
                    return false;
                }
                let script = script.get_or_insert_with(|| document_script(doc));
                let given = script_items(script, &f.name);
                given.is_empty() || (self.edited_combo_items.contains(&f.name) && given != items)
            })
            .collect()
    }

    /// How many combo boxes would lose their items in a save to `path` (only .hwp can).
    /// A plain save refuses then; [`Self::save_dropping_combo_items`] saves without them.
    pub fn combo_items_blocking_save(&self, path: &Path) -> usize {
        self.combo_items_lost(path).len()
    }

    pub(crate) fn refuse_combo_item_loss(&self, path: &Path) -> Result<()> {
        let lost = self.combo_items_blocking_save(path);
        if lost > 0 {
            bail!(
                "콤보 상자 {lost}개의 목록 항목은 HWP(.hwp)에 저장할 수 없습니다(한글은 HWP에서 \
                 목록을 문서 스크립트에 둡니다). HWPX로 저장하거나, 목록을 빼고 저장하려면 \
                 dropComboItems(MCP)·--drop-combo-items(CLI)를 쓰세요."
            );
        }
        Ok(())
    }

    /// Save to `path` without the combo box items HWP cannot hold, removing them from
    /// the document too so it shows what the file has. A failed save puts them back. `accept_track_changes` merges tracked changes as
    /// `save_accepting_track_changes`. Returns how many combo boxes lost their items.
    pub fn save_dropping_combo_items(
        &mut self,
        path: &Path,
        accept_track_changes: bool,
    ) -> Result<usize> {
        let lost = self.combo_items_lost(path);
        let mut removed: Vec<(FormAt, HashMap<String, String>)> = Vec::new();
        for at in &lost {
            let f = self.form_mut(at)?;
            let items: HashMap<String, String> = f
                .properties
                .iter()
                .filter(|(k, _)| k.starts_with("listItem"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            f.properties.retain(|k, _| !k.starts_with("listItem"));
            removed.push((*at, items));
        }
        self.relayout_forms(&lost)?;
        let saved = if accept_track_changes {
            self.save_accepting_track_changes(path)
        } else {
            self.save(path)
        };
        if let Err(e) = saved {
            for (at, items) in removed {
                self.form_mut(&at)?.properties.extend(items);
            }
            self.relayout_forms(&lost)?;
            return Err(e);
        }
        Ok(lost.len())
    }

    fn relayout_forms(&mut self, places: &[FormAt]) -> Result<()> {
        for at in places {
            crate::form_insert::paragraph_at(self.core.document_mut(), cursor_of(at))?
                .invalidate_layout_inputs();
            self.core
                .relayout_paragraph_native(at.section, at.paragraph, at.cell.map(|t| t.control))
                .map_err(err)?;
        }
        if !places.is_empty() {
            self.generation += 1;
        }
        Ok(())
    }
}
