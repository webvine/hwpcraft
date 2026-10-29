use hwpcraft_engine::{Command, Session};
use serde_json::json;

const LINE: &str =
    "이번 분기에는 계획한 과제를 차질 없이 진행했으며 세부 일정과 예산 집행률을 함께 점검했습니다.";

fn pages(s: &Session) -> anyhow::Result<Vec<String>> {
    (0..s.page_count()?).map(|p| s.render_page_svg(p)).collect()
}

/// A multi-paragraph insert lays the document out once at the end; the pages must
/// come out exactly as when every paragraph is typed and split on its own.
#[test]
fn multiline_insert_matches_line_by_line_typing_across_pages() -> anyhow::Result<()> {
    let text: String = (1..=60)
        .map(|k| format!("{k}. {LINE}"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut whole = Session::new()?;
    whole.execute(Command::Insert("머리 문단".into()))?;
    whole.run("text.insert", json!({"text": format!("\n{text}\n끝")}))?;

    let mut typed = Session::new()?;
    typed.execute(Command::Insert("머리 문단".into()))?;
    for (i, line) in format!("\n{text}\n끝").split('\n').enumerate() {
        if i > 0 {
            typed.execute(Command::Split)?;
        }
        if !line.is_empty() {
            typed.execute(Command::Insert(line.into()))?;
        }
    }
    assert!(
        whole.page_count()? > 2,
        "the sample must span several pages"
    );
    assert_eq!(whole.page_count()?, typed.page_count()?);
    assert_eq!(
        whole.run("document.text", json!({}))?,
        typed.run("document.text", json!({}))?
    );
    assert_eq!(pages(&whole)?, pages(&typed)?);
    assert_eq!(whole.cursor, typed.cursor);
    Ok(())
}

/// The deferred insert is still one undo step, and a failed insert leaves layout live.
#[test]
fn multiline_insert_is_one_undo_step_and_leaves_layout_live() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("처음".into()))?;
    let before = pages(&s)?;
    let text = (0..40).map(|_| LINE).collect::<Vec<_>>().join("\n");
    s.run("text.insert", json!({"text": format!("\n{text}")}))?;
    assert!(s.page_count()? > 1);
    assert!(!s.layout_deferred());
    s.execute(Command::Undo)?;
    assert_eq!(pages(&s)?, before);
    assert!(
        s.run("text.insert", json!({"text": "가".repeat(1_000_001)}))
            .is_err()
    );
    assert!(!s.layout_deferred());
    Ok(())
}
