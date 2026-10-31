//! 바탕쪽 개체: pictures and drawing shapes on a master page (양쪽·홀수·짝수).
//!
//! Hangul stores them as floating objects placed on the paper, in front of or behind
//! the text (RHWP samples: every master-page picture and nearly every shape is
//! paper-relative, mostly in front of the text). An object is made by the body's own
//! insertion (`picture.insert`, `shape.insert`) and then moved into the master page's
//! first paragraph; the body paragraph is put back as it was. One undo step.
use super::*;
use crate::ext_format::open_control_slot;
use hwpcraft_doc::model::header_footer::HeaderFooterApply;
use hwpcraft_doc::model::shape::{CommonObjAttr, HorzRelTo, TextWrap, VertRelTo};
use serde::Deserialize;

pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("page.master.object.list", "바탕쪽 그림·도형 목록", ""),
    ("page.master.object.insert", "바탕쪽에 그림·도형 넣기", ""),
    ("page.master.object.delete", "바탕쪽 그림·도형 지우기", ""),
];

pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "page.master.object.list",
        "section?(기본 캐럿 구역) → objects[{applyTo both|odd|even, index, kind picture|shape, x_mm, y_mm, width_mm, height_mm, wrap behind|front}]",
        r#"{}"#,
    ),
    (
        "page.master.object.insert",
        "applyTo both|odd|even(바탕쪽이 없으면 빈 바탕쪽을 만듦), kind picture|rectangle|ellipse|line, path(그림), x_mm·y_mm(종이 왼쪽 위 기준), width_mm·height_mm(도형; 그림은 비율 유지), wrap behind|front(기본 behind), section? → {applyTo, index}. 한글 바탕쪽 개체처럼 종이 기준으로 놓습니다. 한 번 실행 취소.",
        r#"{"applyTo":"both","kind":"picture","path":"logo.png","x_mm":150,"y_mm":10,"width_mm":40}"#,
    ),
    (
        "page.master.object.delete",
        "applyTo both|odd|even, index(page.master.object.list의 번호), section?.",
        r#"{"applyTo":"both","index":0}"#,
    ),
];

const HU_PER_MM: f64 = 7200.0 / 25.4;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct List {
    #[serde(default)]
    section: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Insert {
    apply_to: String,
    kind: String,
    #[serde(default)]
    path: Option<std::path::PathBuf>,
    #[serde(rename = "x_mm")]
    x_mm: f64,
    #[serde(rename = "y_mm")]
    y_mm: f64,
    #[serde(rename = "width_mm")]
    width_mm: f64,
    #[serde(default, rename = "height_mm")]
    height_mm: Option<f64>,
    #[serde(default)]
    wrap: Option<String>,
    #[serde(default)]
    section: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Delete {
    apply_to: String,
    index: usize,
    #[serde(default)]
    section: Option<usize>,
}

fn kind_of(apply_to: &str) -> Result<HeaderFooterApply> {
    Ok(match apply_to {
        "both" => HeaderFooterApply::Both,
        "odd" => HeaderFooterApply::Odd,
        "even" => HeaderFooterApply::Even,
        _ => bail!("applyTo는 both, odd, even 중 하나입니다."),
    })
}

fn name_of(kind: HeaderFooterApply) -> &'static str {
    match kind {
        HeaderFooterApply::Odd => "odd",
        HeaderFooterApply::Even => "even",
        _ => "both",
    }
}

fn common(c: &Control) -> Option<&CommonObjAttr> {
    match c {
        Control::Picture(p) => Some(&p.common),
        Control::Shape(s) => Some(s.common()),
        _ => None,
    }
}

fn common_mut(c: &mut Control) -> Option<&mut CommonObjAttr> {
    match c {
        Control::Picture(p) => Some(&mut p.common),
        Control::Shape(s) => Some(s.common_mut()),
        _ => None,
    }
}

/// Master pages are kept on the section and on its section-definition control:
/// apply `edit` to both, drop the raw copies and lay the section out again.
fn edit_masters(
    s: &mut Session,
    section: usize,
    edit: impl Fn(&mut Vec<hwpcraft_doc::model::header_footer::MasterPage>) -> Result<()>,
) -> Result<()> {
    let doc = s.core.document_mut();
    let sec = doc
        .sections
        .get_mut(section)
        .ok_or_else(|| anyhow!("구역이 없습니다."))?;
    edit(&mut sec.section_def.master_pages)?;
    let masters = sec.section_def.master_pages.clone();
    for control in sec
        .paragraphs
        .iter_mut()
        .flat_map(|p| p.controls.iter_mut())
    {
        if let Control::SectionDef(def) = control {
            def.master_pages = masters.clone();
        }
    }
    sec.raw_stream = None;
    s.refresh_layout()
}

impl Session {
    pub(crate) fn run_master_objects_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        match id {
            "page.master.object.list" => Some((|| {
                let p: List = serde_json::from_value(params.clone())?;
                self.master_objects(p.section.unwrap_or(self.cursor.section))
            })()),
            "page.master.object.insert" => Some((|| {
                let p: Insert = serde_json::from_value(params.clone())?;
                self.insert_master_object(p)
            })()),
            "page.master.object.delete" => Some((|| {
                let p: Delete = serde_json::from_value(params.clone())?;
                let section = p.section.unwrap_or(self.cursor.section);
                self.delete_master_object(section, kind_of(&p.apply_to)?, p.index)?;
                self.master_objects(section)
            })()),
            _ => None,
        }
    }

    fn master_objects(&self, section: usize) -> Result<Value> {
        let sec = self
            .core
            .document()
            .sections
            .get(section)
            .ok_or_else(|| anyhow!("구역이 없습니다."))?;
        let mm = |hu: u32| (f64::from(hu) / HU_PER_MM * 100.0).round() / 100.0;
        let mut out = Vec::new();
        for master in sec
            .section_def
            .master_pages
            .iter()
            .filter(|m| !m.is_extension)
        {
            let objects = master.paragraphs.iter().flat_map(|p| &p.controls);
            for (index, c) in objects.filter(|c| common(c).is_some()).enumerate() {
                let Some(o) = common(c) else { continue };
                out.push(json!({
                    "applyTo": name_of(master.apply_to),
                    "index": index,
                    "kind": if matches!(c, Control::Picture(_)) { "picture" } else { "shape" },
                    "x_mm": mm(o.horizontal_offset),
                    "y_mm": mm(o.vertical_offset),
                    "width_mm": mm(o.width),
                    "height_mm": mm(o.height),
                    "wrap": if o.text_wrap == TextWrap::BehindText { "behind" } else { "front" },
                }));
            }
        }
        Ok(json!({"section": section, "objects": out}))
    }

    fn insert_master_object(&mut self, p: Insert) -> Result<Value> {
        let kind = kind_of(&p.apply_to)?;
        let section = p.section.unwrap_or(self.cursor.section);
        let wrap = match p.wrap.as_deref().unwrap_or("behind") {
            "behind" => TextWrap::BehindText,
            "front" => TextWrap::InFrontOfText,
            _ => bail!("wrap은 behind 또는 front입니다."),
        };
        if ![p.x_mm, p.y_mm, p.width_mm]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1000.0).contains(v))
            || p.width_mm < 1.0
            || p.height_mm
                .is_some_and(|h| !h.is_finite() || !(1.0..=1000.0).contains(&h))
        {
            bail!("위치는 종이 안(0–1000 mm), 크기는 1–1000 mm입니다.");
        }
        let picture = match p.kind.as_str() {
            "picture" => true,
            "rectangle" | "ellipse" | "line" => false,
            _ => bail!("kind는 picture, rectangle, ellipse, line 중 하나입니다."),
        };
        if picture != p.path.is_some() {
            bail!("그림은 path가 필요하고, 도형에는 path를 쓰지 않습니다.");
        }
        if picture && p.height_mm.is_some() {
            bail!("그림은 비율을 지키므로 height_mm 없이 width_mm만 지정합니다.");
        }
        if self.core.document().sections.get(section).is_none() {
            bail!("구역이 없습니다.");
        }
        self.guard_protected()?;
        let (cursor, anchor) = (self.cursor, self.anchor);
        let mut index = 0;
        let result = self.transaction(|s| {
            let exists = |s: &Session| {
                s.core.document().sections[section]
                    .section_def
                    .master_pages
                    .iter()
                    .any(|m| m.apply_to == kind && !m.is_extension)
            };
            if !exists(s) {
                // `page.master.set` works on the caret's section.
                s.cursor = Cursor {
                    section,
                    paragraph: 0,
                    offset: 0,
                    cell: None,
                };
                s.anchor = None;
                s.run(
                    "page.master.set",
                    json!({"applyTo": p.apply_to, "text": ""}),
                )?;
            }
            // Made in the body where the section starts, then moved; the body
            // paragraph goes back exactly as it was.
            let body = s.core.document().sections[section].paragraphs[0].clone();
            s.cursor = Cursor {
                section,
                paragraph: 0,
                offset: body.text.chars().count(),
                cell: None,
            };
            s.anchor = None;
            let made = if let Some(path) = &p.path {
                s.insert_picture(path, Some(p.width_mm), false)?
                    .ok_or_else(|| anyhow!("그림을 넣지 못했습니다."))?
            } else {
                let v = s.run(
                    "shape.insert",
                    json!({"kind": p.kind, "width_mm": p.width_mm,
                        "height_mm": p.height_mm.unwrap_or(p.width_mm),
                        "x_mm": p.x_mm, "y_mm": p.y_mm}),
                )?;
                serde_json::from_value(v["address"].clone())?
            };
            let ObjectAddress {
                paragraph, control, ..
            } = made;
            let mut object = {
                let sec = &mut s.core.document_mut().sections[section];
                let moved = sec.paragraphs[paragraph].controls[control].clone();
                sec.paragraphs[paragraph] = body;
                moved
            };
            let o = common_mut(&mut object).ok_or_else(|| anyhow!("개체를 만들지 못했습니다."))?;
            o.treat_as_char = false;
            o.vert_rel_to = VertRelTo::Paper;
            o.horz_rel_to = HorzRelTo::Paper;
            o.horizontal_offset = (p.x_mm * HU_PER_MM).round() as u32;
            o.vertical_offset = (p.y_mm * HU_PER_MM).round() as u32;
            o.text_wrap = wrap;
            edit_masters(s, section, |masters| {
                let master = masters
                    .iter_mut()
                    .find(|m| m.apply_to == kind && !m.is_extension)
                    .ok_or_else(|| anyhow!("바탕쪽을 만들지 못했습니다."))?;
                if master.paragraphs.is_empty() {
                    master.paragraphs.push(Paragraph::new_empty());
                }
                let para = &mut master.paragraphs[0];
                let ci = open_control_slot(para, para.text.chars().count());
                para.align_ctrl_data_records();
                para.controls.insert(ci, object.clone());
                para.ctrl_data_records.insert(ci, None);
                for range in &mut para.field_ranges {
                    if range.control_idx >= ci {
                        range.control_idx += 1;
                    }
                }
                para.control_mask |= 1 << 11;
                para.has_para_text = true;
                para.invalidate_layout_inputs();
                Ok(())
            })?;
            index = s.core.document().sections[section]
                .section_def
                .master_pages
                .iter()
                .find(|m| m.apply_to == kind && !m.is_extension)
                .map_or(0, |m| {
                    m.paragraphs
                        .iter()
                        .flat_map(|p| &p.controls)
                        .filter(|c| common(c).is_some())
                        .count()
                        .saturating_sub(1)
                });
            Ok(())
        });
        self.cursor = cursor;
        self.anchor = anchor;
        result?;
        Ok(json!({"section": section, "applyTo": p.apply_to, "index": index}))
    }

    fn delete_master_object(
        &mut self,
        section: usize,
        kind: HeaderFooterApply,
        index: usize,
    ) -> Result<()> {
        let found = self
            .core
            .document()
            .sections
            .get(section)
            .and_then(|s| {
                s.section_def
                    .master_pages
                    .iter()
                    .find(|m| m.apply_to == kind && !m.is_extension)
            })
            .is_some_and(|m| {
                m.paragraphs
                    .iter()
                    .flat_map(|p| &p.controls)
                    .filter(|c| common(c).is_some())
                    .count()
                    > index
            });
        if !found {
            bail!("그 바탕쪽에 {index}번 그림·도형이 없습니다.");
        }
        self.guard_protected()?;
        self.transaction(|s| {
            edit_masters(s, section, |masters| {
                let master = masters
                    .iter_mut()
                    .find(|m| m.apply_to == kind && !m.is_extension)
                    .ok_or_else(|| anyhow!("바탕쪽이 없습니다."))?;
                let mut seen = 0;
                for para in &mut master.paragraphs {
                    let at = para.controls.iter().enumerate().find_map(|(ci, c)| {
                        common(c)?;
                        seen += 1;
                        (seen == index + 1).then_some(ci)
                    });
                    if let Some(ci) = at {
                        crate::control_slots::remove_control(para, ci);
                        return Ok(());
                    }
                }
                bail!("그 바탕쪽에 {index}번 그림·도형이 없습니다.")
            })
        })
    }
}
