//! A table split across pages moves with the mouse, as in Hangul: the drag on any
//! piece moves the whole table (sideways on every page, down/up from where it
//! starts), it stays split, and the move is one undo step.
use hwpcraft_engine::{Command, ObjectAddress, Session};
use serde_json::{Value, json};

/// The boxes (page px) of the top-level table's pieces on `page`.
fn pieces(s: &Session, a: ObjectAddress, page: u32) -> anyhow::Result<Vec<[f64; 4]>> {
    let v: Value = serde_json::from_str(&s.page_control_layout(page)?)?;
    Ok(v["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| ObjectAddress::top_level_table(t) == Some(a))
        .map(|t| {
            let f = |k: &str| t[k].as_f64().unwrap_or(f64::NAN);
            [f("x"), f("y"), f("w"), f("h")]
        })
        .collect())
}
/// The table's first box on `page`, if a piece of it is there.
fn piece(s: &Session, a: ObjectAddress, page: u32) -> anyhow::Result<Option<[f64; 4]>> {
    Ok(pieces(s, a, page)?.first().copied())
}
fn all_pieces(s: &Session, a: ObjectAddress) -> anyhow::Result<Vec<(u32, [f64; 4])>> {
    let mut out = Vec::new();
    for page in 0..s.page_count()? {
        out.extend(pieces(s, a, page)?.into_iter().map(|b| (page, b)));
    }
    Ok(out)
}

fn split_table() -> anyhow::Result<(Session, ObjectAddress)> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "표 앞 문단"}))?;
    s.run("paragraph.split", json!({}))?;
    s.run("table.create", json!({"rows": 60, "columns": 3}))?;
    let a: ObjectAddress =
        serde_json::from_value(s.run("table.info", json!({}))?["table"].clone())?;
    assert!(piece(&s, a, 0)?.is_some() && piece(&s, a, 1)?.is_some());
    Ok((s, a))
}

const PX_PER_MM: f64 = 96.0 / 25.4;

#[test]
fn a_split_table_moves_sideways_on_every_page() -> anyhow::Result<()> {
    let (mut s, a) = split_table()?;
    let (first, last) = (piece(&s, a, 0)?.unwrap(), piece(&s, a, 1)?.unwrap());
    // Dragged on the second piece.
    s.run(
        "table.move",
        json!({"table": a, "page": 1, "dx_mm": 10.0, "dy_mm": 0.0}),
    )?;
    let (f2, l2) = (piece(&s, a, 0)?.unwrap(), piece(&s, a, 1)?.unwrap());
    assert!(
        (f2[0] - first[0] - 10.0 * PX_PER_MM).abs() < 1.0,
        "{first:?} -> {f2:?}"
    );
    assert!(
        (l2[0] - last[0] - 10.0 * PX_PER_MM).abs() < 1.0,
        "{last:?} -> {l2:?}"
    );
    assert!((f2[1] - first[1]).abs() < 1.0, "{first:?} -> {f2:?}");
    assert!((l2[1] - last[1]).abs() < 1.0, "{last:?} -> {l2:?}");
    s.execute(Command::Undo)?;
    assert_eq!(piece(&s, a, 0)?.unwrap(), first);
    assert_eq!(piece(&s, a, 1)?.unwrap(), last);
    Ok(())
}

#[test]
fn a_split_table_moves_down_from_where_it_starts() -> anyhow::Result<()> {
    let (mut s, a) = split_table()?;
    let first = piece(&s, a, 0)?.unwrap();
    let pages = s.page_count()?;
    // Dragged down on the second piece: the table starts lower on the first page
    // and is still split.
    s.run(
        "table.move",
        json!({"table": a, "page": 1, "dx_mm": 0.0, "dy_mm": 20.0}),
    )?;
    let f2 = piece(&s, a, 0)?.unwrap();
    assert!(
        (f2[1] - first[1] - 20.0 * PX_PER_MM).abs() < 1.0,
        "{first:?} -> {f2:?}"
    );
    assert!((f2[0] - first[0]).abs() < 1.0);
    assert!(piece(&s, a, 1)?.is_some());
    assert!(s.page_count()? >= pages);
    // The new place survives saving (both formats).
    let (p0, p1) = (piece(&s, a, 0)?.unwrap(), piece(&s, a, 1)?.unwrap());
    for ext in ["hwp", "hwpx"] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(format!("split.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        for (page, before) in [(0, p0), (1, p1)] {
            let after = piece(&r, a, page)?.unwrap();
            for k in 0..4 {
                assert!(
                    (after[k] - before[k]).abs() < 1.0,
                    "{ext} page {page}: {before:?} vs {after:?}"
                );
            }
        }
    }
    // And back up again from the first piece.
    s.run(
        "table.move",
        json!({"table": a, "page": 0, "dx_mm": 0.0, "dy_mm": -20.0}),
    )?;
    let f3 = piece(&s, a, 0)?.unwrap();
    assert!((f3[1] - first[1]).abs() < 1.0, "{first:?} -> {f3:?}");
    // Not above the paper.
    assert!(
        s.run(
            "table.move",
            json!({"table": a, "page": 1, "dx_mm": 0.0, "dy_mm": -400.0}),
        )
        .is_err()
    );
    assert_eq!(piece(&s, a, 0)?.unwrap(), f3);
    Ok(())
}

/// In a two-column section every piece moves sideways within its own column: the
/// table keeps its column reference instead of being placed on the paper (the
/// second-column piece used to jump onto column 1).
#[test]
fn a_table_split_across_columns_moves_each_piece_in_its_column() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("page.columns.count", json!({"count": 2}))?;
    s.run("table.create", json!({"rows": 120, "columns": 2}))?;
    let a: ObjectAddress =
        serde_json::from_value(s.run("table.info", json!({}))?["table"].clone())?;
    let before = all_pieces(&s, a)?;
    assert!(pieces(&s, a, 0)?.len() > 1, "{before:?}");
    let rel = s.table_properties(a)?["horzRelTo"].clone();
    s.run(
        "table.move",
        json!({"table": a, "page": 0, "dx_mm": 5.0, "dy_mm": 0.0}),
    )?;
    let after = all_pieces(&s, a)?;
    assert_eq!(after.len(), before.len(), "{before:?} -> {after:?}");
    for ((pb, b), (pa, x)) in before.iter().zip(&after) {
        assert_eq!(pb, pa);
        assert!(
            (x[0] - b[0] - 5.0 * PX_PER_MM).abs() < 1.0,
            "{b:?} -> {x:?}"
        );
        assert!((x[1] - b[1]).abs() < 1.0, "{b:?} -> {x:?}");
    }
    assert_eq!(s.table_properties(a)?["horzRelTo"], rel);
    Ok(())
}

/// The page layout gives a table nested in a cell the indices of its cell
/// paragraph (section, paragraph and control inside the cell), which can equal a
/// top-level table's address (s2/p8/c0 of a 370-page handbook). Such
/// a nested table is not a piece of that top-level table: a one-page table still
/// moves as a one-page table, and a split one moves by its own pieces.
#[test]
fn a_nested_table_is_not_a_piece_of_its_host() -> anyhow::Result<()> {
    let nest = |s: &mut Session, a: ObjectAddress, cell: usize| -> anyhow::Result<()> {
        s.run(
            "context.table.create",
            json!({"address":{"section":a.section,"paragraph":a.paragraph,
                "path":[{"control":a.control,"cell":cell,"paragraph":0}]},"rows":2,"columns":2}),
        )?;
        Ok(())
    };
    // T1 (paragraph 1) alone on page 1; T2 on page 2 holds a nested table whose
    // layout entry reads section 0, paragraph 1, control 0, as T1.
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞"}))?;
    s.run("paragraph.split", json!({}))?;
    s.run("paragraph.split", json!({}))?;
    s.run(
        "text.insert",
        json!({"text": "쪽을 넘기는 긴 문단입니다. ".repeat(130)}),
    )?;
    s.run("paragraph.split", json!({}))?;
    s.run("table.create", json!({"rows": 2, "columns": 2}))?;
    s.run(
        "selection.set",
        json!({"cursor":{"section":0,"paragraph":1,"offset":0}}),
    )?;
    s.run("table.create", json!({"rows": 2, "columns": 2}))?;
    let a: ObjectAddress =
        serde_json::from_value(s.run("table.info", json!({}))?["table"].clone())?;
    let host = ObjectAddress {
        section: 0,
        paragraph: 4,
        control: 0,
    };
    assert_eq!(piece(&s, host, 1)?.map(|_| ()), Some(()));
    nest(&mut s, host, 0)?;
    let collides = |s: &Session| -> anyhow::Result<bool> {
        let v: Value = serde_json::from_str(&s.page_control_layout(1)?)?;
        Ok(v["controls"].as_array().into_iter().flatten().any(|t| {
            t["secIdx"] == a.section && t["paraIdx"] == a.paragraph && t["controlIdx"] == a.control
        }))
    };
    assert!(collides(&s)?, "the nested table's entry reads as T1");
    assert_eq!(pieces(&s, a, 0)?.len(), 1);
    assert!(pieces(&s, a, 1)?.is_empty());
    // On the paper: a one-page move keeps that, a split one would anchor it to
    // its paragraph.
    s.run(
        "table.properties",
        json!({"table": a, "props": {"treatAsChar": false, "textWrap": "TopAndBottom",
            "horzRelTo": "Paper", "vertRelTo": "Paper"}}),
    )?;
    assert!(collides(&s)? && pieces(&s, a, 1)?.is_empty());
    let before = piece(&s, a, 0)?.unwrap();
    s.run(
        "table.move",
        json!({"table": a, "page": 0, "dx_mm": 5.0, "dy_mm": 10.0}),
    )?;
    let after = piece(&s, a, 0)?.unwrap();
    assert!((after[0] - before[0] - 5.0 * PX_PER_MM).abs() < 1.0);
    assert!((after[1] - before[1] - 10.0 * PX_PER_MM).abs() < 1.0);
    assert_eq!(s.table_properties(a)?["vertRelTo"], "Paper");

    // Split, with a nested table on its second page.
    let (mut s, a) = split_table()?;
    nest(&mut s, a, 150)?;
    assert_eq!(pieces(&s, a, 1)?.len(), 1);
    let before = all_pieces(&s, a)?;
    s.run(
        "table.move",
        json!({"table": a, "page": 1, "dx_mm": 5.0, "dy_mm": 0.0}),
    )?;
    let after = all_pieces(&s, a)?;
    assert_eq!(after.len(), before.len());
    for ((_, b), (_, x)) in before.iter().zip(&after) {
        assert!(
            (x[0] - b[0] - 5.0 * PX_PER_MM).abs() < 1.0,
            "{b:?} -> {x:?}"
        );
    }
    Ok(())
}

/// A one-page floating table: a paragraph-anchored one at paragraph 1, after a
/// text paragraph.
fn floating_table(rel: &str) -> anyhow::Result<(Session, ObjectAddress)> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "표 앞 문단"}))?;
    s.run("paragraph.split", json!({}))?;
    s.run("table.create", json!({"rows": 3, "columns": 2}))?;
    let a: ObjectAddress =
        serde_json::from_value(s.run("table.info", json!({}))?["table"].clone())?;
    s.run(
        "table.properties",
        json!({"table": a, "props": {"treatAsChar": false, "textWrap": "TopAndBottom",
            "horzRelTo": rel, "vertRelTo": rel}}),
    )?;
    Ok((s, a))
}
fn refs(s: &Session, a: ObjectAddress) -> anyhow::Result<(Value, Value)> {
    let p = s.table_properties(a)?;
    Ok((p["horzRelTo"].clone(), p["vertRelTo"].clone()))
}

/// Moving a one-page table keeps its references (0.22): it lands where it
/// was dropped and only its offsets change. A paragraph-anchored table then
/// still follows its text: more text above pushes it down and onto the next
/// page. Before, every moved table was placed on the paper.
#[test]
fn a_paragraph_table_moved_on_its_page_stays_with_its_paragraph() -> anyhow::Result<()> {
    let (mut s, a) = floating_table("Para")?;
    let before = piece(&s, a, 0)?.unwrap();
    let props_before = s.table_properties(a)?;
    s.run(
        "table.move",
        json!({"table": a, "page": 0, "dx_mm": 10.0, "dy_mm": 20.0}),
    )?;
    let after = piece(&s, a, 0)?.unwrap();
    assert!(
        (after[0] - before[0] - 10.0 * PX_PER_MM).abs() < 0.5,
        "{before:?} -> {after:?}"
    );
    assert!(
        (after[1] - before[1] - 20.0 * PX_PER_MM).abs() < 0.5,
        "{before:?} -> {after:?}"
    );
    assert_eq!(&after[2..], &before[2..]);
    assert_eq!(refs(&s, a)?, (json!("Para"), json!("Para")));

    // One undo step back to the very same table.
    s.execute(Command::Undo)?;
    assert_eq!(piece(&s, a, 0)?.unwrap(), before);
    assert_eq!(s.table_properties(a)?, props_before);
    s.execute(Command::Redo)?;
    assert_eq!(piece(&s, a, 0)?.unwrap(), after);

    // Text above grows: the table goes down with its paragraph, then onto the
    // next page (split or whole).
    s.run(
        "selection.set",
        json!({"cursor":{"section":0,"paragraph":0,"offset":0}}),
    )?;
    s.run(
        "text.insert",
        json!({"text": "늘어나는 앞 문단입니다. ".repeat(20)}),
    )?;
    let lower = piece(&s, a, 0)?.unwrap();
    assert!(lower[1] > after[1] + 10.0, "{after:?} -> {lower:?}");
    s.run(
        "text.insert",
        json!({"text": "늘어나는 앞 문단입니다. ".repeat(400)}),
    )?;
    assert!(
        (1..s.page_count()?).any(|page| piece(&s, a, page).ok().flatten().is_some()),
        "the table follows its paragraph onto a later page"
    );
    Ok(())
}

/// A table placed on the paper stays on the paper, at the dropped position.
#[test]
fn a_paper_table_moved_stays_on_the_paper() -> anyhow::Result<()> {
    let (mut s, a) = floating_table("Paper")?;
    let before = piece(&s, a, 0)?.unwrap();
    s.run(
        "table.move",
        json!({"table": a, "page": 0, "dx_mm": 5.0, "dy_mm": 30.0}),
    )?;
    let after = piece(&s, a, 0)?.unwrap();
    assert!(
        (after[0] - before[0] - 5.0 * PX_PER_MM).abs() < 0.5,
        "{before:?} -> {after:?}"
    );
    assert!(
        (after[1] - before[1] - 30.0 * PX_PER_MM).abs() < 0.5,
        "{before:?} -> {after:?}"
    );
    assert_eq!(refs(&s, a)?, (json!("Paper"), json!("Paper")));
    // Text above does not move it.
    s.run(
        "selection.set",
        json!({"cursor":{"section":0,"paragraph":0,"offset":0}}),
    )?;
    s.run(
        "text.insert",
        json!({"text": "늘어나는 앞 문단입니다. ".repeat(20)}),
    )?;
    assert_eq!(piece(&s, a, 0)?.unwrap(), after);
    Ok(())
}

/// Dragged into the bottom margin, a paragraph-anchored table stays on its page:
/// at the drop point, or stopped at the body's bottom edge with that position
/// stored, or the move is refused with nothing changed. It never ends up on
/// another page unannounced.
#[test]
fn a_table_dragged_below_the_body_lands_there_or_is_refused() -> anyhow::Result<()> {
    let (mut s, a) = floating_table("Para")?;
    let before = piece(&s, a, 0)?.unwrap();
    let props = s.table_properties(a)?;
    let generation = s.generation;
    let paper_h = s.page_info(0)?["height"].as_f64().unwrap();
    // The table's bottom 2 px above the paper's edge.
    let dy_mm = (paper_h - 2.0 - before[3] - before[1]) / PX_PER_MM;
    match s.run(
        "table.move",
        json!({"table": a, "page": 0, "dx_mm": 0.0, "dy_mm": dy_mm}),
    ) {
        Ok(_) => {
            // At the drop point, or stopped at the body's bottom edge on its page;
            // the file stores where it is shown (the same after reopening).
            let after = piece(&s, a, 0)?.expect("still on its page");
            let target = before[1] + dy_mm * PX_PER_MM;
            assert!(after[1] > before[1] && after[1] < target + 0.5, "{after:?}");
            for ext in ["hwp", "hwpx"] {
                let dir = tempfile::tempdir()?;
                let path = dir.path().join(format!("bottom.{ext}"));
                s.save(&path)?;
                let mut r = Session::new()?;
                r.open(&path)?;
                let again = piece(&r, a, 0)?.expect("still on its page");
                assert!(
                    (again[1] - after[1]).abs() < 0.6,
                    "{ext}: {after:?} {again:?}"
                );
            }
            s.execute(Command::Undo)?;
            assert_eq!(piece(&s, a, 0)?.unwrap(), before);
        }
        Err(e) => {
            assert!(e.to_string().contains("쪽 안에 들어가지 않습니다"), "{e}");
            assert_eq!(piece(&s, a, 0)?.unwrap(), before);
            assert_eq!(s.table_properties(a)?, props);
            assert_eq!(s.generation, generation);
        }
    }
    Ok(())
}

/// Not restricted to the page area (쪽 영역 안으로 제한 off): RHWP's layout still
/// stops a paragraph-anchored table at the body's bottom edge (a layout limit,
/// not the move). The move then stores the shown position as for a restricted
/// table, so the file says what the page shows; the setting itself is kept.
#[test]
fn an_unrestricted_table_dragged_below_the_body_is_stored_where_shown() -> anyhow::Result<()> {
    let (mut s, a) = floating_table("Para")?;
    s.run(
        "table.properties",
        json!({"table": a, "props": {"restrictInPage": false}}),
    )?;
    assert_eq!(s.table_properties(a)?["restrictInPage"], false);
    let before = piece(&s, a, 0)?.unwrap();
    let paper_h = s.page_info(0)?["height"].as_f64().unwrap();
    let dy_mm = (paper_h - 2.0 - before[3] - before[1]) / PX_PER_MM;
    s.run(
        "table.move",
        json!({"table": a, "page": 0, "dx_mm": 0.0, "dy_mm": dy_mm}),
    )?;
    let after = piece(&s, a, 0)?.expect("still on its page");
    let target = before[1] + dy_mm * PX_PER_MM;
    assert!(
        after[1] > before[1] && after[1] < target + 0.5,
        "{before:?} -> {after:?}"
    );
    assert_eq!(s.table_properties(a)?["restrictInPage"], false);
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("unrestricted.hwp");
    s.save(&path)?;
    let mut r = Session::new()?;
    r.open(&path)?;
    let again = piece(&r, a, 0)?.expect("still on its page");
    assert!((again[1] - after[1]).abs() < 0.6, "{after:?} {again:?}");
    Ok(())
}
