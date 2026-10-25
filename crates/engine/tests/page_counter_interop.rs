//! A PAGE field's cached number is distinct from its actual printing page.
use hwpcraft_doc::model::control::{AutoNumberType, Control};
use hwpcraft_engine::{Command, Session};
use serde_json::json;

fn numbers(s: &Session) -> Vec<(AutoNumberType, u16, u8)> {
    s.core
        .document()
        .sections
        .iter()
        .flat_map(|section| &section.paragraphs)
        .flat_map(|p| &p.controls)
        .filter_map(|control| match control {
            Control::AutoNumber(n) => Some((n.number_type, n.number, n.format)),
            _ => None,
        })
        .collect()
}

fn assert_values(s: &Session, start: u16, count: usize) -> anyhow::Result<()> {
    assert_eq!(s.page_count()?, 1);
    let svg = s.render_page_svg(0)?;
    assert_eq!(
        svg.matches(&format!(
            "data-hwpcraft-page-field=\"page\" data-value=\"{start}\""
        ))
        .count(),
        count
    );
    assert_eq!(
        svg.matches("data-hwpcraft-page-field=\"total\" data-value=\"1\"")
            .count(),
        1
    );
    assert!(s.render_pdf(&[0])?.starts_with(b"%PDF-"));
    Ok(())
}

#[test]
fn high_page_restart_reopens_with_cached_values_and_dynamic_outputs_preserved() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    for (start, count) in [(65535_u16, 2_usize), (65530, 7)] {
        let mut source = Session::new()?;
        source.execute(Command::Insert("높은 쪽 번호 ".into()))?;
        for _ in 0..count {
            source.run("insert.auto_text", json!({"kind":"page_number"}))?;
            source.execute(Command::Insert(" | ".into()))?;
        }
        source.run("insert.auto_text", json!({"kind":"total_pages"}))?;
        source.run("page.new_number", json!({"start":start}))?;
        let expected = numbers(&source);
        assert_values(&source, start, count)?;
        for (label, extension, password) in [
            ("plain", "hwp", None),
            ("encrypted", "hwp", Some("own-fixture-only")),
            ("xml", "hwpx", None),
        ] {
            source.set_password(password.map(str::to_owned))?;
            let path = dir.path().join(format!("{label}-{start}.{extension}"));
            source.save(&path)?;
            let original_bytes = std::fs::read(&path)?;
            let mut reopened = Session::new()?;
            if let Some(password) = password {
                reopened.open_with_password(&path, password)?;
            } else {
                reopened.open(&path)?;
            }
            assert_eq!(numbers(&reopened), expected, "{label}-{start}");
            assert_values(&reopened, start, count)?;
            assert_eq!(std::fs::read(&path)?, original_bytes);
            // Regeneration as HWPX must not retain the native reader's former
            // sequential/wrapped PAGE cache values.
            reopened.set_password(None)?;
            let next = dir.path().join(format!("{label}-{start}-again.hwpx"));
            reopened.save(&next)?;
            let mut again = Session::new()?;
            again.open(&next)?;
            assert_eq!(numbers(&again), expected, "HWPX after {label}-{start}");
            assert_values(&again, start, count)?;
            if std::env::var_os("HWPCRAFT_WRITE_PAGE_COUNTER_FIXTURES").is_some() {
                let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../output/implementation/0.17/page-counter-read");
                std::fs::create_dir_all(&output)?;
                std::fs::copy(&path, output.join(format!("{label}-{start}.{extension}")))?;
                std::fs::write(
                    output.join(format!("{label}-{start}.pdf")),
                    again.render_pdf(&[0])?,
                )?;
            }
        }
    }
    Ok(())
}
