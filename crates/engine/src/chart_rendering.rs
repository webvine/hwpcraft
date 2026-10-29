//! Presentation-only correction for verified HwpCraft chart authoring profiles.
//!
//! The public chart render model omits the XML legend-presence flag and its
//! parser skips entity-reference events in strings. Correct the transient
//! model, never the source CFB/document. Foreign or extended chart profiles
//! retain their native payload and rendering path byte-for-byte.
use super::*;
use hwpcraft_doc::{RenderProfile, chart_io};
use std::collections::{BTreeMap, BTreeSet};

const MAX_XML: usize = 2 * 1024 * 1024;
const MAX_CHARTS: usize = 64;

struct DisplayChart {
    key: (usize, usize, usize),
    model: chart_io::OoxmlChart,
    legend: bool,
}

fn display_charts(core: &DocumentCore) -> Result<Vec<DisplayChart>> {
    if !core
        .document()
        .bin_data_content
        .iter()
        .any(|b| b.extension.eq_ignore_ascii_case("ole"))
    {
        return Ok(Vec::new());
    }
    let charts: Value = serde_json::from_str(&core.list_charts_native().map_err(err)?)?;
    let items = charts
        .as_array()
        .ok_or_else(|| anyhow!("차트 표시 목록이 잘못됐습니다."))?;
    let mut parsed = BTreeMap::<usize, Option<(chart_io::OoxmlChart, bool)>>::new();
    let mut out = Vec::new();
    for chart in items {
        // Authoring/configure only promise direct body OLEs. In particular,
        // do not truncate a native container path and bind the wrong object.
        if chart.get("zipPart").is_some()
            || chart
                .get("container")
                .and_then(Value::as_array)
                .is_some_and(|v| !v.is_empty())
        {
            continue;
        }
        let Some(slot) = chart["nestedCopy"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
        else {
            continue;
        };
        if let std::collections::btree_map::Entry::Vacant(entry) = parsed.entry(slot) {
            let model = core
                .document()
                .bin_data_content
                .get(slot)
                .and_then(|b| b.data.load_limited(4 * MAX_XML))
                .and_then(|b| chart_io::all_ole_streams(&b))
                .and_then(|streams| {
                    streams
                        .into_iter()
                        .find(|(name, _)| name.trim_start_matches('/') == "OOXMLChartContents")
                })
                .and_then(|(_, xml)| super::chart_creation::authored_render_model(&xml));
            entry.insert(model);
        }
        let Some((model, legend)) = parsed.get(&slot).and_then(Option::as_ref) else {
            continue;
        };
        // Do not project a shared resource using assumptions from one body
        // object. Imported/shared profiles remain on their original path.
        if items
            .iter()
            .filter(|item| item["nestedCopy"].as_u64() == Some(slot as u64))
            .count()
            != 1
        {
            continue;
        }
        let index = |name: &str| {
            chart[name]
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| anyhow!("자체 차트 표시 주소 없음"))
        };
        out.push(DisplayChart {
            key: (index("section")?, index("paragraph")?, index("control")?),
            model: model.clone(),
            legend: *legend,
        });
        if out.len() > MAX_CHARTS {
            bail!("자체 차트 표시 보정은 문서당 64개 이하에서 지원합니다.");
        }
    }
    Ok(out)
}

/// Remove only the renderer's exact legend group, with XML depth accounting.
/// Keep every other byte (including plot paths, titles and nested groups).
fn without_legend(svg: &str) -> Result<String> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_str(svg);
    let mut depth = 0usize;
    let mut active = None;
    let mut spans = Vec::new();
    loop {
        let start = usize::try_from(reader.buffer_position())?;
        let event = reader.read_event()?;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let legend = e.local_name().as_ref() == "g"
                    && e.attributes().any(|a| {
                        a.is_ok_and(|a| {
                            a.key.as_ref() == "class"
                                && a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .is_ok_and(|v| {
                                        v.split_whitespace().any(|c| c == "hwp-chart-legend")
                                    })
                        })
                    });
                if legend {
                    if active.is_some() {
                        bail!("차트 범례 그룹이 중복 중첩됐습니다.");
                    }
                    if empty {
                        spans.push(start..usize::try_from(reader.buffer_position())?);
                    } else {
                        active = Some((depth, start));
                    }
                }
                if !empty {
                    depth = depth
                        .checked_add(1)
                        .ok_or_else(|| anyhow!("차트 SVG 깊이 초과"))?;
                }
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| anyhow!("차트 SVG 닫기 구조 오류"))?;
                if active.is_some_and(|(level, _)| level == depth) {
                    let (_, start) = active
                        .take()
                        .ok_or_else(|| anyhow!("차트 범례 범위 없음"))?;
                    spans.push(start..usize::try_from(reader.buffer_position())?);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 || active.is_some() || spans.len() != 1 {
        bail!("차트 범례 그룹을 정확히 하나 식별하지 못했습니다.");
    }
    let mut output = svg.to_owned();
    output.replace_range(spans.remove(0), "");
    Ok(output)
}

fn plan(core: &DocumentCore, page: u32, profile: RenderProfile) -> Result<Vec<(String, String)>> {
    let charts = display_charts(core)?;
    if charts.is_empty() {
        return Ok(Vec::new());
    }
    let tree = core
        .build_page_layer_tree_with_profile(page, profile)
        .map_err(err)?;
    fn visit(
        node: &hwpcraft_doc::LayerNode,
        charts: &[DisplayChart],
        out: &mut Vec<(String, String)>,
        depth: usize,
    ) -> Result<()> {
        if depth > 128 {
            bail!("차트 표시 레이어 깊이 초과");
        }
        match &node.kind {
            hwpcraft_doc::LayerNodeKind::Group { children, .. } => {
                for child in children {
                    visit(child, charts, out, depth + 1)?;
                }
            }
            hwpcraft_doc::LayerNodeKind::ClipRect { child, .. } => {
                visit(child, charts, out, depth + 1)?
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
                        let matches = charts.iter().filter(|c| c.key == key).collect::<Vec<_>>();
                        if matches.is_empty() {
                            continue;
                        }
                        if matches.len() != 1
                            || raw.cell_context.is_some()
                            || reference.kind != "ole"
                            || raw.svg.is_empty()
                        {
                            bail!("차트 표시 payload를 본문 개체 하나에 대응하지 못했습니다.");
                        }
                        if ![bbox.x, bbox.y, bbox.width, bbox.height]
                            .iter()
                            .all(|v| v.is_finite())
                            || bbox.width <= 0.0
                            || bbox.height <= 0.0
                        {
                            bail!("차트 표시 프레임이 잘못됐습니다.");
                        }
                        let chart = matches[0];
                        let rendered =
                            chart
                                .model
                                .render_svg(bbox.x, bbox.y, bbox.width, bbox.height);
                        let rendered = if chart.legend {
                            rendered
                        } else {
                            without_legend(&rendered)?
                        };
                        if rendered.is_empty() {
                            bail!("자체 차트 표시 결과가 비었습니다.");
                        }
                        out.push((raw.svg.clone(), rendered));
                    }
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    visit(&tree.root, &charts, &mut out, 0)?;
    Ok(out)
}

pub(crate) fn page_has_overrides(
    core: &DocumentCore,
    page: u32,
    profile: RenderProfile,
) -> Result<bool> {
    Ok(!plan(core, page, profile)?.is_empty())
}

pub(crate) fn with_overrides(
    core: &DocumentCore,
    page: u32,
    profile: RenderProfile,
    mut svg: String,
) -> Result<String> {
    let plan = plan(core, page, profile)?;
    let mut seen = BTreeSet::new();
    for (old, _) in &plan {
        if !seen.insert(old) || svg.match_indices(old).count() != 1 {
            bail!("차트 원본 paint payload를 쪽 SVG에 정확히 대응하지 못했습니다.");
        }
    }
    for (old, new) in plan {
        svg = svg.replacen(&old, &new, 1);
    }
    Ok(svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: &str, legend: bool) -> Value {
        let series = if kind == "stock_hlc" {
            json!([{"name":"고&가","values":[15,20]},{"name":"저<가>","values":[5,7]},{"name":"종가😀","values":[12,9]}])
        } else if kind == "stock_ohlc" {
            json!([{"name":"시가","values":[8,16]},{"name":"고&가","values":[15,20]},{"name":"저<가>","values":[5,7]},{"name":"종가😀","values":[12,9]}])
        } else if kind == "scatter" {
            json!([{"name":"A&B <계열>","x_values":[-4,9],"values":[3,7]}])
        } else {
            json!([{"name":"A&B <계열>","values":[3,7]}])
        };
        let mut value =
            json!({"kind":kind,"title":"제목 A&B <검증> 😀","series":series,"legend":legend});
        if kind != "scatter" {
            value["categories"] = json!(["첫&분기", "다음<분기>"]);
        }
        value
    }

    fn source(session: &Session) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{}|{}|{}|{}|{:?}|{:?}|{:?}",
            session.core.document(),
            session.cursor,
            session.anchor,
            session.span_anchor,
            session.generation,
            session.dirty(),
            session.can_undo(),
            session.can_redo(),
            session.path,
            session
                .core
                .document()
                .bin_data_content
                .iter()
                .map(|b| b.data.load())
                .collect::<Vec<_>>(),
            (
                session.undo.len(),
                session.redo.len(),
                session.insertion_shape.revision(),
                session.insertion_shape.has_pending(),
                session.insertion_shape.properties(session)
            )
        )
    }

    #[test]
    fn legend_group_removal_is_exact_and_preserves_every_other_byte() -> Result<()> {
        let svg = r#"<g><text>before &amp; after</text><g class="extra hwp-chart-legend"><g><path d="M0,0L1,1"/></g><text>hidden</text></g><path class="hwp-chart-legend-other"/></g>"#;
        assert_eq!(
            without_legend(svg)?,
            r#"<g><text>before &amp; after</text><path class="hwp-chart-legend-other"/></g>"#
        );
        for bad in [
            "<g/>",
            "<g><g class=\"hwp-chart-legend\"/><g class=\"hwp-chart-legend\"/></g>",
            "<g><g class=\"hwp-chart-legend\">",
        ] {
            assert!(without_legend(bad).is_err());
        }
        Ok(())
    }

    #[test]
    fn eleven_profiles_render_actual_entities_and_hidden_legends_without_mutating_source()
    -> Result<()> {
        let temp = tempfile::tempdir()?;
        for kind in [
            "column",
            "bar",
            "line",
            "pie",
            "doughnut",
            "scatter",
            "stock_hlc",
            "stock_ohlc",
            "column_3d",
            "bar_3d",
            "pie_3d",
        ] {
            for legend in [false, true] {
                let mut session = Session::new()?;
                session.run("chart.create", json!({"chart":spec(kind,legend)}))?;
                let before = source(&session);
                let svg = session.render_page_svg(0)?;
                assert!(
                    svg.contains("제목 A&amp;B &lt;검증&gt; 😀"),
                    "{kind}: title entity lost"
                );
                if kind == "doughnut" {
                    assert_eq!(svg.contains(">첫&amp;분기</text>"), legend);
                } else {
                    assert_eq!(
                        svg.contains("class=\"hwp-chart-legend\""),
                        legend,
                        "{kind}: legend flag ignored"
                    );
                    if legend {
                        assert!(
                            svg.contains(if kind.starts_with("stock") {
                                "고&amp;가"
                            } else if kind.starts_with("pie") {
                                "첫&amp;분기"
                            } else {
                                "A&amp;B &lt;계열&gt;"
                            }),
                            "{kind}: legend text entity lost"
                        );
                    }
                    if !kind.starts_with("pie") && kind != "scatter" {
                        assert!(svg.contains("첫&amp;분기"), "{kind}: category entity lost");
                    }
                }
                assert!(session.render_pdf(&[0])?.starts_with(b"%PDF-"));
                assert_eq!(source(&session), before);
                for extension in ["hwp", "hwpx"] {
                    let path = temp.path().join(format!("{kind}-{legend}.{extension}"));
                    session.save(&path)?;
                    let mut reopened = Session::new()?;
                    reopened.open(&path)?;
                    let before = source(&reopened);
                    let svg = reopened.render_page_svg(0)?;
                    assert!(svg.contains("제목 A&amp;B &lt;검증&gt; 😀"));
                    if kind != "doughnut" {
                        assert_eq!(svg.contains("class=\"hwp-chart-legend\""), legend);
                    }
                    let pdf = reopened.render_pdf(&[0])?;
                    assert!(pdf.starts_with(b"%PDF-"));
                    assert_eq!(source(&reopened), before);
                    if std::env::var_os("HWPCRAFT_WRITE_CHART_FIXTURES").as_deref()
                        == Some(std::ffi::OsStr::new("1"))
                    {
                        let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../output/implementation/0.17/charts/display");
                        std::fs::create_dir_all(&output)?;
                        let name = format!("{kind}-legend-{legend}-{extension}");
                        std::fs::copy(&path, output.join(format!("{name}.{extension}")))?;
                        std::fs::write(output.join(format!("{name}.svg")), svg)?;
                        std::fs::write(output.join(format!("{name}.pdf")), pdf)?;
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn foreign_and_unknown_profiles_keep_native_payload_and_source_unchanged() -> Result<()> {
        for kind in ["column", "doughnut"] {
            for foreign in [false, true] {
                let mut session = Session::new()?;
                session.run("chart.create", json!({"chart":spec(kind,false)}))?;
                let charts: Value =
                    serde_json::from_str(&session.core.list_charts_native().map_err(err)?)?;
                let slot = charts[0]["nestedCopy"]
                    .as_u64()
                    .ok_or_else(|| anyhow!("slot"))? as usize;
                let mut doc = session.core.document().clone();
                let bytes = doc.bin_data_content[slot].data.load();
                let streams = chart_io::all_ole_streams(&bytes).ok_or_else(|| anyhow!("CFB"))?;
                let xml = streams
                    .iter()
                    .find(|(n, _)| n.trim_start_matches('/') == "OOXMLChartContents")
                    .ok_or_else(|| anyhow!("XML"))?;
                let mut xml = String::from_utf8(xml.1.clone())?;
                if foreign {
                    xml = xml.replace("<hcchart:created version=\"1\"/>", "<hcchart:foreign/>");
                } else {
                    xml = xml.replace("</c:plotArea>", "<c:spPr><a:noFill/></c:spPr></c:plotArea>");
                }
                let bytes =
                    chart_io::replace_ole_stream(&bytes, "OOXMLChartContents", xml.as_bytes())
                        .map_err(|e| anyhow!("{e}"))?;
                doc.bin_data_content[slot].data =
                    hwpcraft_doc::model::bin_data::BinDataBytes::from_shared(bytes);
                session.core.set_document(doc);
                let before = source(&session);
                assert!(!page_has_overrides(
                    &session.core,
                    0,
                    RenderProfile::Screen
                )?);
                let native = session
                    .core
                    .render_page_svg_layer_with_profile_native(0, RenderProfile::Screen)
                    .map_err(err)?;
                assert_eq!(
                    with_overrides(&session.core, 0, RenderProfile::Screen, native.clone())?,
                    native
                );
                assert_eq!(session.render_page_svg(0)?, native);
                assert_eq!(source(&session), before);
            }
        }
        Ok(())
    }
}
