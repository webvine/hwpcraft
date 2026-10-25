//! Typed container editing. This module deliberately leaves v1 addresses intact.
use super::contexts::{checked_range, guard_inline_deletion, logical_split_offset};
use super::*;
use hwpcraft_doc::model::{document::Document, shape::ShapeObject};
use serde::{Deserialize, Serialize};

const MAX_DEPTH: usize = 64;
const MAX_LIST: usize = 100_000;
const MAX_TEXT: usize = 1_000_000;
fn version_two() -> u8 {
    2
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextAddress {
    #[serde(default = "version_two")]
    pub version: u8,
    pub section: usize,
    pub paragraph: usize,
    #[serde(default)]
    pub path: Vec<ContextStep>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContextStep {
    TableCell {
        control: usize,
        cell: usize,
        paragraph: usize,
    },
    Header {
        control: usize,
        paragraph: usize,
    },
    Footer {
        control: usize,
        paragraph: usize,
    },
    Footnote {
        control: usize,
        paragraph: usize,
    },
    Endnote {
        control: usize,
        paragraph: usize,
    },
    #[serde(rename = "textbox")]
    TextBox {
        control: usize,
        #[serde(default)]
        shape_path: Vec<usize>,
        paragraph: usize,
    },
}

impl ContextStep {
    fn control(&self) -> usize {
        match self {
            Self::TableCell { control, .. }
            | Self::Header { control, .. }
            | Self::Footer { control, .. }
            | Self::Footnote { control, .. }
            | Self::Endnote { control, .. }
            | Self::TextBox { control, .. } => *control,
        }
    }
    fn paragraph(&self) -> usize {
        match self {
            Self::TableCell { paragraph, .. }
            | Self::Header { paragraph, .. }
            | Self::Footer { paragraph, .. }
            | Self::Footnote { paragraph, .. }
            | Self::Endnote { paragraph, .. }
            | Self::TextBox { paragraph, .. } => *paragraph,
        }
    }
    fn set_paragraph(&mut self, index: usize) {
        match self {
            Self::TableCell { paragraph, .. }
            | Self::Header { paragraph, .. }
            | Self::Footer { paragraph, .. }
            | Self::Footnote { paragraph, .. }
            | Self::Endnote { paragraph, .. }
            | Self::TextBox { paragraph, .. } => *paragraph = index,
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Self::TableCell { .. } => "table_cell",
            Self::Header { .. } => "header",
            Self::Footer { .. } => "footer",
            Self::Footnote { .. } => "footnote",
            Self::Endnote { .. } => "endnote",
            Self::TextBox { .. } => "textbox",
        }
    }
}

impl Default for ContextAddress {
    fn default() -> Self {
        Self {
            version: 2,
            section: 0,
            paragraph: 0,
            path: vec![],
        }
    }
}
impl From<DocumentAddress> for ContextAddress {
    fn from(address: DocumentAddress) -> Self {
        Self {
            version: 2,
            section: address.section,
            paragraph: address.paragraph,
            path: address
                .path
                .into_iter()
                .map(|step| ContextStep::TableCell {
                    control: step.control,
                    cell: step.cell,
                    paragraph: step.paragraph,
                })
                .collect(),
        }
    }
}
impl ContextAddress {
    fn index(&self) -> usize {
        self.path
            .last()
            .map_or(self.paragraph, ContextStep::paragraph)
    }
    fn with_index(&self, index: usize) -> Self {
        let mut result = self.clone();
        if let Some(step) = result.path.last_mut() {
            step.set_paragraph(index);
        } else {
            result.paragraph = index;
        }
        result
    }
    fn legacy(&self) -> Option<DocumentAddress> {
        let path = self
            .path
            .iter()
            .map(|step| match step {
                ContextStep::TableCell {
                    control,
                    cell,
                    paragraph,
                } => Some(CellStep {
                    control: *control,
                    cell: *cell,
                    paragraph: *paragraph,
                }),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        Some(DocumentAddress {
            section: self.section,
            paragraph: self.paragraph,
            path,
        })
    }
}

pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "context.v2.list",
        "본문·표·머리말·꼬리말·글상자·주석 문맥 목록",
        "",
    ),
    ("context.v2.get", "종류가 지정된 문맥 내용·서식 조회", ""),
    ("context.v2.replace", "종류가 지정된 문맥 텍스트 교체", ""),
    ("context.v2.split", "종류가 지정된 문맥 문단 나누기", ""),
    (
        "context.v2.merge",
        "종류가 지정된 문맥 앞 문단과 합치기",
        "",
    ),
    ("context.v2.character", "종류가 지정된 문맥 글자 모양", ""),
    ("context.v2.paragraph", "종류가 지정된 문맥 문단 모양", ""),
];
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "context.v2.list",
        "start, limit 1..1000, include optional kinds: body/table_cell/header/footer/textbox/footnote/endnote. Returns typed version-2 addresses and generation.",
        r#"{"limit":100}"#,
    ),
    (
        "context.v2.get",
        "address:{version:2,section,paragraph,path:[{kind,control,paragraph,...}]}, offset optional Unicode scalar.",
        r#"{"address":{"version":2,"section":0,"paragraph":0,"path":[]}}"#,
    ),
    (
        "context.v2.replace",
        "address, start/end Unicode scalar range, single-paragraph text, required expected_generation. Refuses inline control loss and protected ancestors.",
        r#"{"address":{"version":2,"section":0,"paragraph":0,"path":[{"kind":"header","control":2,"paragraph":0}]},"start":0,"end":0,"text":"머리말","expected_generation":0}"#,
    ),
    (
        "context.v2.split",
        "address, offset Unicode scalar, required expected_generation. Returns next paragraph in the same container.",
        r#"{"address":{"version":2,"section":0,"paragraph":0,"path":[]},"offset":0,"expected_generation":0}"#,
    ),
    (
        "context.v2.merge",
        "address of later paragraph, required expected_generation. Never merges across container boundaries.",
        r#"{"address":{"version":2,"section":0,"paragraph":1,"path":[]},"expected_generation":0}"#,
    ),
    (
        "context.v2.character",
        "address, start/end optional whole paragraph, format as format.character, required expected_generation.",
        r#"{"address":{"version":2,"section":0,"paragraph":0,"path":[]},"format":{"bold":true},"expected_generation":0}"#,
    ),
    (
        "context.v2.paragraph",
        "address, format as format.paragraph, required expected_generation.",
        r#"{"address":{"version":2,"section":0,"paragraph":0,"path":[]},"format":{"alignment":"center"},"expected_generation":0}"#,
    ),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    #[serde(default)]
    start: usize,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    include: Vec<String>,
}
fn default_limit() -> usize {
    100
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    address: ContextAddress,
    #[serde(default)]
    offset: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replace {
    address: ContextAddress,
    start: usize,
    end: usize,
    text: String,
    expected_generation: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Split {
    address: ContextAddress,
    offset: usize,
    expected_generation: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Merge {
    address: ContextAddress,
    expected_generation: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Format {
    address: ContextAddress,
    #[serde(default)]
    start: Option<usize>,
    #[serde(default)]
    end: Option<usize>,
    format: Value,
    expected_generation: u64,
}

struct Resolved<'a> {
    paragraphs: &'a Vec<Paragraph>,
    width: u32,
    protection: Vec<&'static str>,
    readonly: Vec<&'static str>,
}

fn shape_at_mut<'a>(shape: &'a mut ShapeObject, path: &[usize]) -> Result<&'a mut ShapeObject> {
    if let Some((index, rest)) = path.split_first() {
        let ShapeObject::Group(group) = shape else {
            bail!("글상자 경로가 묶음 개체가 아닙니다.");
        };
        shape_at_mut(
            group
                .children
                .get_mut(*index)
                .ok_or_else(|| anyhow!("묶음 자식이 없습니다."))?,
            rest,
        )
    } else {
        Ok(shape)
    }
}
fn content_width(width: u32, left: i64, right: i64) -> u32 {
    (i64::from(width) - left.max(0) - right.max(0)).clamp(0, i64::from(u32::MAX)) as u32
}
fn check_address(address: &ContextAddress) -> Result<()> {
    if address.version != 2 {
        bail!("문맥 주소 version은 2여야 합니다.");
    }
    let depth = address.path.len().saturating_add(
        address
            .path
            .iter()
            .map(|step| match step {
                ContextStep::TextBox { shape_path, .. } => shape_path.len(),
                _ => 0,
            })
            .sum::<usize>(),
    );
    if depth > MAX_DEPTH {
        bail!("문맥 경로는 최대 {MAX_DEPTH}단계까지 지원합니다.");
    }
    Ok(())
}
fn resolve<'a>(doc: &'a Document, address: &ContextAddress) -> Result<Resolved<'a>> {
    check_address(address)?;
    let section = doc
        .sections
        .get(address.section)
        .ok_or_else(|| anyhow!("구역 주소가 범위를 벗어났습니다."))?;
    let page = &section.section_def.page_def;
    let body = content_width(
        page.width,
        i64::from(page.margin_left) + i64::from(page.margin_gutter),
        i64::from(page.margin_right),
    );
    let mut result = Resolved {
        paragraphs: &section.paragraphs,
        width: body,
        protection: vec![],
        readonly: vec![],
    };
    let mut index = address.paragraph;
    for step in &address.path {
        let para = result
            .paragraphs
            .get(index)
            .ok_or_else(|| anyhow!("문맥 부모 문단이 없습니다."))?;
        let control = para
            .controls
            .get(step.control())
            .ok_or_else(|| anyhow!("문맥 컨트롤이 없습니다."))?;
        result.paragraphs = match (step, control) {
            (ContextStep::TableCell { cell, .. }, Control::Table(table)) => {
                let cell = table
                    .cells
                    .get(*cell)
                    .ok_or_else(|| anyhow!("셀 주소가 범위를 벗어났습니다."))?;
                if table.common.locked {
                    result.protection.push("locked_table");
                }
                if cell.cell_protect() {
                    result.protection.push("protected_cell");
                }
                let padding = cell.effective_padding(&table.padding);
                result.width = content_width(
                    cell.width,
                    i64::from(padding.left),
                    i64::from(padding.right),
                );
                &cell.paragraphs
            }
            (ContextStep::Header { .. }, Control::Header(header)) => {
                if header.list_attr & 7 != 0 {
                    result.readonly.push("vertical_header_footer_not_validated");
                }
                result.width = if header.text_width > 0 {
                    header.text_width
                } else {
                    body
                };
                &header.paragraphs
            }
            (ContextStep::Footer { .. }, Control::Footer(footer)) => {
                if footer.list_attr & 7 != 0 {
                    result.readonly.push("vertical_header_footer_not_validated");
                }
                result.width = if footer.text_width > 0 {
                    footer.text_width
                } else {
                    body
                };
                &footer.paragraphs
            }
            (ContextStep::Footnote { .. }, Control::Footnote(note)) => {
                result.width = body;
                &note.paragraphs
            }
            (ContextStep::Endnote { .. }, Control::Endnote(note)) => {
                result.width = body;
                &note.paragraphs
            }
            (ContextStep::TextBox { shape_path, .. }, Control::Shape(shape)) => {
                let mut ancestor = shape.as_ref();
                if ancestor.common().locked {
                    result.protection.push("locked_shape");
                }
                for child in shape_path {
                    let ShapeObject::Group(group) = ancestor else {
                        bail!("글상자 묶음 경로가 올바르지 않습니다.");
                    };
                    ancestor = group
                        .children
                        .get(*child)
                        .ok_or_else(|| anyhow!("묶음 자식이 없습니다."))?;
                    if ancestor.common().locked {
                        result.protection.push("locked_shape");
                    }
                }
                let text = ancestor
                    .drawing()
                    .and_then(|drawing| drawing.text_box.as_ref())
                    .ok_or_else(|| anyhow!("지정한 도형에 글상자가 없습니다."))?;
                if !matches!(
                    ancestor,
                    ShapeObject::Rectangle(_)
                        | ShapeObject::Ellipse(_)
                        | ShapeObject::Polygon(_)
                        | ShapeObject::Curve(_)
                ) {
                    result.readonly.push("unsupported_textbox_shape");
                }
                if text.vertical_all || text.list_attr & 7 != 0 {
                    result.readonly.push("vertical_textbox_not_validated");
                }
                result.width = content_width(
                    ancestor.common().width,
                    i64::from(text.margin_left),
                    i64::from(text.margin_right),
                );
                &text.paragraphs
            }
            _ => bail!(
                "문맥 경로 종류 {}와 실제 개체가 일치하지 않습니다.",
                step.kind()
            ),
        };
        index = step.paragraph();
    }
    if result.paragraphs.get(index).is_none() {
        bail!("문맥 문단 주소가 범위를 벗어났습니다.");
    }
    if result.width == 0 {
        result.readonly.push("empty_content_width");
    }
    if section.section_def.text_direction != 0 {
        result.readonly.push("vertical_section_not_validated");
    }
    Ok(result)
}
fn child_mut<'a>(para: &'a mut Paragraph, step: &ContextStep) -> Result<&'a mut Vec<Paragraph>> {
    let control = para
        .controls
        .get_mut(step.control())
        .ok_or_else(|| anyhow!("문맥 컨트롤이 없습니다."))?;
    match (step, control) {
        (ContextStep::TableCell { cell, .. }, Control::Table(table)) => Ok(&mut table
            .cells
            .get_mut(*cell)
            .ok_or_else(|| anyhow!("셀이 없습니다."))?
            .paragraphs),
        (ContextStep::Header { .. }, Control::Header(header)) => Ok(&mut header.paragraphs),
        (ContextStep::Footer { .. }, Control::Footer(footer)) => Ok(&mut footer.paragraphs),
        (ContextStep::Footnote { .. }, Control::Footnote(note)) => Ok(&mut note.paragraphs),
        (ContextStep::Endnote { .. }, Control::Endnote(note)) => Ok(&mut note.paragraphs),
        (ContextStep::TextBox { shape_path, .. }, Control::Shape(shape)) => {
            Ok(&mut shape_at_mut(shape, shape_path)?
                .drawing_mut()
                .and_then(|drawing| drawing.text_box.as_mut())
                .ok_or_else(|| anyhow!("글상자가 없습니다."))?
                .paragraphs)
        }
        _ => bail!("문맥 개체 종류가 일치하지 않습니다."),
    }
}
fn container_mut<'a>(
    doc: &'a mut Document,
    address: &ContextAddress,
) -> Result<&'a mut Vec<Paragraph>> {
    fn descend<'a>(
        paras: &'a mut Vec<Paragraph>,
        index: usize,
        steps: &[ContextStep],
    ) -> Result<&'a mut Vec<Paragraph>> {
        let Some((step, rest)) = steps.split_first() else {
            return Ok(paras);
        };
        let para = paras
            .get_mut(index)
            .ok_or_else(|| anyhow!("문맥 부모 문단이 없습니다."))?;
        descend(child_mut(para, step)?, step.paragraph(), rest)
    }
    let section = doc
        .sections
        .get_mut(address.section)
        .ok_or_else(|| anyhow!("구역이 없습니다."))?;
    descend(&mut section.paragraphs, address.paragraph, &address.path)
}
impl Session {
    pub(crate) fn run_context_v2_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(command, _, _)| *command == id) {
            return None;
        }
        Some((|| match id {
            "context.v2.list" => self.context_v2_list(serde_json::from_value(params.clone())?),
            "context.v2.get" => {
                let p: Query = serde_json::from_value(params.clone())?;
                self.context_v2_info(&p.address, p.offset)
            }
            "context.v2.replace" => {
                self.context_v2_replace(serde_json::from_value(params.clone())?)
            }
            "context.v2.split" => self.context_v2_split(serde_json::from_value(params.clone())?),
            "context.v2.merge" => self.context_v2_merge(serde_json::from_value(params.clone())?),
            "context.v2.character" | "context.v2.paragraph" => self.context_v2_format(
                serde_json::from_value(params.clone())?,
                id == "context.v2.character",
            ),
            _ => bail!("알 수 없는 v2 문맥 명령"),
        })())
    }
    fn context_v2_writable(&self, address: &ContextAddress, generation: u64) -> Result<()> {
        if generation != self.generation {
            bail!("문서가 변경돼 문맥 주소를 다시 조회해야 합니다.");
        }
        let result = resolve(self.core.document(), address)?;
        if !result.protection.is_empty() {
            bail!(
                "보호된 문맥은 고칠 수 없습니다: {}",
                result.protection.join(", ")
            );
        }
        if !result.readonly.is_empty() {
            bail!(
                "현재 읽기 전용인 문맥입니다: {}",
                result.readonly.join(", ")
            );
        }
        Ok(())
    }
    /// Copy only a container into a width-correct temporary body. The complete
    /// source's resource/style tables remain available and are never renumbered.
    fn context_v2_projection(&self, address: &ContextAddress) -> Result<DocumentCore> {
        let resolved = resolve(self.core.document(), address)?;
        let mut document = self.core.document().clone();
        let section = &mut document.sections[address.section];
        section.paragraphs = resolved.paragraphs.clone();
        section.raw_stream = None;
        let page = &mut section.section_def.page_def;
        page.width = resolved.width.max(1);
        page.landscape = false;
        page.margin_left = 0;
        page.margin_right = 0;
        page.margin_gutter = 0;
        section.section_def.text_direction = 0;
        let mut core = DocumentCore::new_empty();
        core.set_document(document);
        Ok(core)
    }
    fn context_v2_commit_projection(
        &mut self,
        address: &ContextAddress,
        projection: DocumentCore,
    ) -> Result<()> {
        let section = &projection.document().sections[address.section];
        let mut paragraphs = section.paragraphs.clone();
        for para in &mut paragraphs {
            para.invalidate_layout_inputs();
        }
        *container_mut(self.core.document_mut(), address)? = paragraphs;
        self.core.document_mut().doc_info = projection.document().doc_info.clone();
        self.core.document_mut().sections[address.section].raw_stream = None;
        self.core.set_document(self.core.document().clone());
        Ok(())
    }
    fn context_v2_apply_projection(
        &mut self,
        address: &ContextAddress,
        projection: DocumentCore,
        result_address: &ContextAddress,
        offset: usize,
    ) -> Result<Value> {
        let mut result = Value::Null;
        self.transaction(|session| {
            session.context_v2_commit_projection(address, projection)?;
            result = session.context_v2_info(result_address, offset)?;
            Ok(())
        })?;
        result["generation"] = json!(self.generation);
        Ok(result)
    }
    pub fn context_v2_info(&self, address: &ContextAddress, offset: usize) -> Result<Value> {
        let resolved = resolve(self.core.document(), address)?;
        let para = &resolved.paragraphs[address.index()];
        checked_range(para, offset, offset)?;
        let projection = self.context_v2_projection(address)?;
        let character: Value = serde_json::from_str(
            &projection
                .get_char_properties_at_native(address.section, address.index(), offset)
                .map_err(err)?,
        )?;
        let mut paragraph: Value = serde_json::from_str(
            &projection
                .get_para_properties_at_native(address.section, address.index())
                .map_err(err)?,
        )?;
        // Native dialog properties contain pixel margins. The context protocol
        // must return the same raw units accepted by its formatter.
        if let Some(shape) = self
            .core
            .document()
            .doc_info
            .para_shapes
            .get(usize::from(para.para_shape_id))
        {
            for (key, value) in [
                ("marginLeft", shape.margin_left),
                ("marginRight", shape.margin_right),
                ("indent", shape.indent),
                ("spacingBefore", shape.spacing_before),
                ("spacingAfter", shape.spacing_after),
                ("lineSpacing", shape.line_spacing),
            ] {
                paragraph[key] = json!(value);
            }
        }
        let readonly = !resolved.protection.is_empty() || !resolved.readonly.is_empty();
        Ok(
            json!({"address":address,"kind":address.path.last().map_or("body",ContextStep::kind),"containerKinds":address.path.iter().map(ContextStep::kind).collect::<Vec<_>>(),
            "text":para.text.chars().take(MAX_TEXT).collect::<String>(),"length":para.text.chars().count(),"textTruncated":para.text.chars().count()>MAX_TEXT,
            "offset":offset,"offsetUnit":"unicode_scalar","generation":self.generation,"controls":para.controls.len(),"contentWidth":resolved.width,
            "protected":!resolved.protection.is_empty(),"protectionReasons":resolved.protection,"readonlyReasons":resolved.readonly,
            "capabilities":{"read":true,"replace":!readonly,"split":!readonly,"merge":!readonly&&address.index()>0,"character":!readonly,"paragraph":!readonly,"inlineCanvas":false,"tableCreate":false},
            "character":character,"paragraph":paragraph}),
        )
    }
    fn context_v2_list(&self, p: List) -> Result<Value> {
        if !(1..=1000).contains(&p.limit) {
            bail!("limit은 1–1000이어야 합니다.");
        }
        for kind in &p.include {
            if ![
                "body",
                "table_cell",
                "header",
                "footer",
                "textbox",
                "footnote",
                "endnote",
            ]
            .contains(&kind.as_str())
            {
                bail!("알 수 없는 문맥 종류: {kind}");
            }
        }
        fn visit(
            session: &Session,
            address: ContextAddress,
            rows: &mut Vec<Value>,
            truncated: &mut bool,
            include: &[String],
            visited: &mut usize,
        ) -> Result<()> {
            if *visited >= MAX_LIST {
                *truncated = true;
                return Ok(());
            }
            *visited += 1;
            let resolved = resolve(session.core.document(), &address)?;
            let para = &resolved.paragraphs[address.index()];
            let kind = address.path.last().map_or("body", ContextStep::kind);
            if include.is_empty() || include.iter().any(|wanted| wanted == kind) {
                rows.push(json!({"address":address,"kind":kind,"depth":address.path.len(),"text":para.text.chars().take(160).collect::<String>(),"length":para.text.chars().count(),"protected":!resolved.protection.is_empty(),"readonly":!resolved.protection.is_empty()||!resolved.readonly.is_empty()}));
            }
            if address.path.len() >= MAX_DEPTH {
                *truncated |= !para.controls.is_empty();
                return Ok(());
            }
            let mut children = Vec::new();
            for (control, node) in para.controls.iter().enumerate() {
                match node {
                    Control::Table(table) => {
                        for (cell, value) in table.cells.iter().enumerate() {
                            for paragraph in 0..value.paragraphs.len() {
                                children.push(ContextStep::TableCell {
                                    control,
                                    cell,
                                    paragraph,
                                });
                            }
                        }
                    }
                    Control::Header(value) => {
                        for paragraph in 0..value.paragraphs.len() {
                            children.push(ContextStep::Header { control, paragraph });
                        }
                    }
                    Control::Footer(value) => {
                        for paragraph in 0..value.paragraphs.len() {
                            children.push(ContextStep::Footer { control, paragraph });
                        }
                    }
                    Control::Footnote(value) => {
                        for paragraph in 0..value.paragraphs.len() {
                            children.push(ContextStep::Footnote { control, paragraph });
                        }
                    }
                    Control::Endnote(value) => {
                        for paragraph in 0..value.paragraphs.len() {
                            children.push(ContextStep::Endnote { control, paragraph });
                        }
                    }
                    Control::Shape(shape) => {
                        fn boxes(
                            shape: &ShapeObject,
                            path: Vec<usize>,
                            control: usize,
                            children: &mut Vec<ContextStep>,
                            truncated: &mut bool,
                        ) {
                            if path.len() > MAX_DEPTH || children.len() >= MAX_LIST {
                                *truncated = true;
                                return;
                            }
                            if let Some(text) = shape.drawing().and_then(|d| d.text_box.as_ref()) {
                                for paragraph in 0..text.paragraphs.len() {
                                    children.push(ContextStep::TextBox {
                                        control,
                                        shape_path: path.clone(),
                                        paragraph,
                                    });
                                }
                            }
                            if let ShapeObject::Group(group) = shape {
                                for (index, child) in group.children.iter().enumerate() {
                                    let mut next = path.clone();
                                    next.push(index);
                                    boxes(child, next, control, children, truncated);
                                }
                            }
                        }
                        boxes(shape, vec![], control, &mut children, truncated);
                    }
                    _ => {}
                }
                if children.len() >= MAX_LIST {
                    *truncated = true;
                    children.truncate(MAX_LIST);
                    break;
                }
            }
            for child in children {
                let mut next = address.clone();
                next.path.push(child);
                if check_address(&next).is_err() {
                    *truncated = true;
                    continue;
                }
                visit(session, next, rows, truncated, include, visited)?;
                if *visited >= MAX_LIST {
                    *truncated = true;
                    break;
                }
            }
            Ok(())
        }
        let mut rows = vec![];
        let mut truncated = false;
        let mut visited = 0;
        for (section, value) in self.core.document().sections.iter().enumerate() {
            for paragraph in 0..value.paragraphs.len() {
                visit(
                    self,
                    ContextAddress {
                        version: 2,
                        section,
                        paragraph,
                        path: vec![],
                    },
                    &mut rows,
                    &mut truncated,
                    &p.include,
                    &mut visited,
                )?;
                if visited >= MAX_LIST {
                    break;
                }
            }
            if visited >= MAX_LIST {
                break;
            }
        }
        let total = rows.len();
        let end = p.start.saturating_add(p.limit).min(total);
        let items = if p.start < total {
            rows.drain(p.start..end).collect::<Vec<_>>()
        } else {
            vec![]
        };
        Ok(
            json!({"items":items,"start":p.start,"total":total,"nextStart":(end<total).then_some(end),"truncated":truncated,"generation":self.generation,"addressVersion":2,"offsetUnit":"unicode_scalar"}),
        )
    }
    fn context_v2_replace(&mut self, p: Replace) -> Result<Value> {
        self.context_v2_writable(&p.address, p.expected_generation)?;
        let resolved = resolve(self.core.document(), &p.address)?;
        let para = &resolved.paragraphs[p.address.index()];
        checked_range(para, p.start, p.end)?;
        guard_inline_deletion(para, p.start, p.end)?;
        if p.text.len() > MAX_TEXT
            || p.text.contains(['\r', '\n'])
            || p.text.chars().any(|c| c.is_control() && c != '\t')
        {
            bail!("문맥 교체는 줄바꿈/제어 문자 없는 1 MB 이하 문자열이어야 합니다.");
        }
        if para
            .text
            .chars()
            .skip(p.start)
            .take(p.end - p.start)
            .collect::<String>()
            == p.text
        {
            return self.context_v2_info(&p.address, p.start + p.text.chars().count());
        }
        if let Some(legacy) = p.address.legacy() {
            self.run("context.replace",json!({"address":legacy,"start":p.start,"end":p.end,"text":p.text,"expected_generation":p.expected_generation}))?;
        } else {
            let mut projection = self.context_v2_projection(&p.address)?;
            if p.start != p.end {
                projection
                    .delete_text_native(
                        p.address.section,
                        p.address.index(),
                        p.start,
                        p.end - p.start,
                    )
                    .map_err(err)?;
            }
            if !p.text.is_empty() {
                projection
                    .insert_text_native(p.address.section, p.address.index(), p.start, &p.text)
                    .map_err(err)?;
            }
            return self.context_v2_apply_projection(
                &p.address,
                projection,
                &p.address,
                p.start + p.text.chars().count(),
            );
        }
        self.context_v2_info(&p.address, p.start + p.text.chars().count())
    }
    fn context_v2_split(&mut self, p: Split) -> Result<Value> {
        self.context_v2_writable(&p.address, p.expected_generation)?;
        let resolved = resolve(self.core.document(), &p.address)?;
        let para = &resolved.paragraphs[p.address.index()];
        checked_range(para, p.offset, p.offset)?;
        if let Some(legacy) = p.address.legacy() {
            self.run("context.split",json!({"address":legacy,"offset":p.offset,"expected_generation":p.expected_generation}))?;
        } else {
            let offset = logical_split_offset(para, p.offset);
            let mut projection = self.context_v2_projection(&p.address)?;
            projection
                .split_paragraph_native(p.address.section, p.address.index(), offset, None)
                .map_err(err)?;
            return self.context_v2_apply_projection(
                &p.address,
                projection,
                &p.address.with_index(p.address.index() + 1),
                0,
            );
        }
        self.context_v2_info(&p.address.with_index(p.address.index() + 1), 0)
    }
    fn context_v2_merge(&mut self, p: Merge) -> Result<Value> {
        self.context_v2_writable(&p.address, p.expected_generation)?;
        if p.address.index() == 0 {
            bail!("문맥의 첫 문단은 합칠 수 없습니다.");
        }
        let previous = p.address.with_index(p.address.index() - 1);
        self.context_v2_writable(&previous, p.expected_generation)?;
        let prefix = resolve(self.core.document(), &previous)?.paragraphs[previous.index()]
            .text
            .chars()
            .count();
        if let Some(legacy) = p.address.legacy() {
            self.run(
                "context.merge",
                json!({"address":legacy,"expected_generation":p.expected_generation}),
            )?;
        } else {
            let mut projection = self.context_v2_projection(&p.address)?;
            projection
                .merge_paragraph_native(p.address.section, p.address.index())
                .map_err(err)?;
            return self.context_v2_apply_projection(&p.address, projection, &previous, prefix);
        }
        self.context_v2_info(&previous, prefix)
    }
    fn context_v2_format(&mut self, p: Format, character: bool) -> Result<Value> {
        self.context_v2_writable(&p.address, p.expected_generation)?;
        let resolved = resolve(self.core.document(), &p.address)?;
        let para = &resolved.paragraphs[p.address.index()];
        let (start, end) = (
            p.start.unwrap_or(0),
            p.end.unwrap_or_else(|| para.text.chars().count()),
        );
        checked_range(para, start, end)?;
        if !character && (p.start.is_some() || p.end.is_some()) {
            bail!("문단 서식은 문자 범위를 받지 않습니다.");
        }
        validate_format(&p.format, character)?;
        if let Some(legacy) = p.address.legacy() {
            let mut params = json!({"address":legacy,"format":p.format,"expected_generation":p.expected_generation});
            if character {
                params["start"] = json!(start);
                params["end"] = json!(end);
            }
            self.run(
                if character {
                    "context.character"
                } else {
                    "context.paragraph"
                },
                params,
            )?;
        } else {
            let projection = self.context_v2_projection(&p.address)?;
            let mut formatter = Self::with_core(projection, None);
            if character {
                let current = serde_json::from_str(
                    &formatter
                        .core
                        .get_char_properties_at_native(p.address.section, p.address.index(), start)
                        .map_err(err)?,
                )?;
                let props = formatter
                    .character_parameters_from(p.format, Some(current))?
                    .to_string();
                formatter
                    .core
                    .apply_char_format_native(
                        p.address.section,
                        p.address.index(),
                        start,
                        end,
                        &props,
                    )
                    .map_err(err)?;
            } else {
                let current = serde_json::from_str(
                    &formatter
                        .core
                        .get_para_properties_at_native(p.address.section, p.address.index())
                        .map_err(err)?,
                )?;
                let props = formatter
                    .paragraph_parameters_from(p.format, || Ok(current))?
                    .to_string();
                formatter
                    .core
                    .apply_para_format_native(p.address.section, p.address.index(), &props)
                    .map_err(err)?;
            }
            return self.context_v2_apply_projection(&p.address, formatter.core, &p.address, start);
        }
        self.context_v2_info(&p.address, start)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hwpcraft_doc::model::{
        footnote::{Endnote, Footnote},
        header_footer::{Footer, Header},
        shape::{CommonObjAttr, DrawingObjAttr, RectangleShape, TextBox},
    };
    fn run(s: &mut Session, id: &str, params: Value) -> Result<Value> {
        s.run_context_v2_ext(id, &params)
            .ok_or_else(|| anyhow!("missing v2 command"))?
    }
    fn edit(s: &mut Session, id: &str, mut params: Value) -> Result<Value> {
        params["expected_generation"] = json!(s.generation);
        run(s, id, params)
    }
    fn empty() -> Paragraph {
        Paragraph::new_empty()
    }
    fn fixture() -> Result<Session> {
        let mut s = Session::new()?;
        let text = Control::Shape(Box::new(ShapeObject::Rectangle(RectangleShape {
            common: CommonObjAttr {
                width: 18000,
                height: 10000,
                ..Default::default()
            },
            drawing: DrawingObjAttr {
                text_box: Some(TextBox {
                    paragraphs: vec![empty()],
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })));
        let mut nested = Paragraph {
            controls: vec![text.clone()],
            char_count: 9,
            ..empty()
        };
        nested.align_ctrl_data_records();
        let host = &mut s.core.document_mut().sections[0].paragraphs[0];
        host.controls.extend([
            Control::Header(Box::new(Header {
                paragraphs: vec![nested],
                ..Default::default()
            })),
            Control::Footer(Box::new(Footer {
                paragraphs: vec![empty()],
                ..Default::default()
            })),
            Control::Footnote(Box::new(Footnote {
                number: 1,
                instance_id: 9001,
                paragraphs: vec![empty()],
                ..Default::default()
            })),
            Control::Endnote(Box::new(Endnote {
                number: 1,
                instance_id: 9002,
                paragraphs: vec![empty()],
                ..Default::default()
            })),
            text,
        ]);
        host.align_ctrl_data_records();
        host.char_count += 40;
        s.core.set_document(s.core.document().clone());
        Ok(s)
    }
    fn find_address(s: &mut Session, kind: &str, depth: usize) -> Result<ContextAddress> {
        let list = run(s, "context.v2.list", json!({"limit":1000}))?;
        let row = list["items"]
            .as_array()
            .ok_or_else(|| anyhow!("rows"))?
            .iter()
            .find(|row| row["kind"] == kind && row["depth"] == depth)
            .ok_or_else(|| anyhow!("missing {kind}"))?;
        Ok(serde_json::from_value(row["address"].clone())?)
    }
    #[test]
    fn headers_footers_notes_and_textboxes_have_full_editing_and_undo() -> Result<()> {
        let mut s = fixture()?;
        for (kind, depth) in [
            ("header", 1),
            ("footer", 1),
            ("footnote", 1),
            ("endnote", 1),
            ("textbox", 1),
            ("textbox", 2),
        ] {
            let address = find_address(&mut s, kind, depth)?;
            let cursor = s.cursor;
            let text = format!("{kind} 앞😀뒤");
            let result = edit(
                &mut s,
                "context.v2.replace",
                json!({"address":address,"start":0,"end":0,"text":text}),
            )?;
            assert_eq!(result["text"], text);
            assert_eq!(s.cursor, cursor);
            s.execute(Command::Undo)?;
            assert_eq!(s.context_v2_info(&address, 0)?["text"], "");
            s.execute(Command::Redo)?;
            edit(
                &mut s,
                "context.v2.character",
                json!({"address":address,"format":{"bold":true,"fontSize":1500}}),
            )?;
            let formatted = edit(
                &mut s,
                "context.v2.paragraph",
                json!({"address":address,"format":{"alignment":"center"}}),
            )?;
            assert_eq!(formatted["character"]["bold"], true);
            assert_eq!(formatted["character"]["fontSize"], 1500);
            let next = edit(
                &mut s,
                "context.v2.split",
                json!({"address":address,"offset":1}),
            )?;
            edit(
                &mut s,
                "context.v2.merge",
                json!({"address":next["address"]}),
            )?;
            assert_eq!(s.context_v2_info(&address, 0)?["text"], text);
        }
        let dir = tempfile::tempdir()?;
        for extension in ["hwp", "hwpx"] {
            let path = dir.path().join(format!("typed-contexts.{extension}"));
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            let rows = run(&mut reopened, "context.v2.list", json!({"limit":1000}))?;
            for text in [
                "footer 앞😀뒤",
                "footnote 앞😀뒤",
                "endnote 앞😀뒤",
                "textbox 앞😀뒤",
            ] {
                assert!(
                    rows["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|row| row["text"] == text),
                    "{extension}: {text}"
                );
            }
            assert!(reopened.render_pdf(&[0])?.starts_with(b"%PDF-"));
        }
        if std::env::var("HWPCRAFT_WRITE_CONTEXT_V2_FIXTURES").as_deref() == Ok("1") {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()?;
            let output = root.join("output/implementation/0.17/fixtures");
            std::fs::create_dir_all(&output)?;
            for extension in ["hwp", "hwpx"] {
                s.save(&output.join(format!("typed-contexts.{extension}")))?;
            }
            std::fs::write(output.join("typed-contexts.pdf"), s.render_pdf(&[0])?)?;
            std::fs::write(output.join("typed-contexts.svg"), s.render_page_svg(0)?)?;
        }
        Ok(())
    }
    #[test]
    fn strict_kinds_stale_protection_and_control_loss_are_atomic() -> Result<()> {
        let mut s = fixture()?;
        let address = find_address(&mut s, "textbox", 2)?;
        let old = s.generation;
        let original = run(&mut s, "context.v2.list", json!({"limit":1000}))?;
        for params in [
            json!({"address":address,"start":0,"end":99,"text":"x","expected_generation":old}),
            json!({"address":address,"start":0,"end":0,"text":"x","expected_generation":old+1}),
            json!({"address":address,"start":0,"end":0,"text":"a\nb","expected_generation":old}),
        ] {
            assert!(run(&mut s, "context.v2.replace", params).is_err());
            assert_eq!(s.generation, old);
            assert_eq!(
                run(&mut s, "context.v2.list", json!({"limit":1000}))?,
                original
            );
        }
        let mut bad = address.clone();
        bad.path[0] = ContextStep::Footer {
            control: bad.path[0].control(),
            paragraph: 0,
        };
        assert!(s.context_v2_info(&bad, 0).is_err());
        let parent = address.with_index(0);
        let mut header = parent.clone();
        header.path.pop();
        assert!(
            run(
                &mut s,
                "context.v2.replace",
                json!({"address":header,"start":0,"end":1,"text":"","expected_generation":old})
            )
            .is_err()
        );
        let root = find_address(&mut s, "textbox", 1)?;
        let control = root.path[0].control();
        let Control::Shape(shape) =
            &mut s.core.document_mut().sections[0].paragraphs[0].controls[control]
        else {
            panic!("shape");
        };
        shape.common_mut().locked = true;
        assert_eq!(s.context_v2_info(&root, 0)?["protected"], true);
        assert!(
            run(
                &mut s,
                "context.v2.replace",
                json!({"address":root,"start":0,"end":0,"text":"x","expected_generation":old})
            )
            .is_err()
        );
        assert_eq!(s.generation, old);
        Ok(())
    }

    #[test]
    fn mixed_character_runs_fonts_and_paragraph_resources_survive_minimal_edits() -> Result<()> {
        let mut s = fixture()?;
        let address = find_address(&mut s, "footnote", 1)?;
        edit(
            &mut s,
            "context.v2.replace",
            json!({"address":address,"start":0,"end":0,"text":"A😀BC"}),
        )?;
        let fonts_before = s
            .core
            .document()
            .doc_info
            .font_faces
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>();
        edit(
            &mut s,
            "context.v2.character",
            json!({"address":address,"start":0,"end":1,"format":{"bold":true,"fontFamily":"Malgun Gothic"}}),
        )?;
        let family = s.context_v2_info(&address, 0)?["character"]["fontFamily"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        assert!(
            matches!(family.as_str(), "Malgun Gothic" | "맑은 고딕"),
            "unexpected font family: {family}"
        );
        s.execute(Command::Undo)?;
        assert_eq!(
            s.core
                .document()
                .doc_info
                .font_faces
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            fonts_before
        );
        s.execute(Command::Redo)?;
        edit(
            &mut s,
            "context.v2.character",
            json!({"address":address,"start":1,"end":3,"format":{"italic":true,"textColor":"#123456"}}),
        )?;
        let para = &resolve(s.core.document(), &address)?.paragraphs[address.index()];
        let first_style = para.char_shape_id_at(0);
        let last_style = para.char_shape_id_at(3);
        edit(
            &mut s,
            "context.v2.replace",
            json!({"address":address,"start":2,"end":3,"text":"한"}),
        )?;
        edit(
            &mut s,
            "context.v2.paragraph",
            json!({"address":address,"format":{"alignment":"right","marginLeft":600,"spacingAfter":300}}),
        )?;
        let value = s.context_v2_info(&address, 0)?;
        assert_eq!(value["text"], "A😀한C");
        assert_eq!(value["paragraph"]["marginLeft"], 600);
        assert_eq!(value["paragraph"]["spacingAfter"], 300);
        let para = &resolve(s.core.document(), &address)?.paragraphs[address.index()];
        assert_eq!(para.char_shape_id_at(0), first_style);
        assert_eq!(para.char_shape_id_at(3), last_style);
        assert_eq!(s.context_v2_info(&address, 0)?["character"]["bold"], true);
        assert_eq!(s.context_v2_info(&address, 1)?["character"]["italic"], true);
        assert_eq!(
            s.context_v2_info(&address, 3)?["character"]["italic"],
            false
        );
        for reference in &para.char_shapes {
            let shape = &s.core.document().doc_info.char_shapes[reference.char_shape_id as usize];
            for (language, id) in shape.font_ids.iter().enumerate() {
                assert!(
                    s.core.document().doc_info.font_faces[language]
                        .get(usize::from(*id))
                        .is_some()
                );
            }
        }
        let generation = s.generation;
        let shapes = s.core.document().doc_info.char_shapes.len();
        let fonts = s
            .core
            .document()
            .doc_info
            .font_faces
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>();
        assert!(run(&mut s,"context.v2.character",json!({"address":address,"start":0,"end":99,"format":{"fontFamily":"Uncommitted probe font"},"expected_generation":generation})).is_err());
        assert_eq!(s.generation, generation);
        assert_eq!(s.core.document().doc_info.char_shapes.len(), shapes);
        assert_eq!(
            s.core
                .document()
                .doc_info
                .font_faces
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            fonts
        );
        Ok(())
    }

    #[test]
    fn header_textbox_table_content_survives_parent_text_and_structure_edits() -> Result<()> {
        let mut s = fixture()?;
        let temporary = s.core.document().sections[0].paragraphs.len();
        s.core.document_mut().sections[0].paragraphs.push(empty());
        s.core.set_document(s.core.document().clone());
        s.cursor = Cursor {
            paragraph: temporary,
            ..Cursor::default()
        };
        s.run(
            "table.create",
            json!({"rows":1,"columns":1,"width_mm":40.0,"treat_as_char":true}),
        )?;
        let control = s.cursor.cell.unwrap().control;
        let table =
            s.core.document().sections[0].paragraphs[s.cursor.paragraph].controls[control].clone();
        s.core.document_mut().sections[0]
            .paragraphs
            .remove(temporary);
        s.cursor = Cursor::default();
        s.clear_selection();
        let parent = find_address(&mut s, "textbox", 2)?;
        let index = parent.index();
        let para = &mut container_mut(s.core.document_mut(), &parent)?[index];
        para.controls.push(table);
        para.align_ctrl_data_records();
        para.char_count += 8;
        para.invalidate_layout_inputs();
        s.core.set_document(s.core.document().clone());
        let mut cell = parent.clone();
        cell.path.push(ContextStep::TableCell {
            control: 0,
            cell: 0,
            paragraph: 0,
        });
        edit(
            &mut s,
            "context.v2.replace",
            json!({"address":cell,"start":0,"end":0,"text":"중첩 표😀"}),
        )?;
        edit(
            &mut s,
            "context.v2.replace",
            json!({"address":parent,"start":0,"end":0,"text":"표 앞 문장"}),
        )?;
        assert_eq!(s.context_v2_info(&parent, 0)?["controls"], 1);
        assert_eq!(s.context_v2_info(&cell, 0)?["text"], "중첩 표😀");
        let next = edit(
            &mut s,
            "context.v2.split",
            json!({"address":parent,"offset":2}),
        )?;
        edit(
            &mut s,
            "context.v2.merge",
            json!({"address":next["address"]}),
        )?;
        assert_eq!(s.context_v2_info(&parent, 0)?["text"], "표 앞 문장");
        assert_eq!(s.context_v2_info(&cell, 0)?["text"], "중첩 표😀");
        let generation = s.generation;
        let para = &mut container_mut(s.core.document_mut(), &parent)?[parent.index()];
        let Control::Table(table) = &mut para.controls[0] else {
            panic!("nested table");
        };
        table.common.locked = true;
        assert_eq!(s.context_v2_info(&cell, 0)?["protected"], true);
        assert!(run(&mut s,"context.v2.replace",json!({"address":cell,"start":0,"end":1,"text":"x","expected_generation":generation})).is_err());
        assert_eq!(s.generation, generation);
        Ok(())
    }

    #[test]
    fn legacy_body_and_table_addresses_keep_existing_editing_path() -> Result<()> {
        let mut s = Session::new()?;
        let body = ContextAddress::from(DocumentAddress::default());
        edit(
            &mut s,
            "context.v2.replace",
            json!({"address":body,"start":0,"end":0,"text":"본문😀"}),
        )?;
        assert_eq!(s.text(), "본문😀");
        s.select(Cursor::default(), false)?;
        let made = s.run(
            "context.table.create",
            json!({"address":DocumentAddress::default(),"rows":1,"columns":1,"width_mm":80.0}),
        )?;
        let old: DocumentAddress = serde_json::from_value(made["address"].clone())?;
        let table = ContextAddress::from(old);
        edit(
            &mut s,
            "context.v2.replace",
            json!({"address":table,"start":0,"end":0,"text":"셀😀"}),
        )?;
        assert_eq!(s.context_v2_info(&table, 0)?["text"], "셀😀");
        let next = edit(
            &mut s,
            "context.v2.split",
            json!({"address":table,"offset":1}),
        )?;
        edit(
            &mut s,
            "context.v2.merge",
            json!({"address":next["address"]}),
        )?;
        assert_eq!(s.context_v2_info(&table, 0)?["text"], "셀😀");
        Ok(())
    }
}
