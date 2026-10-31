use super::*;
use hwpcraft_doc::model::shape::ShapeObject;
use serde::{Deserialize, Serialize};

/// Addresses refer to top-level controls, never silently to nested objects.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObjectAddress {
    pub section: usize,
    pub paragraph: usize,
    pub control: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EquationOptions {
    pub script: String,
    pub size_pt: f64,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Line,
    Textbox,
    Arc,
    Polygon,
    /// Created as a polygon, then converted to a Bézier curve.
    Curve,
    /// 연결선 (Hangul's six kinds): straight / elbow, without arrow, one arrow, both arrows.
    ConnectorStraight,
    ConnectorStraightArrow,
    ConnectorStraightBoth,
    ConnectorStroke,
    ConnectorStrokeArrow,
    ConnectorStrokeBoth,
}
impl ShapeKind {
    /// RHWP `create_shape_control_native` shape type. RHWP creates one-way
    /// connectors only; `*Both` is created one-way and widened afterwards.
    pub fn name(self) -> &'static str {
        match self {
            Self::Rectangle => "rectangle",
            Self::Ellipse => "ellipse",
            Self::Line => "line",
            Self::Textbox => "textbox",
            Self::Arc => "arc",
            Self::Polygon | Self::Curve => "polygon",
            Self::ConnectorStraight => "connector-straight",
            Self::ConnectorStraightArrow | Self::ConnectorStraightBoth => {
                "connector-straight-arrow"
            }
            Self::ConnectorStroke => "connector-stroke",
            Self::ConnectorStrokeArrow | Self::ConnectorStrokeBoth => "connector-stroke-arrow",
        }
    }
    pub fn is_connector(self) -> bool {
        matches!(
            self,
            Self::ConnectorStraight
                | Self::ConnectorStraightArrow
                | Self::ConnectorStraightBoth
                | Self::ConnectorStroke
                | Self::ConnectorStrokeArrow
                | Self::ConnectorStrokeBoth
        )
    }
    /// Straight lines and connectors share the line geometry (start/end, flips).
    pub fn is_line(self) -> bool {
        self == Self::Line || self.is_connector()
    }
    /// Arrow heads: (start, end).
    pub fn arrows(self) -> (bool, bool) {
        match self {
            Self::ConnectorStraightArrow | Self::ConnectorStrokeArrow => (false, true),
            Self::ConnectorStraightBoth | Self::ConnectorStrokeBoth => (true, true),
            _ => (false, false),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeOptions {
    pub kind: ShapeKind,
    pub width_mm: f64,
    pub height_mm: f64,
    pub text: String,
}
fn shape_label(s: &ShapeObject) -> &'static str {
    match s {
        ShapeObject::Line(_) => "선",
        ShapeObject::Rectangle(_) => "사각형",
        ShapeObject::Ellipse(_) => "타원",
        ShapeObject::Arc(_) => "호",
        ShapeObject::Polygon(_) => "다각형",
        ShapeObject::Curve(_) => "곡선",
        ShapeObject::Group(_) => "묶음 개체",
        ShapeObject::Picture(_) => "그림",
        _ => "도형",
    }
}
fn plain_text(text: &str) -> Result<()> {
    if text.len() > 10_000 || text.chars().any(char::is_control) {
        bail!("제어 문자가 없는 한 줄, 10 KB 이하의 텍스트를 입력하세요.");
    }
    Ok(())
}
pub(crate) fn equation_size(p: &EquationOptions) -> Result<u32> {
    plain_text(&p.script)?;
    if p.script.trim().is_empty() || !p.size_pt.is_finite() || !(4.0..=100.0).contains(&p.size_pt) {
        bail!("수식과 4–100 pt 글자 크기를 입력하세요.");
    }
    // Bound recursive equation parsing, including hostile imported scripts edited through the UI.
    let mut depth = 0u32;
    for ch in p.script.chars() {
        match ch {
            '{' | '(' | '[' => {
                depth += 1;
                if depth > 32 {
                    bail!("수식 중첩은 32단계 이하입니다.");
                }
            }
            '}' | ')' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    if p.script.chars().count() > 2000 {
        bail!("수식은 2,000자 이하입니다.");
    }
    Ok((p.size_pt * 100.0).round() as u32)
}
fn dimension(mm: f64) -> Result<u32> {
    if !mm.is_finite() || !(1.0..=300.0).contains(&mm) {
        bail!("개체 크기는 1–300 mm입니다.");
    }
    Ok((mm * 7200.0 / 25.4).round() as u32)
}
pub(crate) fn inserted_address(section: usize, response: &str) -> Result<ObjectAddress> {
    let v: Value = serde_json::from_str(response)?;
    let index = |key: &str| {
        v[key]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| anyhow!("개체 주소 누락: {key}"))
    };
    if v["ok"] != true {
        bail!("개체 삽입 실패: {v}");
    }
    Ok(ObjectAddress {
        section,
        paragraph: index("paraIdx")?,
        control: index("controlIdx")?,
    })
}
impl Session {
    // Controls occupy UTF-16 gaps, while this editor's cursor counts scalars.
    // A trailing space supplies an unambiguous position after the new object.
    pub(crate) fn reserve_object_position(&mut self) -> Result<Cursor> {
        let c = self.cursor;
        self.core
            .insert_text_native(c.section, c.paragraph, c.offset, " ")
            .map_err(err)?;
        self.cursor.offset += 1;
        self.anchor = None;
        Ok(c)
    }
    pub fn equation_preview(&self, options: &EquationOptions) -> Result<String> {
        let size = equation_size(options)?;
        self.core
            .render_equation_preview_native(&options.script, size, 0)
            .map_err(err)
    }
    pub(crate) fn body_insert(&self) -> Result<()> {
        if self.cursor.cell.is_some() {
            bail!("이 개체는 본문에 삽입하세요.");
        }
        if self.has_selection() {
            bail!("선택을 해제하고 개체를 넣을 위치에 커서를 놓으세요.");
        }
        self.paragraph(self.cursor)?;
        Ok(())
    }
    pub(crate) fn object(&self, at: ObjectAddress) -> Result<&Control> {
        self.core
            .document()
            .sections
            .get(at.section)
            .and_then(|s| s.paragraphs.get(at.paragraph))
            .and_then(|p| p.controls.get(at.control))
            .ok_or_else(|| anyhow!("개체를 찾지 못했습니다."))
    }
    pub fn objects(&self) -> Value {
        let mut list = Vec::new();
        for (si, s) in self.core.document().sections.iter().enumerate() {
            for (pi, p) in s.paragraphs.iter().enumerate() {
                for (ci, c) in p.controls.iter().enumerate() {
                    let (kind, label) = match c {
                        Control::Equation(e) => ("equation", e.script.clone()),
                        Control::Footnote(n) => (
                            "footnote",
                            n.paragraphs
                                .iter()
                                .map(|p| p.text.as_str())
                                .collect::<Vec<_>>()
                                .join(" / "),
                        ),
                        Control::Endnote(n) => (
                            "endnote",
                            n.paragraphs
                                .iter()
                                .map(|p| p.text.as_str())
                                .collect::<Vec<_>>()
                                .join(" / "),
                        ),
                        Control::Picture(p) => (
                            "picture",
                            if p.common.description.is_empty() {
                                "그림".into()
                            } else {
                                p.common.description.clone()
                            },
                        ),
                        Control::Table(t) => {
                            ("table", format!("표 {}×{}", t.row_count, t.col_count))
                        }
                        Control::Shape(s) => {
                            if let Some(t) = s.drawing().and_then(|d| d.text_box.as_ref()) {
                                (
                                    "textbox",
                                    t.paragraphs
                                        .iter()
                                        .map(|p| p.text.as_str())
                                        .collect::<Vec<_>>()
                                        .join(" / "),
                                )
                            } else if matches!(s.as_ref(), ShapeObject::Group(_)) {
                                ("group", shape_label(s).into())
                            } else {
                                ("shape", shape_label(s).into())
                            }
                        }
                        Control::Form(f) => (
                            "form",
                            format!(
                                "{} {}",
                                crate::ext_object::form_type_name(f.form_type),
                                f.name
                            ),
                        ),
                        _ => continue,
                    };
                    list.push(json!({"address":ObjectAddress{section:si,paragraph:pi,control:ci},"kind":kind,"label":label.chars().take(80).collect::<String>()}));
                }
            }
        }
        json!(list)
    }
    pub fn equation(&self, at: ObjectAddress) -> Result<EquationOptions> {
        match self.object(at)? {
            Control::Equation(e) => Ok(EquationOptions {
                script: e.script.clone(),
                size_pt: e.font_size as f64 / 100.0,
            }),
            _ => bail!("수식 개체가 아닙니다."),
        }
    }
    pub fn set_equation(&mut self, at: Option<ObjectAddress>, p: EquationOptions) -> Result<()> {
        // Editing keeps the equation's current color.
        let color = match at.map(|a| self.object(a)).transpose()? {
            Some(Control::Equation(e)) => e.color,
            _ => 0,
        };
        self.put_equation(at, p, color).map(|_| ())
    }
    /// Insert (or edit) an equation; returns the address of the equation.
    pub(crate) fn put_equation(
        &mut self,
        at: Option<ObjectAddress>,
        p: EquationOptions,
        color: u32,
    ) -> Result<ObjectAddress> {
        let size = equation_size(&p)?;
        if let Some(at) = at {
            self.equation(at)?;
        } else {
            self.body_insert()?;
        }
        let mut address = at;
        self.transaction(|s| {
            if let Some(a) = at {
                s.core
                    .set_equation_properties_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        None,
                        None,
                        &json!({"script":p.script,"fontSize":size,"color":color}).to_string(),
                    )
                    .map_err(err)?;
            } else {
                let c = s.reserve_object_position()?;
                let response = s
                    .core
                    .insert_equation_native(
                        c.section,
                        c.paragraph,
                        c.offset,
                        &p.script,
                        size,
                        color,
                    )
                    .map_err(err)?;
                address = Some(inserted_address(c.section, &response)?);
            }
            Ok(())
        })?;
        address.ok_or_else(|| anyhow!("수식을 넣지 못했습니다."))
    }
    pub fn insert_note(&mut self, endnote: bool, text: &str) -> Result<ObjectAddress> {
        self.body_insert()?;
        plain_text(text)?;
        let mut inserted = None;
        // The note and its text are two native edits; lay the section out once.
        self.transaction(|s| {
            s.deferred_layout(|s| {
                inserted = Some(s.insert_note_raw(endnote, text)?);
                Ok(())
            })
        })?;
        inserted.ok_or_else(|| anyhow!("주석을 넣지 못했습니다."))
    }
    /// Insert at the caret without a transaction; callers own the undo step.
    pub(crate) fn insert_note_raw(&mut self, endnote: bool, text: &str) -> Result<ObjectAddress> {
        {
            let s = self;
            let c = s.reserve_object_position()?;
            let response = if endnote {
                s.core
                    .insert_endnote_native(c.section, c.paragraph, c.offset)
            } else {
                s.core
                    .insert_footnote_native(c.section, c.paragraph, c.offset)
            }
            .map_err(err)?;
            let a = inserted_address(c.section, &response)?;
            let offset = match s.object(a)? {
                Control::Footnote(n) => n.paragraphs.first(),
                Control::Endnote(n) => n.paragraphs.first(),
                _ => bail!("주석 삽입 결과 오류"),
            }
            .ok_or_else(|| anyhow!("주석 문단이 없습니다."))?
            .text
            .chars()
            .count();
            s.core
                .insert_text_in_footnote_native(a.section, a.paragraph, a.control, 0, offset, text)
                .map_err(err)?;
            Ok(a)
        }
    }
    /// Only replace the editable suffix after the original automatic note number.
    fn note_suffix(&self, at: ObjectAddress) -> Result<&str> {
        let paragraphs = match self.object(at)? {
            Control::Footnote(n) => &n.paragraphs,
            Control::Endnote(n) => &n.paragraphs,
            _ => bail!("각주/미주가 아닙니다."),
        };
        if paragraphs.len() != 1 {
            bail!("여러 문단이 있는 주석은 단순 텍스트 편집을 지원하지 않습니다.");
        }
        let p = &paragraphs[0];
        if p.controls.len() != 1
            || !matches!(p.controls[0], Control::AutoNumber(_))
            || !p.field_ranges.is_empty()
            || !p.text.starts_with("  ")
        {
            bail!("번호·필드·개체가 있는 복잡한 주석은 단순 대체하지 않습니다.");
        }
        Ok(&p.text[2..])
    }
    pub fn note_text(&self, at: ObjectAddress) -> Result<String> {
        Ok(self.note_suffix(at)?.to_owned())
    }
    pub fn set_note_text(&mut self, at: ObjectAddress, text: &str) -> Result<()> {
        plain_text(text)?;
        let count = self.note_suffix(at)?.chars().count();
        self.transaction(|s| {
            s.core
                .delete_text_in_footnote_native(at.section, at.paragraph, at.control, 0, 2, count)
                .map_err(err)?;
            s.core
                .insert_text_in_footnote_native(at.section, at.paragraph, at.control, 0, 2, text)
                .map_err(err)?;
            Ok(())
        })
    }
    pub fn insert_shape(&mut self, p: ShapeOptions) -> Result<()> {
        self.body_insert()?;
        let width = dimension(p.width_mm)?;
        let height = dimension(p.height_mm)?;
        plain_text(&p.text)?;
        if p.kind != ShapeKind::Textbox && !p.text.is_empty() {
            bail!("텍스트는 글상자에만 넣을 수 있습니다.");
        }
        let points = if p.kind == ShapeKind::Polygon {
            crate::ext_object::default_polygon(width, height)
        } else {
            Vec::new()
        };
        self.transaction(|s| {
            let c = s.reserve_object_position()?;
            let response = s
                .core
                .create_shape_control_native(
                    c.section,
                    c.paragraph,
                    c.offset,
                    width,
                    height,
                    0,
                    0,
                    true,
                    "TopAndBottom",
                    p.kind.name(),
                    false,
                    false,
                    &points,
                )
                .map_err(err)?;
            let a = inserted_address(c.section, &response)?;
            if p.kind == ShapeKind::Curve {
                s.polygon_to_curve(a, false)?;
            }
            if !p.text.is_empty() {
                s.core
                    .insert_text_in_cell_native(a.section, a.paragraph, a.control, 0, 0, 0, &p.text)
                    .map_err(err)?;
            }
            Ok(())
        })
    }
    /// Replace a freshly created polygon by a curve shape with the same frame.
    /// `keep_points`: use the polygon's own points (freehand drawing) as an open curve.
    pub(crate) fn polygon_to_curve(&mut self, at: ObjectAddress, keep_points: bool) -> Result<()> {
        use hwpcraft_doc::model::{Point, shape::CurveShape};
        let slot = self
            .core
            .document_mut()
            .sections
            .get_mut(at.section)
            .and_then(|s| s.paragraphs.get_mut(at.paragraph))
            .and_then(|p| p.controls.get_mut(at.control))
            .ok_or_else(|| anyhow!("곡선으로 바꿀 도형이 없습니다."))?;
        let Control::Shape(shape) = slot else {
            bail!("곡선으로 바꿀 도형이 아닙니다.");
        };
        let ShapeObject::Polygon(poly) = shape.as_ref() else {
            bail!("다각형만 곡선으로 바꿀 수 있습니다.");
        };
        let (w, h) = (poly.common.width as i32, poly.common.height as i32);
        // HWPX `hp:seg type="CURVE"` is not the HWP5 Bézier segment (RHWP #4676),
        // so a smooth wave is sampled into short line segments that every format,
        // reader and RHWP renderer interpret the same way.
        let points: Vec<Point> = if keep_points {
            poly.points.clone()
        } else {
            (0..=32)
                .map(|i| {
                    let t = f64::from(i) / 32.0;
                    Point {
                        x: (f64::from(w) * t).round() as i32,
                        y: (f64::from(h) * (0.5 - 0.5 * (std::f64::consts::TAU * t).sin())).round()
                            as i32,
                    }
                })
                .collect()
        };
        let mut common = poly.common.clone();
        common.description = "곡선입니다.".into();
        **shape = ShapeObject::Curve(CurveShape {
            common,
            drawing: poly.drawing.clone(),
            segment_types: vec![0; points.len() - 1],
            points,
        });
        // Re-run layout through the property path with an empty change set.
        self.core
            .set_shape_properties_native(at.section, at.paragraph, at.control, "{}")
            .map_err(err)?;
        Ok(())
    }
    pub fn shape_size(&self, at: ObjectAddress) -> Result<(f64, f64)> {
        match self.object(at)? {
            Control::Shape(s) => Ok((
                s.common().width as f64 * 25.4 / 7200.0,
                s.common().height as f64 * 25.4 / 7200.0,
            )),
            _ => bail!("도형/글상자가 아닙니다."),
        }
    }
    pub fn resize_shape(&mut self, at: ObjectAddress, width_mm: f64, height_mm: f64) -> Result<()> {
        self.shape_size(at)?;
        let w = dimension(width_mm)?;
        let h = dimension(height_mm)?;
        self.transaction(|s| {
            s.core
                .set_shape_properties_native(
                    at.section,
                    at.paragraph,
                    at.control,
                    &json!({"width":w,"height":h}).to_string(),
                )
                .map_err(err)?;
            Ok(())
        })
    }
    pub fn textbox_text(&self, at: ObjectAddress) -> Result<String> {
        let Control::Shape(shape) = self.object(at)? else {
            bail!("글상자가 아닙니다.");
        };
        let tb = shape
            .drawing()
            .and_then(|d| d.text_box.as_ref())
            .ok_or_else(|| anyhow!("글상자가 아닙니다."))?;
        if tb.paragraphs.len() != 1
            || tb
                .paragraphs
                .iter()
                .any(|p| !p.controls.is_empty() || !p.field_ranges.is_empty())
        {
            bail!("복잡한 글상자는 단순 텍스트 대체를 지원하지 않습니다.");
        }
        Ok(tb.paragraphs[0].text.clone())
    }
    pub fn set_textbox_text(&mut self, at: ObjectAddress, text: &str) -> Result<()> {
        plain_text(text)?;
        let count = self.textbox_text(at)?.chars().count();
        self.transaction(|s| {
            s.core
                .delete_text_in_cell_native(at.section, at.paragraph, at.control, 0, 0, 0, count)
                .map_err(err)?;
            s.core
                .insert_text_in_cell_native(at.section, at.paragraph, at.control, 0, 0, 0, text)
                .map_err(err)?;
            Ok(())
        })
    }
    pub fn delete_object(&mut self, at: ObjectAddress) -> Result<()> {
        let kind = match self.object(at)? {
            Control::Equation(_) => 0,
            Control::Shape(_) => 1,
            Control::Footnote(_) | Control::Endnote(_) => 2,
            Control::Picture(_) => 3,
            Control::Table(_) => 4,
            _ => bail!("이 개체 삭제는 지원하지 않습니다."),
        };
        self.guard_object_lock(at)?;
        self.transaction(|s| {
            match kind {
                0 => {
                    s.core
                        .delete_equation_control_native(at.section, at.paragraph, at.control)
                        .map_err(err)?;
                }
                1 => {
                    s.core
                        .delete_shape_control_native(at.section, at.paragraph, at.control)
                        .map_err(err)?;
                }
                2 => {
                    s.core
                        .delete_footnote_native(at.section, at.paragraph, at.control)
                        .map_err(err)?;
                }
                3 => {
                    s.core
                        .delete_picture_control_native(at.section, at.paragraph, at.control)
                        .map_err(err)?;
                }
                _ => {
                    s.core
                        .delete_table_control_native(at.section, at.paragraph, at.control)
                        .map_err(err)?;
                }
            }
            let paragraphs = s
                .core
                .document()
                .sections
                .get(at.section)
                .map_or(0, |x| x.paragraphs.len());
            s.cursor = Cursor {
                section: at.section,
                paragraph: at.paragraph.min(paragraphs.saturating_sub(1)),
                offset: 0,
                cell: None,
            };
            s.clear_selection();
            Ok(())
        })
    }
    pub fn styles(&self) -> Value {
        json!(
            self.core
                .document()
                .doc_info
                .styles
                .iter()
                .enumerate()
                .map(|(id, s)| json!({"id":id,"name":s.local_name,"character":s.style_type==1}))
                .collect::<Vec<_>>()
        )
    }
    pub fn apply_style(&mut self, id: usize) -> Result<()> {
        if id > 255 || id >= self.core.document().doc_info.styles.len() {
            bail!("스타일을 찾지 못했습니다.");
        }
        self.transaction(|s| {
            let c = s.cursor;
            if let Some(cell) = c.cell {
                s.core
                    .apply_cell_style_native(
                        c.section,
                        c.paragraph,
                        cell.control,
                        cell.index,
                        cell.paragraph,
                        id,
                    )
                    .map_err(err)?;
            } else {
                s.core
                    .apply_style_native(c.section, c.paragraph, id)
                    .map_err(err)?;
            }
            Ok(())
        })
    }
}
