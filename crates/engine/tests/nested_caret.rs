//! Editing at a caret inside a nested table cell or a text box: the places the body
//! caret (`hit_test`) refuses. A click lands there, typing, Backspace, Delete and Enter
//! edit that paragraph with one undo step each, and the result survives HWP/HWPX.
use hwpcraft_engine::{Command, ContextAddress, ContextStep, Motion, Session};
use serde_json::json;

/// A body table whose first cell holds a 2×2 table; returns the address of that nested
/// table's first cell paragraph.
fn nested_table(s: &mut Session) -> anyhow::Result<ContextAddress> {
    s.run("insert.table", json!({"rows": 2, "columns": 2}))?;
    let created = s.run(
        "context.table.create",
        json!({"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},
               "rows":2,"columns":2}),
    )?;
    let first = &created["address"];
    let steps = first["path"].as_array().cloned().unwrap_or_default();
    anyhow::ensure!(steps.len() == 2, "{created}");
    let step = |v: &serde_json::Value| ContextStep::TableCell {
        control: v["control"].as_u64().unwrap_or(0) as usize,
        cell: v["cell"].as_u64().unwrap_or(0) as usize,
        paragraph: v["paragraph"].as_u64().unwrap_or(0) as usize,
    };
    Ok(ContextAddress {
        version: 2,
        section: 0,
        paragraph: first["paragraph"].as_u64().unwrap_or(0) as usize,
        path: steps.iter().map(step).collect(),
    })
}

fn text_at(s: &mut Session, address: &ContextAddress) -> anyhow::Result<String> {
    let v = s.run("context.v2.get", json!({ "address": address }))?;
    Ok(v["text"].as_str().unwrap_or_default().to_owned())
}

#[test]
fn a_click_in_a_nested_cell_puts_the_caret_there_and_typing_edits_it() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let address = nested_table(&mut s)?;
    // The body caret refuses the nested cell; the nested hit finds it.
    let mut placed = None;
    'search: for y in (0..1100).step_by(4) {
        for x in (0..800).step_by(8) {
            if let Ok(Some(c)) = s.hit_nested(0, f64::from(x), f64::from(y))
                && c.address == address
            {
                placed = Some(c);
                break 'search;
            }
        }
    }
    let caret = placed.ok_or_else(|| anyhow::anyhow!("no point hits the nested cell"))?;
    let rect = {
        s.set_nested(caret, false)?;
        s.nested_caret_rect()?
    };
    assert_eq!(rect["pageIndex"], 0, "{rect}");
    assert!(
        s.hit_test(
            0,
            rect["x"].as_f64().unwrap_or(0.0) + 1.0,
            rect["y"].as_f64().unwrap_or(0.0) + 2.0
        )
        .is_err(),
        "the body caret still refuses nested cells"
    );

    s.nested_insert("가나다")?;
    assert_eq!(text_at(&mut s, &address)?, "가나다");
    assert_eq!(s.nested_caret().map(|c| c.offset), Some(3));
    s.nested_move(Motion::Left, false)?;
    s.nested_backspace()?;
    assert_eq!(text_at(&mut s, &address)?, "가다");
    s.nested_delete()?;
    assert_eq!(text_at(&mut s, &address)?, "가");

    // Enter splits inside the nested cell and moves to the new paragraph.
    s.nested_insert("나다")?;
    s.nested_move(Motion::Left, false)?;
    s.nested_enter()?;
    let second = s.nested_caret().map(|c| c.address.clone()).expect("caret");
    assert_ne!(second, address);
    assert_eq!(text_at(&mut s, &address)?, "가나");
    assert_eq!(text_at(&mut s, &second)?, "다");
    // Backspace at the start joins it back.
    s.nested_backspace()?;
    assert_eq!(text_at(&mut s, &address)?, "가나다");
    assert_eq!(
        s.nested_caret().map(|c| (c.address.clone(), c.offset)),
        Some((address.clone(), 2))
    );

    // A selection replaced by typing; format the selection.
    s.nested_move(Motion::LineStart, false)?;
    s.nested_move(Motion::Right, true)?;
    assert_eq!(s.nested_selected_text()?, "가");
    assert!(!s.nested_selection_rects()?.is_empty());
    s.nested_character(json!({"bold": true}))?;
    s.nested_insert("라")?;
    assert_eq!(text_at(&mut s, &address)?, "라나다");

    // Each edit is one undo step, and the caret survives undo.
    s.execute(Command::Undo)?;
    s.revalidate_nested();
    assert_eq!(text_at(&mut s, &address)?, "가나다");
    assert!(s.nested_caret().is_some());

    for name in ["n.hwp", "n.hwpx"] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(text_at(&mut reopened, &address)?, "가나다", "{name}");
    }
    Ok(())
}

#[test]
fn a_text_box_in_the_body_takes_a_nested_caret() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "shape.insert",
        json!({"kind":"textbox","width_mm":60.0,"height_mm":30.0,"paragraphs":["글상자"]}),
    )?;
    let mut found = None;
    'search: for y in (0..1100).step_by(4) {
        for x in (0..800).step_by(6) {
            if let Ok(Some(c)) = s.hit_nested(0, f64::from(x), f64::from(y)) {
                found = Some(c);
                break 'search;
            }
        }
    }
    let caret = found.ok_or_else(|| anyhow::anyhow!("no point hits the text box"))?;
    assert!(
        matches!(caret.address.path.last(), Some(ContextStep::TextBox { .. })),
        "{:?}",
        caret.address
    );
    let address = caret.address.clone();
    s.set_nested(caret, false)?;
    s.nested_move(Motion::LineEnd, false)?;
    s.nested_insert(" 고침")?;
    assert_eq!(text_at(&mut s, &address)?, "글상자 고침");
    s.nested_caret_rect()?;
    Ok(())
}

#[test]
fn edits_without_a_nested_caret_are_refused() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    assert!(s.nested_insert("x").is_err());
    assert!(s.nested_backspace().is_err());
    assert!(s.nested_caret_rect().is_err());
    Ok(())
}
