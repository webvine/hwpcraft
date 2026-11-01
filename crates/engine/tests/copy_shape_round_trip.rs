//! format.copy_shape.apply, format.character and format.paragraph take
//! format.character.get / format.paragraph.get results as they are, so a shape read
//! from one paragraph can be applied to another (모양 복사, or get → edit → set through MCP).
use hwpcraft_engine::Session;
use serde_json::{Value, json};

fn caret(s: &mut Session, paragraph: usize) -> anyhow::Result<()> {
    s.run(
        "selection.set",
        json!({"anchor": null, "cursor": {"cell": null, "offset": 0, "paragraph": paragraph, "section": 0}}),
    )?;
    Ok(())
}

/// Character and paragraph shape of a whole paragraph.
fn shapes(s: &mut Session, paragraph: usize) -> anyhow::Result<(Value, Value)> {
    caret(s, paragraph)?;
    let par = s.run("format.paragraph.get", json!({}))?;
    s.run("select.paragraph", json!({}))?;
    let ch = s.run("format.character.get", json!({}))?;
    s.run("select.clear", json!({}))?;
    Ok((ch, par))
}

#[test]
fn a_shape_read_from_one_paragraph_applies_to_another() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "제목 문단\n본문 문단"}))?;
    caret(&mut s, 0)?;
    s.run("select.paragraph", json!({}))?;
    s.run(
        "format.character",
        json!({"bold": true, "fontSize": 1600, "textColor": "#1f4e79"}),
    )?;
    s.run(
        "format.paragraph",
        json!({"alignment": "center", "lineSpacing": 180}),
    )?;
    let (ch, par) = shapes(&mut s, 0)?;
    assert_eq!(ch["bold"], true, "{ch}");

    caret(&mut s, 1)?;
    s.run("select.paragraph", json!({}))?;
    s.run(
        "format.copy_shape.apply",
        json!({"character": ch, "paragraph": par}),
    )?;
    let (ch1, par1) = shapes(&mut s, 1)?;
    for key in ["bold", "fontSize", "textColor"] {
        assert_eq!(ch1[key], ch[key], "character {key}");
    }
    for key in ["alignment", "lineSpacing"] {
        assert_eq!(par1[key], par[key], "paragraph {key}");
    }
    Ok(())
}

#[test]
fn format_commands_take_get_results_too() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "첫 문단\n둘째 문단"}))?;
    caret(&mut s, 0)?;
    s.run("select.paragraph", json!({}))?;
    s.run(
        "format.character",
        json!({"italic": true, "fontSize": 1400}),
    )?;
    s.run("format.paragraph", json!({"alignment": "right"}))?;
    let (mut ch, mut par) = shapes(&mut s, 0)?;
    // Edit the read shape and set it back on the other paragraph.
    ch["bold"] = json!(true);
    par["lineSpacing"] = json!(200);
    caret(&mut s, 1)?;
    s.run("select.paragraph", json!({}))?;
    s.run("format.character", ch.clone())?;
    s.run("format.paragraph", par.clone())?;
    let (ch1, par1) = shapes(&mut s, 1)?;
    for key in ["italic", "fontSize", "bold"] {
        assert_eq!(ch1[key], ch[key], "character {key}");
    }
    for key in ["alignment", "lineSpacing"] {
        assert_eq!(par1[key], par[key], "paragraph {key}");
    }
    Ok(())
}

#[test]
fn unknown_keys_are_still_refused() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "글"}))?;
    s.run("select.all", json!({}))?;
    let err = s
        .run(
            "format.copy_shape.apply",
            json!({"character": {"bold": true, "noSuchKey": 1}}),
        )
        .unwrap_err();
    assert!(err.to_string().contains("noSuchKey"), "{err}");
    Ok(())
}
