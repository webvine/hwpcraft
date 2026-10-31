//! Owner: shell/page area. 배포용 문서 opens read-only: no edit, save, copy, print or text.
use hwpcraft_engine::{Command, DISTRIBUTION_READ_ONLY, Session};
use serde_json::json;

fn sample() -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples/20250130-hongbo-no.hwp");
    path.is_file().then_some(path)
}

#[test]
fn a_distribution_document_can_be_read_but_not_edited_saved_copied_or_printed() -> anyhow::Result<()>
{
    let Some(path) = sample() else {
        eprintln!("RHWP 배포용 샘플이 없어 건너뜁니다.");
        return Ok(());
    };
    let mut s = Session::new()?;
    s.open(&path)?;
    assert!(s.is_distribution());
    // Looking and moving around work.
    assert!(s.page_count()? > 0);
    assert!(s.render_page_svg(0)?.contains("<svg"));
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.run("select.all", json!({}))?;
    let stats = s.run("document.statistics", json!({}))?;
    assert!(stats.get("text").is_none());
    let refused = |r: anyhow::Result<serde_json::Value>| {
        let e = r.expect_err("refused");
        assert!(e.to_string().contains(DISTRIBUTION_READ_ONLY), "{e:#}");
    };
    // Editing, text extraction, export and printing are refused.
    for (id, params) in [
        ("text.insert", json!({"text": "x"})),
        ("document.text", json!({})),
        ("select.text", json!({"text": "a"})),
        ("document.inspect", json!({})),
        ("file.print_pdf", json!({"path": "x.pdf", "pages": "all"})),
        (
            "file.save_pages",
            json!({"path": "x.txt", "from": 1, "to": 1}),
        ),
        ("format.character", json!({"bold": true})),
    ] {
        refused(
            s.run(id, params)
                .map_err(|e| anyhow::anyhow!("{id}: {e:#}")),
        );
    }
    let insert = s.execute(Command::Insert("x".into()));
    assert!(insert.is_err_and(|e| e.to_string().contains(DISTRIBUTION_READ_ONLY)));
    assert!(s.copy().is_err() && s.copy_html().is_err());
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx", "pdf"] {
        let target = temp.path().join(format!("out.{ext}"));
        assert!(s.save(&target).is_err(), "{ext}");
        assert!(!target.exists(), "{ext} not written");
    }
    assert!(s.render_pdf(&[0]).is_err());
    assert!(!s.dirty());
    Ok(())
}
