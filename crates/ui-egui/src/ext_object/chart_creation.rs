//! Desktop authoring for the eleven editable chart kinds. Engine commands own all
//! serialization, validation, resource allocation, and undo transactions.
use super::*;
use anyhow::{Context, Result, bail};
use hwpcraft_engine::Cursor;

const KINDS: &[(&str, &str)] = &[
    ("column", "세로 막대"),
    ("bar", "가로 막대"),
    ("line", "꺾은선"),
    ("pie", "원형"),
    ("doughnut", "도넛"),
    ("scatter", "분산형"),
    ("stock_hlc", "주가: 고가·저가·종가"),
    ("stock_ohlc", "주가: 시가·고가·저가·종가"),
    ("column_3d", "입체 세로 막대"),
    ("bar_3d", "입체 가로 막대"),
    ("pie_3d", "입체 원형"),
];
const SCATTER_STYLES: &[(&str, &str)] = &[
    ("marker", "표식"),
    ("line", "직선"),
    ("line_marker", "직선과 표식"),
    ("smooth_marker", "곡선과 표식"),
];
const GROUPINGS: &[(&str, &str)] = &[
    ("clustered", "묶은"),
    ("stacked", "누적"),
    ("percent_stacked", "100% 누적"),
];
/// Kinds whose plot takes a grouping.
fn groups(kind: &str) -> bool {
    matches!(kind, "column" | "bar" | "line")
}
fn stock_roles(kind: &str) -> &'static [&'static str] {
    match kind {
        "stock_hlc" => &["고가", "저가", "종가"],
        "stock_ohlc" => &["시가", "고가", "저가", "종가"],
        _ => &[],
    }
}
fn family(kind: &str) -> &str {
    if matches!(kind, "scatter" | "stock_hlc" | "stock_ohlc") {
        kind
    } else {
        "category"
    }
}

#[derive(Clone)]
struct SeriesDraft {
    name: String,
    values: Vec<String>,
    color: Option<String>,
}

#[derive(Clone)]
struct Draft {
    kind: String,
    title: String,
    categories: Vec<String>,
    series: Vec<SeriesDraft>,
    width_mm: f64,
    height_mm: f64,
    legend: bool,
    x_values: Vec<String>,
    scatter_style: String,
    grouping: String,
    // UI-only backups preserve all draft input when switching data families.
    saved_data: std::collections::BTreeMap<String, DataDraft>,
}

#[derive(Clone)]
struct DataDraft {
    categories: Vec<String>,
    series: Vec<SeriesDraft>,
    x_values: Vec<String>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            kind: "column".into(),
            title: "차트 제목".into(),
            categories: vec!["항목 1".into(), "항목 2".into(), "항목 3".into()],
            series: vec![SeriesDraft {
                name: "계열 1".into(),
                values: vec!["10".into(), "20".into(), "15".into()],
                color: None,
            }],
            width_mm: 120.0,
            height_mm: 75.0,
            legend: true,
            x_values: vec!["1".into(), "2".into(), "3".into()],
            scatter_style: "marker".into(),
            grouping: "clustered".into(),
            saved_data: Default::default(),
        }
    }
}

impl Draft {
    fn from_spec(spec: &Value) -> Result<Self> {
        let text = |key| {
            spec[key]
                .as_str()
                .map(str::to_owned)
                .with_context(|| format!("차트 {key} 정보가 없습니다."))
        };
        let kind = text("kind")?;
        if !KINDS.iter().any(|(id, _)| *id == kind) {
            bail!("이 화면에서 지원하지 않는 차트 종류입니다. 기존 데이터 편집을 사용하세요.");
        }
        // Do not discard future engine fields when round-tripping a definition.
        let keys = [
            "kind",
            "title",
            "categories",
            "series",
            "width_mm",
            "height_mm",
            "legend",
            "scatter_style",
            "grouping",
        ];
        if spec
            .as_object()
            .context("차트 정의가 객체가 아닙니다.")?
            .keys()
            .any(|key| !keys.contains(&key.as_str()))
        {
            bail!(
                "이 화면에서 보존할 수 없는 차트 설정이 있습니다. 기존 데이터 편집을 사용하세요."
            );
        }
        let categories = spec["categories"]
            .as_array()
            .context("차트 범주가 없습니다.")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .context("범주는 문자열이어야 합니다.")
            })
            .collect::<Result<Vec<_>>>()?;
        let series = spec["series"]
            .as_array()
            .context("차트 계열이 없습니다.")?
            .iter()
            .map(|s| {
                if s.as_object()
                    .context("계열 형식 오류")?
                    .keys()
                    .any(|key| !["name", "values", "color", "x_values"].contains(&key.as_str()))
                {
                    bail!("이 화면에서 보존할 수 없는 계열 설정이 있습니다.");
                }
                Ok(SeriesDraft {
                    name: s["name"].as_str().context("계열 이름 없음")?.into(),
                    values: s["values"]
                        .as_array()
                        .context("계열 값 없음")?
                        .iter()
                        .map(|v| {
                            v.as_f64()
                                .map(|n| n.to_string())
                                .context("숫자가 아닌 계열 값")
                        })
                        .collect::<Result<Vec<_>>>()?,
                    color: s
                        .get("color")
                        .filter(|v| !v.is_null())
                        .map(|v| v.as_str().map(str::to_owned).context("계열 색 형식 오류"))
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let scatter = kind == "scatter";
        if scatter && !categories.is_empty() {
            bail!("분산형의 비어 있지 않은 범주를 삭제하지 않고 전체 구성을 거절합니다.");
        }
        let mut x_values = None;
        for s in spec["series"].as_array().context("계열 없음")? {
            if scatter {
                let values = s["x_values"]
                    .as_array()
                    .context("분산형 X 좌표 없음")?
                    .iter()
                    .map(|v| v.as_f64().context("숫자가 아닌 X 좌표"))
                    .collect::<Result<Vec<_>>>()?;
                if x_values
                    .as_ref()
                    .is_some_and(|previous| *previous != values)
                {
                    bail!("계열별 X 좌표가 다릅니다. 이 화면은 공통 X 좌표만 지원합니다.");
                }
                x_values = Some(values);
            } else if s.get("x_values").is_some_and(|v| !v.is_null()) {
                bail!(
                    "분산형이 아닌 차트에 X 좌표 설정이 있습니다. 삭제하지 않고 전체 구성을 거절합니다."
                );
            }
        }
        let scatter_style = if scatter {
            spec.get("scatter_style")
                .filter(|v| !v.is_null())
                .map(|v| v.as_str().context("분산형 표시 방식 오류"))
                .transpose()?
                .unwrap_or("marker")
                .to_owned()
        } else {
            if spec.get("scatter_style").is_some_and(|v| !v.is_null()) {
                bail!("분산형이 아닌 차트의 분산형 설정을 삭제하지 않고 전체 구성을 거절합니다.");
            }
            "marker".into()
        };
        let grouping = spec
            .get("grouping")
            .filter(|v| !v.is_null())
            .map(|v| v.as_str().context("묶음 방식 오류"))
            .transpose()?
            .unwrap_or("clustered")
            .to_owned();
        let draft = Self {
            kind,
            title: text("title")?,
            categories,
            series,
            width_mm: spec["width_mm"].as_f64().context("차트 너비 없음")?,
            height_mm: spec["height_mm"].as_f64().context("차트 높이 없음")?,
            legend: spec["legend"].as_bool().context("차트 범례 설정 없음")?,
            x_values: x_values
                .unwrap_or_default()
                .iter()
                .map(ToString::to_string)
                .collect(),
            scatter_style,
            grouping,
            saved_data: Default::default(),
        };
        draft.spec()?;
        Ok(draft)
    }

    fn spec(&self) -> Result<Value> {
        if !KINDS.iter().any(|(kind, _)| *kind == self.kind) {
            bail!("전체 구성에서 지원하지 않는 차트 종류입니다.");
        }
        let scatter = self.kind == "scatter";
        let points = self.points();
        if !(1..=1000).contains(&points)
            || !(1..=64).contains(&self.series.len())
            || points.saturating_mul(self.series.len()) > 16_000
        {
            bail!("범주 1–1000개, 계열 1–64개, 전체 값 16000개 이하로 입력하세요.");
        }
        if !self.width_mm.is_finite()
            || !self.height_mm.is_finite()
            || !(10.0..=300.0).contains(&self.width_mm)
            || !(10.0..=300.0).contains(&self.height_mm)
        {
            bail!("차트 너비와 높이는 10–300 mm로 입력하세요.");
        }
        let mut names = std::collections::BTreeSet::new();
        let mut series = Vec::new();
        let parse_number = |value: &str, row: usize, name: &str| -> Result<f64> {
            let number: f64 = value
                .trim()
                .parse()
                .with_context(|| format!("{name} / {}행: 숫자를 입력하세요.", row + 1))?;
            if !number.is_finite() || number.abs() > 1.0e12 {
                bail!(
                    "{name} / {}행: 유한한 ±1e12 범위의 숫자를 입력하세요.",
                    row + 1
                );
            }
            Ok(number)
        };
        let x_values = if scatter {
            if !SCATTER_STYLES
                .iter()
                .any(|(style, _)| *style == self.scatter_style)
            {
                bail!("지원하지 않는 분산형 표시 방식입니다.");
            }
            self.x_values
                .iter()
                .enumerate()
                .map(|(row, value)| parse_number(value, row, "공통 X"))
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        for draft in &self.series {
            if draft.name.trim().is_empty() || !names.insert(&draft.name) {
                bail!("계열 이름은 비어 있지 않고 서로 달라야 합니다.");
            }
            if draft.values.len() != points {
                bail!("각 계열의 값 개수와 범주 개수가 다릅니다.");
            }
            let values = draft
                .values
                .iter()
                .enumerate()
                .map(|(row, value)| parse_number(value, row, &draft.name))
                .collect::<Result<Vec<_>>>()?;
            if matches!(self.kind.as_str(), "pie" | "doughnut" | "pie_3d")
                && (self.series.len() != 1
                    || values.iter().any(|v| *v < 0.0)
                    || !values.iter().any(|v| *v > 0.0))
            {
                bail!(
                    "원형·도넛은 계열 1개, 음수가 없는 값, 양수 합계가 필요합니다. 초안의 다른 계열은 직접 제거하세요."
                );
            }
            let mut item = json!({"name":draft.name,"values":values});
            if scatter {
                item["x_values"] = json!(x_values);
            }
            if let Some(color) = &draft.color {
                item["color"] = json!(color);
            }
            series.push(item);
        }
        let roles = stock_roles(&self.kind);
        if !roles.is_empty() {
            if series.len() != roles.len() {
                bail!(
                    "주가형은 역할 순서대로 {}개 계열이 필요합니다.",
                    roles.len()
                );
            }
            let high = roles.len() - 3;
            let low = roles.len() - 2;
            let close = roles.len() - 1;
            for (row, _) in series[high]["values"]
                .as_array()
                .context("주가 고가 배열 없음")?
                .iter()
                .enumerate()
            {
                let number = |column: usize| {
                    series[column]["values"][row]
                        .as_f64()
                        .context("주가 숫자 없음")
                };
                let h = number(high)?;
                let l = number(low)?;
                let c = number(close)?;
                if h < l
                    || !(l..=h).contains(&c)
                    || (roles.len() == 4 && !(l..=h).contains(&number(0)?))
                {
                    bail!(
                        "{}행: 고가는 저가 이상, 시가·종가는 저가와 고가 사이여야 합니다.",
                        row + 1
                    );
                }
            }
        }
        let mut spec = json!({"kind":self.kind,"title":self.title,
            "categories":if scatter {Vec::new()} else {self.categories.clone()},
            "series":series,"width_mm":self.width_mm,"height_mm":self.height_mm,"legend":self.legend});
        if scatter {
            spec["scatter_style"] = json!(self.scatter_style);
        }
        if !GROUPINGS.iter().any(|(id, _)| *id == self.grouping) {
            bail!("지원하지 않는 묶음 방식입니다.");
        }
        if groups(&self.kind) && self.grouping != "clustered" {
            spec["grouping"] = json!(self.grouping);
        }
        Ok(spec)
    }

    fn points(&self) -> usize {
        if self.kind == "scatter" {
            self.x_values.len()
        } else {
            self.categories.len()
        }
    }

    fn select_kind(&mut self, kind: &str) {
        let old = family(&self.kind).to_owned();
        let new = family(kind).to_owned();
        if old != new {
            self.saved_data.insert(
                old,
                DataDraft {
                    categories: self.categories.clone(),
                    series: self.series.clone(),
                    x_values: self.x_values.clone(),
                },
            );
            let next = self.saved_data.remove(&new).unwrap_or_else(|| {
                let points = self.points();
                let categories = (0..points)
                    .map(|row| {
                        self.categories
                            .get(row)
                            .cloned()
                            .unwrap_or_else(|| format!("항목 {}", row + 1))
                    })
                    .collect();
                let roles = stock_roles(kind);
                let series = if roles.is_empty() {
                    self.series.clone()
                } else {
                    roles
                        .iter()
                        .map(|role| SeriesDraft {
                            name: (*role).into(),
                            values: vec![String::new(); points],
                            color: None,
                        })
                        .collect()
                };
                DataDraft {
                    categories,
                    series,
                    x_values: (1..=points).map(|n| n.to_string()).collect(),
                }
            });
            self.categories = next.categories;
            self.series = next.series;
            self.x_values = next.x_values;
        }
        self.kind = kind.into();
    }

    fn add_row(&mut self) {
        self.x_values.push((self.points() + 1).to_string());
        self.categories
            .push(format!("항목 {}", self.categories.len() + 1));
        for series in &mut self.series {
            series.values.push("0".into());
        }
    }
    fn add_series(&mut self) {
        let mut number = self.series.len() + 1;
        while self
            .series
            .iter()
            .any(|s| s.name == format!("계열 {number}"))
        {
            number += 1;
        }
        self.series.push(SeriesDraft {
            name: format!("계열 {number}"),
            values: vec!["0".into(); self.points()],
            color: None,
        });
    }
}

#[derive(Clone)]
struct Capture {
    generation: u64,
    cursor: Cursor,
    anchor: Option<usize>,
    span_anchor: Option<Cursor>,
    target: Option<(usize, Value, u64)>,
}

pub(super) struct Editor {
    draft: Draft,
    available_kinds: Vec<String>,
    capture: Capture,
    error: Option<String>,
    preview: Option<(egui::TextureHandle, Vec2)>,
    preview_stale: bool,
    /// Live preview: set when the form changes; regenerated after a short pause.
    changed: bool,
    edited_at: f64,
    preview_error: Option<String>,
    last_spec: Option<Value>,
    #[cfg(test)]
    regions: std::collections::BTreeMap<&'static str, (Rect, Rect, egui::Id)>,
}

pub(super) fn can_configure(session: &mut Session, index: usize) -> bool {
    session
        .run("chart.definition.get", json!({"index":index}))
        .and_then(|definition| Draft::from_spec(&definition["chart"]).map(|_| ()))
        .is_ok()
}

pub(super) fn apply_legacy_data(session: &mut Session, editor: &ChartEditor) -> Result<Value> {
    if editor.names.len() != editor.values.len() {
        bail!("차트 이름과 계열 수가 다릅니다.");
    }
    let series: Vec<Value> = editor
        .names
        .iter()
        .zip(&editor.values)
        .map(|(name, values)| json!({"name":name,"values":values}))
        .collect();
    session.run(
        "chart.data",
        json!({"index":editor.index,"structure":true,"series":series}),
    )
}

impl Editor {
    fn open(session: &mut Session, index: Option<usize>) -> Result<Self> {
        let discovery = session.run("chart.types", json!({}))?;
        let available_kinds = discovery["types"]
            .as_array()
            .context("차트 종류 목록이 없습니다.")?
            .iter()
            .filter_map(|item| item["kind"].as_str())
            .filter(|kind| KINDS.iter().any(|(id, _)| id == kind))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if available_kinds.is_empty() {
            bail!("엔진에서 작성 가능한 차트 종류를 찾지 못했습니다.");
        }
        let (draft, target) = if let Some(index) = index {
            let definition = session.run("chart.definition.get", json!({"index":index}))?;
            let draft = Draft::from_spec(&definition["chart"])?;
            let instance = definition["instance_id"]
                .as_u64()
                .context("차트 식별자가 없습니다.")?;
            let address = definition
                .get("address")
                .filter(|v| v.is_object())
                .context("차트 주소 없음")?
                .clone();
            if definition["generation"].as_u64() != Some(session.generation) {
                bail!("차트 정의를 읽는 동안 문서가 변경됐습니다.");
            }
            (draft, Some((index, address, instance)))
        } else {
            (Draft::default(), None)
        };
        if !available_kinds.contains(&draft.kind) {
            bail!("현재 엔진에서 작성할 수 없는 차트 종류입니다.");
        }
        Ok(Self {
            draft,
            available_kinds,
            capture: Capture {
                generation: session.generation,
                cursor: session.cursor,
                anchor: session.anchor,
                span_anchor: session.span_anchor,
                target,
            },
            error: None,
            preview: None,
            preview_stale: true,
            changed: true,
            edited_at: f64::NEG_INFINITY,
            preview_error: None,
            last_spec: None,
            #[cfg(test)]
            regions: Default::default(),
        })
    }

    fn apply(&self, session: &mut Session) -> Result<Value> {
        if session.generation != self.capture.generation {
            bail!(
                "창을 연 뒤 문서가 변경됐습니다. 입력은 보존했습니다. 현재 문서 기준으로 갱신한 뒤 다시 적용하세요."
            );
        }
        let chart = self.draft.spec()?;
        if let Some((index, address, instance)) = &self.capture.target {
            let current = session.run("chart.definition.get", json!({"index":index}))?;
            if current["address"] != *address || current["instance_id"].as_u64() != Some(*instance)
            {
                bail!("원래 차트의 식별 정보가 달라졌습니다. 다른 차트에 적용하지 않았습니다.");
            }
            session.run(
                "chart.configure",
                json!({"index":index,"chart":chart,
                "expected_generation":self.capture.generation}),
            )
        } else {
            if session.cursor != self.capture.cursor
                || session.anchor != self.capture.anchor
                || session.span_anchor != self.capture.span_anchor
            {
                bail!(
                    "차트를 넣을 위치나 선택 범위가 변경됐습니다. 현재 위치로 갱신한 뒤 다시 적용하세요."
                );
            }
            // Creation must not replace an unrelated selected text fragment.
            if !session.selection().is_empty() || session.span_anchor.is_some() {
                bail!("본문의 선택을 해제한 뒤 차트를 넣으세요.");
            }
            session.run(
                "chart.create",
                json!({"chart":chart,
                "expected_generation":self.capture.generation}),
            )
        }
    }

    fn refresh_capture(&mut self, session: &mut Session) -> Result<()> {
        if let Some((index, address, instance)) = &self.capture.target {
            let current = session.run("chart.definition.get", json!({"index":index}))?;
            if current["address"] != *address || current["instance_id"].as_u64() != Some(*instance)
            {
                bail!(
                    "원래 차트가 이동·삭제됐거나 다른 차트입니다. 입력은 보존했습니다. 차트 편집창에서 다시 여세요."
                );
            }
        }
        self.capture.generation = session.generation;
        self.capture.cursor = session.cursor;
        self.capture.anchor = session.anchor;
        self.capture.span_anchor = session.span_anchor;
        Ok(())
    }

    /// Uses an isolated real engine document, never edits or serializes the live document.
    fn preview_page(&self) -> Result<(String, [f64; 4])> {
        let chart = self.draft.spec()?;
        let mut preview = Session::new()?;
        let mut setup = preview.page_setup()?;
        setup.width_mm = (self.draft.width_mm + 30.0).max(80.0);
        setup.height_mm = (self.draft.height_mm + 40.0).max(80.0);
        setup.landscape = false;
        setup.left_mm = 10.0;
        setup.right_mm = 10.0;
        setup.top_mm = 10.0;
        setup.bottom_mm = 10.0;
        setup.header_mm = 0.0;
        setup.footer_mm = 0.0;
        setup.gutter_mm = 0.0;
        preview.execute(Command::PageSetup(setup))?;
        let result = preview.run(
            "chart.create",
            json!({"chart":chart,"expected_generation":preview.generation}),
        )?;
        let address = &result["address"];
        let tree = preview
            .core
            .build_page_layer_tree_with_profile(0, hwpcraft_doc::RenderProfile::Screen)
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        fn bounds(node: &hwpcraft_doc::LayerNode, address: &Value, out: &mut Vec<[f64; 4]>) {
            match &node.kind {
                hwpcraft_doc::LayerNodeKind::Group { children, .. } => {
                    for child in children {
                        bounds(child, address, out);
                    }
                }
                hwpcraft_doc::LayerNodeKind::ClipRect { child, .. } => bounds(child, address, out),
                hwpcraft_doc::LayerNodeKind::Leaf { ops } => {
                    for op in ops {
                        if let hwpcraft_doc::PaintOp::RawSvg { bbox, raw } = op
                            && raw.control_ref.as_ref().is_some_and(|r| {
                                address["section"].as_u64() == Some(r.section_index as u64)
                                    && address["paragraph"].as_u64() == Some(r.para_index as u64)
                                    && address["control"].as_u64() == Some(r.control_index as u64)
                            })
                        {
                            out.push([bbox.x, bbox.y, bbox.width, bbox.height]);
                        }
                    }
                }
            }
        }
        let mut matches = Vec::new();
        bounds(&tree.root, address, &mut matches);
        if matches.len() != 1 {
            bail!("미리 보기 차트의 실제 표시 영역을 확인하지 못했습니다.");
        }
        Ok((preview.render_page_svg(0)?, matches[0]))
    }

    fn make_preview(&mut self, ctx: &egui::Context, renderer: &Renderer) -> Result<()> {
        let (svg, bbox) = self.preview_page()?;
        let page = renderer.svg(&svg, 1.0)?;
        let image = ::image::RgbaImage::from_raw(page.width, page.height, page.rgba)
            .context("차트 미리 보기 픽셀 크기가 잘못됐습니다.")?;
        let x = (bbox[0].floor() - 2.0).max(0.0) as u32;
        let y = (bbox[1].floor() - 2.0).max(0.0) as u32;
        let right = (bbox[0] + bbox[2] + 2.0).ceil().min(image.width() as f64) as u32;
        let bottom = (bbox[1] + bbox[3] + 2.0).ceil().min(image.height() as f64) as u32;
        if right <= x || bottom <= y {
            bail!("차트 미리 보기 영역이 쪽 밖에 있습니다.");
        }
        let crop = ::image::imageops::crop_imm(&image, x, y, right - x, bottom - y).to_image();
        let size = Vec2::new(crop.width() as f32, crop.height() as f32);
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [crop.width() as usize, crop.height() as usize],
            crop.as_raw(),
        );
        self.preview = Some((
            ctx.load_texture(
                "chart-authoring-preview",
                color,
                egui::TextureOptions::LINEAR,
            ),
            size,
        ));
        self.preview_stale = false;
        Ok(())
    }

    #[cfg(test)]
    fn record(&mut self, name: &'static str, ui: &egui::Ui, response: &egui::Response) {
        self.regions
            .insert(name, (response.rect, ui.clip_rect(), response.id));
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        let mut next_kind = self.draft.kind.clone();
        ui.horizontal(|ui| {
            ui.label("제목");
            let response =
                ui.add(egui::TextEdit::singleline(&mut self.draft.title).desired_width(300.0));
            #[cfg(test)]
            self.record("title", ui, &response);
            #[cfg(not(test))]
            let _ = response;
            ui.label("종류");
            egui::ComboBox::from_id_salt("chart-create-kind")
                .selected_text(
                    KINDS
                        .iter()
                        .find(|(id, _)| *id == self.draft.kind)
                        .map_or("지원되지 않음", |(_, name)| *name),
                )
                .show_ui(ui, |ui| {
                    for (id, name) in KINDS {
                        if self.available_kinds.iter().any(|kind| kind == id) {
                            ui.selectable_value(&mut next_kind, (*id).into(), *name);
                        }
                    }
                });
        });
        if next_kind != self.draft.kind {
            self.draft.select_kind(&next_kind);
            self.preview_stale = true;
            self.changed = true;
        }
        if self.draft.kind == "scatter" {
            ui.horizontal(|ui| {
                ui.label("표시 방식");
                egui::ComboBox::from_id_salt("chart-scatter-style")
                    .selected_text(
                        SCATTER_STYLES
                            .iter()
                            .find(|(id, _)| *id == self.draft.scatter_style)
                            .map_or("지원되지 않음", |(_, label)| *label),
                    )
                    .show_ui(ui, |ui| {
                        for (id, label) in SCATTER_STYLES {
                            ui.selectable_value(
                                &mut self.draft.scatter_style,
                                (*id).into(),
                                *label,
                            );
                        }
                    });
                ui.label(RichText::new("모든 계열이 같은 X열을 사용합니다.").weak());
            });
        }
        if groups(&self.draft.kind) {
            ui.horizontal(|ui| {
                ui.label("묶음");
                let before = self.draft.grouping.clone();
                for (id, label) in GROUPINGS {
                    ui.selectable_value(&mut self.draft.grouping, (*id).into(), *label);
                }
                if self.draft.grouping != before {
                    self.preview_stale = true;
                    self.changed = true;
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("너비");
            ui.add(
                egui::DragValue::new(&mut self.draft.width_mm)
                    .range(10.0..=300.0)
                    .suffix(" mm"),
            );
            ui.label("높이");
            ui.add(
                egui::DragValue::new(&mut self.draft.height_mm)
                    .range(10.0..=300.0)
                    .suffix(" mm"),
            );
            ui.checkbox(&mut self.draft.legend, "범례")
                .on_hover_text("범례 표시 설정을 미리 보기와 문서 저장에 적용합니다.");
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.draft.points() < 1000
                        && (self.draft.points() + 1) * self.draft.series.len() <= 16_000,
                    egui::Button::new("행 추가"),
                )
                .on_disabled_hover_text("항목은 1000개, 값은 모두 16000개까지 넣을 수 있습니다.")
                .clicked()
            {
                self.draft.add_row();
            }
            if ui
                .add_enabled(self.draft.points() > 1, egui::Button::new("마지막 행 제거"))
                .on_disabled_hover_text("항목이 하나뿐이라 지울 수 없습니다.")
                .clicked()
            {
                self.draft.categories.pop();
                self.draft.x_values.pop();
                for s in &mut self.draft.series {
                    s.values.pop();
                }
            }
            if ui
                .add_enabled(
                    stock_roles(&self.draft.kind).is_empty()
                        && self.draft.series.len() < 64
                        && (self.draft.series.len() + 1) * self.draft.points() <= 16_000,
                    egui::Button::new("계열 추가"),
                )
                .on_disabled_hover_text(if stock_roles(&self.draft.kind).is_empty() {
                    "계열은 64개, 값은 모두 16000개까지 넣을 수 있습니다."
                } else {
                    "주식형 차트는 시가·고가·저가·종가 계열이 정해져 있습니다."
                })
                .clicked()
            {
                self.draft.add_series();
            }
            if ui
                .add_enabled(
                    stock_roles(&self.draft.kind).is_empty() && self.draft.series.len() > 1,
                    egui::Button::new("마지막 계열 제거"),
                )
                .on_disabled_hover_text(if stock_roles(&self.draft.kind).is_empty() {
                    "계열이 하나뿐이라 지울 수 없습니다."
                } else {
                    "주식형 차트는 시가·고가·저가·종가 계열이 정해져 있습니다."
                })
                .clicked()
            {
                self.draft.series.pop();
            }
        });
        egui::ScrollArea::both()
            .id_salt("chart-authoring-grid-scroll")
            .max_height(240.0)
            .show(ui, |ui| {
                egui::Grid::new("chart-authoring-grid")
                    .striped(true)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        let scatter = self.draft.kind == "scatter";
                        let roles = stock_roles(&self.draft.kind);
                        ui.label(if scatter { "공통 X" } else { "범주" });
                        for (index, series) in self.draft.series.iter_mut().enumerate() {
                            ui.vertical(|ui| {
                                if let Some(role) = roles.get(index) {
                                    ui.label(*role);
                                }
                                ui.add_sized(
                                    [VALUE_WIDTH, 22.0],
                                    egui::TextEdit::singleline(&mut series.name)
                                        .background_color(HEADER_FILL),
                                );
                            });
                        }
                        ui.end_row();
                        for row in 0..self.draft.points() {
                            let response = ui.add_sized(
                                [LABEL_WIDTH, 22.0],
                                egui::TextEdit::singleline(if scatter {
                                    &mut self.draft.x_values[row]
                                } else {
                                    &mut self.draft.categories[row]
                                })
                                .background_color(HEADER_FILL),
                            );
                            #[cfg(test)]
                            if scatter && row == 0 {
                                self.record("scatter_x", ui, &response);
                            }
                            #[cfg(not(test))]
                            let _ = response;
                            for (index, series) in self.draft.series.iter_mut().enumerate() {
                                if let Some(value) = series.values.get_mut(row) {
                                    let response = ui.add_sized(
                                        [VALUE_WIDTH, 22.0],
                                        egui::TextEdit::singleline(value),
                                    );
                                    #[cfg(test)]
                                    if row == 0 && !roles.is_empty() {
                                        let name = if index == roles.len() - 3 {
                                            "stock_high"
                                        } else if index == roles.len() - 2 {
                                            "stock_low"
                                        } else if index == roles.len() - 1 {
                                            "stock_close"
                                        } else {
                                            "stock_open"
                                        };
                                        self.regions.insert(
                                            name,
                                            (response.rect, ui.clip_rect(), response.id),
                                        );
                                    }
                                    #[cfg(not(test))]
                                    let _ = (index, response);
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
        ui.label(
            RichText::new(if stock_roles(&self.draft.kind).is_empty() {
                "값을 고치면 잠시 뒤 오른쪽 미리 보기가 바뀝니다."
            } else {
                "주식형: 고가는 저가보다 크거나 같고, 시가·종가는 그 사이여야 합니다."
            })
            .weak(),
        );
        // Compare with the last spec seen, so changes made outside the form
        // (refresh from the document, programmatic edits) also refresh the preview.
        let now = self.draft.spec().ok();
        if now != self.last_spec {
            self.last_spec = now;
            self.preview_stale = true;
            self.changed = true;
        }
    }
}

/// Category / X column and value columns of the data grid (fixed, so a first
/// frame does not shrink cells to the grid's unknown column width).
const LABEL_WIDTH: f32 = 120.0;
/// Background of the header row (series names) and column (categories, X).
const HEADER_FILL: Color32 = Color32::from_rgb(238, 243, 250);
const VALUE_WIDTH: f32 = 84.0;
/// Pause after the last edit before the live preview is regenerated (seconds).
const PREVIEW_PAUSE: f64 = 0.35;

impl HwpApp {
    pub(super) fn open_chart_creation(&mut self, index: Option<usize>) {
        match Editor::open(&mut self.session, index) {
            Ok(editor) => {
                self.ext.object.chart = None;
                self.ext.object.chart_authoring = Some(Box::new(editor));
            }
            Err(error) => self.message = format!("{error:#}"),
        }
    }

    pub(super) fn chart_creation_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.ext.object.chart_authoring.take() else {
            return;
        };
        let (mut apply, mut preview, mut refresh, mut close) = (false, false, false, false);
        let now = ctx.input(|i| i.time);
        let response = chrome::modal("ext-chart-authoring").show(ctx, |ui| {
            ui.set_width(820.0);
            ui.set_max_height((ctx.content_rect().height() - 120.0).max(300.0));
            close = chrome::title(
                ui,
                if editor.capture.target.is_some() {
                    "차트 전체 구성"
                } else {
                    "새 차트"
                },
            );
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(500.0);
                    egui::ScrollArea::vertical()
                        .id_salt("chart-authoring-content")
                        .max_height((ctx.content_rect().height() - 240.0).max(160.0))
                        .show(ui, |ui| {
                            editor.contents(ui);
                            if let Some(error) = &editor.error {
                                ui.colored_label(Color32::DARK_RED, error);
                            }
                        });
                });
                // Spacing, not a vertical separator: a separator here takes the full
                // available height and stretches the dialog.
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.set_width(280.0);
                    chrome::section(ui, "미리 보기");
                    let (frame, _) =
                        ui.allocate_exact_size(Vec2::new(280.0, 200.0), egui::Sense::hover());
                    ui.painter().rect_stroke(
                        frame,
                        2.0,
                        Stroke::new(1.0, Color32::from_gray(210)),
                        egui::StrokeKind::Inside,
                    );
                    if let Some((texture, size)) = &editor.preview {
                        let scale = (frame.width() - 8.0) / size.x;
                        let scale = scale.min((frame.height() - 8.0) / size.y).min(1.0);
                        let rect = Rect::from_center_size(frame.center(), *size * scale);
                        ui.painter().image(
                            texture.id(),
                            rect,
                            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    } else {
                        ui.painter().text(
                            frame.center(),
                            egui::Align2::CENTER_CENTER,
                            "미리 보기를 만드는 중…",
                            egui::FontId::proportional(13.0),
                            Color32::GRAY,
                        );
                    }
                    if let Some(error) = &editor.preview_error {
                        ui.colored_label(Color32::DARK_RED, error);
                    } else if editor.preview_stale {
                        ui.weak("입력을 멈추면 미리 보기가 바뀝니다.");
                    }
                    let r = ui.button("미리 보기 새로 고침");
                    preview = r.clicked();
                    #[cfg(test)]
                    editor.record("preview", ui, &r);
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                let r = ui
                    .button("현재 문서 기준으로 갱신")
                    .on_hover_text("창을 연 뒤 문서가 바뀌었으면 입력을 지금 문서에 맞춥니다.");
                refresh = r.clicked();
                #[cfg(test)]
                editor.record("refresh", ui, &r);
                // Same order as the other dialogs: main action first, cancel last,
                // at the right edge; Enter applies, Esc cancels.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let r = ui.add(egui::Button::new("취소").min_size(Vec2::new(80.0, 28.0)));
                    close |= r.clicked();
                    #[cfg(test)]
                    editor.record("cancel", ui, &r);
                    let r = ui.add(
                        egui::Button::new(if editor.capture.target.is_some() {
                            "구성 적용"
                        } else {
                            "차트 넣기"
                        })
                        .fill(Color32::WHITE)
                        .stroke(Stroke::new(1.5, chrome::Tokens::ACCENT))
                        .min_size(Vec2::new(80.0, 28.0)),
                    );
                    apply = r.clicked();
                    #[cfg(test)]
                    editor.record("apply", ui, &r);
                });
            });
        });
        if !apply
            && !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            apply = true;
        }
        // Live preview: regenerate once the user has paused typing.
        if editor.changed {
            editor.changed = false;
            editor.edited_at = now;
        }
        if editor.preview_stale && !preview {
            let wait = PREVIEW_PAUSE - (now - editor.edited_at);
            if wait <= 0.0 {
                preview = true;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
            }
        }
        if preview {
            match editor.make_preview(ctx, &self.renderer) {
                Ok(()) => editor.preview_error = None,
                Err(error) => {
                    // Keep the last good picture; do not retry until the next edit.
                    editor.preview_stale = false;
                    editor.preview_error = Some(format!("미리 보기: {error:#}"));
                }
            }
        }
        if refresh {
            match editor.refresh_capture(&mut self.session) {
                Ok(()) => editor.error = None,
                Err(error) => editor.error = Some(format!("{error:#}")),
            }
        }
        if apply {
            match editor.apply(&mut self.session) {
                Ok(_) => {
                    close = true;
                    self.ext.object.layout = None;
                    self.clear_object_selection();
                    self.sync_formatting();
                    self.follow_cursor();
                    self.message = "차트를 적용했습니다.".into();
                }
                Err(error) => editor.error = Some(format!("{error:#}")),
            }
        }
        if !(close || response.should_close()) {
            self.ext.object.chart_authoring = Some(editor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft_for_kind(kind: &str) -> Draft {
        let mut draft = Draft::default();
        draft.select_kind(kind);
        let roles = stock_roles(kind);
        for (index, series) in draft.series.iter_mut().enumerate() {
            if !roles.is_empty() {
                let number = match roles[index] {
                    "고가" => 12,
                    "저가" => 8,
                    "종가" => 11,
                    _ => 10,
                };
                series.values = vec![number.to_string(); draft.categories.len()];
            }
        }
        if kind == "scatter" {
            draft.x_values = vec!["-2".into(), "0".into(), "3".into()];
        }
        draft
    }

    #[test]
    fn eleven_ui_definitions_create_configure_undo_and_roundtrip_without_dropping_fields()
    -> Result<()> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../output/implementation/0.17/ui-chart-eleven")
            .join(format!("{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        for (kind, _) in KINDS {
            let mut session = Session::new()?;
            session.execute(Command::Insert("차트 앞 본문 😀".into()))?;
            let mut editor = Editor::open(&mut session, None)?;
            assert_eq!(editor.available_kinds.len(), 11);
            editor.draft = draft_for_kind(kind);
            editor.draft.title = format!("UI {kind} 생성");
            let result = editor.apply(&mut session)?;
            assert_eq!(result["kind"], *kind);
            let mut configured = Editor::open(&mut session, Some(0))?;
            let definition = session.run("chart.definition.get", json!({"index":0}))?;
            assert_eq!(
                configured.draft.spec()?,
                definition["chart"],
                "{kind} getter fields"
            );
            configured.draft.title = format!("UI {kind} 수정");
            configured.draft.legend = false;
            if *kind == "scatter" {
                configured.draft.scatter_style = "smooth_marker".into();
                configured.draft.x_values[0] = "-7.25".into();
            }
            let before = format!("{:?}", session.core.document());
            configured.apply(&mut session)?;
            session.execute(Command::Undo)?;
            assert_eq!(
                format!("{:?}", session.core.document()),
                before,
                "{kind}: one undo"
            );
            session.execute(Command::Redo)?;
            let expected =
                session.run("chart.definition.get", json!({"index":0}))?["chart"].clone();
            assert_eq!(expected["legend"], false);
            for extension in ["hwp", "hwpx"] {
                let path = root.join(format!("chart-{kind}.{extension}"));
                session.save(&path)?;
                let mut reopened = Session::new()?;
                reopened.open(&path)?;
                let loaded = Editor::open(&mut reopened, Some(0))?;
                assert_eq!(
                    loaded.draft.spec()?,
                    expected,
                    "{kind}/{extension}: full editable definition"
                );
                assert!(reopened.text().contains("차트 앞 본문 😀"));
                let svg = reopened.render_page_svg(0)?;
                assert!(
                    svg.contains(&format!("UI {kind} 수정")),
                    "{kind}/{extension}: actual chart title"
                );
                std::fs::write(root.join(format!("chart-{kind}-{extension}.svg")), svg)?;
            }
        }
        Ok(())
    }

    #[test]
    fn chart_family_switches_preserve_invalid_input_and_scatter_stock_contracts() -> Result<()> {
        let mut draft = Draft::default();
        draft.series[0].values[0] = "작성 중인 값".into();
        draft.select_kind("scatter");
        draft.x_values[0] = "작성 중인 X".into();
        draft.scatter_style = "line_marker".into();
        draft.select_kind("stock_hlc");
        assert_eq!(draft.series.len(), 3);
        assert!(draft.series.iter().all(|s| s.values[0].is_empty()));
        draft.series[0].values[0] = "임시 고가".into();
        draft.select_kind("stock_ohlc");
        assert_eq!(draft.series.len(), 4);
        draft.select_kind("stock_hlc");
        assert_eq!(draft.series[0].values[0], "임시 고가");
        draft.select_kind("scatter");
        assert_eq!(draft.x_values[0], "작성 중인 X");
        assert_eq!(draft.scatter_style, "line_marker");
        draft.select_kind("column_3d");
        assert_eq!(draft.series[0].values[0], "작성 중인 값");
        for (style, _) in SCATTER_STYLES {
            let mut scatter = draft_for_kind("scatter");
            scatter.scatter_style = (*style).into();
            scatter.add_series();
            let spec = scatter.spec()?;
            assert!(
                spec["categories"]
                    .as_array()
                    .context("categories")?
                    .is_empty()
            );
            assert_eq!(spec["series"][0]["x_values"], spec["series"][1]["x_values"]);
            assert_eq!(Draft::from_spec(&spec)?.spec()?, spec);
            let mut unknown = spec.clone();
            unknown["series"][1]["x_values"][0] = json!(999);
            assert!(
                Draft::from_spec(&unknown).is_err(),
                "unequal X must not be coerced"
            );
            unknown = spec.clone();
            unknown["scatter_style"] = json!("unknown_future_style");
            assert!(Draft::from_spec(&unknown).is_err());
            unknown = spec;
            unknown["future_field"] = json!({"preserve":true});
            assert!(Draft::from_spec(&unknown).is_err());
        }
        for kind in ["stock_hlc", "stock_ohlc"] {
            let mut stock = draft_for_kind(kind);
            let high = stock.series.len() - 3;
            stock.series[high].values[0] = "5".into();
            assert!(stock.spec().is_err());
            assert_eq!(stock.series[high].values[0], "5");
        }
        Ok(())
    }

    fn document_state(session: &Session) -> Value {
        json!({"model":format!("{:?}",session.core.document()),"generation":session.generation,
            "cursor":session.cursor,"anchor":session.anchor,"span":session.span_anchor,
            "dirty":session.dirty(),"undo":session.can_undo(),"redo":session.can_redo()})
    }

    #[test]
    fn chart_authoring_uses_real_engine_one_undo_and_preserves_stale_drafts() -> Result<()> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("기존 본문 😀".into()))?;
        let original = format!("{:?}", session.core.document());
        let mut editor = Editor::open(&mut session, None)?;
        editor.draft.title = "사용자 차트".into();
        editor.draft.kind = "doughnut".into();
        let created = editor.apply(&mut session)?;
        assert_eq!(created["kind"], "doughnut");
        assert_eq!(
            session
                .run("chart.list", json!({}))?
                .as_array()
                .context("차트 목록")?
                .len(),
            1
        );
        assert!(session.text().contains("기존 본문 😀"));
        session.execute(Command::Undo)?;
        assert_eq!(format!("{:?}", session.core.document()), original);
        session.execute(Command::Redo)?;
        let mut editor = Editor::open(&mut session, Some(0))?;
        assert_eq!(editor.draft.title, "사용자 차트");
        session.run(
            "chart.data",
            json!({"index":0,"structure":true,"series":[{"name":"변경된 계열","values":["2","4","8"]}]}),
        )?;
        let stale_state = document_state(&session);
        editor.draft.title = "계속 보존할 초안".into();
        assert!(editor.apply(&mut session).is_err());
        assert_eq!(document_state(&session), stale_state);
        assert_eq!(editor.draft.title, "계속 보존할 초안");
        // Explicit rebase retains draft data, and verifies the original object's identity.
        editor.refresh_capture(&mut session)?;
        let before_configure = format!("{:?}", session.core.document());
        editor.draft.kind = "bar".into();
        editor.apply(&mut session)?;
        assert_eq!(
            session.run("chart.definition.get", json!({"index":0}))?["chart"]["title"],
            "계속 보존할 초안"
        );
        session.execute(Command::Undo)?;
        assert_eq!(format!("{:?}", session.core.document()), before_configure);
        Ok(())
    }

    #[test]
    fn legacy_chart_editor_renames_and_edits_values_with_one_undo_and_format_roundtrips()
    -> Result<()> {
        let context = egui::Context::default();
        let mut app = HwpApp::with_context(context, None, None, false)?;
        app.session
            .execute(Command::Insert("보존할 차트 본문".into()))?;
        let mut created = Editor::open(&mut app.session, None)?;
        created.draft.title = "기존 데이터 편집 경로".into();
        created.apply(&mut app.session)?;
        app.open_chart_editor(0);
        let mut editor = app.ext.object.chart.take().context("legacy data editor")?;
        editor.names[0] = "수정된 매출 😀".into();
        editor.values[0] = vec!["1.25".into(), "9.50".into(), "-2.5".into()];
        let before = format!("{:?}", app.session.core.document());
        // The existing default remains value-only, so accidental structural edits fail.
        assert!(
            app.session
                .run(
                    "chart.data",
                    json!({"index":0,"series":[{
            "name":editor.names[0],"values":editor.values[0]}]})
                )
                .is_err()
        );
        assert_eq!(format!("{:?}", app.session.core.document()), before);
        apply_legacy_data(&mut app.session, &editor)?;
        let changed = format!("{:?}", app.session.core.document());
        let data = app.session.run("chart.data.get", json!({"index":0}))?;
        assert_eq!(data["series"][0]["name"], "수정된 매출 😀");
        assert_eq!(data["series"][0]["values"], json!(["1.25", "9.50", "-2.5"]));
        app.session.execute(Command::Undo)?;
        assert_eq!(format!("{:?}", app.session.core.document()), before);
        app.session.execute(Command::Redo)?;
        assert_eq!(format!("{:?}", app.session.core.document()), changed);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../output/implementation/0.17/ui-chart-data")
            .join(format!("{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        for extension in ["hwp", "hwpx"] {
            let path = root.join(format!("renamed-data.{extension}"));
            app.session.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            let after = reopened.run("chart.data.get", json!({"index":0}))?;
            assert_eq!(after["series"][0], data["series"][0], "{extension}");
            assert!(reopened.text().contains("보존할 차트 본문"));
            let svg = reopened.render_page_svg(0)?;
            assert!(
                svg.contains("수정된 매출 😀"),
                "renamed legend must render: {extension}"
            );
            std::fs::write(root.join(format!("renamed-data-{extension}.svg")), svg)?;
        }
        Ok(())
    }

    #[test]
    fn chart_authoring_rejects_invalid_numbers_selection_and_unknown_settings_atomically()
    -> Result<()> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("선택 보존".into()))?;
        session.select(
            Cursor {
                offset: 0,
                ..session.cursor
            },
            false,
        )?;
        session.select(
            Cursor {
                offset: 2,
                ..session.cursor
            },
            true,
        )?;
        let mut editor = Editor::open(&mut session, None)?;
        let original = document_state(&session);
        assert!(editor.apply(&mut session).is_err());
        assert_eq!(document_state(&session), original);
        session.select(
            Cursor {
                offset: 0,
                ..session.cursor
            },
            false,
        )?;
        editor.refresh_capture(&mut session)?;
        for invalid in ["NaN", "inf", "-inf", "1000000000001", "", "한국어"] {
            editor.draft.series[0].values[0] = invalid.into();
            let before = document_state(&session);
            assert!(editor.apply(&mut session).is_err(), "{invalid}");
            assert_eq!(document_state(&session), before);
            assert_eq!(editor.draft.series[0].values[0], invalid);
        }
        editor.draft.series[0].values[0] = "1".into();
        editor.draft.add_series();
        editor.draft.kind = "pie".into();
        assert!(editor.draft.spec().is_err());
        assert_eq!(
            editor.draft.series.len(),
            2,
            "changing kind must not delete data"
        );
        let mut future = Draft::default().spec()?;
        future["series"][0]["x_values"] = json!([1, 2, 3]);
        assert!(
            Draft::from_spec(&future).is_err(),
            "unknown settings must not disappear on configure"
        );
        future["kind"] = json!("scatter");
        assert!(Draft::from_spec(&future).is_err());
        Ok(())
    }

    #[test]
    fn chart_authoring_preview_is_real_rendered_chart_and_live_document_is_unchanged() -> Result<()>
    {
        let context = egui::Context::default();
        let mut app = HwpApp::with_context(context.clone(), None, None, false)?;
        app.session
            .execute(Command::Insert("미리 보기로 변경하지 않을 문서".into()))?;
        let original = document_state(&app.session);
        for (kind, _) in KINDS {
            let mut editor = Editor::open(&mut app.session, None)?;
            editor.draft = draft_for_kind(kind);
            editor.draft.title = "실제 차트 미리 보기".into();
            let (svg, bbox) = editor.preview_page()?;
            assert!(svg.contains("실제 차트 미리 보기"), "{kind}");
            assert!(!svg.contains("차트 (Chart)"));
            assert!(bbox[2] > 100.0 && bbox[3] > 100.0);
            if *kind == "doughnut" {
                assert!(svg.contains("data-hwpcraft-chart=\"doughnut\""));
                assert!(svg.contains("fill-rule=\"evenodd\""));
            }
            editor.make_preview(&context, &app.renderer)?;
            let (_, size) = editor.preview.as_ref().context("actual raster texture")?;
            assert!(size.x > 100.0 && size.y > 100.0);
            assert!(!editor.preview_stale);
            assert_eq!(document_state(&app.session), original);
        }
        Ok(())
    }

    fn frame(app: &mut HwpApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| app.chart_creation_dialog(ui.ctx()),
        );
        output.textures_delta.clear();
    }
    fn click(app: &mut HwpApp, ctx: &egui::Context, at: Pos2) {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
    }

    fn type_in_region(
        app: &mut HwpApp,
        ctx: &egui::Context,
        name: &'static str,
        text: &str,
    ) -> Result<()> {
        let (rect, clip, id) = app
            .ext
            .object
            .chart_authoring
            .as_ref()
            .context("editor")?
            .regions[name];
        assert!(
            clip.contains_rect(rect),
            "{name} must be a fully visible real editor"
        );
        click(app, ctx, rect.center());
        assert!(
            ctx.memory(|m| m.has_focus(id)),
            "{name}: actual input focus"
        );
        frame(
            app,
            ctx,
            vec![
                egui::Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                },
                egui::Event::Text(text.into()),
            ],
        );
        frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        Ok(())
    }

    #[test]
    fn scatter_and_stock_forms_accept_real_keyboard_input_and_reject_bad_prices_without_losing_draft()
    -> Result<()> {
        for kind in ["scatter", "stock_hlc", "stock_ohlc"] {
            let ctx = egui::Context::default();
            let mut app = HwpApp::with_context(ctx.clone(), None, None, false)?;
            app.session
                .execute(Command::Insert("가격 오류에도 보존".into()))?;
            let original = document_state(&app.session);
            app.open_chart_creation(None);
            app.ext
                .object
                .chart_authoring
                .as_mut()
                .context("editor")?
                .draft = draft_for_kind(kind);
            for _ in 0..3 {
                frame(&mut app, &ctx, vec![]);
            }
            let field = if kind == "scatter" {
                "scatter_x"
            } else {
                "stock_high"
            };
            type_in_region(
                &mut app,
                &ctx,
                field,
                if kind == "scatter" { "-7.25" } else { "5" },
            )?;
            if kind == "scatter" {
                assert_eq!(
                    app.ext
                        .object
                        .chart_authoring
                        .as_ref()
                        .context("draft")?
                        .draft
                        .x_values[0],
                    "-7.25"
                );
            } else {
                let apply = app
                    .ext
                    .object
                    .chart_authoring
                    .as_ref()
                    .context("draft")?
                    .regions["apply"]
                    .0;
                click(&mut app, &ctx, apply.center());
                let editor = app
                    .ext
                    .object
                    .chart_authoring
                    .as_ref()
                    .context("invalid draft preserved")?;
                assert!(
                    editor
                        .error
                        .as_ref()
                        .is_some_and(|message| message.contains("고가"))
                );
                assert_eq!(document_state(&app.session), original);
                type_in_region(&mut app, &ctx, field, "15")?;
            }
            let preview = app
                .ext
                .object
                .chart_authoring
                .as_ref()
                .context("draft")?
                .regions["preview"]
                .0;
            click(&mut app, &ctx, preview.center());
            let editor = app.ext.object.chart_authoring.as_ref().context("draft")?;
            assert!(editor.preview.is_some(), "{kind}: {:?}", editor.error);
            let apply = editor.regions["apply"].0;
            assert!(ctx.content_rect().contains_rect(apply));
            click(&mut app, &ctx, apply.center());
            assert!(
                app.ext.object.chart_authoring.is_none(),
                "{kind}: apply succeeded"
            );
            let definition = app
                .session
                .run("chart.definition.get", json!({"index":0}))?;
            assert_eq!(definition["chart"]["kind"], kind);
            if kind == "scatter" {
                assert_eq!(definition["chart"]["series"][0]["x_values"][0], -7.25);
            }
            app.session.execute(Command::Undo)?;
            assert_eq!(document_state(&app.session)["model"], original["model"]);
        }
        Ok(())
    }

    #[test]
    fn chart_authoring_preview_appears_after_a_pause_without_clicking() -> Result<()> {
        let context = egui::Context::default();
        let mut app = HwpApp::with_context(context.clone(), None, None, false)?;
        app.session.execute(Command::Insert("본문".into()))?;
        app.open_chart_editor(0);
        let timed = |app: &mut HwpApp, time: f64| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 900.0))),
                    time: Some(time),
                    ..Default::default()
                },
                |ui| app.chart_creation_dialog(ui.ctx()),
            );
            output.textures_delta.clear();
        };
        let preview = |app: &HwpApp| {
            app.ext
                .object
                .chart_authoring
                .as_ref()
                .is_some_and(|e| e.preview.is_some() && !e.preview_stale)
        };
        timed(&mut app, 10.0);
        assert!(!preview(&app), "no preview before the pause");
        timed(&mut app, 10.1);
        assert!(!preview(&app));
        timed(&mut app, 10.5);
        assert!(preview(&app), "live preview after the pause");
        // An edit makes it stale again; the next pause refreshes it.
        if let Some(editor) = app.ext.object.chart_authoring.as_mut() {
            editor.draft.title = "바뀐 제목".into();
        }
        timed(&mut app, 11.0);
        let stale = app
            .ext
            .object
            .chart_authoring
            .as_ref()
            .is_some_and(|e| e.preview_stale);
        assert!(stale, "edit marks the preview stale");
        timed(&mut app, 11.5);
        assert!(preview(&app), "refreshed after the second pause");
        Ok(())
    }

    #[test]
    fn chart_authoring_dialog_real_buttons_focus_preview_and_apply_are_visible() -> Result<()> {
        let context = egui::Context::default();
        let mut app = HwpApp::with_context(context.clone(), None, None, false)?;
        app.session
            .execute(Command::Insert("차트 앞 본문".into()))?;
        let original = format!("{:?}", app.session.core.document());
        assert_eq!(app.object_ext_enabled("chart.edit"), Some(true));
        app.open_chart_editor(0); // existing entry point opens creation in an empty chart list
        assert!(app.object_ext_modal_open());
        for _ in 0..3 {
            frame(&mut app, &context, vec![]);
        }
        let editor = app
            .ext
            .object
            .chart_authoring
            .as_ref()
            .context("authoring opened")?;
        for name in ["title", "preview", "refresh", "apply", "cancel"] {
            let (rect, clip, _) = editor
                .regions
                .get(name)
                .context("real widget response missing")?;
            assert!(
                context.content_rect().contains_rect(*rect),
                "{name}: {rect:?}"
            );
            let visible = rect.intersect(*clip);
            assert!(
                visible.width() >= rect.width() - 0.5 && visible.height() >= rect.height() - 0.5,
                "{name} clipped: {rect:?} {clip:?}"
            );
        }
        // Common dialog order: refresh on the left, main action then cancel at the right.
        let (refresh, apply, cancel) = (
            editor.regions["refresh"].0,
            editor.regions["apply"].0,
            editor.regions["cancel"].0,
        );
        assert!(refresh.right() < apply.left() && apply.right() <= cancel.left());
        assert!(
            cancel.left() - refresh.right() > 200.0,
            "{refresh:?} {cancel:?}"
        );
        let (title_rect, _, title_id) = editor.regions["title"];
        click(&mut app, &context, title_rect.center());
        assert!(
            context.memory(|memory| memory.has_focus(title_id)),
            "actual text editor must gain focus"
        );
        frame(
            &mut app,
            &context,
            vec![egui::Event::Text("입력 확인".into())],
        );
        assert!(
            app.ext
                .object
                .chart_authoring
                .as_ref()
                .context("draft")?
                .draft
                .title
                .contains("입력 확인")
        );
        let preview_rect = app
            .ext
            .object
            .chart_authoring
            .as_ref()
            .context("draft")?
            .regions["preview"]
            .0;
        click(&mut app, &context, preview_rect.center());
        let editor = app.ext.object.chart_authoring.as_ref().context("draft")?;
        assert!(
            editor.preview.is_some(),
            "actual engine preview button must generate a texture: {:?}",
            editor.error
        );
        assert_eq!(format!("{:?}", app.session.core.document()), original);
        let apply_rect = editor.regions["apply"].0;
        click(&mut app, &context, apply_rect.center());
        assert!(
            app.ext.object.chart_authoring.is_none(),
            "successful apply closes the modal"
        );
        assert_eq!(
            app.session
                .run("chart.list", json!({}))?
                .as_array()
                .context("charts")?
                .len(),
            1
        );
        app.session.execute(Command::Undo)?;
        assert_eq!(format!("{:?}", app.session.core.document()), original);
        Ok(())
    }

    #[test]
    fn grouping_round_trips_for_bars_and_lines_only() -> Result<()> {
        for kind in ["column", "bar", "line"] {
            let mut draft = draft_for_kind(kind);
            draft.add_series();
            draft.grouping = "percent_stacked".into();
            let spec = draft.spec()?;
            assert_eq!(spec["grouping"], "percent_stacked", "{kind}");
            assert_eq!(Draft::from_spec(&spec)?.spec()?, spec);
            draft.grouping = "clustered".into();
            assert!(
                draft.spec()?.get("grouping").is_none(),
                "{kind}: default is not written"
            );
        }
        let mut pie = draft_for_kind("pie");
        pie.grouping = "stacked".into();
        assert!(pie.spec()?.get("grouping").is_none(), "pie has no grouping");
        let mut bad = draft_for_kind("column");
        bad.grouping = "sideways".into();
        assert!(bad.spec().is_err());
        Ok(())
    }
}
