use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::{Value, json};

fn reopen(s: &mut Session, ext: &str) -> anyhow::Result<Session> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(format!("ext-format.{ext}"));
    s.save(&path)?;
    let mut result = Session::new()?;
    result.open(&path)?;
    Ok(result)
}
fn doc(text: &str) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.execute(Command::Insert(text.into()))?;
    Ok(s)
}
/// A failed command must leave the document, history and revision untouched.
fn rejected(s: &mut Session, id: &str, params: Value) {
    let info = s.info();
    let generation = s.generation;
    let undo = s.can_undo();
    assert!(s.run(id, params.clone()).is_err(), "{id} {params}");
    assert_eq!(s.info(), info, "{id}");
    assert_eq!(s.generation, generation, "{id}");
    assert_eq!(s.can_undo(), undo, "{id}");
}
fn para(s: &mut Session) -> anyhow::Result<Value> {
    s.run("format.paragraph.get", json!({}))
}
fn chars(s: &mut Session) -> anyhow::Result<Value> {
    s.run("format.character.get", json!({}))
}

#[test]
fn character_toggles_steps_and_undo() -> anyhow::Result<()> {
    let mut s = doc("가나다 ABC")?;
    s.run("format.bold", json!({}))?;
    assert_eq!(chars(&mut s)?["bold"], true);
    s.run("format.bold", json!({}))?;
    assert_eq!(chars(&mut s)?["bold"], false);
    s.run("format.underline", json!({"value": true}))?;
    assert_eq!(chars(&mut s)?["underlineType"], "Bottom");
    s.run("format.subscript", json!({}))?;
    s.run("format.superscript", json!({}))?;
    let p = chars(&mut s)?;
    assert_eq!(
        (p["superscript"].clone(), p["subscript"].clone()),
        (json!(true), json!(false))
    );
    s.run("format.strikethrough", json!({}))?;
    s.run("format.normal", json!({}))?;
    let p = chars(&mut s)?;
    for key in [
        "bold",
        "underline",
        "strikethrough",
        "superscript",
        "subscript",
    ] {
        assert_eq!(p[key], false, "{key}");
    }
    let size = chars(&mut s)?["fontSize"].as_i64().unwrap();
    s.run("format.font_size.increase", json!({}))?;
    assert_eq!(chars(&mut s)?["fontSize"], size + 100);
    s.run("format.ratio.decrease", json!({}))?;
    s.run("format.spacing.increase", json!({}))?;
    let p = chars(&mut s)?;
    assert_eq!(p["ratios"][0], 99);
    assert_eq!(p["spacings"][0], 1);
    s.run("format.highlight", json!({"color": "#ffff00"}))?;
    let h = s.run("format.highlight.get", json!({}))?;
    assert_eq!(h["color"], "#ffff00", "형광펜 is a markpen mark, not 음영");
    assert_ne!(chars(&mut s)?["shadeColor"], "#ffff00");
    for _ in 0..4 {
        s.execute(Command::Undo)?;
    }
    assert_eq!(chars(&mut s)?["fontSize"], size);
    for (id, params) in [
        ("format.bold", json!({"value": "yes"})),
        ("format.bold", json!({"other": true})),
        ("format.normal", json!({"x": 1})),
        ("format.font_size.increase", json!({"step": 2})),
        ("format.highlight", json!({"color": "yellow"})),
        ("format.highlight", json!({"colour": "#ffff00"})),
        ("format.highlight.get", json!({"x": 1})),
        (
            "format.character",
            json!({"ratios": [40, 100, 100, 100, 100, 100, 100]}),
        ),
        (
            "format.character",
            json!({"spacings": [60, 0, 0, 0, 0, 0, 0]}),
        ),
    ] {
        rejected(&mut s, id, params);
    }
    Ok(())
}

#[test]
fn paragraph_alignment_spacing_indent_and_margins() -> anyhow::Result<()> {
    let mut s = doc("문단 도구 검사")?;
    for align in ["left", "center", "right", "distribute", "split", "justify"] {
        s.run(&format!("format.align.{align}"), json!({}))?;
        assert_eq!(para(&mut s)?["alignment"], align);
    }
    let before = para(&mut s)?;
    s.run("format.line_spacing.increase", json!({}))?;
    assert_eq!(
        para(&mut s)?["lineSpacing"].as_i64(),
        before["lineSpacing"].as_i64().map(|v| v + 10)
    );
    // Hangul 2024 steps indents and margins by 1 pt = 200 raw units.
    let step = 200;
    s.run("format.indent.positive", json!({}))?;
    s.run("format.margin_left.increase", json!({}))?;
    s.run("format.margin.increase", json!({}))?;
    let p = para(&mut s)?;
    assert_eq!(p["indent"], step);
    assert_eq!(p["marginLeft"], step * 2);
    assert_eq!(p["marginRight"], step);
    s.run("format.margin_left.decrease", json!({}))?;
    s.run("format.margin_left.decrease", json!({}))?;
    s.run("format.margin_left.decrease", json!({}))?;
    assert_eq!(
        para(&mut s)?["marginLeft"],
        0,
        "left margin never goes negative"
    );
    for _ in 0..6 {
        s.execute(Command::Undo)?;
    }
    assert_eq!(para(&mut s)?["indent"], before["indent"]);
    rejected(&mut s, "format.align.left", json!({"x": 1}));
    rejected(&mut s, "format.paragraph", json!({"alignment": "middle"}));
    Ok(())
}

#[test]
fn character_and_paragraph_borders_keep_unspecified_sides() -> anyhow::Result<()> {
    let mut s = doc("테두리 글자")?;
    let line = json!({"type": 1, "width": 2, "color": "#ff0000"});
    s.run("format.character", json!({"borderLeft": line}))?;
    let p = chars(&mut s)?;
    assert_eq!(p["borderLeft"]["type"], 1);
    // RHWP would default an omitted side to a solid line; the engine fills it from
    // the current (none) value instead.
    assert_eq!(p["borderRight"]["type"], 0);
    let box_line = json!({"type": 8, "width": 3, "color": "#0000ff"});
    s.run(
        "format.paragraph",
        json!({"borderLeft": box_line, "borderRight": box_line, "borderTop": box_line,
            "borderBottom": box_line, "fillType": "solid", "fillColor": "#eeeeee",
            "borderSpacing": [200, 200, 100, 100], "borderConnect": true}),
    )?;
    s.run(
        "format.paragraph",
        json!({"borderTop": {"type": 2, "width": 0, "color": "#000000"}}),
    )?;
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let p = para(&mut r)?;
        assert_eq!(p["borderLeft"]["type"], 8, "{ext}");
        assert_eq!(p["borderTop"]["type"], 2, "{ext}");
        assert_eq!(p["fillType"], "solid", "{ext}");
        assert_eq!(p["fillColor"], "#eeeeee", "{ext}");
        assert_eq!(p["borderSpacing"], json!([200, 200, 100, 100]), "{ext}");
        assert_eq!(chars(&mut r)?["borderLeft"]["color"], "#ff0000", "{ext}");
    }
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    assert_eq!(para(&mut s)?["fillType"], "none");
    for params in [
        json!({"borderLeft": {"type": 30, "width": 0, "color": "#000000"}}),
        json!({"borderLeft": {"type": 1, "width": 0}}),
        json!({"fillType": "gradient"}),
        json!({"borderSpacing": [1, 2]}),
    ] {
        rejected(&mut s, "format.paragraph", params);
    }
    Ok(())
}

#[test]
fn user_tab_stops_roundtrip_and_validate() -> anyhow::Result<()> {
    let mut s = doc("이름\t값\t금액")?;
    let stops = json!([
        {"position": 8000, "type": 0, "fill": 0},
        {"position": 20000, "type": 2, "fill": 3},
        {"position": 30000, "type": 3, "fill": 1}
    ]);
    s.run(
        "format.paragraph",
        json!({"tabStops": stops, "tabAutoLeft": true}),
    )?;
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let p = para(&mut r)?;
        assert_eq!(p["tabStops"], stops, "{ext}");
        assert_eq!(p["tabAutoLeft"], true, "{ext}");
    }
    s.execute(Command::Undo)?;
    assert_eq!(para(&mut s)?["tabStops"], json!([]));
    for stops in [
        json!([{"position": 2000, "type": 0, "fill": 0}, {"position": 1000, "type": 0, "fill": 0}]),
        json!([{"position": 2000, "type": 4, "fill": 0}]),
        json!([{"position": -5, "type": 0, "fill": 0}]),
        json!([{"position": 2000, "type": 0}]),
    ] {
        rejected(&mut s, "format.paragraph", json!({"tabStops": stops}));
    }
    Ok(())
}

#[test]
fn styles_create_update_delete_and_shortcuts() -> anyhow::Result<()> {
    let mut s = doc("스타일 문단")?;
    let list = s.run("format.style.list", json!({}))?;
    let count = list.as_array().unwrap().len();
    assert_eq!(list[0]["shortcut"], "Ctrl+1");
    assert_eq!(list[9]["shortcut"], "Ctrl+0");
    s.run("format.bold", json!({}))?;
    let id = s.run(
        "format.style.create",
        json!({"name": "강조 문단", "englishName": "Emphasis"}),
    )?["id"]
        .as_u64()
        .unwrap() as usize;
    assert_eq!(id, count);
    let detail = s.run("format.style.detail", json!({"id": id}))?;
    assert_eq!(
        detail["character"]["bold"], true,
        "new style takes the caret's shapes"
    );
    assert_eq!(detail["nextStyleId"], id);
    s.run("style.apply", json!({"id": id}))?;
    s.run(
        "format.style.update",
        json!({"id": id, "name": "강조", "character": {"italic": true, "fontFamily": "Arial"},
            "paragraph": {"alignment": "center", "marginLeft": 2000}}),
    )?;
    let p = chars(&mut s)?;
    assert_eq!(
        p["italic"], true,
        "edited style propagates to its paragraphs"
    );
    assert_eq!(p["fontFamily"], "Arial");
    assert_eq!(para(&mut s)?["alignment"], "center");
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let list = r.run("format.style.list", json!({}))?;
        assert_eq!(list[id]["name"], "강조", "{ext}");
        assert_eq!(chars(&mut r)?["italic"], true, "{ext}");
        assert_eq!(para(&mut r)?["alignment"], "center", "{ext}");
    }
    // Ctrl+2 = second style.
    s.run("format.style.shortcut", json!({"number": 2}))?;
    let second = s.run("format.style.list", json!({}))?[1]["current"].clone();
    assert_eq!(second, true);
    s.run("style.apply", json!({"id": id}))?;
    let replaced = s.run("format.style.delete", json!({"id": id, "replacement": 0}))?;
    assert_eq!(replaced["replaced"], 1);
    let list = s.run("format.style.list", json!({}))?;
    assert_eq!(list.as_array().unwrap().len(), count);
    assert_eq!(list[0]["current"], true);
    s.execute(Command::Undo)?;
    assert_eq!(
        s.run("format.style.list", json!({}))?[id]["name"],
        "강조",
        "undo restores a deleted style"
    );
    for (cmd, params) in [
        ("format.style.create", json!({"name": "강조"})),
        ("format.style.create", json!({"name": " "})),
        ("format.style.create", json!({"name": "x", "type": "table"})),
        ("format.style.update", json!({"id": 999})),
        (
            "format.style.update",
            json!({"id": id, "character": {"bold": "yes"}}),
        ),
        ("format.style.delete", json!({"id": 0})),
        ("format.style.delete", json!({"id": id, "replacement": id})),
        ("format.style.shortcut", json!({"number": 10})),
        ("format.style.detail", json!({"id": 999})),
    ] {
        rejected(&mut s, cmd, params);
    }
    Ok(())
}

#[test]
fn numbering_bullets_outline_levels_and_restart() -> anyhow::Result<()> {
    let mut s = doc("첫째\n둘째\n셋째")?;
    s.select(
        Cursor {
            paragraph: 2,
            offset: 1,
            ..Default::default()
        },
        false,
    )?;
    s.span_anchor = Some(Cursor::default());
    s.run("format.numbering.toggle", json!({}))?;
    for i in 0..3 {
        s.select(
            Cursor {
                paragraph: i,
                ..Default::default()
            },
            false,
        )?;
        assert_eq!(para(&mut s)?["headType"], "Number", "paragraph {i}");
    }
    let list = s.run("format.numbering.list", json!({}))?;
    assert_eq!(list["numberings"][0]["levelFormats"][0], "^1.");
    s.run("format.level.increase", json!({}))?;
    assert_eq!(para(&mut s)?["paraLevel"], 1);
    s.run("format.level.decrease", json!({}))?;
    s.run("format.level.decrease", json!({}))?;
    assert_eq!(para(&mut s)?["paraLevel"], 0, "level 1 is the minimum");
    s.run("format.numbering.toggle", json!({}))?;
    assert_eq!(para(&mut s)?["headType"], "None");
    s.run("format.bullet.toggle", json!({}))?;
    assert_eq!(para(&mut s)?["headType"], "Bullet");
    s.run(
        "format.numbering.apply",
        json!({"kind": "bullet", "char": "■"}),
    )?;
    let bullets = s.run("format.numbering.list", json!({}))?["bullets"].clone();
    assert!(bullets.as_array().unwrap().iter().any(|b| b["char"] == "■"));
    s.run("format.outline.toggle", json!({}))?;
    assert_eq!(para(&mut s)?["headType"], "Outline");
    s.run(
        "format.numbering.apply",
        json!({"kind": "number", "levelFormats": ["^1)", "^2)", "(^3)", "(^4)", "^5", "^6", "^7)"],
            "numberFormats": [0, 8, 0, 8, 1, 9, 5], "startNumber": 1, "level": 1,
            "restart": {"mode": "new", "start": 5}}),
    )?;
    let p = para(&mut s)?;
    assert_eq!(p["headType"], "Number");
    assert_eq!(p["paraLevel"], 1);
    let list = s.run("format.numbering.list", json!({}))?;
    let id = p["numberingId"].as_u64().unwrap() as usize;
    assert_eq!(list["numberings"][id - 1]["levelStartNumbers"][1], 5);
    assert_eq!(p["numberingRestartMode"], 2, "a new list starts here");
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        r.select(
            Cursor {
                paragraph: 2,
                ..Default::default()
            },
            false,
        )?;
        let p = para(&mut r)?;
        assert_eq!(p["headType"], "Number", "{ext}");
        assert_eq!(p["paraLevel"], 1, "{ext}");
    }
    s.execute(Command::Undo)?;
    assert_eq!(para(&mut s)?["headType"], "Outline");
    for (cmd, params) in [
        ("format.numbering.apply", json!({"kind": "roman"})),
        (
            "format.numbering.apply",
            json!({"kind": "number", "id": 99}),
        ),
        (
            "format.numbering.apply",
            json!({"kind": "number", "level": 7}),
        ),
        (
            "format.numbering.apply",
            json!({"kind": "bullet", "char": "●●"}),
        ),
        (
            "format.numbering.apply",
            json!({"kind": "number", "levelFormats": ["^1."]}),
        ),
        (
            "format.numbering.create",
            json!({"levelFormats": ["^1."], "numberFormats": [0]}),
        ),
        (
            "format.numbering.restart",
            json!({"mode": "new", "start": 0}),
        ),
        ("format.numbering.restart", json!({"mode": "later"})),
        ("format.numbering.toggle", json!({"x": 1})),
    ] {
        rejected(&mut s, cmd, params);
    }
    Ok(())
}

#[test]
fn table_cell_formatting_commands_use_the_cell_paragraph() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Table {
        rows: 1,
        columns: 1,
    })?;
    let cell = s
        .entries()
        .into_iter()
        .find(|(c, _)| c.cell.is_some())
        .unwrap()
        .0;
    s.select(cell, false)?;
    s.execute(Command::Insert("셀 번호".into()))?;
    s.run("format.numbering.toggle", json!({}))?;
    s.run("format.align.center", json!({}))?;
    let before = para(&mut s)?["numberingId"].clone();
    s.run(
        "format.numbering.restart",
        json!({"mode": "new", "start": 3}),
    )?;
    let p = para(&mut s)?;
    assert_eq!(p["headType"], "Number");
    assert_ne!(
        p["numberingId"], before,
        "restart uses a new numbering definition"
    );
    assert!(
        s.run("format.numbering.restart", json!({"mode": "continue"}))
            .is_err(),
        "no preceding list in this cell"
    );
    assert_eq!(p["alignment"], "center");
    s.select(Cursor::default(), false)?;
    assert_eq!(para(&mut s)?["headType"], "None");
    Ok(())
}

#[test]
fn paragraph_dialog_applies_shape_and_head_in_one_undo_step() -> anyhow::Result<()> {
    let mut s = doc("문단 모양 창")?;
    s.run(
        "format.paragraph.apply",
        json!({"format": {"alignment": "center", "marginLeft": 4000},
            "head": {"kind": "outline", "level": 2}}),
    )?;
    let p = para(&mut s)?;
    assert_eq!(p["alignment"], "center");
    assert_eq!(p["headType"], "Outline");
    assert_eq!(p["paraLevel"], 2);
    s.execute(Command::Undo)?;
    let p = para(&mut s)?;
    assert_eq!(p["headType"], "None", "one undo restores both");
    assert_eq!(p["alignment"], "justify");
    s.run(
        "format.paragraph.apply",
        json!({"head": {"kind": "number"}}),
    )?;
    assert_eq!(para(&mut s)?["headType"], "Number");
    for params in [
        json!({"format": {"headType": "Number"}}),
        json!({"head": {"kind": "roman"}}),
        json!({"head": {"kind": "number", "level": 9}}),
        json!({"format": {"alignment": 3}}),
        json!({"other": 1}),
    ] {
        rejected(&mut s, "format.paragraph.apply", params);
    }
    Ok(())
}

#[test]
fn outline_shape_sets_section_numbering_and_roundtrips() -> anyhow::Result<()> {
    let mut s = doc("개요 문단")?;
    s.run("format.outline.toggle", json!({}))?;
    let formats = json!(["^1.", "^2.", "^3.", "^4)", "^5)", "(^6)", "(^7)"]);
    let codes = json!([2, 4, 0, 3, 5, 0, 8]);
    let id = s.run(
        "format.outline.shape",
        json!({"levelFormats": formats, "numberFormats": codes, "startNumber": 3}),
    )?["id"]
        .as_u64()
        .unwrap();
    let list = s.run("format.numbering.list", json!({}))?;
    assert_eq!(list["outlineId"], id);
    assert_eq!(list["numberings"][id as usize - 1]["levelFormats"], formats);
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let list = r.run("format.numbering.list", json!({}))?;
        let id = list["outlineId"].as_u64().unwrap() as usize;
        assert!(id > 0, "{ext}");
        assert_eq!(list["numberings"][id - 1]["numberFormats"], codes, "{ext}");
        assert_eq!(list["numberings"][id - 1]["startNumber"], 3, "{ext}");
    }
    s.execute(Command::Undo)?;
    assert_ne!(s.run("format.numbering.list", json!({}))?["outlineId"], id);
    for params in [
        json!({"levelFormats": ["^1."], "numberFormats": [0]}),
        json!({"levelFormats": formats, "numberFormats": [0, 0, 0, 0, 0, 0, 99]}),
        json!({"levelFormats": formats, "numberFormats": codes, "startNumber": 0}),
        json!({"levelFormats": formats}),
    ] {
        rejected(&mut s, "format.outline.shape", params);
    }
    Ok(())
}

fn highlights(s: &mut Session) -> anyhow::Result<Value> {
    Ok(s.run("format.highlight.get", json!({}))?["ranges"].clone())
}
fn select_range(s: &mut Session, start: usize, end: usize) -> anyhow::Result<()> {
    let base = s.cursor;
    let at = |offset| Cursor { offset, ..base };
    s.select(at(start), false)?;
    s.select(at(end), true)
}

#[test]
fn highlight_writes_markpen_marks_and_roundtrips() -> anyhow::Result<()> {
    let mut s = doc("형광펜 검사 문장입니다")?;
    select_range(&mut s, 0, 3)?;
    s.run("format.highlight", json!({"color": "#FFFF00"}))?;
    assert_eq!(
        highlights(&mut s)?,
        json!([{"start": 0, "end": 3, "color": "#ffff00"}])
    );
    // Overlapping color trims the earlier range; adjacent same colors merge.
    select_range(&mut s, 2, 6)?;
    s.run("format.highlight", json!({"color": "#00ff00"}))?;
    select_range(&mut s, 6, 8)?;
    s.run("format.highlight", json!({"color": "#00ff00"}))?;
    assert_eq!(
        highlights(&mut s)?,
        json!([
            {"start": 0, "end": 2, "color": "#ffff00"},
            {"start": 2, "end": 8, "color": "#00ff00"},
        ])
    );
    // No color removes the highlight of the block only.
    select_range(&mut s, 4, 5)?;
    s.run("format.highlight", json!({}))?;
    let expected = json!([
        {"start": 0, "end": 2, "color": "#ffff00"},
        {"start": 2, "end": 4, "color": "#00ff00"},
        {"start": 5, "end": 8, "color": "#00ff00"},
    ]);
    assert_eq!(highlights(&mut s)?, expected);
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        assert_eq!(highlights(&mut r)?, expected, "{ext}");
        assert_eq!(r.text(), "형광펜 검사 문장입니다", "{ext}");
        // Removing every highlight must survive saving too (stored range tags are dropped).
        r.select(Cursor::default(), false)?;
        r.run("format.highlight", json!({}))?;
        assert_eq!(highlights(&mut r)?, json!([]), "{ext}");
        let mut again = reopen(&mut r, ext)?;
        assert_eq!(highlights(&mut again)?, json!([]), "{ext} reopened");
    }
    s.execute(Command::Undo)?;
    assert_eq!(
        highlights(&mut s)?,
        json!([
            {"start": 0, "end": 2, "color": "#ffff00"},
            {"start": 2, "end": 8, "color": "#00ff00"},
        ])
    );
    // Typing before a highlight moves it (RHWP shifts marks on insert).
    s.select(Cursor::default(), false)?;
    s.execute(Command::Insert("AB".into()))?;
    assert_eq!(
        highlights(&mut s)?[0],
        json!({"start": 2, "end": 4, "color": "#ffff00"})
    );
    Ok(())
}

#[test]
fn header_footer_formatting_commands_target_the_hf_paragraph() -> anyhow::Result<()> {
    let mut s = doc("본문 문단")?;
    for (id, params) in [
        (
            "format.hf.character",
            json!({"header": true, "format": {"bold": true}}),
        ),
        (
            "format.hf.paragraph",
            json!({"header": true, "format": {"alignment": "center"}}),
        ),
        ("format.hf.character.get", json!({"header": true})),
    ] {
        rejected(&mut s, id, params);
    }
    s.set_header_footer(true, "머리말 글")?;
    let r = s.run(
        "format.hf.character",
        json!({"header": true, "start": 0, "end": 3, "format": {"bold": true, "textColor": "#ff0000"}}),
    )?;
    assert_eq!(r["bold"], true);
    let tail = s.run(
        "format.hf.character.get",
        json!({"header": true, "offset": 4}),
    )?;
    assert_eq!(tail["bold"], false, "outside the range is unchanged");
    let p = s.run(
        "format.hf.paragraph",
        json!({"header": true, "format": {"alignment": "center", "marginLeft": 1000}}),
    )?;
    assert_eq!(p["alignment"], "center");
    assert_eq!(p["marginLeft"], 1000);
    assert_eq!(
        para(&mut s)?["alignment"],
        "justify",
        "the body paragraph is untouched"
    );
    assert_eq!(chars(&mut s)?["bold"], false);
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let c = r.run("format.hf.character.get", json!({"header": true}))?;
        assert_eq!(c["bold"], true, "{ext}");
        assert_eq!(c["textColor"], "#ff0000", "{ext}");
        let p = r.run("format.hf.paragraph.get", json!({"header": true}))?;
        assert_eq!(p["alignment"], "center", "{ext}");
        assert_eq!(p["marginLeft"], 1000, "{ext}");
        assert_eq!(r.header_footer_text(true)?, "머리말 글", "{ext}");
    }
    s.execute(Command::Undo)?;
    let p = s.run("format.hf.paragraph.get", json!({"header": true}))?;
    assert_eq!(
        p["alignment"], "justify",
        "undo restores the paragraph shape"
    );
    s.execute(Command::Undo)?;
    let c = s.run("format.hf.character.get", json!({"header": true}))?;
    assert_eq!(c["bold"], false, "undo restores the character shape");
    for (id, params) in [
        (
            "format.hf.character",
            json!({"header": false, "format": {"bold": true}}),
        ),
        (
            "format.hf.character",
            json!({"header": true, "applyTo": "all", "format": {"bold": true}}),
        ),
        (
            "format.hf.character",
            json!({"header": true, "start": 3, "end": 2, "format": {"bold": true}}),
        ),
        (
            "format.hf.character",
            json!({"header": true, "end": 99, "format": {"bold": true}}),
        ),
        (
            "format.hf.character",
            json!({"header": true, "format": {"bold": "yes"}}),
        ),
        (
            "format.hf.character",
            json!({"header": true, "paragraph": 3, "format": {"bold": true}}),
        ),
        (
            "format.hf.paragraph",
            json!({"header": true, "format": {"alignment": "middle"}}),
        ),
        (
            "format.hf.paragraph",
            json!({"header": true, "start": 0, "format": {"alignment": "left"}}),
        ),
    ] {
        rejected(&mut s, id, params);
    }
    Ok(())
}

#[test]
fn footnote_paragraph_formatting_targets_the_note_paragraph() -> anyhow::Result<()> {
    let mut s = doc("각주 검사")?;
    s.run("insert.footnote", json!({"text": "주석 본문"}))?;
    let objects = s.run("object.list", json!({}))?;
    let note = objects
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["kind"] == "footnote")
        .cloned()
        .unwrap();
    let control = note["address"]["control"].as_u64().unwrap();
    let paragraph = note["address"]["paragraph"].as_u64().unwrap();
    let target = json!({"paragraph": paragraph, "control": control});
    let before = s.run("format.footnote.paragraph.get", target.clone())?;
    assert_ne!(before["alignment"], "right");
    let mut params = target.clone();
    params["format"] = json!({"alignment": "right", "spacingBefore": 300});
    let after = s.run("format.footnote.paragraph", params)?;
    assert_eq!(after["alignment"], "right");
    assert_eq!(after["spacingBefore"], 300);
    assert_ne!(
        para(&mut s)?["alignment"],
        "right",
        "body paragraph unchanged"
    );
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let p = r.run("format.footnote.paragraph.get", target.clone())?;
        assert_eq!(p["alignment"], "right", "{ext}");
        assert_eq!(p["spacingBefore"], 300, "{ext}");
    }
    s.execute(Command::Undo)?;
    let p = s.run("format.footnote.paragraph.get", target.clone())?;
    assert_ne!(p["alignment"], "right", "undo restores the note paragraph");
    for (id, params) in [
        (
            "format.footnote.paragraph",
            json!({"paragraph": paragraph, "control": 99, "format": {"alignment": "left"}}),
        ),
        (
            "format.footnote.paragraph",
            json!({"paragraph": paragraph, "control": control, "footnoteParagraph": 5,
                "format": {"alignment": "left"}}),
        ),
        (
            "format.footnote.paragraph",
            json!({"paragraph": paragraph, "control": control, "format": {"alignment": 3}}),
        ),
        (
            "format.footnote.paragraph.get",
            json!({"control": control, "x": 1}),
        ),
    ] {
        rejected(&mut s, id, params);
    }
    Ok(())
}

#[test]
fn copy_shape_apply_sets_character_and_paragraph_in_one_undo_step() -> anyhow::Result<()> {
    let mut s = doc("원본\n대상 문단")?;
    s.select(Cursor::default(), false)?;
    s.run("format.bold", json!({}))?;
    s.run("format.align.center", json!({}))?;
    let character = chars(&mut s)?;
    let paragraph = para(&mut s)?;
    assert_eq!(character["bold"], true);
    assert_eq!(paragraph["alignment"], "center");
    s.select(
        Cursor {
            paragraph: 1,
            ..Default::default()
        },
        false,
    )?;
    select_range(&mut s, 0, 2)?;
    assert_eq!(chars(&mut s)?["bold"], false);
    s.run(
        "format.copy_shape.apply",
        json!({"character": {"bold": character["bold"]}, "paragraph": {"alignment": paragraph["alignment"]}}),
    )?;
    assert_eq!(chars(&mut s)?["bold"], true);
    assert_eq!(para(&mut s)?["alignment"], "center");
    s.execute(Command::Undo)?;
    assert_eq!(chars(&mut s)?["bold"], false, "one undo step");
    assert_eq!(para(&mut s)?["alignment"], "justify");
    for (id, params) in [
        ("format.copy_shape.apply", json!({})),
        (
            "format.copy_shape.apply",
            json!({"character": {"bold": "yes"}}),
        ),
        (
            "format.copy_shape.apply",
            json!({"character": {"bold": true}, "paragraph": {"alignment": "x"}}),
        ),
    ] {
        rejected(&mut s, id, params);
    }
    Ok(())
}
