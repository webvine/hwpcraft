//! Public engine-path checks for the body snippet fragment + separate store.
//! The settings override is private to this test executable's temporary folder.
use hwpcraft_engine::{Command, Cursor, Session, set_config_dir};
use serde_json::json;
use std::sync::{Mutex, OnceLock};

fn settings() -> std::sync::MutexGuard<'static, tempfile::TempDir> {
    static SETTINGS: OnceLock<Mutex<tempfile::TempDir>> = OnceLock::new();
    SETTINGS
        .get_or_init(|| {
            let directory = tempfile::tempdir().unwrap();
            assert!(set_config_dir(directory.path().to_owned()));
            Mutex::new(directory)
        })
        .lock()
        .unwrap()
}
fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}

#[test]
fn formatted_body_snippet_public_commands_preserve_plain_store_and_report_output()
-> anyhow::Result<()> {
    let settings = settings();
    let mut source = Session::new()?;
    source.run(
        "tools.autotext.add",
        json!({"name":"기존 글","abbreviation":"plain","text":"글 상용구 보존"}),
    )?;
    let plain = std::fs::read(settings.path().join("autotext.json"))?;
    source.execute(Command::Insert("보고서 제목😀\n재사용할 본문".into()))?;
    source.select(at(0, 0), false)?;
    source.select(at(0, 8), true)?;
    source.execute(Command::Character(
        json!({"bold":true,"fontSize":1800,"textColor":"#123456"}),
    ))?;
    let source_range = json!({"section":0,"start":0,"end":2});
    let current = source.run("tools.autotext.body.source", json!({}))?;
    let made=source.run("tools.autotext.body.add",json!({"name":"보고서 본문","source":source_range,"expected_generation":current["generation"]}))?;
    let id = made["entry"]["id"].clone();
    assert_eq!(made["entry"]["paragraphs"], 2);
    assert_eq!(std::fs::read(settings.path().join("autotext.json"))?, plain);
    assert!(settings.path().join("autotext-body-v1.json").is_file());
    let mut report = Session::new()?;
    report.execute(Command::Insert("기존 보고서".into()))?;
    let head = report.run("tools.autotext.body.source", json!({}))?;
    let shown = report.run(
        "tools.autotext.body.preview",
        json!({"id":id,"expected_generation":head["generation"]}),
    )?;
    assert!(shown["svg"].as_str().unwrap().contains("<svg"));
    assert_eq!(report.text(), "기존 보고서");
    report.run(
        "tools.autotext.body.insert",
        json!({"id":id,"expected_generation":head["generation"]}),
    )?;
    report.execute(Command::Undo)?;
    assert_eq!(report.text(), "기존 보고서");
    report.execute(Command::Redo)?;
    let files = tempfile::tempdir()?;
    for extension in ["hwp", "hwpx"] {
        let path = files.path().join(format!("report.{extension}"));
        report.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(
            reopened.core.document().sections[0]
                .paragraphs
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>(),
            vec!["기존 보고서", "보고서 제목😀", "재사용할 본문"]
        );
        reopened.select(at(1, 0), false)?;
        reopened.select(at(1, 8), true)?;
        let format = reopened.run("format.character.get", json!({}))?;
        assert_eq!(format["bold"], true);
        assert_eq!(format["fontSize"], 1800);
        let svg = reopened.render_page_svg(0)?;
        assert!(svg.contains("<svg"));
        let pdf = reopened
            .core
            .render_document_pdf_native_with_options(&hwpcraft_formats::pdf_options())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.len() > 1_000);
    }
    let snapshot = report.run("tools.autotext.body.list", json!({}))?;
    report.run(
        "tools.autotext.body.delete",
        json!({"id":id,"expected_store_revision":snapshot["revision"]}),
    )?;
    assert_eq!(std::fs::read(settings.path().join("autotext.json"))?, plain);
    Ok(())
}

#[test]
fn source_query_and_failed_capture_leave_settings_and_document_unchanged() -> anyhow::Result<()> {
    let settings = settings();
    let mut session = Session::new()?;
    session.execute(Command::Insert("부분 선택 시험".into()))?;
    let state = session.run("tools.autotext.body.source", json!({}))?;
    session.select(at(0, 1), false)?;
    session.select(at(0, 3), true)?;
    assert!(
        session
            .run("tools.autotext.body.source", json!({}))
            .is_err()
    );
    let prior = std::fs::read(settings.path().join("autotext-body-v1.json")).ok();
    assert!(
        session
            .run(
                "tools.autotext.body.add",
                json!({"name":"잘못된 부분","expected_generation":state["generation"]})
            )
            .is_err()
    );
    assert_eq!(
        std::fs::read(settings.path().join("autotext-body-v1.json")).ok(),
        prior
    );
    assert_eq!(session.text(), "부분 선택 시험");
    Ok(())
}
