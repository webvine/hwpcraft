//! Visual equation editing: drawn pieces tied to script ranges, empty slots,
//! and saved equation macros.
use hwpcraft_engine::Session;
use serde_json::{Value, json};

fn source(script: &str, item: &Value) -> Option<String> {
    let start = usize::try_from(item["start"].as_u64()?).ok()?;
    let end = usize::try_from(item["end"].as_u64()?).ok()?;
    Some(script.chars().skip(start).take(end - start).collect())
}

#[test]
fn layout_items_point_into_the_script_inside_the_drawing() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let script = "x = {-b +- sqrt {b^2 -4ac}} over {2a}";
    let layout = s.run(
        "equation.layout",
        json!({"script": script, "size_pt": 12.0}),
    )?;
    let (w, h) = (
        layout["width"].as_f64().unwrap_or(0.0),
        layout["height"].as_f64().unwrap_or(0.0),
    );
    assert!(w > 0.0 && h > 0.0, "{layout}");
    let items = layout["items"].as_array().cloned().unwrap_or_default();
    for item in &items {
        let (x, y) = (
            item["x"].as_f64().unwrap_or(-1.0),
            item["y"].as_f64().unwrap_or(-1.0),
        );
        assert!(
            x >= -0.5 && y >= -0.5 && x <= w + 0.5 && y <= h + 0.5,
            "{item}"
        );
    }
    let found = |text: &str, src: &str| {
        items
            .iter()
            .any(|i| i["text"] == text && source(script, i).as_deref() == Some(src))
    };
    assert!(
        found("x", "x") && found("=", "=") && found("√", "sqrt"),
        "{layout}"
    );
    assert!(
        found("±", "+-") && found("4ac", "4ac") || found("4", "4"),
        "{layout}"
    );
    // Pieces left of the fraction bar come first, the denominator below them.
    let b = items
        .iter()
        .find(|i| i["text"] == "b")
        .cloned()
        .unwrap_or_default();
    let two = items
        .iter()
        .find(|i| source(script, i).as_deref() == Some("2a") || i["text"] == "2")
        .cloned()
        .unwrap_or_default();
    assert!(
        two["y"].as_f64() > b["y"].as_f64(),
        "denominator below numerator: {b} {two}"
    );
    assert!(layout["slots"].as_array().is_some_and(Vec::is_empty));

    // A template's empty braces become slots, in script order, with a box each.
    let template = "sum_{} ^{} {} over {}";
    let layout = s.run(
        "equation.layout",
        json!({"script": template, "size_pt": 10.0}),
    )?;
    let slots: Vec<u64> = layout["slots"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    assert_eq!(slots, [5, 9, 12, 20]);
    let boxes = layout["items"]
        .as_array()
        .map_or(0, |a| a.iter().filter(|i| i["kind"] == "slot").count());
    assert_eq!(boxes, 4);
    assert!(
        s.run("equation.layout", json!({"script": "", "size_pt": 10.0}))
            .is_err()
    );
    assert!(
        s.run("equation.layout", json!({"script": "a", "size_pt": 1000.0}))
            .is_err()
    );
    Ok(())
}

#[test]
fn unmatched_pieces_carry_no_script_range() -> anyhow::Result<()> {
    // A legacy hwpeq script is rewritten before parsing: no position may point
    // into the original text.
    let s = Session::new()?;
    let layout = s.equation_layout(&hwpcraft_engine::EquationOptions {
        script: "\\frac a b \\TAB".into(),
        size_pt: 10.0,
    })?;
    assert!(
        layout
            .items
            .iter()
            .all(|i| i.start.is_none() && i.end.is_none())
    );
    Ok(())
}

#[test]
fn equation_macros_save_list_get_delete() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    assert!(hwpcraft_engine::set_config_dir(dir.path().to_owned()));
    let mut s = Session::new()?;
    let formula = "x = {-b +- sqrt {b^2 -4ac}} over {2a}";
    s.run(
        "equation.macro.save",
        json!({"name": " 근의 공식 ", "script": formula}),
    )?;
    s.run(
        "equation.macro.save",
        json!({"name": "피타고라스", "script": "a^2+b^2=c^2"}),
    )?;
    let list = s.run("equation.macro.list", json!({}))?;
    assert_eq!(list["macros"][0]["name"], "근의 공식", "{list}");
    assert_eq!(list["macros"][1]["script"], "a^2+b^2=c^2");
    let got = s.run("equation.macro.get", json!({"name": "근의 공식"}))?;
    assert_eq!(got["script"], formula);
    // Replacing keeps one entry of that name.
    s.run(
        "equation.macro.save",
        json!({"name": "피타고라스", "script": "c^2=a^2+b^2"}),
    )?;
    assert_eq!(
        s.run("equation.macro.list", json!({}))?["macros"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    // Bad names and scripts are refused and change nothing.
    for bad in [
        json!({"name": "", "script": "a"}),
        json!({"name": "가".repeat(41), "script": "a"}),
        json!({"name": "깊이", "script": "{".repeat(40)}),
        json!({"name": "빈", "script": "  "}),
        json!({"name": "x", "script": "a", "extra": 1}),
    ] {
        assert!(s.run("equation.macro.save", bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(
        s.run("equation.macro.list", json!({}))?["macros"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    s.run("equation.macro.delete", json!({"name": "근의 공식"}))?;
    assert!(
        s.run("equation.macro.get", json!({"name": "근의 공식"}))
            .is_err()
    );
    assert!(
        s.run("equation.macro.delete", json!({"name": "근의 공식"}))
            .is_err()
    );
    // A saved macro inserts like any script.
    let script = s.run("equation.macro.get", json!({"name": "피타고라스"}))?["script"].clone();
    s.run(
        "equation.insert",
        json!({"script": script, "size_pt": 10.0}),
    )?;
    let objects = s.objects();
    assert!(
        objects
            .as_array()
            .is_some_and(|l| l.iter().any(|o| o["kind"] == "equation")),
        "{objects}"
    );
    assert!(dir.path().join("equation_macros.json").exists());
    Ok(())
}
