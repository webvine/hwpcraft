//! Hangul actions whose name fixes their parameters run with their defaults
//! alone; one that still needs a caller's value says which command it runs and
//! what that command takes.
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

/// A document with a 2×2 table, the caret in its first cell.
fn table() -> Session {
    let mut s = Session::new().unwrap();
    s.execute(Command::Insert("앞 문단".into())).unwrap();
    s.run("table.create", json!({"rows": 2, "columns": 2}))
        .unwrap();
    s
}

fn props(s: &mut Session) -> Value {
    s.run("table.properties.get", json!({})).unwrap()
}

#[test]
fn table_caption_positions_turn_the_caption_on_where_the_name_says() {
    for (id, direction, vert) in [
        ("TableCaptionPosTop", 2, None),
        ("TableCaptionPosBottom", 3, None),
        ("TableCaptionPosLeftTop", 0, Some(0)),
        ("TableCaptionPosLeftCenter", 0, Some(1)),
        ("TableCaptionPosLeftBottom", 0, Some(2)),
        ("TableCaptionPosRightTop", 1, Some(0)),
        ("TableCaptionPosRightCenter", 1, Some(1)),
        ("TableCaptionPosRightBottom", 1, Some(2)),
    ] {
        let mut s = table();
        s.execute_hwp_action(id, json!({}), None)
            .unwrap_or_else(|e| panic!("{id}: {e:#}"));
        let p = props(&mut s);
        assert_eq!(p["hasCaption"], true, "{id}: {p}");
        assert_eq!(p["captionDirection"], direction, "{id}: {p}");
        if let Some(v) = vert {
            assert_eq!(p["captionVertAlign"], v, "{id}: {p}");
        }
        // The caption can then be written.
        s.run("table.caption_text", json!({"text": "분기별 매출"}))
            .unwrap();
    }
}

#[test]
fn cell_and_row_actions_split_add_delete_and_resize_by_their_name() {
    let mut s = table();
    s.execute_hwp_action("CellSplitColTwo", json!({}), None)
        .unwrap();
    let cells = s.run("table.info", json!({})).unwrap()["cells"].clone();
    assert_eq!(cells.as_array().unwrap().len(), 5, "{cells}");

    let mut s = table();
    s.execute_hwp_action("CellSplitRowTwo", json!({}), None)
        .unwrap();
    let cells = s.run("table.info", json!({})).unwrap()["cells"].clone();
    assert_eq!(cells.as_array().unwrap().len(), 5, "{cells}");

    let mut s = table();
    s.execute_hwp_action("TableAppendRow", json!({}), None)
        .unwrap();
    assert_eq!(props(&mut s)["rows"], 3);
    s.execute_hwp_action("TableSubtractRow", json!({}), None)
        .unwrap();
    assert_eq!(props(&mut s)["rows"], 2);

    let width = |s: &mut Session| {
        s.run("table.info", json!({})).unwrap()["cells"][0]["width"]
            .as_i64()
            .unwrap()
    };
    let height = |s: &mut Session| {
        s.run("table.info", json!({})).unwrap()["cells"][0]["height"]
            .as_i64()
            .unwrap()
    };
    let mut s = table();
    let (w, h) = (width(&mut s), height(&mut s));
    s.execute_hwp_action("TableResizeCellRight", json!({}), None)
        .unwrap();
    assert!(width(&mut s) > w);
    s.execute_hwp_action("TableResizeCellLeft", json!({}), None)
        .unwrap();
    assert_eq!(width(&mut s), w);
    s.execute_hwp_action("TableResizeCellDown", json!({}), None)
        .unwrap();
    s.execute_hwp_action("TableResizeCellDown", json!({}), None)
        .unwrap();
    let taller = height(&mut s);
    assert!(taller > h);
    // (A new row is 282 HU, just under the 1 mm the resize allows: shrink from 2 mm up.)
    s.execute_hwp_action("TableResizeCellUp", json!({}), None)
        .unwrap();
    assert!(height(&mut s) < taller);
}

#[test]
fn an_action_that_needs_a_callers_value_says_what_to_give() {
    let mut s = table();
    // The border under the caret is not in the name: the caller gives it.
    let e = s
        .execute_hwp_action("TableResizeLineRight", json!({}), None)
        .unwrap_err();
    let e = format!("{e:#}");
    assert!(
        e.contains("table.move_border") && e.contains("boundary") && e.contains("params"),
        "{e}"
    );
    s.execute_hwp_action("TableResizeLineRight", json!({"boundary": 1}), None)
        .unwrap();
    // A caption on a table that has none says how to add one.
    let mut s = table();
    let e = format!(
        "{:#}",
        s.run("table.caption_text", json!({"text": "x"}))
            .unwrap_err()
    );
    assert!(
        e.contains("hasCaption") && e.contains("TableCaptionPosBottom"),
        "{e}"
    );
    // Each changed action keeps its public id, with the mapping the catalog shows.
    for (id, mapping) in [
        ("TableCaptionPosBottom", "mapped"),
        ("CellSplitColTwo", "mapped"),
        ("TableResizeCellDown", "mapped"),
        ("CaptionPosBottom", "partial"),
        ("TableResizeLineLeft", "partial"),
        ("PageNumPageBoth", "partial"),
    ] {
        assert_eq!(
            hwpcraft_engine::hwp_action(id).map(|a| a.2),
            Some(mapping),
            "{id}"
        );
    }
}
