//! Owner: format area. Body text a new document lays out must stay above the footnote
//! area. RHWP let body lines run into an empty footer band below it (a correction for
//! Hangul-saved line positions; Hangul itself draws footnotes inside the body area),
//! while the footnote area is drawn at the bottom of the body area, so a report's last
//! line on a page was drawn over the footnote rule.
use hwpcraft_engine::Session;
use serde_json::json;

fn end(s: &mut Session) -> anyhow::Result<()> {
    s.run("caret.move", json!({"motion": "document_end"}))?;
    Ok(())
}

fn body(s: &mut Session) -> anyhow::Result<()> {
    s.run("style.apply", json!({"id": 0}))?;
    s.run("format.paragraph", json!({"alignment": "justify"}))?;
    s.run("format.character", json!({"bold": false, "fontSize": 1100}))?;
    Ok(())
}

fn heading(s: &mut Session, t: &str) -> anyhow::Result<()> {
    s.run("text.insert", json!({"text": t}))?;
    s.run("select.text", json!({"text": t}))?;
    s.run("style.apply", json!({"id": 2}))?;
    s.run("format.character", json!({"bold": true, "fontSize": 1500}))?;
    s.run("format.paragraph", json!({"alignment": "left"}))?;
    end(s)?;
    s.run("text.insert", json!({"text": "\n"}))?;
    body(s)
}

/// The README example report (title, footnote, table, chart, equation, memo, header
/// and page number) in a section without a footer; its last lines on page 1 reach the
/// bottom of the body area.
fn report(page_number: bool, memo: bool, extra: usize) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("format.style.set.apply", json!({"name": "보고서"}))?;
    let title = "2026년 지역 도서관 이용 현황 보고서";
    s.run("text.insert", json!({"text": title}))?;
    s.run("select.text", json!({"text": title}))?;
    s.run("format.character", json!({"bold": true, "fontSize": 2200}))?;
    s.run("format.paragraph", json!({"alignment": "center"}))?;
    end(&mut s)?;
    s.run("text.insert", json!({"text": "\n"}))?;
    s.run("format.character", json!({"bold": false, "fontSize": 1100}))?;
    let sub = "HwpCraft MCP로 작성한 예제 문서 · 작성: 자동화 에이전트";
    s.run("text.insert", json!({"text": sub}))?;
    s.run("select.text", json!({"text": sub}))?;
    s.run("format.paragraph", json!({"alignment": "center"}))?;
    end(&mut s)?;
    s.run("text.insert", json!({"text": "\n"}))?;
    body(&mut s)?;
    heading(&mut s, "개요")?;
    s.run("text.insert", json!({"text": "이 보고서는 2026년 분기별 도서관 방문자 수와 대출 권수를 정리하고, 다음 해 운영 방향을 제안합니다. 표와 차트, 수식, 각주, 메모는 모두 MCP 명령으로 넣었습니다."}))?;
    s.run(
        "insert.footnote",
        json!({"text": "자료: 각 도서관 이용 통계(예제 값)."}),
    )?;
    end(&mut s)?;
    s.run("text.insert", json!({"text": "\n"}))?;
    heading(&mut s, "분기별 이용 현황")?;
    s.run(
        "text.insert",
        json!({"text": "분기별 방문자와 대출 권수는 다음과 같습니다.\n"}),
    )?;
    let t = s.run("insert.table", json!({"rows": 5, "columns": 4}))?;
    let table = t["table"].clone();
    s.run(
        "table.cells.set",
        json!({"table": table, "row": 0, "col": 0, "cells": [
        ["분기", "방문자(명)", "대출(권)", "전년 대비"],
        ["1분기", "12,400", "8,950", "+4.2%"],
        ["2분기", "13,870", "9,610", "+6.8%"],
        ["3분기", "15,320", "10,240", "+9.1%"],
        ["4분기", "14,050", "9,880", "+5.5%"]]}),
    )?;
    s.run(
        "table.style",
        json!({"table": table, "preset": "blue_header", "header_row": true}),
    )?;
    s.run(
        "table.properties",
        json!({"table": table, "props": {"hasCaption": true}}),
    )?;
    s.run(
        "table.caption_text",
        json!({"table": table, "text": "분기별 이용 현황"}),
    )?;
    end(&mut s)?;
    s.run("text.insert", json!({"text": "\n"}))?;
    s.run("chart.create", json!({"chart": {"kind": "column", "title": "분기별 방문자와 대출",
        "categories": ["1분기", "2분기", "3분기", "4분기"],
        "series": [{"name": "방문자(백 명)", "values": [124, 138.7, 153.2, 140.5], "color": "#4472C4"},
                   {"name": "대출(백 권)", "values": [89.5, 96.1, 102.4, 98.8], "color": "#ED7D31"}],
        "width_mm": 140, "height_mm": 75, "legend": true}}))?;
    end(&mut s)?;
    s.run("text.insert", json!({"text": "\n"}))?;
    body(&mut s)?;
    for k in 0..extra {
        s.run(
            "text.insert",
            json!({"text": format!("추가 본문 {k}
")}),
        )?;
    }
    heading(&mut s, "증가율 계산")?;
    s.run(
        "text.insert",
        json!({"text": "전년 대비 증가율은 다음 식으로 계산했습니다.\n"}),
    )?;
    s.run(
        "insert.equation",
        json!({"script": "r = {x_{2026} - x_{2025}} over {x_{2025}} times 100", "size_pt": 12.0}),
    )?;
    end(&mut s)?;
    s.run("text.insert", json!({"text": "\n"}))?;
    body(&mut s)?;
    s.run("text.insert", json!({"text": "결론적으로 3분기 이용이 가장 많았으며, 여름 방학 프로그램의 효과로 보입니다."}))?;
    if memo {
        s.run(
            "select.text",
            json!({"text": "여름 방학 프로그램의 효과로 보입니다"}),
        )?;
        s.run(
            "memo.create",
            json!({"text": "자료를 덧붙일 것", "author": "검토자"}),
        )?;
        end(&mut s)?;
    }
    s.run(
        "page.header_footer.text",
        json!({"header": true, "apply_to": "both", "text": "머리말"}),
    )?;
    if page_number {
        s.run("page.number", json!({"position": "bottom_center"}))?;
    }
    Ok(s)
}

#[test]
fn body_lines_stay_above_the_footnote_area() -> anyhow::Result<()> {
    let s = report(true, true, 2)?;
    let layout = s.core.get_page_text_layout_native(0).unwrap();
    let v: serde_json::Value = serde_json::from_str(&layout)?;
    let runs: Vec<(String, f64, f64)> = v["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            (
                r["text"].as_str().unwrap_or("").to_owned(),
                r["y"].as_f64().unwrap_or(0.0),
                r["h"].as_f64().unwrap_or(0.0),
            )
        })
        .collect();
    let note_top = runs
        .iter()
        .find(|(t, _, _)| t.contains("자료:"))
        .map(|(_, y, _)| *y)
        .expect("the footnote on page 1");
    // The footnote rule sits above the note text; body text must end above it.
    // Body runs: every run of a word or more except the footnote's.
    let body = |t: &str| !t.contains("자료") && !t.starts_with(' ') && t.trim().chars().count() > 3;
    for (text, y, h) in runs.iter().filter(|(t, _, _)| body(t)) {
        assert!(
            y + h <= note_top - 6.0,
            "{text:?} ({y}+{h}) runs into the footnote area (note text at {note_top})"
        );
    }
    Ok(())
}
