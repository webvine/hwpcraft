use anyhow::{Context, Result};
use hwpcraft_doc::{
    chart_io,
    model::{control::Control, shape::ShapeObject},
};
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

fn spec(kind: &str) -> Value {
    json!({"kind":kind,"title":"자체 생성 차트","categories":["가분기","나분기"],"series":[{"name":"매출","values":[4.25,7.5],"color":"#4472C4"}],"width_mm":120,"height_mm":75,"legend":true})
}
fn state(s: &Session) -> Value {
    json!({"document":format!("{:?}",s.core.document()),"cursor":s.cursor,"anchor":s.anchor,"spanAnchor":s.span_anchor,"generation":s.generation,"dirty":s.dirty(),"undo":s.can_undo(),"redo":s.can_redo(),"path":s.path})
}
fn chart_bytes(s: &Session) -> Result<Vec<u8>> {
    let charts: Value = serde_json::from_str(
        &s.core
            .list_charts_native()
            .map_err(|e| anyhow::anyhow!("{e}"))?,
    )?;
    let slot = charts[0]["nestedCopy"]
        .as_u64()
        .context("chart resource slot")? as usize;
    Ok(s.core.document().bin_data_content[slot].data.load())
}

#[test]
fn five_kinds_create_actual_editable_chart_controls_and_render_svg_pdf() -> Result<()> {
    let root = tempfile::tempdir()?;
    for kind in ["column", "bar", "line", "pie", "doughnut"] {
        let mut s = Session::new()?;
        s.execute(Command::Insert("차트 보고서\n".into()))?;
        let result = s.run("chart.create", json!({"chart":spec(kind)}))?;
        assert_eq!(result["index"], 0);
        assert_eq!(result["nativeInterop"], "unverified");
        let address = &result["address"];
        let p = address["paragraph"].as_u64().context("paragraph")? as usize;
        let c = address["control"].as_u64().context("control")? as usize;
        assert!(
            matches!(&s.core.document().sections[0].paragraphs[p].controls[c],Control::Shape(shape) if matches!(shape.as_ref(),ShapeObject::Ole(_)))
        );
        let data = s.run("chart.data.get", json!({"index":0}))?;
        assert!(data.to_string().contains("4.25"));
        let before_render = state(&s);
        let svg = s.render_page_svg(0)?;
        assert!(svg.contains("자체 생성 차트"), "{kind} title missing");
        assert!(svg.contains("가분기"), "{kind} category missing");
        assert!(
            !svg.contains("차트 (Chart)"),
            "ChartShape placeholder is not a chart"
        );
        assert!(s.render_pdf(&[0])?.starts_with(b"%PDF-"));
        assert_eq!(
            state(&s),
            before_render,
            "rendering must not mutate source/history/cursor"
        );
        if kind == "doughnut" {
            assert!(svg.contains("data-hwpcraft-chart=\"doughnut\""));
            assert!(
                svg.contains("fill-rule=\"evenodd\""),
                "a pie substituted for a doughnut is invalid"
            );
        }
        s.run(
            "chart.data",
            json!({"index":0,"series":[{"values":["9.25","11"]}]}),
        )?;
        assert!(
            s.run("chart.data.get", json!({"index":0}))?
                .to_string()
                .contains("9.25")
        );
        for extension in ["hwp", "hwpx"] {
            let path = root.path().join(format!("chart-{kind}.{extension}"));
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            assert_eq!(
                reopened
                    .run("chart.list", json!({}))?
                    .as_array()
                    .context("charts")?
                    .len(),
                1
            );
            assert!(
                reopened
                    .run("chart.data.get", json!({"index":0}))?
                    .to_string()
                    .contains("9.25")
            );
            assert!(reopened.render_page_svg(0)?.contains("자체 생성 차트"));
            if std::env::var_os("HWPCRAFT_WRITE_CHART_FIXTURES").as_deref()
                == Some(std::ffi::OsStr::new("1"))
            {
                let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../output/implementation/0.17/charts");
                std::fs::create_dir_all(&output)?;
                std::fs::copy(&path, output.join(format!("chart-{kind}.{extension}")))?;
                std::fs::write(
                    output.join(format!("chart-{kind}-{extension}.svg")),
                    reopened.render_page_svg(0)?,
                )?;
                std::fs::write(
                    output.join(format!("chart-{kind}-{extension}.pdf")),
                    reopened.render_pdf(&[0])?,
                )?;
            }
        }
    }
    Ok(())
}

#[test]
fn create_and_configuration_are_each_one_undo_and_redo_with_distinct_resources() -> Result<()> {
    let mut s = Session::new()?;
    let source = format!("{:?}", s.core.document());
    s.run("chart.create", json!({"chart":spec("column")}))?;
    let created = chart_bytes(&s)?;
    s.run("edit.undo", json!({}))?;
    assert_eq!(format!("{:?}", s.core.document()), source);
    s.run("edit.redo", json!({}))?;
    assert_eq!(chart_bytes(&s)?, created);
    let mut changed = spec("line");
    changed["title"] = json!("구성 변경 후");
    changed["width_mm"] = json!(100);
    changed["series"][0]["values"] = json!([12, 15]);
    let generation = s.generation;
    s.run(
        "chart.configure",
        json!({"index":0,"chart":changed,"expected_generation":generation}),
    )?;
    assert_ne!(chart_bytes(&s)?, created);
    assert!(s.render_page_svg(0)?.contains("구성 변경 후"));
    let configured = chart_bytes(&s)?;
    s.run("edit.undo", json!({}))?;
    assert_eq!(chart_bytes(&s)?, created);
    s.run("edit.redo", json!({}))?;
    assert_eq!(chart_bytes(&s)?, configured);
    s.run("chart.create", json!({"chart":spec("bar")}))?;
    let doc = s.core.document();
    assert_eq!(doc.bin_data_content.len(), 2);
    assert_ne!(doc.bin_data_content[0].id, doc.bin_data_content[1].id);
    assert_ne!(
        doc.bin_data_content[0].data.load(),
        doc.bin_data_content[1].data.load()
    );
    Ok(())
}

#[test]
fn invalid_data_stale_targets_and_duplicate_resources_preserve_every_state() -> Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("보존 😀".into()))?;
    s.execute(Command::Character(json!({"italic":true})))?;
    let mut invalid = Vec::new();
    let mut x = spec("column");
    x["series"] = json!([]);
    invalid.push(json!({"chart":x}));
    let mut x = spec("column");
    x["series"][0]["values"] = json!(["NaN", 2]);
    invalid.push(json!({"chart":x}));
    let mut x = spec("column");
    x["series"][0]["values"] = json!([1]);
    invalid.push(json!({"chart":x}));
    let mut x = spec("column");
    x["series"][0]["values"] = json!([1e20, 2]);
    invalid.push(json!({"chart":x}));
    let mut x = spec("pie");
    x["series"][0]["values"] = json!([0, 0]);
    invalid.push(json!({"chart":x}));
    let mut x = spec("column");
    x["series"] = json!([{"name":"중복","values":[1,2]},{"name":"중복","values":[3,4]}]);
    invalid.push(json!({"chart":x}));
    invalid.push(json!({"chart":spec("column"),"expected_generation":s.generation+1}));
    for request in invalid {
        let before = state(&s);
        assert!(s.run("chart.create", request).is_err());
        assert_eq!(state(&s), before);
    }
    s.run("chart.create", json!({"chart":spec("column")}))?;
    for request in [
        json!({"index":0,"chart":spec("line"),"expected_generation":s.generation+1}),
        json!({"index":7,"chart":spec("line"),"expected_generation":s.generation}),
    ] {
        let before = state(&s);
        assert!(s.run("chart.configure", request).is_err());
        assert_eq!(state(&s), before);
    }
    let mut doc = s.core.document().clone();
    doc.bin_data_content.push(doc.bin_data_content[0].clone());
    doc.doc_info
        .bin_data_list
        .push(doc.doc_info.bin_data_list[0].clone());
    s.core.set_document(doc);
    let before = state(&s);
    assert!(
        s.run("chart.create", json!({"chart":spec("column")}))
            .is_err()
    );
    assert_eq!(state(&s), before);
    Ok(())
}

#[test]
fn configure_preserves_opaque_streams_clsid_and_supports_both_reopened_formats() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.run("chart.create", json!({"chart":spec("column")}))?;
    let original = chart_bytes(&s)?;
    let xml = chart_io::all_ole_streams(&original)
        .context("streams")?
        .into_iter()
        .find(|(name, _)| name.trim_start_matches('/') == "OOXMLChartContents")
        .context("XML stream")?
        .1;
    let clsid = chart_io::ole_root_clsid(&original).context("CLSID")?;
    let opaque = b"own opaque bytes must survive";
    let augmented = chart_io::build_cfb_with_root_clsid(
        &[("OOXMLChartContents", &xml), ("HwpCraftOpaque", opaque)],
        clsid,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut doc = s.core.document().clone();
    doc.bin_data_content[0].data =
        hwpcraft_doc::model::bin_data::BinDataBytes::from_shared(augmented);
    s.core.set_document(doc);
    for extension in ["hwp", "hwpx"] {
        let path = root.path().join(format!("opaque.{extension}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        let generation = reopened.generation;
        reopened.run(
            "chart.configure",
            json!({"index":0,"chart":spec("line"),"expected_generation":generation}),
        )?;
        let data = chart_bytes(&reopened)?;
        assert_eq!(chart_io::ole_root_clsid(&data), Some(clsid));
        assert!(
            chart_io::all_ole_streams(&data)
                .context("preserved streams")?
                .iter()
                .any(
                    |(name, bytes)| name.trim_start_matches('/') == "HwpCraftOpaque"
                        && bytes == opaque
                )
        );
    }
    Ok(())
}
