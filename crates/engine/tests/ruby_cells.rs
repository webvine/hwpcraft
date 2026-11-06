//! Owner: format area. 덧말 in a table cell: insert, list, update and delete as in the
//! body, kept on save. Before, a cell caret was refused.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Cell, Cursor, Session};
use serde_json::json;

fn cell_caret(s: &Session, offset: usize) -> Cursor {
    let (paragraph, control) = s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(p, para)| {
            para.controls
                .iter()
                .position(|c| matches!(c, Control::Table(_)))
                .map(|c| (p, c))
        })
        .expect("table");
    Cursor {
        section: 0,
        paragraph,
        offset,
        cell: Some(Cell {
            control,
            index: 0,
            paragraph: 0,
        }),
    }
}

fn cell_text(s: &Session) -> String {
    let c = cell_caret(s, 0);
    let cell = c.cell.unwrap();
    match &s.core.document().sections[0].paragraphs[c.paragraph].controls[cell.control] {
        Control::Table(t) => t.cells[0].paragraphs[0].text.clone(),
        _ => unreachable!(),
    }
}

#[test]
fn ruby_in_a_table_cell() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("insert.table", json!({"rows": 1, "columns": 2}))?;
    s.cursor = cell_caret(&s, 0);
    s.run("text.insert", json!({"text": "漢字 공부"}))?;
    // Select 漢字 in the cell and give it a 덧말.
    s.cursor = cell_caret(&s, 2);
    s.anchor = Some(0);
    let made = s.run("format.ruby", json!({"text": "한자"}))?;
    assert_eq!(made["position"], 0, "{made}");
    assert_eq!(
        cell_text(&s),
        " 공부",
        "the base text moved into the control"
    );
    s.cursor = cell_caret(&s, 0);
    let list = s.run("format.ruby.list", json!({}))?;
    assert_eq!(list[0]["base"], "漢字", "{list}");
    assert_eq!(list[0]["text"], "한자", "{list}");
    let control = list[0]["control"].clone();
    s.run(
        "format.ruby.update",
        json!({"control": control, "text": "한문 글자"}),
    )?;
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("ruby.{ext}"));
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        back.cursor = cell_caret(&back, 0);
        let list = back.run("format.ruby.list", json!({}))?;
        assert_eq!(list[0]["text"], "한문 글자", "{ext}: {list}");
        assert_eq!(list[0]["base"], "漢字", "{ext}");
    }
    s.cursor = cell_caret(&s, 0);
    s.run("format.ruby.delete", json!({"control": control}))?;
    assert_eq!(cell_text(&s), "漢字 공부", "the base text comes back");
    s.run("edit.undo", json!({}))?;
    assert_eq!(cell_text(&s), " 공부");
    Ok(())
}
