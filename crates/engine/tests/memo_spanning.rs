//! Owner: format area. A memo over a selection across paragraphs, stored as Hangul
//! stores one: the memo field begins in the first paragraph and its end marker sits in
//! the last (an end without a begin in its own paragraph). Listing, saving in both
//! formats, deleting and undo keep the text and its stream positions.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Cursor, Session};
use serde_json::{Value, json};

fn texts(s: &Session) -> Vec<String> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .collect()
}

fn offsets(s: &Session) -> Vec<Vec<u32>> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| p.char_offsets.clone())
        .collect()
}

/// (paragraph of each memo begin, (paragraph, offset, begin ctrl id) of each end that
/// closes a field begun in an earlier paragraph).
fn markers(s: &Session) -> (Vec<usize>, Vec<(usize, usize, u32)>) {
    let paragraphs = &s.core.document().sections[0].paragraphs;
    let begins = paragraphs
        .iter()
        .enumerate()
        .flat_map(|(p, para)| {
            para.controls
                .iter()
                .filter(|c| matches!(c, Control::Field(_)))
                .map(move |_| p)
        })
        .collect();
    let ends = paragraphs
        .iter()
        .enumerate()
        .flat_map(|(p, para)| {
            para.orphan_field_ends
                .iter()
                .map(move |o| (p, o.char_idx, o.begin_ctrl_id))
        })
        .collect();
    (begins, ends)
}

fn document() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run(
        "text.insert",
        json!({"text": "첫째 문단 글\n가운데 문단\n끝 문단 글자"}),
    )?;
    Ok(s)
}

fn select(s: &mut Session) {
    s.span_anchor = Some(Cursor {
        paragraph: 0,
        offset: 3,
        ..Default::default()
    });
    s.cursor = Cursor {
        paragraph: 2,
        offset: 2,
        ..Default::default()
    };
}

fn only_memo(s: &Session) -> Value {
    let list = s.memo_details();
    assert_eq!(list.as_array().map(Vec::len), Some(1), "{list}");
    list[0].clone()
}

#[test]
fn a_memo_across_three_paragraphs() -> anyhow::Result<()> {
    let mut s = document()?;
    let (before_texts, before_offsets) = (texts(&s), offsets(&s));
    select(&mut s);
    let made = s.run(
        "memo.create",
        json!({"text": "세 문단 검토", "author": "검토자"}),
    )?;
    assert_eq!(texts(&s), before_texts, "the text stays");
    let memo = only_memo(&s);
    assert_eq!(memo["id"], made["id"], "{made}");
    assert_eq!(memo["anchor"], "문단 글\n가운데 문단\n끝 ", "{memo}");
    assert_eq!(memo["spanStart"], 3, "{memo}");
    assert_eq!(
        memo["endAt"],
        json!({"section": 0, "paragraph": 2, "offset": 2})
    );
    assert_eq!(memo["text"], "세 문단 검토");
    let (begins, ends) = markers(&s);
    assert_eq!(begins, vec![0]);
    assert_eq!(ends.len(), 1);
    assert_eq!((ends[0].0, ends[0].1), (2, 2));
    assert_ne!(ends[0].2, 0, "the end knows its field kind for HWP");
    // The end slot is 8 stream units in front of character 2 of the last paragraph.
    let last = &offsets(&s)[2];
    assert_eq!(
        last[2] - last[1],
        before_offsets[2][2] - before_offsets[2][1] + 8
    );

    let dir = tempfile::tempdir()?;
    for ext in ["hwpx", "hwp"] {
        let path = dir.path().join(format!("memo.{ext}"));
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(texts(&back), before_texts, "{ext}");
        let reopened = only_memo(&back);
        assert_eq!(reopened["anchor"], memo["anchor"], "{ext}: {reopened}");
        assert_eq!(reopened["endAt"], memo["endAt"], "{ext}");
        assert_eq!(reopened["text"], "세 문단 검토", "{ext}");
        assert_eq!(markers(&back).0, vec![0], "{ext}");
        // Deleted after reopening: both markers go, the text and positions stay.
        back.run("memo.delete", json!({"id": reopened["id"]}))?;
        assert_eq!(texts(&back), before_texts, "{ext}");
        assert_eq!(markers(&back), (vec![], vec![]), "{ext}");
        assert_eq!(offsets(&back), before_offsets, "{ext}");
    }

    s.run("memo.delete", json!({"id": made["id"]}))?;
    assert_eq!(markers(&s), (vec![], vec![]));
    assert_eq!(offsets(&s), before_offsets);
    s.run("edit.undo", json!({}))?;
    assert_eq!(only_memo(&s)["endAt"], memo["endAt"]);
    s.run("edit.undo", json!({}))?;
    assert!(s.memo_details().as_array().is_some_and(Vec::is_empty));
    assert_eq!(offsets(&s), before_offsets);
    Ok(())
}

#[test]
fn replies_and_cells_are_refused_for_a_spanning_memo() -> anyhow::Result<()> {
    let mut s = document()?;
    select(&mut s);
    let made = s.run("memo.create", json!({"text": "검토"}))?;
    let generation = s.generation;
    assert!(
        s.run("memo.reply", json!({"id": made["id"], "text": "답"}))
            .is_err()
    );
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn typing_in_the_last_paragraph_moves_the_end() -> anyhow::Result<()> {
    let mut s = document()?;
    select(&mut s);
    s.run("memo.create", json!({"text": "검토"}))?;
    s.cursor = Cursor {
        paragraph: 2,
        offset: 0,
        ..Default::default()
    };
    s.run("text.insert", json!({"text": "새"}))?;
    let memo = only_memo(&s);
    assert_eq!(memo["endAt"]["offset"], 3, "{memo}");
    assert_eq!(memo["anchor"], "문단 글\n가운데 문단\n새끝 ", "{memo}");
    Ok(())
}
