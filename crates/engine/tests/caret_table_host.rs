//! A body paragraph that holds only a table has no caret position of its own in
//! RHWP. Moving the caret up or down onto or over it used to fail with "커서 위치를
//! 찾을 수 없습니다"; it now steps to the nearest paragraph with a caret.
use hwpcraft_doc::Cursor;
use hwpcraft_engine::{Motion, Session};
use serde_json::json;

/// A Hangul research report with table-only paragraphs between text.
const SAMPLE: &str = "../../../rhwp/samples/정책연구용역사업 중간진도보고서(살아있는 간장 기증자의 의학적 선별기준 연구).hwp";

fn open() -> anyhow::Result<Option<Session>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE);
    if !path.exists() {
        return Ok(None);
    }
    let mut s = Session::new()?;
    s.open(&path)?;
    Ok(Some(s))
}

fn at(paragraph: usize) -> Cursor {
    Cursor {
        section: 0,
        paragraph,
        offset: 0,
        cell: None,
    }
}

/// The first table-only body paragraph RHWP gives no caret, with text paragraphs
/// around it.
fn table_host(s: &Session) -> Option<usize> {
    let paragraphs = &s.core.document().sections[0].paragraphs;
    (1..paragraphs.len().saturating_sub(1)).find(|&i| {
        let p = &paragraphs[i];
        p.text.is_empty() && !p.controls.is_empty() && s.caret_at(at(i)).is_err()
    })
}

#[test]
fn the_caret_moves_down_and_up_over_a_table_only_paragraph() -> anyhow::Result<()> {
    let Some(mut s) = open()? else {
        return Ok(());
    };
    let host = table_host(&s).expect("a table-only paragraph without a caret");

    // Down from the paragraph before it, then on: never an error, and the caret
    // never rests where RHWP has no caret.
    s.select(at(host - 1), false)?;
    for _ in 0..40 {
        s.move_caret(Motion::Down, false)?;
        assert!(s.caret().is_ok(), "{:?}", s.cursor);
        if s.cursor.paragraph > host {
            break;
        }
    }
    assert!(s.cursor.paragraph > host, "{:?}", s.cursor);

    // Up from below it comes back over it.
    for _ in 0..40 {
        s.move_caret(Motion::Up, false)?;
        assert!(s.caret().is_ok(), "{:?}", s.cursor);
        if s.cursor.paragraph < host {
            break;
        }
    }
    assert!(s.cursor.paragraph < host, "{:?}", s.cursor);

    // A caret already in the table-only paragraph (placed there by a command)
    // moves out in both directions.
    s.select(at(host), false)?;
    s.move_caret(Motion::Down, false)?;
    assert!(
        s.cursor.paragraph > host && s.caret().is_ok(),
        "{:?}",
        s.cursor
    );
    s.select(at(host), false)?;
    s.move_caret(Motion::Up, false)?;
    assert!(
        s.cursor.paragraph < host && s.caret().is_ok(),
        "{:?}",
        s.cursor
    );
    Ok(())
}

#[test]
fn mcp_caret_move_passes_a_table_only_paragraph() -> anyhow::Result<()> {
    let Some(mut s) = open()? else {
        return Ok(());
    };
    let host = table_host(&s).expect("a table-only paragraph without a caret");
    s.select(at(host), false)?;
    s.run("caret.move", json!({"motion": "down"}))?;
    assert!(s.cursor.paragraph > host, "{:?}", s.cursor);
    s.select(at(host), false)?;
    s.run("caret.move", json!({"motion": "up"}))?;
    assert!(s.cursor.paragraph < host, "{:?}", s.cursor);
    Ok(())
}
