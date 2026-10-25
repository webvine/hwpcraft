use hwpcraft_doc::model::{control::Control, style::Alignment};
use hwpcraft_engine::{Command, DocumentAddress, Session};
use serde_json::{Value, json};

fn fixture() -> anyhow::Result<Session> {
    let mut inner = Session::new()?;
    inner.run(
        "table.create",
        json!({"rows":1,"columns":1,"width_mm":40.0,"treat_as_char":true}),
    )?;
    inner.execute(Command::Insert("안😀쪽".into()))?;
    let inner_table = inner
        .core
        .document()
        .sections
        .iter()
        .flat_map(|section| &section.paragraphs)
        .flat_map(|paragraph| &paragraph.controls)
        .find_map(|control| match control {
            Control::Table(table) => Some(table.clone()),
            _ => None,
        })
        .unwrap();
    let mut outer = Session::new()?;
    outer.run(
        "table.create",
        json!({"rows":1,"columns":2,"width_mm":120.0,"treat_as_char":true}),
    )?;
    let cursor = outer.cursor;
    let cell = cursor.cell.unwrap();
    let Some(Control::Table(table)) = outer.core.document_mut().sections[cursor.section].paragraphs
        [cursor.paragraph]
        .controls
        .get_mut(cell.control)
    else {
        panic!("fixture table");
    };
    let paragraph = &mut table.cells[0].paragraphs[0];
    paragraph.controls.push(Control::Table(inner_table));
    paragraph.char_count += 8;
    paragraph.invalidate_layout_inputs();
    outer.core.set_document(outer.core.document().clone());
    Ok(outer)
}

fn target(s: &mut Session, text: &str) -> anyhow::Result<DocumentAddress> {
    let rows = s.run("context.list", json!({"limit":1000}))?;
    let row = rows["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["depth"] == 2 && row["text"] == text)
        .unwrap();
    Ok(serde_json::from_value(row["address"].clone())?)
}
fn get(s: &mut Session, address: &DocumentAddress) -> anyhow::Result<Value> {
    s.run("context.get", json!({"address":address}))
}

fn assert_nested_text_visible(s: &Session, depth: usize, text: &str) -> anyhow::Result<()> {
    use hwpcraft_doc::{RenderNode, RenderNodeType};
    fn visit(
        node: &RenderNode,
        depth: usize,
        wanted: usize,
        frames: &[(f64, f64, f64, f64)],
        text: &mut String,
    ) {
        let mut depth = depth;
        let mut frames = frames.to_vec();
        if matches!(node.node_type, RenderNodeType::Table(_)) {
            depth += 1;
        }
        if matches!(node.node_type, RenderNodeType::TableCell(_)) {
            frames.push((
                node.bbox.x,
                node.bbox.y,
                node.bbox.x + node.bbox.width,
                node.bbox.y + node.bbox.height,
            ));
        }
        if let RenderNodeType::TextRun(run) = &node.node_type
            && depth == wanted
            && !run.text.trim().is_empty()
        {
            assert!(node.visible, "nested text run is hidden: {}", run.text);
            for &(left, top, right, bottom) in &frames {
                assert!(
                    node.bbox.x >= left - 1.0
                        && node.bbox.y >= top - 1.0
                        && node.bbox.x + node.bbox.width <= right + 1.0
                        && node.bbox.y + node.bbox.height <= bottom + 1.0,
                    "nested run {:?} ({:?}) escapes cell [{left},{top},{right},{bottom}]",
                    run.text,
                    node.bbox
                );
            }
            text.push_str(&run.text);
        }
        for child in &node.children {
            visit(child, depth, wanted, &frames, text);
        }
    }
    let mut actual = String::new();
    for page in 0..s.page_count()? {
        let tree = s
            .core
            .build_page_render_tree(page)
            .map_err(anyhow::Error::msg)?;
        visit(&tree.root, 0, depth, &[], &mut actual);
    }
    let compact = |value: &str| {
        value
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    assert!(
        compact(&actual).contains(&compact(text)),
        "missing visible depth-{depth} text: {actual:?}"
    );
    let svg = s.render_page_svg(0)?;
    assert!(!svg.is_empty());
    Ok(())
}

fn emit_fixture(s: &mut Session, name: &str) -> anyhow::Result<()> {
    if std::env::var("HWPCRAFT_WRITE_CONTEXT_FIXTURES").as_deref() == Ok("1") {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;
        let output = root.join("output/implementation/0.16/fixtures");
        std::fs::create_dir_all(&output)?;
        for extension in ["hwp", "hwpx"] {
            s.save(&output.join(format!("{name}.{extension}")))?;
        }
        std::fs::write(output.join(format!("{name}.pdf")), s.render_pdf(&[0])?)?;
        std::fs::write(output.join(format!("{name}.svg")), s.render_page_svg(0)?)?;
    }
    Ok(())
}

fn check_native_nested_fixture() -> anyhow::Result<()> {
    if std::env::var("HWPCRAFT_CHECK_NATIVE_CONTEXT_FIXTURE").as_deref() != Ok("1") {
        return Ok(());
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/implementation/0.16/nested-interop/native_roundtrip.hwp");
    let mut reopened = Session::new()?;
    reopened.open(&path)?;
    let list = reopened.run("context.list", json!({"limit":1000}))?;
    for (text, depth) in [("호스트", 1), ("새 중첩😀", 2), ("세 단계", 3)] {
        let row = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["depth"] == depth && row["text"] == text)
            .expect("native Hancom roundtrip must retain nested context text/depth");
        let value = reopened.run("context.get", json!({"address":row["address"]}))?;
        assert_eq!(value["character"]["bold"], true);
        assert_eq!(value["character"]["fontSize"], 1400);
    }
    assert!(reopened.render_pdf(&[0])?.starts_with(b"%PDF-"));
    Ok(())
}

#[test]
fn nested_text_edit_split_merge_undo_and_roundtrip() -> anyhow::Result<()> {
    let mut s = fixture()?;
    let address = target(&mut s, "안😀쪽")?;
    let cursor = s.cursor;
    let generation = s.generation;
    let changed = s.run(
        "context.replace",
        json!({"address":address,"start":1,"end":2,"text":"다른","expected_generation":generation}),
    )?;
    assert_eq!(changed["text"], "안다른쪽");
    assert_eq!(changed["offset"], 3);
    assert_eq!(
        s.cursor, cursor,
        "explicit context editing must preserve legacy cursor"
    );
    s.execute(Command::Undo)?;
    assert_eq!(get(&mut s, &address)?["text"], "안😀쪽");
    s.execute(Command::Redo)?;
    assert_eq!(get(&mut s, &address)?["text"], "안다른쪽");
    let split = s.run("context.split", json!({"address":address,"offset":2}))?;
    assert_eq!(get(&mut s, &address)?["text"], "안다");
    assert_eq!(split["text"], "른쪽");
    let second: DocumentAddress = serde_json::from_value(split["address"].clone())?;
    let merged = s.run("context.merge", json!({"address":second}))?;
    assert_eq!(merged["text"], "안다른쪽");
    assert_eq!(merged["offset"], 2);
    assert_eq!(s.cursor, cursor);
    s.run(
        "context.character",
        json!({"address":address,"start":0,"end":2,"format":{"bold":true,"fontSize":1600}}),
    )?;
    s.run(
        "context.paragraph",
        json!({"address":address,"format":{"alignment":"center","marginLeft":200}}),
    )?;
    let shape = s.context_paragraph(&address)?.para_shape_id;
    assert_eq!(
        s.core.document().doc_info.para_shapes[usize::from(shape)].alignment,
        Alignment::Center
    );
    let temp = tempfile::tempdir()?;
    for extension in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("nested.{extension}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        let address = target(&mut reopened, "안다른쪽")?;
        let result = reopened.run(
            "context.character",
            json!({"address":address,"start":0,"end":2,"format":{"italic":false}}),
        )?;
        assert_eq!(result["character"]["bold"], true, "{extension}");
        assert_eq!(result["character"]["fontSize"], 1600, "{extension}");
        let shape = reopened.context_paragraph(&address)?.para_shape_id;
        assert_eq!(
            reopened.core.document().doc_info.para_shapes[usize::from(shape)].alignment,
            Alignment::Center,
            "{extension}"
        );
        assert!(reopened.render_pdf(&[0])?.starts_with(b"%PDF-"));
    }
    emit_fixture(&mut s, "nested-contexts")?;
    Ok(())
}

#[test]
fn nested_table_creation_preserves_text_clipboard_styles_and_unique_instances() -> anyhow::Result<()>
{
    let mut s = Session::new()?;
    s.run(
        "table.create",
        json!({"rows":1,"columns":1,"width_mm":140.0,"treat_as_char":true}),
    )?;
    s.execute(Command::Insert("호스트".into()))?;
    let host = DocumentAddress::from_cursor(s.cursor);
    s.anchor = Some(0);
    let clipboard = s.copy()?;
    assert_eq!(clipboard, "호스트");
    s.clear_selection();
    s.run(
        "context.character",
        json!({"address":host,"format":{"bold":true,"fontSize":1400}}),
    )?;
    let generation = s.generation;
    let made=s.run("context.table.create",json!({"address":host,"rows":2,"columns":2,"width_mm":80.0,"expected_generation":generation}))?;
    assert_eq!(s.core.get_clipboard_text_native(), clipboard);
    assert_eq!(get(&mut s, &host)?["text"], "호스트");
    let first: DocumentAddress = serde_json::from_value(made["address"].clone())?;
    assert_eq!(first.path.len(), 2);
    assert_eq!(made["character"]["bold"], true);
    assert_eq!(made["character"]["fontSize"], 1400);
    let outer = &s.core.document().sections[host.section].paragraphs[host.paragraph].controls
        [host.path[0].control];
    let Control::Table(outer) = outer else {
        panic!("fixture outer table");
    };
    assert_ne!(
        u64::from(outer.common.instance_id),
        made["table"]["instanceId"].as_u64().unwrap()
    );
    s.run(
        "context.replace",
        json!({"address":first,"start":0,"end":0,"text":"새 중첩😀"}),
    )?;
    let deeper = s.run(
        "context.table.create",
        json!({"address":first,"rows":1,"columns":1,"width_mm":20.0}),
    )?;
    let deep: DocumentAddress = serde_json::from_value(deeper["address"].clone())?;
    assert_eq!(deep.path.len(), 3);
    assert_ne!(made["table"]["instanceId"], deeper["table"]["instanceId"]);
    s.run(
        "context.replace",
        json!({"address":deep,"start":0,"end":0,"text":"세 단계"}),
    )?;
    assert_nested_text_visible(&s, 3, "세 단계")?;
    let temp = tempfile::tempdir()?;
    for extension in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("created-nested.{extension}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        let list = reopened.run("context.list", json!({"limit":1000}))?;
        assert!(
            list["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["depth"] == 3 && row["text"] == "세 단계"),
            "{extension}"
        );
        assert!(
            list["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["depth"] == 2 && row["text"] == "새 중첩😀"),
            "{extension}"
        );
        assert!(reopened.render_pdf(&[0])?.starts_with(b"%PDF-"));
        assert_nested_text_visible(&reopened, 3, "세 단계")?;
    }
    emit_fixture(&mut s, "created-nested-contexts")?;
    check_native_nested_fixture()?;
    s.execute(Command::Undo)?;
    assert_eq!(get(&mut s, &deep)?["text"], "");
    s.execute(Command::Undo)?;
    let list = s.run("context.list", json!({"limit":1000}))?;
    assert!(
        !list["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["depth"] == 3)
    );
    s.execute(Command::Redo)?;
    assert_eq!(get(&mut s, &deep)?["text"], "");
    Ok(())
}

#[test]
fn nested_table_defaults_fit_the_cell_and_invalid_creation_is_atomic() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "table.create",
        json!({"rows":1,"columns":1,"width_mm":80.0,"treat_as_char":true}),
    )?;
    s.execute(Command::Insert("유지할 문단".into()))?;
    let host = DocumentAddress::from_cursor(s.cursor);
    let generation = s.generation;
    let original = s.run("context.list", json!({"limit":1000}))?;
    for dimensions in [
        json!({"rows":0,"columns":1}),
        json!({"rows":200,"columns":64}),
        json!({"rows":1,"columns":1,"width_mm":80.0}),
        json!({"rows":1,"columns":2,"column_widths_mm":[10.0]}),
        json!({"rows":1,"columns":1,"expected_generation":generation+1}),
    ] {
        let mut params = dimensions;
        params["address"] = json!(host);
        assert!(s.run("context.table.create", params).is_err());
        assert_eq!(s.generation, generation);
        assert_eq!(s.run("context.list", json!({"limit":1000}))?, original);
    }
    let made = s.run(
        "context.table.create",
        json!({"address":host,"rows":1,"columns":1,"position":"before"}),
    )?;
    let cell: DocumentAddress = serde_json::from_value(made["address"].clone())?;
    let last = cell.path.last().unwrap();
    let mut parent = cell.clone();
    parent.path.pop();
    let Control::Table(inner) = &s.context_paragraph(&parent)?.controls[last.control] else {
        panic!("created inner table");
    };
    let Control::Table(outer) = &s.core.document().sections[host.section].paragraphs
        [host.paragraph]
        .controls[host.path[0].control]
    else {
        panic!("outer table");
    };
    let padding = outer.cells[0].effective_padding(&outer.padding);
    assert!(
        i64::from(inner.common.width)
            + i64::from(inner.common.margin.left)
            + i64::from(inner.common.margin.right)
            <= i64::from(outer.cells[0].width) - i64::from(padding.left) - i64::from(padding.right)
                + 2
    );
    let mut preserved = host.clone();
    preserved.path.last_mut().unwrap().paragraph += 1;
    assert_eq!(get(&mut s, &preserved)?["text"], "유지할 문단");
    assert_eq!(DocumentAddress::from_cursor(s.cursor), preserved);
    let outer_step = &host.path[0];
    s.run("context.table.delete",json!({"address":{"section":host.section,"paragraph":host.paragraph,"path":[]},"control":outer_step.control,"allow_nonempty":true}))?;
    assert!(
        s.cursor.cell.is_none(),
        "deleting the active table moves the legacy caret to its host"
    );
    s.execute(Command::Insert("이어서".into()))?;
    Ok(())
}

#[test]
fn ancestor_growth_preserves_declared_minima_and_size_protection_is_atomic() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "table.create",
        json!({"rows":1,"columns":1,"width_mm":100.0,"height_mm":40.0,"treat_as_char":true}),
    )?;
    let host = DocumentAddress::from_cursor(s.cursor);
    let step = &host.path[0];
    let Control::Table(table) =
        &s.core.document().sections[host.section].paragraphs[host.paragraph].controls[step.control]
    else {
        panic!("outer table");
    };
    let original_minimum = table.common.height;
    let made = s.run(
        "context.table.create",
        json!({"address":host,"rows":1,"columns":1,"width_mm":20.0}),
    )?;
    let cell: DocumentAddress = serde_json::from_value(made["address"].clone())?;
    let Control::Table(table) = &mut s.core.document_mut().sections[host.section].paragraphs
        [host.paragraph]
        .controls[step.control]
    else {
        panic!("outer table");
    };
    assert!(table.common.height >= original_minimum);
    table.common.size_protect = true;
    let generation = s.generation;
    let cursor = s.cursor;
    let text = "내용이 길어 여러 줄로 자랍니다. ".repeat(40);
    assert!(
        s.run(
            "context.replace",
            json!({"address":cell,"start":0,"end":0,"text":text})
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    assert_eq!(s.cursor, cursor);
    assert_eq!(get(&mut s, &cell)?["text"], "");
    let Control::Table(table) =
        &s.core.document().sections[host.section].paragraphs[host.paragraph].controls[step.control]
    else {
        panic!("outer table");
    };
    assert!(table.common.size_protect);
    assert!(table.common.height >= original_minimum);
    Ok(())
}

#[test]
fn nested_table_deletion_requires_explicit_content_loss_and_obeys_protection() -> anyhow::Result<()>
{
    let mut s = fixture()?;
    let inner = target(&mut s, "안😀쪽")?;
    let step = inner.path.last().unwrap().clone();
    let mut host = inner.clone();
    host.path.pop();
    let generation = s.generation;
    assert!(
        s.run(
            "context.table.delete",
            json!({"address":host,"control":step.control})
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    assert_eq!(get(&mut s, &inner)?["text"], "안😀쪽");
    s.run("context.table.delete",json!({"address":host,"control":step.control,"allow_nonempty":true,"expected_generation":generation}))?;
    assert!(get(&mut s, &inner).is_err());
    s.execute(Command::Undo)?;
    assert_eq!(get(&mut s, &inner)?["text"], "안😀쪽");
    let outer_step = &host.path[0];
    let Some(Control::Table(outer)) = s.core.document_mut().sections[host.section].paragraphs
        [host.paragraph]
        .controls
        .get_mut(outer_step.control)
    else {
        panic!("outer fixture");
    };
    let Some(Control::Table(table)) = outer.cells[outer_step.cell].paragraphs[outer_step.paragraph]
        .controls
        .get_mut(step.control)
    else {
        panic!("inner fixture");
    };
    table.cells[0].set_cell_protect(true);
    let generation = s.generation;
    assert!(
        s.run(
            "context.table.delete",
            json!({"address":host,"control":step.control,"allow_nonempty":true})
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    assert_eq!(get(&mut s, &inner)?["text"], "안😀쪽");
    Ok(())
}

#[test]
fn deletion_counts_captions_and_refuses_locked_group_descendants() -> anyhow::Result<()> {
    use hwpcraft_doc::model::{
        paragraph::Paragraph,
        shape::{Caption, GroupShape, RectangleShape, ShapeObject},
    };
    let mut s = Session::new()?;
    s.run(
        "table.create",
        json!({"rows":1,"columns":1,"treat_as_char":true}),
    )?;
    let cursor = s.cursor;
    let control = cursor.cell.unwrap().control;
    let host = DocumentAddress {
        section: cursor.section,
        paragraph: cursor.paragraph,
        path: vec![],
    };
    let generation = s.generation;
    {
        let Control::Table(table) = &mut s.core.document_mut().sections[cursor.section].paragraphs
            [cursor.paragraph]
            .controls[control]
        else {
            panic!("fixture table");
        };
        let mut paragraph = Paragraph::new_empty();
        paragraph.text = "캡션도 표의 내용".into();
        table.caption = Some(Caption {
            paragraphs: vec![paragraph],
            ..Caption::default()
        });
    }
    assert!(
        s.run(
            "context.table.delete",
            json!({"address":host,"control":control})
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    {
        let Control::Table(table) = &mut s.core.document_mut().sections[cursor.section].paragraphs
            [cursor.paragraph]
            .controls[control]
        else {
            panic!("fixture table");
        };
        table.caption = None;
        let mut child = RectangleShape::default();
        child.common.locked = true;
        table.cells[0].paragraphs[0]
            .controls
            .push(Control::Shape(Box::new(ShapeObject::Group(GroupShape {
                children: vec![ShapeObject::Rectangle(child)],
                ..GroupShape::default()
            }))));
    }
    assert!(
        s.run(
            "context.table.delete",
            json!({"address":host,"control":control,"allow_nonempty":true})
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    assert_eq!(s.cursor, cursor);
    Ok(())
}

#[test]
fn paragraph_split_merge_rebases_body_and_cell_selections_for_followup_typing() -> anyhow::Result<()>
{
    for in_cell in [false, true] {
        let mut s = Session::new()?;
        if in_cell {
            s.run(
                "table.create",
                json!({"rows":1,"columns":1,"treat_as_char":true}),
            )?;
        }
        s.execute(Command::Insert("앞😀뒤끝".into()))?;
        let original = s.cursor;
        let address = DocumentAddress::from_cursor(original);
        s.anchor = Some(1);
        let split = s.run("context.split", json!({"address":address,"offset":2}))?;
        let next: DocumentAddress = serde_json::from_value(split["address"].clone())?;
        assert_eq!(s.cursor, next.legacy_cursor(2).unwrap());
        assert_eq!(
            s.span_anchor,
            Some(hwpcraft_engine::Cursor {
                offset: 1,
                ..original
            })
        );
        assert!(s.anchor.is_none());
        assert_eq!(s.copy()?, "😀\n뒤끝");
        s.run("context.merge", json!({"address":next}))?;
        assert_eq!(s.cursor, original);
        assert_eq!(s.anchor, Some(1));
        assert!(s.span_anchor.is_none());
        s.execute(Command::Insert("대체".into()))?;
        assert_eq!(get(&mut s, &address)?["text"], "앞대체");
    }
    Ok(())
}

#[test]
fn table_cell_caret_follows_its_table_through_body_split_merge() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let made = s.run(
        "context.table.create",
        json!({"address":DocumentAddress::default(),"rows":1,"columns":1}),
    )?;
    let cell: DocumentAddress = serde_json::from_value(made["address"].clone())?;
    s.select(cell.legacy_cursor(0).unwrap(), false)?;
    s.execute(Command::Insert("표안".into()))?;
    let table = made["table"]["address"].clone();
    let split = s.run("context.split", json!({"address":table,"offset":0}))?;
    let current = DocumentAddress::from_cursor(s.cursor);
    assert_eq!(get(&mut s, &current)?["text"], "표안");
    assert_eq!(s.cursor.offset, 2);
    s.execute(Command::Insert("유지".into()))?;
    s.run("context.merge", json!({"address":split["address"]}))?;
    assert_eq!(DocumentAddress::from_cursor(s.cursor), cell);
    assert_eq!(s.cursor.offset, 4);
    assert_eq!(get(&mut s, &cell)?["text"], "표안유지");
    s.execute(Command::Insert("됨".into()))?;
    assert_eq!(get(&mut s, &cell)?["text"], "표안유지됨");
    Ok(())
}

#[test]
fn address_validation_generation_and_ancestor_protection_are_atomic() -> anyhow::Result<()> {
    let mut s = fixture()?;
    let address = target(&mut s, "안😀쪽")?;
    let old = s.generation;
    for params in [
        json!({"address":address,"start":0,"end":99,"text":"x"}),
        json!({"address":address,"start":0,"end":1,"text":"new\nparagraph"}),
        json!({"address":address,"start":0,"end":1,"text":"x","extra":true}),
    ] {
        assert!(s.run("context.replace", params).is_err());
        assert_eq!(get(&mut s, &address)?["text"], "안😀쪽");
        assert_eq!(s.generation, old);
    }
    let mut bad = address.clone();
    bad.path[1].control = 999;
    assert!(get(&mut s, &bad).is_err());
    let first = &address.path[0];
    let Some(Control::Table(table)) = s.core.document_mut().sections[address.section].paragraphs
        [address.paragraph]
        .controls
        .get_mut(first.control)
    else {
        panic!("fixture table");
    };
    table.common.locked = true;
    assert_eq!(get(&mut s, &address)?["protected"], true);
    assert!(
        s.run(
            "context.replace",
            json!({"address":address,"start":0,"end":1,"text":"X"})
        )
        .is_err()
    );
    assert_eq!(s.generation, old);
    let Some(Control::Table(table)) = s.core.document_mut().sections[address.section].paragraphs
        [address.paragraph]
        .controls
        .get_mut(first.control)
    else {
        panic!("fixture table");
    };
    table.common.locked = false;
    table.cells[first.cell].set_cell_protect(true);
    assert!(
        s.run("context.split", json!({"address":address,"offset":1}))
            .is_err()
    );
    let Some(Control::Table(table)) = s.core.document_mut().sections[address.section].paragraphs
        [address.paragraph]
        .controls
        .get_mut(first.control)
    else {
        panic!("fixture table");
    };
    table.cells[first.cell].set_cell_protect(false);
    s.run(
        "context.replace",
        json!({"address":address,"start":0,"end":0,"text":"X","expected_generation":old}),
    )?;
    let generation = s.generation;
    assert!(
        s.run(
            "context.replace",
            json!({"address":address,"start":0,"end":1,"text":"Y","expected_generation":old})
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    assert_eq!(get(&mut s, &address)?["text"], "X안😀쪽");
    Ok(())
}

#[test]
fn nested_hit_test_returns_full_address_and_scalar_properties() -> anyhow::Result<()> {
    let mut s = fixture()?;
    let address = target(&mut s, "안😀쪽")?;
    let value: Value = serde_json::from_str(
        &s.core
            .get_page_text_layout_native(0)
            .map_err(anyhow::Error::msg)?,
    )?;
    let run = value["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| {
            run["text"]
                .as_str()
                .is_some_and(|text| text.contains("안😀쪽"))
        })
        .expect("nested text must be laid out");
    let x = run["x"].as_f64().unwrap() + 0.1;
    let y = run["y"].as_f64().unwrap() + run["h"].as_f64().unwrap() / 2.0;
    let hit = s.run("context.hit_test", json!({"page":0,"x":x,"y":y}))?;
    assert_eq!(
        serde_json::from_value::<DocumentAddress>(hit["address"].clone())?,
        address
    );
    assert_eq!(hit["offset"], 0);
    assert_eq!(hit["character"]["bold"], false);
    assert_eq!(hit["paragraph"]["alignment"], "justify");
    assert!(
        address.legacy_cursor(0).is_none(),
        "never truncate a nested path"
    );
    let legacy = hwpcraft_engine::Cursor::default();
    assert_eq!(
        DocumentAddress::from_cursor(legacy).legacy_cursor(0),
        Some(legacy)
    );
    assert!(s.context_hit_test(0, f64::NAN, 0.0).is_err());
    assert!(s.context_hit_test(u32::MAX, 0.0, 0.0).is_err());
    Ok(())
}

#[test]
fn explicit_text_replacement_refuses_inline_control_loss() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("A".into()))?;
    s.run("insert.auto_text", json!({"kind":"page_number"}))?;
    s.execute(Command::Insert("B".into()))?;
    let address = DocumentAddress::default();
    let original = s.text().to_owned();
    let count = s.context_paragraph(&address)?.controls.len();
    let generation = s.generation;
    assert!(
        s.run(
            "context.replace",
            json!({"address":address,"start":0,"end":3,"text":"replacement"})
        )
        .is_err()
    );
    assert_eq!(s.text(), original);
    assert_eq!(s.generation, generation);
    assert_eq!(s.context_paragraph(&address)?.controls.len(), count);
    s.run(
        "context.replace",
        json!({"address":address,"start":2,"end":3,"text":"C"}),
    )?;
    assert_eq!(s.text(), "A C");
    assert_eq!(s.context_paragraph(&address)?.controls.len(), count);
    Ok(())
}
