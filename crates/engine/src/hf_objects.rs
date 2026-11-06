//! 머리말·꼬리말 안 개체: pictures, drawing shapes, text boxes and tables inside a
//! header or footer.
//!
//! The page layout lists them with their place inside the header (`paraIdx`,
//! `controlIdx`) and the header's own control (`headerFooter`). The body's object
//! commands (`object.properties`, `object.size`, `object.move_by`, `object.delete`, ...)
//! reach them through an address with `"header_footer": {"paragraph", "control"}` (the
//! body paragraph and control index of the header or footer): the command runs on a
//! scratch document whose body is the header's paragraphs, and the result goes back
//! into the header as one undo step, as `master_objects` does for master pages.
use super::*;
use hwpcraft_doc::model::document::{Document, Section};
use serde::Deserialize;

pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "page.header_footer.object.list",
        "머리말·꼬리말 안 그림·도형·표 목록",
        "",
    ),
    (
        "page.header_footer.field.insert",
        "머리말·꼬리말에 쪽 번호·날짜 등 필드 넣기",
        "",
    ),
];

pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "page.header_footer.field.insert",
        "kind(page_number|total_pages|file_name|date_created|date_saved), header(기본 true, false면 꼬리말), apply_to(both|odd|even, 기본 both), section?(기본 캐럿 구역), paragraph(기본 0), offset?(기본 그 문단 끝). 머리말·꼬리말을 고치는 중이 아니어도 그 자리에 넣고(날짜는 첫 문단만, date_saved는 저장한 문서만), 실행 취소 한 번. 결과 paragraph·offset은 필드 뒤 자리. 머리말·꼬리말이 없으면 거절(page.header/page.footer로 먼저 만듦)",
        r#"{"kind":"page_number","header":false,"offset":0}"#,
    ),
    (
        "page.header_footer.object.list",
        "section?(기본 캐럿 구역) → objects[{header: bool, applyTo both|odd|even, kind picture|shape|textbox|table|equation, address}]. address({section, paragraph, control, header_footer:{paragraph, control}})는 머리말·꼬리말 안 개체의 자리이고, 본문 개체 명령(object.properties(.get), object.size, object.move_by, object.rotate, object.flip, object.protect_size, object.outer_margin, object.shadow, object.lock, object.delete, object.style, object.format(.get), object.z_order, picture.recolor, picture.replace, picture.save, shape.resize, textbox.*)과 표 명령의 table(table.properties(.get), table.cell_properties(.get), table.properties.apply)에 그대로 쓰며 한 번에 실행 취소됩니다. 쪽 배치(page_control_layout)의 secIdx·paraIdx·controlIdx와 headerFooter.outerParaIdx·outerControlIdx가 같은 주소입니다.",
        r#"{}"#,
    ),
];

/// Body object commands that also take a header/footer object address, and the ones
/// among them that only read.
const ADDRESSED: &[&str] = &[
    "object.properties.get",
    "object.properties",
    "object.size",
    "object.move_by",
    "object.rotate",
    "object.flip",
    "object.protect_size",
    "object.outer_margin",
    "object.shadow",
    "object.lock",
    "object.delete",
    "shape.resize",
    "textbox.paragraphs.get",
    "textbox.paragraphs",
    "textbox.set",
    "textbox.align",
    "textbox.margin",
    "textbox.direction",
    "object.style",
    "object.format.get",
    "object.format",
    "object.z_order",
    "picture.recolor",
    "picture.replace",
    "picture.save",
    "table.properties.get",
    "table.properties",
    "table.cell_properties.get",
    "table.cell_properties",
    "table.properties.apply",
];
const READS: &[&str] = &[
    "object.properties.get",
    "textbox.paragraphs.get",
    "object.format.get",
    "picture.save",
    "table.properties.get",
    "table.cell_properties.get",
];
/// The parameters that hold object addresses: one, or a list. `object.delete` takes
/// the address as its parameters; the table commands name theirs `table`.
const ADDRESS_KEYS: [&str; 4] = ["address", "addresses", "targets", "table"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    #[serde(default)]
    section: Option<usize>,
}

/// Every object address in `params`.
fn addresses(params: &mut Value) -> Vec<&mut Value> {
    if params.get("header_footer").is_some() {
        return vec![params];
    }
    let Some(map) = params.as_object_mut() else {
        return vec![];
    };
    map.iter_mut()
        .filter(|(key, _)| ADDRESS_KEYS.contains(&key.as_str()))
        .flat_map(|(_, v)| match v {
            Value::Array(list) => list.iter_mut().collect(),
            v => vec![v],
        })
        .filter(|v| v.is_object())
        .collect()
}

/// A copy of the document whose only section has `paragraphs` as its body, on the
/// same paper, so object commands work on them as on body objects.
fn projection(doc: &Document, section: usize, paragraphs: &[Paragraph]) -> DocumentCore {
    let mut section_def = doc.sections[section].section_def.clone();
    section_def.master_pages.clear();
    section_def.extra_child_records.clear();
    let projected = Document {
        header: doc.header.clone(),
        doc_properties: doc.doc_properties.clone(),
        doc_info: doc.doc_info.clone(),
        sections: vec![Section {
            section_def,
            paragraphs: paragraphs.to_vec(),
            raw_stream: None,
            raw_provenance: None,
        }],
        bin_data_content: doc.bin_data_content.clone(),
        is_hwp3_variant: doc.is_hwp3_variant,
        is_hwpx_variant: doc.is_hwpx_variant,
        provenance: doc.provenance,
        ..Default::default()
    };
    let mut core = DocumentCore::new_empty();
    core.set_document(projected);
    core
}

/// The paragraphs of the header or footer at (`paragraph`, `control`) of `section`.
fn hf_paragraphs_at(
    doc: &Document,
    section: usize,
    paragraph: usize,
    control: usize,
) -> Result<&[Paragraph]> {
    match doc
        .sections
        .get(section)
        .and_then(|s| s.paragraphs.get(paragraph))
        .and_then(|p| p.controls.get(control))
    {
        Some(Control::Header(h)) => Ok(&h.paragraphs),
        Some(Control::Footer(f)) => Ok(&f.paragraphs),
        _ => bail!("header_footer가 머리말이나 꼬리말을 가리키지 않습니다."),
    }
}

fn apply_name(apply: hwpcraft_doc::model::header_footer::HeaderFooterApply) -> &'static str {
    use hwpcraft_doc::model::header_footer::HeaderFooterApply as A;
    match apply {
        A::Both => "both",
        A::Even => "even",
        A::Odd => "odd",
    }
}

impl Session {
    /// Run a body object command on a header or footer object when its address has
    /// `"header_footer"`; `None` for addresses in the body.
    pub(crate) fn run_hf_addressed(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        let mut local = params.clone();
        let named: Vec<Option<(Value, Value)>> = addresses(&mut local)
            .into_iter()
            .map(|a| Some((a.get("header_footer")?.clone(), a.get("section")?.clone())))
            .collect();
        if named.iter().all(Option::is_none) {
            return None;
        }
        Some((|| {
            if !ADDRESSED.contains(&id) {
                bail!(
                    "머리말·꼬리말 개체 주소(header_footer)는 개체 속성·크기·옮기기·지우기 명령에만 쓸 수 있습니다: {id}"
                );
            }
            let Some(Some(first)) = named.first().cloned() else {
                bail!("머리말·꼬리말 개체와 본문 개체를 함께 지정할 수 없습니다.");
            };
            if named.iter().any(|n| n.as_ref() != Some(&first)) {
                bail!("다른 머리말·꼬리말이나 본문의 개체를 함께 지정할 수 없습니다.");
            }
            let (hf, section) = first;
            let index = |v: &Value, key: &str| {
                v[key]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| anyhow!("header_footer.{key}는 0 이상의 정수입니다."))
            };
            let section = section
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| anyhow!("section은 0 이상의 정수입니다."))?;
            let (paragraph, control) = (index(&hf, "paragraph")?, index(&hf, "control")?);
            for a in addresses(&mut local) {
                if let Some(a) = a.as_object_mut() {
                    a.remove("header_footer");
                    a.insert("section".into(), json!(0));
                }
            }
            self.run_on_hf(id, local, section, paragraph, control)
        })())
    }

    /// `id` on the header or footer at (`paragraph`, `control`) of `section` (addresses
    /// already in its projection); a change goes back into it as one undo step.
    fn run_on_hf(
        &mut self,
        id: &str,
        params: Value,
        section: usize,
        paragraph: usize,
        control: usize,
    ) -> Result<Value> {
        let doc = self.core.document();
        let paragraphs = hf_paragraphs_at(doc, section, paragraph, control)?;
        // The question's pictures (up to 100 MB) only for the command that reads them.
        let mut scratch = self.scratch_session(
            projection(doc, section, paragraphs),
            id == "picture.replace",
        );
        let read = READS.contains(&id);
        if !read {
            self.guard_protected()?;
        }
        let value = scratch.run(id, params)?;
        // Results that describe the scratch document describe this one instead.
        let value = (value != scratch.info()).then_some(value);
        if read || scratch.generation == 0 {
            return Ok(value.unwrap_or_else(|| self.info()));
        }
        let projected = scratch.core.document();
        let mut edited = projected.sections[0].paragraphs.clone();
        for para in &mut edited {
            para.invalidate_layout_inputs();
        }
        let doc_info = projected.doc_info.clone();
        let bin_data = projected.bin_data_content.clone();
        self.transaction(|s| {
            let doc = s.core.document_mut();
            doc.doc_info = doc_info.clone();
            doc.bin_data_content = bin_data.clone();
            let sec = doc
                .sections
                .get_mut(section)
                .ok_or_else(|| anyhow!("구역이 없습니다."))?;
            match sec
                .paragraphs
                .get_mut(paragraph)
                .and_then(|p| p.controls.get_mut(control))
            {
                Some(Control::Header(h)) => h.paragraphs = edited.clone(),
                Some(Control::Footer(f)) => f.paragraphs = edited.clone(),
                _ => bail!("머리말/꼬리말을 찾지 못했습니다."),
            }
            sec.raw_stream = None;
            s.refresh_layout()
        })?;
        self.revalidate_hf();
        Ok(value.unwrap_or_else(|| self.info()))
    }

    pub(crate) fn run_hf_objects_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        match id {
            "page.header_footer.object.list" => Some((|| {
                let p: List = serde_json::from_value(params.clone())?;
                self.hf_objects(p.section.unwrap_or(self.cursor.section))
            })()),
            "page.header_footer.field.insert" => Some(self.insert_hf_field_command(params.clone())),
            _ => None,
        }
    }

    /// Every header and footer of the document in document order (the order of
    /// `page.header_footer.list`), with its `header_footer` address and objects.
    pub(crate) fn hf_list_entries(&self) -> Vec<Value> {
        let doc = self.core.document();
        let mut entries = Vec::new();
        for (si, sec) in doc.sections.iter().enumerate() {
            for (pi, p) in sec.paragraphs.iter().enumerate() {
                for (ci, c) in p.controls.iter().enumerate() {
                    if matches!(c, Control::Header(_) | Control::Footer(_)) {
                        let objects = self
                            .hf_objects(si)
                            .ok()
                            .and_then(|v| v["objects"].as_array().cloned())
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|o| {
                                o["address"]["header_footer"]
                                    == json!({"paragraph": pi, "control": ci})
                            })
                            .map(|mut o| {
                                if let Some(m) = o.as_object_mut() {
                                    m.remove("header");
                                    m.remove("applyTo");
                                }
                                o
                            })
                            .collect::<Vec<_>>();
                        entries.push(json!({
                            "header_footer": {"paragraph": pi, "control": ci},
                            "objects": objects,
                        }));
                    }
                }
            }
        }
        entries
    }

    fn hf_objects(&self, section: usize) -> Result<Value> {
        let sec = self
            .core
            .document()
            .sections
            .get(section)
            .ok_or_else(|| anyhow!("구역이 없습니다."))?;
        let mut objects = Vec::new();
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            for (ci, c) in p.controls.iter().enumerate() {
                let (header, apply, inner) = match c {
                    Control::Header(h) => (true, h.apply_to, &h.paragraphs),
                    Control::Footer(f) => (false, f.apply_to, &f.paragraphs),
                    _ => continue,
                };
                for (ip, para) in inner.iter().enumerate() {
                    for (ic, object) in para.controls.iter().enumerate() {
                        let kind = match object {
                            Control::Picture(_) => "picture",
                            Control::Shape(s)
                                if s.drawing().and_then(|d| d.text_box.as_ref()).is_some() =>
                            {
                                "textbox"
                            }
                            Control::Shape(_) => "shape",
                            Control::Table(_) => "table",
                            Control::Equation(_) => "equation",
                            _ => continue,
                        };
                        objects.push(json!({
                            "header": header,
                            "applyTo": apply_name(apply),
                            "kind": kind,
                            "address": {"section": section, "paragraph": ip, "control": ic,
                                "header_footer": {"paragraph": pi, "control": ci}},
                        }));
                    }
                }
            }
        }
        Ok(json!({"section": section, "objects": objects}))
    }
}
