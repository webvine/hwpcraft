//! Stacked / percent-stacked groupings and combo (column + line, secondary axis)
//! charts: authored, drawn deterministically, read back and kept through HWP and
//! HWPX saves.
use anyhow::{Context, Result};
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

fn base(kind: &str) -> Value {
    json!({"kind":kind,"title":"누적 실적","categories":["1분기","2분기","3분기"],
        "series":[{"name":"국내","values":[4,6,5]},{"name":"해외","values":[2,3,7]}]})
}

fn combo() -> Value {
    json!({"kind":"combo","title":"매출과 이익률","categories":["1분기","2분기","3분기"],
    "series":[
        {"name":"매출","values":[120,150,170],"plot":"column"},
        {"name":"목표","values":[130,140,180],"plot":"line"},
        {"name":"이익률","values":[8.5,9.2,11.0],"plot":"line","secondary_axis":true}
    ]})
}

fn create(chart: &Value) -> Result<Session> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("차트\n".into()))?;
    s.run("chart.create", json!({ "chart": chart }))?;
    Ok(s)
}

fn definition(s: &mut Session) -> Result<Value> {
    Ok(s.run("chart.definition.get", json!({"index":0}))?["chart"].clone())
}

/// The chart part of page 1 (everything after the title text).
fn page(s: &Session) -> Result<String> {
    s.render_page_svg(0)
}

#[test]
fn groupings_draw_differently_and_survive_both_formats() -> Result<()> {
    let dir = tempfile::tempdir()?;
    for kind in ["column", "bar", "line"] {
        let mut drawn = Vec::new();
        for grouping in ["clustered", "stacked", "percent_stacked"] {
            let mut chart = base(kind);
            chart["grouping"] = json!(grouping);
            let mut s = create(&chart)?;
            let svg = page(&s)?;
            assert_eq!(svg, page(&s)?, "{kind}/{grouping}: deterministic");
            assert!(
                !drawn.contains(&svg),
                "{kind}/{grouping}: same drawing as another grouping"
            );
            drawn.push(svg);
            let got = definition(&mut s)?;
            if grouping == "clustered" {
                assert!(got.get("grouping").is_none(), "{got}");
            } else {
                assert_eq!(got["grouping"], grouping, "{got}");
            }
            for ext in ["hwp", "hwpx"] {
                let path = dir.path().join(format!("{kind}-{grouping}.{ext}"));
                s.save(&path)?;
                let mut reopened = Session::new()?;
                reopened.open(&path)?;
                assert_eq!(definition(&mut reopened)?, got, "{kind}/{grouping}/{ext}");
                assert_eq!(
                    page(&reopened)?,
                    page(&s)?,
                    "{kind}/{grouping}/{ext}: same drawing"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn percent_stacked_columns_fill_the_value_axis() -> Result<()> {
    let mut chart = base("column");
    chart["grouping"] = json!("percent_stacked");
    let s = create(&chart)?;
    let svg = page(&s)?;
    assert!(svg.contains("100%"), "percent axis labels");
    Ok(())
}

#[test]
fn combo_reads_back_and_reconfigures() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = create(&combo())?;
    let got = definition(&mut s)?;
    assert_eq!(got["kind"], "combo");
    let series = got["series"].as_array().context("series")?;
    assert_eq!(series[0]["plot"], "column");
    assert_eq!(series[1]["plot"], "line");
    assert!(series[1].get("secondary_axis").is_none());
    assert_eq!(series[2]["secondary_axis"], true);
    let svg = page(&s)?;
    assert_eq!(svg, page(&s)?);
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("combo.{ext}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(definition(&mut reopened)?, got, "{ext}");
        assert_eq!(page(&reopened)?, svg, "{ext}: same drawing");
    }
    // Without the secondary axis the drawing changes; back to plain columns too.
    let mut flat = combo();
    flat["series"][2]["secondary_axis"] = json!(false);
    s.run(
        "chart.configure",
        json!({"index":0,"expected_generation":s.generation,"chart":flat}),
    )?;
    assert_ne!(page(&s)?, svg);
    assert!(
        definition(&mut s)?["series"][2]
            .get("secondary_axis")
            .is_none()
    );
    s.run(
        "chart.configure",
        json!({"index":0,"expected_generation":s.generation,"chart":base("column")}),
    )?;
    assert_eq!(definition(&mut s)?["kind"], "column");
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    assert_eq!(definition(&mut s)?, got, "configure is one undo step each");
    Ok(())
}

#[test]
fn bad_groupings_and_combos_change_nothing() -> Result<()> {
    let mut s = create(&base("column"))?;
    let generation = s.generation;
    let with = |mut chart: Value, f: &dyn Fn(&mut Value)| {
        f(&mut chart);
        chart
    };
    let bad = [
        with(base("pie"), &|c| c["grouping"] = json!("stacked")),
        with(base("column_3d"), &|c| c["grouping"] = json!("stacked")),
        with(combo(), &|c| c["grouping"] = json!("stacked")),
        with(base("column"), &|c| c["grouping"] = json!("sideways")),
        with(base("column"), &|c| c["series"][0]["plot"] = json!("line")),
        with(base("line"), &|c| {
            c["series"][0]["secondary_axis"] = json!(true)
        }),
        with(combo(), &|c| c["series"][0]["secondary_axis"] = json!(true)),
        with(combo(), &|c| {
            let a = c["series"][0].clone();
            c["series"][0] = c["series"][1].clone();
            c["series"][1] = a;
        }),
        with(combo(), &|c| {
            c["series"][0].as_object_mut().map(|o| o.remove("plot"));
        }),
        with(combo(), &|c| {
            c["series"] = json!([{"name":"a","values":[1,2,3],"plot":"column"}]);
        }),
        with(combo(), &|c| {
            c["series"] = json!([
                {"name":"a","values":[1,2,3],"plot":"line"},
                {"name":"b","values":[1,2,3],"plot":"line"}
            ]);
        }),
    ];
    for chart in bad {
        assert!(
            s.run("chart.create", json!({ "chart": chart })).is_err(),
            "{chart}"
        );
    }
    assert_eq!(s.generation, generation);
    let types = s.run("chart.types", json!({}))?;
    let combo_type = types["types"]
        .as_array()
        .and_then(|t| t.iter().find(|t| t["kind"] == "combo"))
        .context("combo listed")?;
    assert_eq!(combo_type["constraints"]["seriesMin"], 2);
    assert!(
        types["unsupported"]
            .as_array()
            .is_some_and(|u| u.iter().all(|k| k["kind"] != "combo"))
    );
    let column = types["types"]
        .as_array()
        .and_then(|t| t.iter().find(|t| t["kind"] == "column"))
        .context("column listed")?;
    assert_eq!(
        column["constraints"]["groupings"],
        json!(["clustered", "stacked", "percent_stacked"])
    );
    Ok(())
}

fn area_radar_bubble() -> Vec<(&'static str, Value)> {
    let mut stacked_area = base("area");
    stacked_area["grouping"] = json!("stacked");
    let mut filled_radar = base("radar");
    filled_radar["radar_style"] = json!("filled");
    let bubble = json!({"kind":"bubble","title":"시장 규모","series":[
        {"name":"가","values":[5,7,6],"x_values":[1,2,3],"bubble_sizes":[1,4,9]},
        {"name":"나","values":[3,4,8],"x_values":[1,2,3],"bubble_sizes":[2,2,2]}]});
    vec![
        ("area", base("area")),
        ("stacked area", stacked_area),
        ("radar", base("radar")),
        ("filled radar", filled_radar),
        ("bubble", bubble),
    ]
}

#[test]
fn area_radar_and_bubble_draw_and_survive_both_formats() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut drawings = Vec::new();
    for (label, chart) in area_radar_bubble() {
        let mut s = create(&chart)?;
        let svg = page(&s)?;
        assert_eq!(svg, page(&s)?, "{label}: deterministic");
        let marker = match chart["kind"].as_str() {
            Some("area") => "hwp-chart-area",
            Some("radar") => "hwp-chart-radar-spoke",
            _ => "hwp-chart-bubble",
        };
        assert!(svg.contains(marker), "{label}: drawn as its kind");
        assert!(!drawings.contains(&svg), "{label}");
        drawings.push(svg.clone());
        let got = definition(&mut s)?;
        assert_eq!(got["kind"], chart["kind"], "{label}");
        for ext in ["hwp", "hwpx"] {
            let path = dir
                .path()
                .join(format!("{}.{ext}", label.replace(' ', "-")));
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            assert_eq!(definition(&mut reopened)?, got, "{label}/{ext}");
            assert_eq!(page(&reopened)?, svg, "{label}/{ext}: same drawing");
        }
    }
    // Bubble sizes and radar styles read back; chart.data edits bubble X/Y only.
    let mut s = create(&area_radar_bubble()[4].1)?;
    let got = definition(&mut s)?;
    assert_eq!(got["series"][0]["bubble_sizes"], json!([1.0, 4.0, 9.0]));
    assert_eq!(got["series"][0]["x_values"], json!([1.0, 2.0, 3.0]));
    let mut s = create(&area_radar_bubble()[3].1)?;
    assert_eq!(definition(&mut s)?["radar_style"], "filled");
    s.run(
        "chart.configure",
        json!({"index":0,"expected_generation":s.generation,"chart":base("radar")}),
    )?;
    assert_eq!(definition(&mut s)?["radar_style"], "standard");
    Ok(())
}

#[test]
fn bad_area_radar_and_bubble_requests_change_nothing() -> Result<()> {
    let mut s = create(&base("column"))?;
    let generation = s.generation;
    let bubble = area_radar_bubble()[4].1.clone();
    let with = |mut chart: Value, f: &dyn Fn(&mut Value)| {
        f(&mut chart);
        chart
    };
    let bad = [
        with(bubble.clone(), &|c| {
            c["series"][0]
                .as_object_mut()
                .map(|o| o.remove("bubble_sizes"));
        }),
        with(bubble.clone(), &|c| {
            c["series"][0]["bubble_sizes"] = json!([1, 2])
        }),
        with(bubble.clone(), &|c| {
            c["series"][0]["bubble_sizes"] = json!([1, -2, 3])
        }),
        with(bubble.clone(), &|c| {
            c["series"][1]["x_values"] = json!([1, 2, 4])
        }),
        with(bubble.clone(), &|c| {
            c["categories"] = json!(["a", "b", "c"])
        }),
        with(bubble.clone(), &|c| c["scatter_style"] = json!("marker")),
        with(base("column"), &|c| {
            c["series"][0]["bubble_sizes"] = json!([1, 2, 3])
        }),
        with(base("line"), &|c| c["radar_style"] = json!("filled")),
        with(base("radar"), &|c| c["grouping"] = json!("stacked")),
        with(base("radar"), &|c| c["radar_style"] = json!("spiky")),
    ];
    for chart in bad {
        assert!(
            s.run("chart.create", json!({ "chart": chart })).is_err(),
            "{chart}"
        );
    }
    assert_eq!(s.generation, generation);
    let types = s.run("chart.types", json!({}))?;
    assert_eq!(types["types"].as_array().map(Vec::len), Some(15));
    for kind in ["area", "radar", "bubble"] {
        assert!(
            types["unsupported"]
                .as_array()
                .is_some_and(|u| u.iter().all(|k| k["kind"] != kind))
        );
    }
    Ok(())
}

/// One chart of every kind `chart.types` lists.
fn every_kind() -> Vec<Value> {
    let types = Session::new()
        .and_then(|mut s| s.run("chart.types", json!({})))
        .unwrap_or_default();
    let stock = |names: &[&str]| {
        let rows = [[8, 16], [15, 20], [5, 7], [12, 9]];
        let start = 4 - names.len();
        names
            .iter()
            .zip(&rows[start..])
            .map(|(n, v)| json!({"name": n, "values": v}))
            .collect::<Vec<_>>()
    };
    types["types"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t["kind"].as_str())
        .map(|kind| match kind {
            "combo" => combo(),
            "bubble" => area_radar_bubble()[4].1.clone(),
            "scatter" => json!({"kind":kind,"series":[
                {"name":"실험","x_values":[-4.0,1.0,9.0],"values":[2.0,8.0,-3.0]}]}),
            "stock_hlc" => json!({"kind":kind,"categories":["첫날","다음날"],
                "series":stock(&["고가","저가","종가"])}),
            "stock_ohlc" => json!({"kind":kind,"categories":["첫날","다음날"],
                "series":stock(&["시가","고가","저가","종가"])}),
            "pie" | "doughnut" | "pie_3d" => json!({"kind":kind,"categories":["가","나","다"],
                "series":[{"name":"비중","values":[4,6,5]}]}),
            _ => base(kind),
        })
        .collect()
}

/// `chart.data` edits a value of every chart kind: the definition, the drawing and
/// both file formats follow, and one undo restores it. Stock charts keep their price
/// order, so the edited value stays between low and high.
#[test]
fn chart_data_edits_every_kind() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let kinds = every_kind();
    assert_eq!(kinds.len(), 15);
    for chart in kinds {
        let kind = chart["kind"].as_str().unwrap_or_default().to_owned();
        let mut s = create(&chart)?;
        let before_svg = page(&s)?;
        let before = definition(&mut s)?;
        let data = s.run("chart.data.get", json!({"index":0}))?;
        let mut series: Vec<Value> = data["series"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| json!({"values": v["values"]}))
            .collect();
        assert!(!series.is_empty(), "{kind}: {data}");
        // The last series's first point: for stock charts that is the close price.
        let last = series.len() - 1;
        let target = if kind.starts_with("stock") {
            "13"
        } else {
            "10.5"
        };
        series[last]["values"][0] = json!(target);
        s.run("chart.data", json!({"index":0,"series":series}))?;
        let after = definition(&mut s)?;
        assert_eq!(
            after["series"][last]["values"][0],
            target.parse::<f64>()?,
            "{kind}"
        );
        assert_eq!(after["kind"], before["kind"], "{kind}");
        let svg = page(&s)?;
        assert_ne!(svg, before_svg, "{kind}: the drawing follows");
        for ext in ["hwp", "hwpx"] {
            let path = dir.path().join(format!("data-{kind}.{ext}"));
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            assert_eq!(definition(&mut reopened)?, after, "{kind}/{ext}");
            assert_eq!(page(&reopened)?, svg, "{kind}/{ext}");
        }
        s.execute(Command::Undo)?;
        assert_eq!(definition(&mut s)?, before, "{kind}: undo");
    }
    Ok(())
}
