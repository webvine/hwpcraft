//! Memo replies (한글의 댓글) and memos in table cells, as Hangul 2024 stores them:
//! a reply is another memo on the same text nested inside the thread's first memo.
use hwpcraft_engine::{Cell, Command, Cursor, Session};
use serde_json::{Value, json};

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}

fn memos(s: &mut Session) -> anyhow::Result<Vec<Value>> {
    Ok(s.run("memo.list", json!({}))?
        .as_array()
        .cloned()
        .unwrap_or_default())
}

/// (text, author, thread position, text of the memo it replies to).
type Thread = (String, String, u64, Option<String>);

/// Every memo as a [`Thread`] row.
fn threads(s: &mut Session) -> anyhow::Result<Vec<Thread>> {
    let list = memos(s)?;
    Ok(list
        .iter()
        .map(|m| {
            let parent = list
                .iter()
                .find(|p| p["id"] == m["replyTo"])
                .map(|p| p["text"].as_str().unwrap_or("").to_owned());
            (
                m["text"].as_str().unwrap_or("").to_owned(),
                m["author"].as_str().unwrap_or("").to_owned(),
                m["thread"].as_u64().unwrap_or(99),
                parent,
            )
        })
        .collect())
}

#[test]
fn replies_nest_in_the_thread_and_survive_both_formats() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("검토할 문장입니다".into()))?;
    s.select(at(0, 4), false)?;
    s.select(at(0, 6), true)?;
    let first = s.run(
        "memo.create",
        json!({"text":"근거를 보완해 주세요.","author":"검토자"}),
    )?;
    let reply = s.run(
        "memo.reply",
        json!({"id":first["id"],"text":"보완했습니다.","author":"작성자"}),
    )?;
    assert_eq!(reply["anchor"], "문장", "{reply}");
    assert_eq!(reply["replyTo"], first["id"]);
    assert_eq!(reply["thread"], 1);
    // A reply to a reply joins the same thread, innermost.
    s.run(
        "memo.reply",
        json!({"id":reply["id"],"text":"확인했습니다.","author":"검토자"}),
    )?;
    let expected = vec![
        (
            "근거를 보완해 주세요.".to_owned(),
            "검토자".to_owned(),
            0,
            None,
        ),
        (
            "보완했습니다.".to_owned(),
            "작성자".to_owned(),
            1,
            Some("근거를 보완해 주세요.".to_owned()),
        ),
        (
            "확인했습니다.".to_owned(),
            "검토자".to_owned(),
            2,
            Some("근거를 보완해 주세요.".to_owned()),
        ),
    ];
    assert_eq!(threads(&mut s)?, expected);
    assert_eq!(s.text(), "검토할 문장입니다");
    // Hangul's command: MEMO/<shape>/<number>/<time low>/<time high>/<author>/\;;
    let field = s.core.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .find_map(|c| match c {
            hwpcraft_doc::model::control::Control::Field(f)
                if Some(f.field_id) == reply["id"].as_u64().map(|v| v as u32) =>
            {
                Some(f.command.clone())
            }
            _ => None,
        })
        .unwrap_or_default();
    let tokens: Vec<&str> = field.split('/').collect();
    assert_eq!(tokens[0], "MEMO", "{field}");
    assert_eq!(tokens[2], "2", "{field}");
    assert!(
        tokens[3].parse::<u32>().is_ok() && tokens[4].parse::<u32>().is_ok(),
        "{field}"
    );
    assert_eq!(tokens[5], "작성자", "{field}");
    let dir = tempfile::tempdir()?;
    for name in ["threads.hwp", "threads.hwpx"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(threads(&mut reopened)?, expected, "{name}");
        assert_eq!(reopened.text(), "검토할 문장입니다", "{name}");
    }
    // Deleting the thread's first memo takes the replies along, in one undo step.
    let removed = s.run("memo.delete", json!({"id":first["id"]}))?;
    assert_eq!(
        removed["removedIds"].as_array().map(Vec::len),
        Some(3),
        "{removed}"
    );
    assert!(memos(&mut s)?.is_empty());
    assert_eq!(s.text(), "검토할 문장입니다");
    s.run("edit.undo", json!({}))?;
    assert_eq!(threads(&mut s)?, expected);
    // Deleting a reply leaves the rest of the thread.
    let list = memos(&mut s)?;
    s.run("memo.delete", json!({"id":list[1]["id"]}))?;
    let left = threads(&mut s)?;
    assert_eq!(left.len(), 2, "{left:?}");
    assert_eq!(left[1].0, "확인했습니다.");
    assert_eq!(left[1].3.as_deref(), Some("근거를 보완해 주세요."));
    Ok(())
}

#[test]
fn a_caret_memo_takes_no_reply() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("문장".into()))?;
    s.select(at(0, 1), false)?;
    let memo = s.run("memo.create", json!({"text":"자리 메모"}))?;
    let error = s
        .run("memo.reply", json!({"id":memo["id"],"text":"댓글"}))
        .unwrap_err();
    assert!(
        error.to_string().contains("댓글을 달 수 없습니다"),
        "{error}"
    );
    assert_eq!(memos(&mut s)?.len(), 1);
    Ok(())
}

#[test]
fn memos_in_table_cells_are_created_edited_and_removed() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("표 앞\n".into()))?;
    let made = s.run("insert.table", json!({"rows":1,"columns":2}))?;
    let table = s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(p, para)| {
            para.controls
                .iter()
                .position(|c| matches!(c, hwpcraft_doc::model::control::Control::Table(_)))
                .map(|c| (p, c))
        })
        .ok_or_else(|| anyhow::anyhow!("table {made}"))?;
    let cell = |offset| Cursor {
        section: 0,
        paragraph: table.0,
        offset,
        cell: Some(Cell {
            control: table.1,
            index: 1,
            paragraph: 0,
        }),
    };
    s.select(cell(0), false)?;
    s.execute(Command::Insert("셀 안 글자".into()))?;
    s.select(cell(6), false)?;
    s.select(cell(4), true)?;
    let memo = s.run("memo.create", json!({"text":"셀 메모","author":"검토자"}))?;
    assert_eq!(memo["anchor"], "글자", "{memo}");
    assert_eq!(memo["at"]["cell"]["index"], 1, "{memo}");
    assert_eq!(memo["editable"], true);
    s.run(
        "memo.update",
        json!({"id":memo["id"],"text":"고친 셀 메모"}),
    )?;
    let dir = tempfile::tempdir()?;
    for name in ["cell.hwp", "cell.hwpx"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        let list = memos(&mut reopened)?;
        assert_eq!(list.len(), 1, "{name}: {list:?}");
        assert_eq!(list[0]["text"], "고친 셀 메모", "{name}");
        assert_eq!(list[0]["anchor"], "글자", "{name}");
        assert_eq!(list[0]["at"]["cell"]["index"], 1, "{name}");
        // Editable after reopening too.
        let id = list[0]["id"].clone();
        reopened.run("memo.delete", json!({"id":id}))?;
        assert!(memos(&mut reopened)?.is_empty(), "{name}");
    }
    s.run("memo.delete", json!({"id":memo["id"]}))?;
    assert!(memos(&mut s)?.is_empty());
    s.run("edit.undo", json!({}))?;
    assert_eq!(memos(&mut s)?.len(), 1);
    Ok(())
}
