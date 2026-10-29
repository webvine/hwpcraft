//! Own chart authoring: public DrawingML XML inside an actual embedded OLE chart.
//! No Hancom sample/template/resources are copied. Native Hancom acceptance of
//! newly synthesized OOXML-only CFB objects remains a separate interoperability
//! check; HwpCraft rendering and both file roundtrips are tested independently.
use super::*;
use anyhow::Context;
use hwpcraft_doc::{
    chart_io,
    model::{
        bin_data::{
            BinData, BinDataBytes, BinDataCompression, BinDataContent, BinDataStatus, BinDataType,
        },
        document::Document,
        shape::{OleShape, ShapeObject},
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(crate) const COMMANDS: &[(&str, &str, &str)] = &[
    ("chart.create", "새 차트 만들기", ""),
    ("chart.configure", "자체 생성 차트 구성 바꾸기", ""),
    ("chart.types", "작성 가능한 차트 종류", ""),
    ("chart.definition.get", "자체 생성 차트 전체 구성 조회", ""),
];
pub(crate) const PARAMS: &[(&str, &str, &str)] = &[
    (
        "chart.create",
        "chart: {kind:column|bar|line|pie|doughnut|scatter|stock_hlc|stock_ohlc|column_3d|bar_3d|pie_3d,title?,categories:[text],series:[{name,values:[finite numbers],color?:#RRGGBB,x_values?:[finite numbers]}],scatter_style?:marker|line|line_marker|smooth_marker,width_mm?,height_mm?,legend?}; expected_generation?; chart.types describes kind constraints. Scatter requires shared numeric x_values and no categories. Stock series roles/order: high,low,close or open,high,low,close. Body caret only. Hancom native interoperability requires a separate oracle.",
        r##"{"chart":{"kind":"column","title":"분기 실적","categories":["1분기","2분기"],"series":[{"name":"매출","values":[12,19]}]}}"##,
    ),
    (
        "chart.configure",
        "index: 0-based chart.list index; chart: complete replacement definition in chart.create shape; required expected_generation. Only HwpCraft-authored charts are accepted. Preserves object placement and all other CFB streams. Existing chart.data remains the value-edit API for other supported charts.",
        r##"{"index":0,"expected_generation":0,"chart":{"kind":"line","categories":["1분기","2분기"],"series":[{"name":"매출","values":[15,21]}]}}"##,
    ),
    (
        "chart.types",
        "No parameters. Actual authoring profiles, required fields, constraints, unsupported kinds and native interoperability limits.",
        "{}",
    ),
    (
        "chart.definition.get",
        "index: 0-based chart.list index. Read actual current OOXML, dimensions, generation/address/instance_id. Only direct, unshared, unlocked HwpCraft canonical authoring profiles; unknown settings are rejected to prevent configure data loss.",
        r#"{"index":0}"#,
    ),
];

const OWNER_URI: &str = "urn:hwpcraft:chart:1";
const MAX_XML: usize = 2 * 1024 * 1024;
// Chart OLE server identifier published in the existing open-source container
// contract. This identifier is metadata, not copied binary/chart resources.
// Its presence alone does not establish native Hancom interoperability.
const CHART_CLSID: [u8; 16] = [
    0x37, 0xa1, 0x3d, 0x4c, 0x90, 0xdc, 0xb9, 0x47, 0x9b, 0xed, 0x59, 0xda, 0xe3, 0x52, 0xa2, 0x80,
];
const OLE_ID: u32 = 0x246f_6c65;
const COLORS: &[&str] = &[
    "4472C4", "ED7D31", "70AD47", "A5A5A5", "FFC000", "5B9BD5", "8064A2", "C0504D",
];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Column,
    Bar,
    Line,
    Pie,
    Doughnut,
    Scatter,
    StockHlc,
    StockOhlc,
    #[serde(rename = "column_3d")]
    Column3d,
    #[serde(rename = "bar_3d")]
    Bar3d,
    #[serde(rename = "pie_3d")]
    Pie3d,
}

impl Kind {
    fn stock(self) -> bool {
        matches!(self, Self::StockHlc | Self::StockOhlc)
    }
    fn pie(self) -> bool {
        matches!(self, Self::Pie | Self::Pie3d | Self::Doughnut)
    }
    fn bars(self) -> bool {
        matches!(
            self,
            Self::Column | Self::Bar | Self::Column3d | Self::Bar3d
        )
    }
    fn horizontal(self) -> bool {
        matches!(self, Self::Bar | Self::Bar3d)
    }
    fn three_d(self) -> bool {
        matches!(self, Self::Column3d | Self::Bar3d | Self::Pie3d)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ScatterMode {
    #[default]
    Marker,
    Line,
    LineMarker,
    SmoothMarker,
}
impl ScatterMode {
    fn xml(self) -> &'static str {
        match self {
            Self::Marker => "marker",
            Self::Line => "line",
            Self::LineMarker => "lineMarker",
            Self::SmoothMarker => "smoothMarker",
        }
    }
}

#[derive(Clone)]
struct Ring {
    title: String,
    labels: Vec<String>,
    values: Vec<f64>,
    legend: bool,
    first_color: String,
}

impl Ring {
    fn from_xml(xml: &[u8]) -> Result<Option<Self>> {
        let Ok(text) = std::str::from_utf8(xml) else {
            return Ok(None);
        };
        if !text.contains(&format!("uri=\"{OWNER_URI}\""))
            || !text.contains("<hcchart:created version=\"1\"/>")
            || !text.contains("<c:doughnutChart>")
        {
            return Ok(None);
        }
        // The ownership marker alone cannot prove that replacing this render
        // payload preserves settings introduced by another editor. A strict
        // profile mismatch retains the original renderer, including native
        // objects with an extended HwpCraft chart stream.
        if Spec::from_xml(xml, width(), height()).is_err() {
            return Ok(None);
        }
        if xml.len() > MAX_XML || !text.contains("<c:holeSize val=\"50\"/>") {
            bail!("자체 도넛 차트 형식 또는 크기 상한이 맞지 않습니다.");
        }
        let data = chart_io::scan_chart_values(xml)
            .map_err(|e| anyhow!("도넛 데이터 검증 실패: {e:?}"))?;
        if data.series.len() != 1
            || data.series[0].values.is_empty()
            || data.series[0].values.len() > 1000
            || data.series[0].values.len() != data.series[0].labels.len()
        {
            bail!("도넛 차트는 1계열, 1–1000개 대응값을 지원합니다.");
        }
        let decode = |text: &str| {
            quick_xml::escape::unescape(text)
                .map(|s| s.into_owned())
                .map_err(|e| anyhow!("도넛 라벨 XML 해제 실패: {e}"))
        };
        let labels = data.series[0]
            .labels
            .iter()
            .map(|p| decode(&p.text))
            .collect::<Result<Vec<_>>>()?;
        let values = data.series[0]
            .values
            .iter()
            .map(|p| {
                p.text
                    .parse::<f64>()
                    .map_err(|e| anyhow!("도넛 숫자 해석 실패: {e}"))
            })
            .collect::<Result<Vec<_>>>()?;
        if values.iter().any(|v| !v.is_finite()) {
            bail!("도넛 값은 유한한 숫자여야 합니다.");
        }
        let first_color = text
            .split_once("<a:srgbClr val=\"")
            .and_then(|(_, s)| s.get(..6))
            .filter(|s| s.bytes().all(|c| c.is_ascii_hexdigit()))
            .unwrap_or(COLORS[0])
            .to_owned();
        let title = actual_title(xml)?;
        Ok(Some(Self {
            title,
            labels,
            values,
            legend: text.contains("<c:legend>"),
            first_color,
        }))
    }

    fn svg(&self, bounds: [f64; 4]) -> Result<String> {
        let [x, y, width, height] = bounds;
        if bounds.iter().any(|v| !v.is_finite() || v.abs() > 1.0e7) || width <= 0.0 || height <= 0.0
        {
            bail!("도넛 차트 표시 영역이 유효하지 않습니다.");
        }
        let title_h = if self.title.is_empty() {
            8.0
        } else {
            28.0_f64.min(height * 0.25)
        };
        let legend_w = if self.legend { width * 0.30 } else { 0.0 };
        let cx = (width - legend_w) * 0.5;
        let cy = title_h + (height - title_h) * 0.5;
        let radius = ((width - legend_w - 20.0).min(height - title_h - 16.0) * 0.5).max(1.0);
        let inner = radius * 0.5;
        let mut svg = format!(
            "<g data-hwpcraft-chart=\"doughnut\"><svg x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" overflow=\"hidden\"><rect width=\"{width}\" height=\"{height}\" fill=\"white\" stroke=\"#8c8c8c\" stroke-width=\"0.75\"/>"
        );
        if !self.title.is_empty() {
            svg.push_str(&format!("<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"14\" fill=\"#222222\">{}</text>",width*0.5,title_h-8.0,escape(&self.title)));
        }
        // Normalize before summing to avoid overflow for finite values received
        // through the existing value-edit API. Pie-family areas use magnitude.
        let maximum = self.values.iter().map(|v| v.abs()).fold(0.0_f64, f64::max);
        let sum = if maximum > 0.0 {
            self.values.iter().map(|v| v.abs() / maximum).sum::<f64>()
        } else {
            0.0
        };
        let mut start = -std::f64::consts::FRAC_PI_2;
        for (i, value) in self.values.iter().enumerate() {
            if *value == 0.0 || sum == 0.0 {
                continue;
            }
            let sweep = (value.abs() / maximum) / sum * std::f64::consts::TAU;
            let end = start + sweep;
            let large = usize::from(sweep > std::f64::consts::PI);
            let point = |r: f64, a: f64| (cx + r * a.cos(), cy + r * a.sin());
            let (a, b) = point(radius, start);
            let (c, d) = point(radius, end);
            let (e, f) = point(inner, end);
            let (g, h) = point(inner, start);
            let path = if sweep >= std::f64::consts::TAU - 1e-10 {
                format!(
                    "M {} {cy} A {radius} {radius} 0 1 1 {} {cy} A {radius} {radius} 0 1 1 {} {cy} Z M {} {cy} A {inner} {inner} 0 1 0 {} {cy} A {inner} {inner} 0 1 0 {} {cy} Z",
                    cx + radius,
                    cx - radius,
                    cx + radius,
                    cx + inner,
                    cx - inner,
                    cx + inner
                )
            } else {
                format!(
                    "M {a} {b} A {radius} {radius} 0 {large} 1 {c} {d} L {e} {f} A {inner} {inner} 0 {large} 0 {g} {h} Z"
                )
            };
            let color = if i == 0 {
                self.first_color.as_str()
            } else {
                COLORS[i % COLORS.len()]
            };
            svg.push_str(&format!("<path d=\"{path}\" fill=\"#{color}\" fill-rule=\"evenodd\" stroke=\"white\" stroke-width=\"0.5\"><title>{}: {value}</title></path>",escape(&self.labels[i])));
            start = end;
        }
        if sum == 0.0 {
            svg.push_str(&format!("<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{radius}\" fill=\"none\" stroke=\"#aaaaaa\"/><text x=\"{cx}\" y=\"{cy}\" text-anchor=\"middle\" fill=\"#666666\" font-size=\"12\">0</text>"));
        }
        if self.legend {
            let rows = ((height - title_h - 8.0) / 18.0).max(0.0) as usize;
            for (i, label) in self.labels.iter().take(rows).enumerate() {
                let color = if i == 0 {
                    self.first_color.as_str()
                } else {
                    COLORS[i % COLORS.len()]
                };
                let ly = title_h + 14.0 + i as f64 * 18.0;
                let lx = width - legend_w + 4.0;
                svg.push_str(&format!("<rect x=\"{lx}\" y=\"{}\" width=\"9\" height=\"9\" fill=\"#{color}\"/><text x=\"{}\" y=\"{ly}\" font-family=\"sans-serif\" font-size=\"11\" fill=\"#222222\">{}</text>",ly-9.0,lx+13.0,escape(label)));
            }
        }
        svg.push_str("</svg></g>");
        Ok(svg)
    }
}

#[derive(Clone)]
struct OwnedRing {
    key: (usize, usize, usize),
    nested: bool,
    ring: Ring,
}

fn owned_rings(core: &DocumentCore) -> Result<Vec<OwnedRing>> {
    // Typical text/image documents return before any layout or CFB work.
    if !core
        .document()
        .bin_data_content
        .iter()
        .any(|b| b.extension.eq_ignore_ascii_case("ole"))
    {
        return Ok(Vec::new());
    }
    let charts: Value = serde_json::from_str(&core.list_charts_native().map_err(err)?)?;
    let mut out = Vec::new();
    let mut parsed = std::collections::BTreeMap::<usize, Option<Ring>>::new();
    for chart in charts.as_array().context("차트 목록")? {
        let Some(slot) = chart["nestedCopy"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
        else {
            continue;
        };
        if !parsed.contains_key(&slot) {
            let content = core
                .document()
                .bin_data_content
                .get(slot)
                .ok_or_else(|| anyhow!("차트 리소스 주소 범위 초과"))?;
            let Some(bytes) = content.data.load_limited(4 * MAX_XML) else {
                // No ownership proof: keep the pre-existing native path for
                // unrelated large charts instead of making them new errors.
                parsed.insert(slot, None);
                continue;
            };
            let streams = chart_io::all_ole_streams(&bytes)
                .ok_or_else(|| anyhow!("차트 렌더 CFB 해석 실패"))?;
            let ring = streams
                .iter()
                .find(|(name, _)| name.trim_start_matches('/') == "OOXMLChartContents")
                .map(|(_, xml)| Ring::from_xml(xml))
                .transpose()?
                .flatten();
            parsed.insert(slot, ring);
        }
        if let Some(ring) = parsed.get(&slot).and_then(Option::as_ref) {
            let index = |field: &str| {
                chart[field]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| anyhow!("차트 렌더 원본 주소 없음"))
            };
            out.push(OwnedRing {
                key: (index("section")?, index("paragraph")?, index("control")?),
                nested: chart["container"].as_array().is_some_and(|p| !p.is_empty()),
                ring: ring.clone(),
            });
            if out.len() > 64 {
                bail!("자체 도넛 렌더는 문서당 64개 이하에서 지원합니다.");
            }
        }
    }
    Ok(out)
}

fn page_ring_plan(
    core: &DocumentCore,
    page: u32,
    profile: hwpcraft_doc::RenderProfile,
) -> Result<Vec<(String, String)>> {
    let rings = owned_rings(core)?;
    if rings.is_empty() {
        return Ok(Vec::new());
    }
    let tree = core
        .build_page_layer_tree_with_profile(page, profile)
        .map_err(err)?;
    fn visit(
        node: &hwpcraft_doc::LayerNode,
        rings: &[OwnedRing],
        out: &mut Vec<(String, String)>,
        depth: usize,
    ) -> Result<()> {
        if depth > 128 {
            bail!("도넛 표시 레이어 중첩 상한 초과");
        }
        match &node.kind {
            hwpcraft_doc::LayerNodeKind::Group { children, .. } => {
                for child in children {
                    visit(child, rings, out, depth + 1)?;
                }
            }
            hwpcraft_doc::LayerNodeKind::ClipRect { child, .. } => {
                visit(child, rings, out, depth + 1)?
            }
            hwpcraft_doc::LayerNodeKind::Leaf { ops } => {
                for op in ops {
                    if let hwpcraft_doc::PaintOp::RawSvg { bbox, raw } = op {
                        let Some(reference) = &raw.control_ref else {
                            continue;
                        };
                        let key = (
                            reference.section_index,
                            reference.para_index,
                            reference.control_index,
                        );
                        let matches = rings.iter().filter(|r| r.key == key).collect::<Vec<_>>();
                        if matches.is_empty() {
                            continue;
                        }
                        if raw.cell_context.is_some() || matches.len() != 1 || matches[0].nested {
                            bail!("도넛 표시 투영은 본문 직접 차트만 지원합니다.");
                        }
                        if reference.kind != "ole" || raw.svg.is_empty() {
                            bail!("도넛 표시 원본 제어 종류가 일치하지 않습니다.");
                        }
                        out.push((
                            raw.svg.clone(),
                            matches[0]
                                .ring
                                .svg([bbox.x, bbox.y, bbox.width, bbox.height])?,
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    visit(&tree.root, &rings, &mut out, 0)?;
    Ok(out)
}

pub(crate) fn page_has_chart_overrides(
    core: &DocumentCore,
    page: u32,
    profile: hwpcraft_doc::RenderProfile,
) -> Result<bool> {
    Ok(!page_ring_plan(core, page, profile)?.is_empty()
        || super::chart_rendering::page_has_overrides(core, page, profile)?)
}

pub(crate) fn with_chart_overrides(
    core: &DocumentCore,
    page: u32,
    profile: hwpcraft_doc::RenderProfile,
    mut svg: String,
) -> Result<String> {
    let plan = page_ring_plan(core, page, profile)?;
    // Validate the entire mapping before changing a payload. Ambiguous duplicate
    // fragments are errors; never replace all matching strings across objects.
    let mut seen = BTreeSet::new();
    for (old, _) in &plan {
        if !seen.insert(old) || svg.match_indices(old).count() != 1 {
            bail!("도넛 원본 paint payload를 쪽 SVG에 정확히 대응하지 못했습니다.");
        }
    }
    for (old, new) in plan {
        svg = svg.replacen(&old, &new, 1);
    }
    super::chart_rendering::with_overrides(core, page, profile, svg)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Series {
    name: String,
    values: Vec<f64>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x_values: Option<Vec<f64>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    kind: Kind,
    #[serde(default)]
    title: String,
    #[serde(default)]
    categories: Vec<String>,
    series: Vec<Series>,
    #[serde(default = "width")]
    width_mm: f64,
    #[serde(default = "height")]
    height_mm: f64,
    #[serde(default = "yes")]
    legend: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scatter_style: Option<ScatterMode>,
}
fn width() -> f64 {
    120.0
}
fn height() -> f64 {
    75.0
}
fn yes() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    chart: Spec,
    #[serde(default)]
    expected_generation: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configure {
    index: usize,
    chart: Spec,
    expected_generation: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChartIndex {
    index: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyParams {}

fn valid_text(text: &str, limit: usize) -> Result<()> {
    if text.len() > limit
        || text.chars().any(|c| {
            !(matches!(c, '\t' | '\n' | '\r')
                || ('\u{20}'..='\u{d7ff}').contains(&c)
                || ('\u{e000}'..='\u{fffd}').contains(&c)
                || c >= '\u{10000}')
        })
    {
        bail!("차트 문자열 길이 또는 XML 문자 범위가 잘못됐습니다.");
    }
    Ok(())
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Both standard float formats round-trip without an explicit precision.
/// Pick the shorter spelling so subnormals do not expand into hundreds of
/// decimal zeroes per point. Keep signed zero and the existing value limits.
fn number_text(value: f64) -> String {
    let decimal = value.to_string();
    let scientific = format!("{value:e}");
    if scientific.len() < decimal.len() {
        scientific
    } else {
        decimal
    }
}

fn numeric_block(xml: &mut String, tag: &str, values: &[f64]) {
    xml.push_str(&format!(
        "<c:{tag}><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>",
        values.len()
    ));
    for (index, value) in values.iter().enumerate() {
        xml.push_str(&format!(
            "<c:pt idx=\"{index}\"><c:v>{}</c:v></c:pt>",
            number_text(*value)
        ));
    }
    xml.push_str(&format!("</c:numLit></c:{tag}>"));
}

fn chart_types() -> Value {
    let kinds = [
        (Kind::Column, "세로 막대형"),
        (Kind::Bar, "가로 막대형"),
        (Kind::Line, "꺾은선형"),
        (Kind::Pie, "원형"),
        (Kind::Doughnut, "도넛형"),
        (Kind::Scatter, "분산형"),
        (Kind::StockHlc, "고가·저가·종가"),
        (Kind::StockOhlc, "시가·고가·저가·종가"),
        (Kind::Column3d, "입체 세로 막대형"),
        (Kind::Bar3d, "입체 가로 막대형"),
        (Kind::Pie3d, "입체 원형"),
    ];
    json!({"types":kinds.into_iter().map(|(kind,label)|json!({
        "kind":kind,"label":label,"requiredFields":if kind==Kind::Scatter {vec!["series[].name","series[].values","series[].x_values"]}else{vec!["categories","series[].name","series[].values"]},
        "constraints":{"pointsMin":1,"pointsMax":1000,"seriesMin":if kind==Kind::StockHlc {3}else if kind==Kind::StockOhlc{4}else{1},"seriesMax":if kind.pie(){1}else if kind==Kind::StockHlc{3}else if kind==Kind::StockOhlc{4}else{64},"totalValuesMax":16000,"absoluteValueMax":1e12,"widthMm":[10,300],"heightMm":[10,300],"sharedX":kind==Kind::Scatter,"scatterStyles":if kind==Kind::Scatter {vec!["marker","line","line_marker","smooth_marker"]}else{vec![]},"seriesRoles":if kind==Kind::StockHlc{vec!["high","low","close"]}else if kind==Kind::StockOhlc{vec!["open","high","low","close"]}else{vec![]},"priceBoundsCheckedOn":"create/configure/chart.data for canonical authored direct, unshared stock profiles; foreign/unknown profiles retain native validation","nonNegative":kind.pie(),"positiveSum":kind.pie(),"camera":if kind.three_d(){json!({"rotX":15,"rotY":20,"perspective":30,"rightAngleAxes":true})}else{Value::Null}},
        "nativeInterop":"unverified"
    })).collect::<Vec<_>>(),"unsupported":[
        {"kind":"area","reason":"The current public parser/renderer has no area plot implementation."},
        {"kind":"radar","reason":"The current public parser/renderer has no radar plot implementation."},
        {"kind":"line_3d","reason":"The current renderer uses a flat line approximation; genuine 3D authoring is not offered."},
        {"kind":"bubble","reason":"Bubble-size data and rendering are not implemented."},
        {"kind":"surface","reason":"Surface data and rendering are not implemented."},
        {"kind":"combo","reason":"Per-series plot/axis authoring is not part of this profile."}
    ],"limitations":["Scatter authoring uses the same X coordinates and point count for every series so the existing chart.data labels contract remains exact.","3D projection is an implementation-specific DrawingML rendering; native Hancom display/editing parity is unverified.","Canonical HwpCraft body profiles hide legends and decode XML strings in screen/PDF output; foreign, shared, nested or extended profiles retain their native rendering. Hidden-legend space is not fully reclaimed."]})
}

/// A strict XML profile comparison prevents the lossy public render model
/// from silently discarding foreign axes, data labels, external links, etc.
/// Numeric lexical changes made by chart.data (9.00 versus 9) are equivalent.
fn xml_profile(bytes: &[u8]) -> Result<Vec<String>> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_reader(bytes);
    let mut output = Vec::new();
    let mut stack = Vec::<String>::new();
    loop {
        let event = reader.read_event()?;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let name = e.name().as_ref().to_owned();
                let mut attrs = e
                    .attributes()
                    .map(|a| {
                        let a = a?;
                        Ok((
                            a.key.as_ref().to_owned(),
                            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                                .into_owned(),
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                if name == "a:srgbClr" {
                    for (key, value) in &mut attrs {
                        if key == "val" {
                            value.make_ascii_uppercase();
                        }
                    }
                }
                attrs.sort();
                output.push(format!("S{name}{attrs:?}"));
                if empty {
                    output.push(format!("E{name}"));
                } else {
                    stack.push(name);
                }
            }
            Event::End(e) => {
                let name = e.name().as_ref().to_owned();
                if stack.pop().as_deref() != Some(&name) {
                    bail!("차트 XML 요소 구조가 잘못됐습니다.");
                }
                output.push(format!("E{name}"));
            }
            Event::Text(e) => {
                let text = quick_xml::escape::unescape(
                    &e.xml_content(quick_xml::XmlVersion::Implicit1_0),
                )?
                .into_owned();
                if !text.trim().is_empty() {
                    let text = if stack.last().is_some_and(|s| s == "c:v")
                        && stack.iter().rev().nth(1).is_some_and(|s| s == "c:pt")
                    {
                        text.parse::<f64>().map(number_text).unwrap_or(text)
                    } else {
                        text
                    };
                    append_profile_text(&mut output, &text);
                }
            }
            Event::GeneralRef(e) => {
                let text = quick_xml::escape::unescape(&format!("&{};", e.as_ref()))?.into_owned();
                append_profile_text(&mut output, &text);
            }
            Event::Decl(_) => {}
            Event::Eof => break,
            _ => bail!("전체 구성을 지원하지 않는 차트 XML 표현입니다."),
        }
    }
    if !stack.is_empty() {
        bail!("차트 XML이 끝나지 않았습니다.");
    }
    Ok(output)
}

fn append_profile_text(output: &mut Vec<String>, text: &str) {
    if let Some(last) = output.last_mut().filter(|s| s.starts_with('T')) {
        last.push_str(text);
    } else {
        output.push(format!("T{text}"));
    }
}

fn actual_title(xml: &[u8]) -> Result<String> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_reader(xml);
    let mut title = String::new();
    let mut in_title = false;
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.name().as_ref() == "c:title" => in_title = true,
            Event::End(e) if e.name().as_ref() == "c:title" => in_title = false,
            Event::Start(e) if in_title && e.name().as_ref() == "a:t" => {
                let value = reader.read_text(e.name())?;
                title.push_str(&quick_xml::escape::unescape(
                    &value.xml_content(quick_xml::XmlVersion::Implicit1_0),
                )?);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(title)
}

impl Spec {
    fn validate(&self) -> Result<()> {
        valid_text(&self.title, 4096)?;
        if !self.width_mm.is_finite()
            || !self.height_mm.is_finite()
            || !(10.0..=300.0).contains(&self.width_mm)
            || !(10.0..=300.0).contains(&self.height_mm)
        {
            bail!("차트 크기는 유한한 10–300 mm입니다.");
        }
        let points = if self.kind == Kind::Scatter {
            self.series.first().map_or(0, |s| s.values.len())
        } else {
            self.categories.len()
        };
        if points == 0
            || points > 1000
            || self.series.is_empty()
            || self.series.len() > 64
            || points.saturating_mul(self.series.len()) > 16_000
        {
            bail!("차트는 1–1000개 범주, 1–64개 계열, 합계 16000개 값 이하입니다.");
        }
        for label in &self.categories {
            valid_text(label, 1024)?;
        }
        let mut names = BTreeSet::new();
        for series in &self.series {
            valid_text(&series.name, 1024)?;
            if series.name.trim().is_empty() || !names.insert(&series.name) {
                bail!("차트 계열 이름은 비어 있지 않고 서로 달라야 합니다.");
            }
            if series.values.len() != points
                || series
                    .values
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 1.0e12)
            {
                bail!("계열 값 개수는 범주 개수와 같아야 하며 각 값은 유한한 ±1e12 범위입니다.");
            }
            if self.kind == Kind::Scatter {
                let x = series
                    .x_values
                    .as_ref()
                    .context("분산형에는 각 계열의 x_values가 필요합니다.")?;
                if !self.categories.is_empty()
                    || x.len() != points
                    || x.iter().any(|v| !v.is_finite() || v.abs() > 1.0e12)
                    || self.series[0].x_values.as_ref() != Some(x)
                {
                    bail!(
                        "분산형은 범주 없이 동일한 유한한 X 좌표를 모든 계열에 사용합니다(각 ±1e12, Y와 같은 개수)."
                    );
                }
            } else if series.x_values.is_some() || self.scatter_style.is_some() {
                bail!("x_values와 scatter_style은 분산형 전용입니다.");
            }
            if let Some(color) = &series.color
                && (color.len() != 7
                    || !color.starts_with('#')
                    || !color[1..].bytes().all(|c| c.is_ascii_hexdigit()))
            {
                bail!("차트 색상은 #RRGGBB입니다.");
            }
        }
        if self.kind.pie()
            && (self.series.len() != 1
                || self.series[0].values.iter().any(|v| *v < 0.0)
                || self.series[0].values.iter().sum::<f64>() <= 0.0)
        {
            bail!("원형·도넛 차트는 한 계열, 음수가 없는 값, 0보다 큰 합계가 필요합니다.");
        }
        if self.kind.stock() {
            let count = if self.kind == Kind::StockHlc { 3 } else { 4 };
            if self.series.len() != count {
                bail!(
                    "주가형 계열은 고가·저가·종가 3개 또는 시가·고가·저가·종가 4개 순서가 필요합니다."
                );
            }
            let high = count - 3;
            let low = count - 2;
            let close = count - 1;
            for i in 0..points {
                let h = self.series[high].values[i];
                let l = self.series[low].values[i];
                if h < l
                    || !(l..=h).contains(&self.series[close].values[i])
                    || (count == 4 && !(l..=h).contains(&self.series[0].values[i]))
                {
                    bail!("주가형의 고가는 저가 이상이며 시가·종가는 저가와 고가 사이여야 합니다.");
                }
            }
        }
        Ok(())
    }

    fn xml(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:hcchart=\"urn:hwpcraft:chart:1\"><c:lang val=\"ko-KR\"/><c:chart>",
        );
        if !self.title.is_empty() {
            xml.push_str(&format!("<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>",escape(&self.title)));
        }
        xml.push_str("<c:autoTitleDeleted val=\"1\"/>");
        if self.kind.three_d() {
            // Public DrawingML camera profile. This renderer projects genuine
            // bar faces / elliptical pie walls, not a renamed flat plot.
            xml.push_str("<c:view3D><c:rotX val=\"15\"/><c:hPercent val=\"100\"/><c:rotY val=\"20\"/><c:depthPercent val=\"100\"/><c:rAngAx val=\"1\"/><c:perspective val=\"30\"/></c:view3D>");
        }
        xml.push_str("<c:plotArea><c:layout/>");
        let plot = match self.kind {
            Kind::Column | Kind::Bar => "barChart",
            Kind::Line => "lineChart",
            Kind::Pie => "pieChart",
            Kind::Doughnut => "doughnutChart",
            Kind::Scatter => "scatterChart",
            Kind::StockHlc | Kind::StockOhlc => "stockChart",
            Kind::Column3d | Kind::Bar3d => "bar3DChart",
            Kind::Pie3d => "pie3DChart",
        };
        xml.push_str(&format!("<c:{plot}>"));
        match self.kind {
            Kind::Column | Kind::Bar | Kind::Column3d | Kind::Bar3d => xml.push_str(&format!(
                "<c:barDir val=\"{}\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>",
                if self.kind.horizontal() { "bar" } else { "col" }
            )),
            Kind::Line => xml.push_str("<c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>"),
            Kind::Scatter => xml.push_str(&format!(
                "<c:scatterStyle val=\"{}\"/><c:varyColors val=\"0\"/>",
                self.scatter_style.unwrap_or_default().xml()
            )),
            Kind::StockHlc | Kind::StockOhlc => {}
            _ => xml.push_str("<c:varyColors val=\"1\"/>"),
        }
        for (index, series) in self.series.iter().enumerate() {
            let color = series
                .color
                .as_deref()
                .map(|c| &c[1..])
                .unwrap_or(COLORS[index % COLORS.len()]);
            xml.push_str(&format!("<c:ser><c:idx val=\"{index}\"/><c:order val=\"{index}\"/><c:tx><c:v>{}</c:v></c:tx><c:spPr><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill><a:ln w=\"25400\">",escape(&series.name)));
            if self.kind.stock() {
                xml.push_str("<a:noFill/>");
            } else {
                xml.push_str(&format!(
                    "<a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill>"
                ));
            }
            xml.push_str("</a:ln></c:spPr>");
            if self.kind.stock() {
                xml.push_str(&format!(
                    "<c:marker><c:symbol val=\"{}\"/></c:marker>",
                    if index == self.series.len() - 1 {
                        "auto"
                    } else {
                        "none"
                    }
                ));
            }
            if self.kind == Kind::Scatter {
                numeric_block(
                    &mut xml,
                    "xVal",
                    series.x_values.as_deref().context("분산형 X 값 없음")?,
                );
                numeric_block(&mut xml, "yVal", &series.values);
                xml.push_str("</c:ser>");
                continue;
            }
            xml.push_str(&format!(
                "<c:cat><c:strLit><c:ptCount val=\"{}\"/>",
                self.categories.len()
            ));
            for (i, label) in self.categories.iter().enumerate() {
                xml.push_str(&format!(
                    "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>",
                    escape(label)
                ));
            }
            xml.push_str(&format!("</c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>",series.values.len()));
            for (i, value) in series.values.iter().enumerate() {
                xml.push_str(&format!(
                    "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>",
                    number_text(*value)
                ));
            }
            xml.push_str("</c:numLit></c:val>");
            if self.kind == Kind::Line {
                xml.push_str("<c:smooth val=\"0\"/>");
            }
            xml.push_str("</c:ser>");
        }
        if self.kind.bars() {
            xml.push_str("<c:gapWidth val=\"150\"/>");
            if self.kind.three_d() {
                xml.push_str("<c:gapDepth val=\"150\"/><c:shape val=\"box\"/>");
            }
        } else {
            match self.kind {
                Kind::Line => xml.push_str("<c:marker val=\"1\"/><c:smooth val=\"0\"/>"),
                Kind::Doughnut => {
                    xml.push_str("<c:firstSliceAng val=\"0\"/><c:holeSize val=\"50\"/>")
                }
                Kind::Pie => xml.push_str("<c:firstSliceAng val=\"0\"/>"),
                Kind::StockHlc | Kind::StockOhlc => {
                    xml.push_str("<c:hiLowLines/>");
                    if self.kind == Kind::StockOhlc {
                        xml.push_str("<c:upDownBars><c:gapWidth val=\"150\"/><c:upBars/><c:downBars/></c:upDownBars>");
                    }
                }
                _ => {}
            }
        }
        if !self.kind.pie() {
            xml.push_str("<c:axId val=\"100\"/><c:axId val=\"200\"/>");
        }
        xml.push_str(&format!("</c:{plot}>"));
        if self.kind == Kind::Scatter {
            for (id, cross, pos) in [(100, 200, "b"), (200, 100, "l")] {
                xml.push_str(&format!("<c:valAx><c:axId val=\"{id}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{pos}\"/><c:majorGridlines/><c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"{cross}\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>"));
            }
        } else if !self.kind.pie() {
            let (catpos, valpos) = if self.kind.horizontal() {
                ("l", "b")
            } else {
                ("b", "l")
            };
            xml.push_str(&format!("<c:catAx><c:axId val=\"100\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{catpos}\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"200\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/></c:catAx><c:valAx><c:axId val=\"200\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{valpos}\"/><c:majorGridlines/><c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"100\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>"));
        }
        xml.push_str("</c:plotArea>");
        if self.legend {
            xml.push_str(
                "<c:legend><c:legendPos val=\"r\"/><c:layout/><c:overlay val=\"0\"/></c:legend>",
            );
        }
        xml.push_str("<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/><c:showDLblsOverMax val=\"0\"/></c:chart><c:extLst><c:ext uri=\"urn:hwpcraft:chart:1\"><hcchart:created version=\"1\"/></c:ext></c:extLst></c:chartSpace>");
        if xml.len() > MAX_XML {
            bail!("차트 XML은 2 MiB 이하여야 합니다.");
        }
        let data = chart_io::scan_chart_values(xml.as_bytes())
            .map_err(|e| anyhow!("차트 데이터 지도 검증 실패: {e:?}"))?;
        if data.series.len() != self.series.len() {
            bail!("차트 계열 구성이 일치하지 않습니다.");
        }
        let svg = if self.kind == Kind::Doughnut {
            Ring {
                title: self.title.clone(),
                labels: self.categories.clone(),
                values: self.series[0].values.clone(),
                legend: self.legend,
                first_color: self.series[0]
                    .color
                    .as_deref()
                    .map(|s| s[1..].to_owned())
                    .unwrap_or_else(|| COLORS[0].into()),
            }
            .svg([0.0, 0.0, 480.0, 300.0])?
        } else {
            let parsed = chart_io::OoxmlChart::parse(xml.as_bytes())
                .ok_or_else(|| anyhow!("차트 렌더 모델 검증 실패"))?;
            parsed.render_svg(0.0, 0.0, 480.0, 300.0)
        };
        if svg.is_empty() || !svg.contains("<") {
            bail!("차트가 렌더되지 않았습니다.");
        }
        Ok(xml.into_bytes())
    }
}

/// Only a fully reconstructed authoring profile can enter our presentation
/// adapter. Unknown/native settings keep their original renderer and bytes.
/// No document or CFB is mutated, including the XML's actual legend flag.
pub(crate) fn authored_render_model(xml: &[u8]) -> Option<(chart_io::OoxmlChart, bool)> {
    let text = std::str::from_utf8(xml).ok()?;
    if xml.len() > MAX_XML
        || !text.contains(&format!("uri=\"{OWNER_URI}\""))
        || !text.contains("<hcchart:created version=\"1\"/>")
    {
        return None;
    }
    let spec = Spec::from_xml(xml, width(), height()).ok()?;
    if spec.kind == Kind::Doughnut {
        return None;
    }
    let mut model = chart_io::OoxmlChart::parse(xml)?;
    let title = (!spec.title.is_empty()).then_some(spec.title);
    let needs_correction = !spec.legend
        || model.title != title
        || model.categories != spec.categories
        || model
            .series
            .iter()
            .zip(&spec.series)
            .any(|(actual, expected)| actual.name != expected.name);
    if !needs_correction {
        return None;
    }
    model.title = title;
    model.categories = spec.categories;
    for (actual, expected) in model.series.iter_mut().zip(spec.series) {
        actual.name = expected.name;
    }
    Some((model, spec.legend))
}

impl Spec {
    fn from_xml(xml: &[u8], width_mm: f64, height_mm: f64) -> Result<Self> {
        use chart_io::{OoxmlChartType as T, ScatterStyle as S};
        if xml.len() > MAX_XML {
            bail!("차트 XML 크기 한도 초과");
        }
        let text = std::str::from_utf8(xml)?;
        let parsed = chart_io::OoxmlChart::parse(xml).context("차트 구성 모델 없음")?;
        let kind = if text.contains("<c:doughnutChart>") {
            Kind::Doughnut
        } else {
            match (parsed.chart_type, parsed.is_3d) {
                (T::Column, false) => Kind::Column,
                (T::Column, true) => Kind::Column3d,
                (T::Bar, false) => Kind::Bar,
                (T::Bar, true) => Kind::Bar3d,
                (T::Line, false) => Kind::Line,
                (T::Pie, false) => Kind::Pie,
                (T::Pie, true) => Kind::Pie3d,
                (T::Scatter, false) => Kind::Scatter,
                (T::Stock, false) => {
                    if parsed.has_up_down_bars {
                        Kind::StockOhlc
                    } else {
                        Kind::StockHlc
                    }
                }
                _ => bail!("전체 구성을 지원하지 않는 차트 종류입니다."),
            }
        };
        let data = chart_io::scan_chart_values(xml)
            .map_err(|e| anyhow!("차트 데이터 해석 실패: {e:?}"))?;
        if data.series.len() != parsed.series.len() {
            bail!("차트 계열 지도가 일치하지 않습니다.");
        }
        let decode =
            |text: &str| -> Result<String> { Ok(quick_xml::escape::unescape(text)?.into_owned()) };
        let spec = Self {
            kind,
            title: actual_title(xml)?,
            categories: if kind == Kind::Scatter {
                Vec::new()
            } else {
                data.series
                    .first()
                    .context("차트 계열 없음")?
                    .labels
                    .iter()
                    .map(|p| decode(&p.text))
                    .collect::<Result<_>>()?
            },
            series: parsed
                .series
                .into_iter()
                .zip(data.series.iter())
                .map(|(s, d)| {
                    Ok(Series {
                        name: decode(d.name.as_deref().context("차트 계열 이름 없음")?)?,
                        values: d
                            .values
                            .iter()
                            .map(|p| Ok(decode(&p.text)?.parse::<f64>()?))
                            .collect::<Result<_>>()?,
                        color: s.color.map(|c| format!("#{c:06X}")),
                        x_values: if kind == Kind::Scatter {
                            Some(
                                d.labels
                                    .iter()
                                    .map(|p| Ok(decode(&p.text)?.parse::<f64>()?))
                                    .collect::<Result<_>>()?,
                            )
                        } else {
                            None
                        },
                    })
                })
                .collect::<Result<_>>()?,
            width_mm,
            height_mm,
            legend: text.contains("<c:legend>"),
            scatter_style: (kind == Kind::Scatter).then_some(match parsed.scatter_style {
                S::Marker => ScatterMode::Marker,
                S::Line => ScatterMode::Line,
                S::LineMarker => ScatterMode::LineMarker,
                S::SmoothMarker => ScatterMode::SmoothMarker,
            }),
        };
        spec.validate()?;
        if xml_profile(xml)? != xml_profile(&spec.xml()?)? {
            bail!(
                "지원 프로필 밖의 차트 설정이 있습니다. 손실 방지를 위해 전체 구성 편집을 거절합니다. chart.data를 사용하세요."
            );
        }
        Ok(spec)
    }
}

fn resource_plan(doc: &Document) -> Result<(u16, u16)> {
    if doc.bin_data_content.len() != doc.doc_info.bin_data_list.len() {
        bail!("BinData 순번과 메타데이터가 불일치한 문서에는 차트를 추가할 수 없습니다.");
    }
    let position = u16::try_from(doc.bin_data_content.len() + 1)
        .map_err(|_| anyhow!("BinData 참조가 고갈됐습니다."))?;
    let mut used = BTreeSet::new();
    for entry in &doc.bin_data_content {
        if entry.id != 0 && !used.insert(entry.id) {
            bail!("중복된 BinData 리소스 ID가 있습니다.");
        }
    }
    let mut metadata = BTreeSet::new();
    for entry in &doc.doc_info.bin_data_list {
        if entry.storage_id != 0 && !metadata.insert(entry.storage_id) {
            bail!("중복된 BinData 메타데이터 ID가 있습니다.");
        }
    }
    let maximum = used
        .iter()
        .chain(metadata.iter())
        .copied()
        .max()
        .unwrap_or(0);
    let storage = maximum
        .checked_add(1)
        .ok_or_else(|| anyhow!("BinData 저장 ID가 고갈됐습니다."))?;
    Ok((position, storage))
}

fn unique_instance(doc: &Document) -> Result<u32> {
    fn shapes(
        shape: &ShapeObject,
        ids: &mut BTreeSet<u32>,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<()> {
        if depth > 16 {
            bail!("차트 인스턴스 ID 검증의 도형 중첩 깊이를 초과했습니다.");
        }
        *nodes += 1;
        if *nodes > 100_000 {
            bail!("차트 리소스 조사 상한을 초과했습니다.");
        }
        ids.insert(shape.common().instance_id);
        if let ShapeObject::Group(group) = shape {
            for child in &group.children {
                shapes(child, ids, depth + 1, nodes)?;
            }
        }
        if let Some(drawing) = shape.drawing() {
            if let Some(text) = &drawing.text_box {
                paragraphs(&text.paragraphs, ids, depth + 1, nodes)?;
            }
            if let Some(caption) = &drawing.caption {
                paragraphs(&caption.paragraphs, ids, depth + 1, nodes)?;
            }
        }
        match shape {
            ShapeObject::Ole(x) => {
                if let Some(c) = &x.caption {
                    paragraphs(&c.paragraphs, ids, depth + 1, nodes)?;
                }
            }
            ShapeObject::Chart(x) => {
                if let Some(c) = &x.caption {
                    paragraphs(&c.paragraphs, ids, depth + 1, nodes)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn paragraphs(
        list: &[Paragraph],
        ids: &mut BTreeSet<u32>,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<()> {
        if depth > 16 {
            bail!("차트 인스턴스 ID 검증의 중첩 깊이를 초과했습니다.");
        }
        for p in list {
            for control in &p.controls {
                *nodes += 1;
                if *nodes > 100_000 {
                    bail!("차트 리소스 조사 상한을 초과했습니다.");
                }
                match control {
                    Control::Shape(s) => shapes(s, ids, depth, nodes)?,
                    Control::Table(t) => {
                        ids.insert(t.common.instance_id);
                        for c in &t.cells {
                            paragraphs(&c.paragraphs, ids, depth + 1, nodes)?;
                        }
                        if let Some(c) = &t.caption {
                            paragraphs(&c.paragraphs, ids, depth + 1, nodes)?;
                        }
                    }
                    Control::Picture(p) => {
                        ids.insert(p.common.instance_id);
                        if let Some(c) = &p.caption {
                            paragraphs(&c.paragraphs, ids, depth + 1, nodes)?;
                        }
                    }
                    Control::Equation(e) => {
                        ids.insert(e.common.instance_id);
                    }
                    Control::Form(form) => {
                        ids.insert(form.common.instance_id);
                    }
                    Control::Header(h) => paragraphs(&h.paragraphs, ids, depth + 1, nodes)?,
                    Control::Footer(f) => paragraphs(&f.paragraphs, ids, depth + 1, nodes)?,
                    Control::Footnote(n) => paragraphs(&n.paragraphs, ids, depth + 1, nodes)?,
                    Control::Endnote(n) => paragraphs(&n.paragraphs, ids, depth + 1, nodes)?,
                    _ => {}
                }
            }
        }
        Ok(())
    }
    let mut ids = BTreeSet::new();
    let mut nodes = 0;
    for section in &doc.sections {
        paragraphs(&section.paragraphs, &mut ids, 0, &mut nodes)?;
        for master in &section.section_def.master_pages {
            paragraphs(&master.paragraphs, &mut ids, 0, &mut nodes)?;
        }
    }
    (0x4000_0000..=0x4000_0000 + ids.len() as u32)
        .find(|id| !ids.contains(id))
        .ok_or_else(|| anyhow!("새 차트 인스턴스 ID가 없습니다."))
}

struct OwnedChart {
    section: usize,
    paragraph: usize,
    control: usize,
    slot: usize,
    bytes: Vec<u8>,
    xml: Vec<u8>,
    ole: OleShape,
}

impl Session {
    /// Apply the exact public setter to an isolated model before the real
    /// transaction. Only canonical, unshared, direct authored stock profiles
    /// acquire financial-role constraints; imported/extended charts retain
    /// the existing native edit contract.
    pub(crate) fn validate_authored_chart_edit(&self, index: usize, edits: &Value) -> Result<()> {
        let charts: Value = serde_json::from_str(&self.core.list_charts_native().map_err(err)?)?;
        let Some(items) = charts.as_array() else {
            return Ok(());
        };
        let Some(chart) = items.get(index) else {
            return Ok(());
        };
        if chart.get("zipPart").is_some()
            || chart
                .get("container")
                .and_then(Value::as_array)
                .is_some_and(|v| !v.is_empty())
        {
            return Ok(());
        }
        let Some(slot) = chart["nestedCopy"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
        else {
            return Ok(());
        };
        if items
            .iter()
            .filter(|item| item["nestedCopy"].as_u64() == Some(slot as u64))
            .count()
            != 1
        {
            return Ok(());
        }
        let read_xml = |core: &DocumentCore| -> Result<Vec<u8>> {
            let bytes = core
                .document()
                .bin_data_content
                .get(slot)
                .context("주가형 차트 리소스 없음")?
                .data
                .load_limited(4 * MAX_XML)
                .context("주가형 차트 CFB 크기 한도 초과")?;
            chart_io::all_ole_streams(&bytes)
                .context("주가형 차트 CFB 해석 실패")?
                .into_iter()
                .find(|(name, _)| name.trim_start_matches('/') == "OOXMLChartContents")
                .map(|(_, xml)| xml)
                .context("주가형 차트 XML 없음")
        };
        let Ok(original_xml) = read_xml(&self.core) else {
            return Ok(());
        };
        let Ok(original) = Spec::from_xml(&original_xml, width(), height()) else {
            return Ok(());
        };
        if !original.kind.stock() {
            return Ok(());
        }

        let mut scratch = DocumentCore::new_empty();
        scratch
            .set_font_environment(self.core.font_environment().cloned())
            .map_err(err)?;
        scratch.set_document(self.core.document().clone());
        let mut candidate_edits = edits.clone();
        // Native dryRun=true deliberately does not produce a modified CFB.
        // Force a real edit in this disposable core, never in the live Session.
        candidate_edits
            .as_object_mut()
            .context("차트 편집 인자는 객체여야 합니다.")?
            .insert("dryRun".into(), json!(false));
        let response: Value = serde_json::from_str(
            &scratch
                .set_chart_data_by_index_native(index, &candidate_edits.to_string())
                .map_err(err)?,
        )?;
        if response["ok"] != true {
            bail!("주가형 차트 편집 사전 검증 실패: {response}");
        }
        let edited = Spec::from_xml(&read_xml(&scratch)?, width(), height()).context(
            "주가형 차트 편집 후 고가·저가·시가·종가 또는 계열 구성이 유효하지 않습니다.",
        )?;
        if edited.kind != original.kind {
            bail!("주가형 차트 값 편집으로 가격 계열 역할을 바꿀 수 없습니다.");
        }
        Ok(())
    }

    pub(crate) fn run_chart_creation(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        Some(match id {
            "chart.types" => serde_json::from_value::<EmptyParams>(params.clone())
                .map_err(Into::into)
                .map(|_| chart_types()),
            "chart.definition.get" => serde_json::from_value::<ChartIndex>(params.clone())
                .map_err(Into::into)
                .and_then(|p| self.chart_definition(p.index)),
            "chart.create" => serde_json::from_value::<Create>(params.clone())
                .map_err(Into::into)
                .and_then(|p| self.create_chart(p)),
            "chart.configure" => serde_json::from_value::<Configure>(params.clone())
                .map_err(Into::into)
                .and_then(|p| self.configure_chart(p)),
            _ => return None,
        })
    }

    fn create_chart(&mut self, p: Create) -> Result<Value> {
        self.body_insert()?;
        if p.expected_generation.is_some_and(|g| g != self.generation) {
            bail!("차트를 만들기 전에 문서가 변경됐습니다.");
        }
        let xml = p.chart.xml()?;
        let cfb = chart_io::build_cfb_with_root_clsid(&[("OOXMLChartContents", &xml)], CHART_CLSID)
            .map_err(|e| anyhow!("차트 CFB 생성 실패: {e}"))?;
        resource_plan(self.core.document())?;
        let current_charts: Value =
            serde_json::from_str(&self.core.list_charts_native().map_err(err)?)?;
        if current_charts.as_array().context("차트 목록")?.len() >= 64 {
            bail!("차트 작성은 문서당 64개 이하에서 지원합니다.");
        }
        let instance = unique_instance(self.core.document())?;
        let width = (p.chart.width_mm * 7200.0 / 25.4).round() as u32;
        let height = (p.chart.height_mm * 7200.0 / 25.4).round() as u32;
        let mut result = Value::Null;
        self.transaction(|s| {
            s.delete_selection()?;
            let cursor=s.reserve_object_position()?;
            // Reuse the native insertion primitive's offset, field-reference and
            // control-slot maintenance. The temporary frame never escapes this
            // transaction: the committed object is an actual OLE chart.
            let raw=s.core.create_shape_control_native(cursor.section,cursor.paragraph,cursor.offset,width,height,0,0,true,"TopAndBottom","rectangle",false,false,&[]).map_err(err)?;
            let inserted:Value=serde_json::from_str(&raw)?;
            if inserted["ok"]!=true {bail!("차트 프레임 삽입 실패: {inserted}");}
            let para=inserted["paraIdx"].as_u64().and_then(|v|usize::try_from(v).ok()).ok_or_else(||anyhow!("차트 문단 주소 없음"))?;
            let control=inserted["controlIdx"].as_u64().and_then(|v|usize::try_from(v).ok()).ok_or_else(||anyhow!("차트 제어 주소 없음"))?;
            let mut doc=s.core.document().clone();
            let (position,storage)=resource_plan(&doc)?;
            let slot=&mut doc.sections[cursor.section].paragraphs[para].controls[control];
            let Control::Shape(shape)=slot else {bail!("차트 삽입 슬롯이 그리기 제어가 아닙니다.");};
            let mut common=shape.common().clone();common.ctrl_id=OLE_ID;common.instance_id=instance;common.description=if p.chart.title.is_empty(){"HwpCraft 차트".into()}else{p.chart.title.clone()};
            let mut drawing=shape.drawing().cloned().ok_or_else(||anyhow!("차트 그리기 프레임 없음"))?;
            drawing.shape_attr.ctrl_id=OLE_ID;drawing.inst_id=instance;drawing.text_box=None;
            let ole=OleShape{common,drawing,extent_x:width as i32,extent_y:height as i32,bin_data_id:u32::from(position),..Default::default()};
            *slot=Control::Shape(Box::new(ShapeObject::Ole(Box::new(ole))));
            doc.bin_data_content.push(BinDataContent{id:storage,data:BinDataBytes::from_shared(cfb.clone()),extension:"ole".into()});
            doc.doc_info.bin_data_list.push(BinData{raw_data:None,attr:0x0102,data_type:BinDataType::Storage,compression:BinDataCompression::Default,status:BinDataStatus::Success,storage_id:storage,extension:Some("ole".into()),..Default::default()});
            doc.doc_info.raw_stream=None;doc.sections[cursor.section].raw_stream=None;
            s.core.set_document(doc);
            let charts:Value=serde_json::from_str(&s.core.list_charts_native().map_err(err)?)?;
            let index=charts.as_array().context("차트 목록 없음")?.iter().position(|item|item["section"]==cursor.section && item["paragraph"]==para && item["control"]==control).ok_or_else(||anyhow!("생성 개체가 실제 차트로 인식되지 않았습니다."))?;
            if charts[index]["nestedCopy"].as_u64()!=Some(u64::from(position-1)) {bail!("새 차트 리소스의 순번 결합이 일치하지 않습니다.");}
            let data:Value=serde_json::from_str(&s.core.get_chart_data_by_index_native(index).map_err(err)?)?;
            result=json!({"index":index,"address":ObjectAddress{section:cursor.section,paragraph:para,control},"kind":p.chart.kind,"storage_id":storage,"data":data,"nativeInterop":"unverified"});
            Ok(())
        })?;
        Ok(result)
    }

    fn owned_chart(&self, index: usize) -> Result<OwnedChart> {
        let charts: Value = serde_json::from_str(&self.core.list_charts_native().map_err(err)?)?;
        let chart = charts
            .as_array()
            .and_then(|items| items.get(index))
            .ok_or_else(|| anyhow!("차트 순번이 범위를 벗어났습니다."))?;
        if chart
            .get("container")
            .and_then(Value::as_array)
            .is_some_and(|path| !path.is_empty())
        {
            bail!("전체 구성 변경은 본문에 직접 있는 자체 차트에서 지원합니다.");
        }
        if chart.get("zipPart").is_some() {
            bail!(
                "HWPX chart part 차트의 전체 구성 변경은 지원하지 않습니다. chart.data를 사용하세요."
            );
        }
        let slot = chart["nestedCopy"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| anyhow!("차트 CFB 리소스 주소 없음"))?;
        if charts
            .as_array()
            .context("차트 목록")?
            .iter()
            .filter(|item| item["nestedCopy"].as_u64() == Some(slot as u64))
            .count()
            != 1
        {
            bail!("공유된 차트 리소스의 전체 구성 변경은 지원하지 않습니다.");
        }
        let index_field = |name: &str| {
            chart[name]
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| anyhow!("차트 개체 주소 없음"))
        };
        let (section, paragraph, control) = (
            index_field("section")?,
            index_field("paragraph")?,
            index_field("control")?,
        );
        let original_ole = self
            .core
            .document()
            .sections
            .get(section)
            .and_then(|s| s.paragraphs.get(paragraph))
            .and_then(|p| p.controls.get(control))
            .and_then(|c| match c {
                Control::Shape(shape) => match shape.as_ref() {
                    ShapeObject::Ole(ole) => Some(ole),
                    _ => None,
                },
                _ => None,
            })
            .ok_or_else(|| anyhow!("차트 OLE 제어 없음"))?;
        if original_ole.common.locked {
            bail!("잠긴 차트의 구성을 바꿀 수 없습니다.");
        }
        let content = self
            .core
            .document()
            .bin_data_content
            .get(slot)
            .ok_or_else(|| anyhow!("차트 CFB 없음"))?;
        let bytes = content
            .data
            .load_limited(4 * MAX_XML)
            .ok_or_else(|| anyhow!("차트 CFB 크기 상한 초과"))?;
        let streams = chart_io::all_ole_streams(&bytes)
            .ok_or_else(|| anyhow!("차트 CFB를 열 수 없습니다."))?;
        let original = streams
            .iter()
            .find(|(name, _)| name.trim_start_matches('/') == "OOXMLChartContents")
            .map(|(_, data)| data.as_slice())
            .ok_or_else(|| anyhow!("차트 XML 없음"))?;
        let original_text = std::str::from_utf8(original)?;
        if !original_text.contains(&format!("uri=\"{OWNER_URI}\""))
            || !original_text.contains("<hcchart:created version=\"1\"/>")
        {
            bail!(
                "자체 생성 차트만 전체 구성을 바꿀 수 있습니다. 다른 차트는 chart.data를 사용하세요."
            );
        }
        Ok(OwnedChart {
            section,
            paragraph,
            control,
            slot,
            bytes: bytes.to_vec(),
            xml: original.to_vec(),
            ole: original_ole.as_ref().clone(),
        })
    }

    fn chart_definition(&self, index: usize) -> Result<Value> {
        let c = self.owned_chart(index)?;
        let spec = Spec::from_xml(
            &c.xml,
            f64::from(c.ole.common.width) * 25.4 / 7200.0,
            f64::from(c.ole.common.height) * 25.4 / 7200.0,
        )?;
        Ok(
            json!({"generation":self.generation,"chart":spec,"address":ObjectAddress{section:c.section,paragraph:c.paragraph,control:c.control},"instance_id":c.ole.common.instance_id}),
        )
    }

    fn configure_chart(&mut self, p: Configure) -> Result<Value> {
        if p.expected_generation != self.generation {
            bail!("차트 구성을 불러온 뒤 문서가 변경됐습니다.");
        }
        let xml = p.chart.xml()?;
        let OwnedChart {
            section,
            paragraph,
            control,
            slot,
            bytes,
            xml: original,
            ole,
        } = self.owned_chart(p.index)?;
        Spec::from_xml(
            &original,
            f64::from(ole.common.width) * 25.4 / 7200.0,
            f64::from(ole.common.height) * 25.4 / 7200.0,
        )?;
        let replaced = chart_io::replace_ole_stream(&bytes, "OOXMLChartContents", &xml)
            .map_err(|e| anyhow!("차트 XML 교체 실패: {e}"))?;
        let mut result = Value::Null;
        self.transaction(|s| {
            let mut doc = s.core.document().clone();
            doc.bin_data_content[slot].data = BinDataBytes::from_shared(replaced);
            let Control::Shape(shape) =
                &mut doc.sections[section].paragraphs[paragraph].controls[control]
            else {
                bail!("차트 제어 변경됨");
            };
            let ShapeObject::Ole(ole) = shape.as_mut() else {
                bail!("차트 OLE 변경됨");
            };
            let width = (p.chart.width_mm * 7200.0 / 25.4).round() as u32;
            let height = (p.chart.height_mm * 7200.0 / 25.4).round() as u32;
            ole.common.width = width;
            ole.common.height = height;
            ole.common.description = p.chart.title.clone();
            ole.extent_x = width as i32;
            ole.extent_y = height as i32;
            ole.drawing.shape_attr.original_width = width;
            ole.drawing.shape_attr.current_width = width;
            ole.drawing.shape_attr.original_height = height;
            ole.drawing.shape_attr.current_height = height;
            ole.raw_tag_data.clear();
            ole.raw_tag_seal = None;
            doc.sections[section].raw_stream = None;
            s.core.set_document(doc);
            result = serde_json::from_str(
                &s.core
                    .get_chart_data_by_index_native(p.index)
                    .map_err(err)?,
            )?;
            Ok(())
        })?;
        Ok(json!({"index":p.index,"kind":p.chart.kind,"data":result,"nativeInterop":"unverified"}))
    }
}
