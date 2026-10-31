//! Owner: format area. DOCX export (C06) with the list of what DOCX could not carry.
//!
//! `Session::save` also writes `.docx` (through `hwpcraft_formats::save`); this entry
//! point additionally reports the omissions so MCP, the CLI and the desktop can show
//! them instead of losing content silently.
use super::*;

impl Session {
    /// Write the document as DOCX and return what it could not carry ("what · count").
    /// The same refusals as an HWP/HWPX save apply: 배포용 documents are never written,
    /// and change-tracked documents only when `accept_track_changes` merges the changes.
    pub fn export_docx(&mut self, path: &Path, accept_track_changes: bool) -> Result<Vec<String>> {
        self.guard_distribution()?;
        if !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
        {
            bail!("DOCX 저장 파일 이름은 .docx로 끝나야 합니다.");
        }
        if accept_track_changes {
            self.drop_track_changes();
        }
        self.refuse_tracked_save(path)?;
        hwpcraft_formats::file_access::check_write(&path.to_string_lossy())
            .map_err(anyhow::Error::msg)?;
        let export = hwpcraft_formats::docx_export::export(&self.core)?;
        hwpcraft_formats::atomic_write(path, &export.bytes)?;
        Ok(export.omitted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn part(path: &Path, name: &str) -> Result<String> {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?)?;
        let mut text = String::new();
        zip.by_name(name)?.read_to_string(&mut text)?;
        Ok(text)
    }

    #[test]
    fn writes_text_formatting_tables_and_notes() -> Result<()> {
        let mut s = Session::new()?;
        s.run(
            "text.insert",
            json!({"text": "보고서 제목\n본문 <첫> 문단 & 둘째 줄"}),
        )?;
        s.run("select.all", json!({}))?;
        s.run("format.character", json!({"bold": true}))?;
        s.clear_selection();
        s.run("caret.move", json!({"motion": "document_end"}))?;
        s.run("text.insert", json!({"text": "\n"}))?;
        s.run("insert.table", json!({"rows": 2, "columns": 3}))?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("report.docx");
        let omitted = s.export_docx(&path, false)?;
        assert!(omitted.is_empty(), "{omitted:?}");
        let document = part(&path, "word/document.xml")?;
        assert!(document.contains("본문 &lt;첫&gt; 문단 &amp; 둘째 줄"));
        assert!(document.contains("<w:b/>"));
        assert_eq!(document.matches("<w:tr>").count(), 2);
        assert_eq!(document.matches("<w:gridCol ").count(), 3);
        assert!(document.contains("<w:pgSz "));
        for name in [
            "[Content_Types].xml",
            "word/styles.xml",
            "word/_rels/document.xml.rels",
        ] {
            part(&path, name)?;
        }
        // Web links become DOCX hyperlinks with an external relationship.
        s.run("caret.move", json!({"motion": "document_end"}))?;
        s.run("text.insert", json!({"text": "\n"}))?;
        s.run(
            "insert.hyperlink",
            json!({"uri": "https://example.com/report", "text": "누리집"}),
        )?;
        let linked = dir.path().join("linked.docx");
        s.export_docx(&linked, false)?;
        assert!(part(&linked, "word/document.xml")?.contains("<w:hyperlink r:id="));
        let rels = part(&linked, "word/_rels/document.xml.rels")?;
        assert!(
            rels.contains("https://example.com/report") && rels.contains("TargetMode=\"External\"")
        );
        // `Session::save` writes the same package.
        s.save(&dir.path().join("plain.docx"))?;
        Ok(())
    }

    #[test]
    fn writes_equations_notes_page_numbers_and_lists_what_is_left_out() -> Result<()> {
        let mut s = Session::new()?;
        s.run("text.insert", json!({"text": "수식과 각주가 있는 문단"}))?;
        s.run(
            "equation.insert",
            json!({"script": "a^2+b^2=c^2", "size_pt": 10.0}),
        )?;
        s.run(
            "note.insert",
            json!({"endnote": false, "paragraphs": ["각주 내용"]}),
        )?;
        s.run(
            "note.insert",
            json!({"endnote": true, "paragraphs": ["미주 내용"]}),
        )?;
        s.run(
            "page.number",
            json!({"position": "bottom_center", "format": 0, "dash": true, "start": 1}),
        )?;
        // A text box behind the text: cut out of the page it would picture the body
        // text twice, so it is reported instead.
        let made = s.run(
            "shape.insert",
            json!({"kind": "textbox", "width_mm": 20.0, "height_mm": 10.0, "paragraphs": ["뒤"]}),
        )?;
        s.run(
            "object.properties",
            json!({"address": made["address"], "props": {"treatAsChar": false, "textWrap": "BehindText"}}),
        )?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("rich.docx");
        let omitted = s.export_docx(&path, false)?;
        assert!(
            omitted.iter().any(|o| o.starts_with("글 뒤·글 앞 도형")),
            "{omitted:?}"
        );
        let document = part(&path, "word/document.xml")?;
        assert!(document.contains("<w:footnoteReference "));
        assert!(document.contains("<w:endnoteReference "));
        assert!(document.contains("name=\"수식\""));
        assert!(part(&path, "word/footnotes.xml")?.contains("각주 내용"));
        assert!(part(&path, "word/endnotes.xml")?.contains("미주 내용"));
        let footer = (1..10)
            .find_map(|n| part(&path, &format!("word/footer{n}.xml")).ok())
            .ok_or_else(|| anyhow!("no footer part"))?;
        assert!(footer.contains("PAGE"), "{footer}");
        // The equation picture is a relationship of the document part.
        assert!(part(&path, "word/_rels/document.xml.rels")?.contains("media/equation1.png"));
        Ok(())
    }

    #[test]
    fn change_tracked_documents_need_acceptance() -> Result<()> {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../rhwp/samples/issue5169_viewtext_changetracking.hwp");
        if !sample.exists() {
            return Ok(());
        }
        let mut s = Session::new()?;
        s.open(&sample)?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("tracked.docx");
        assert!(s.export_docx(&path, false).is_err());
        assert!(!path.exists());
        s.export_docx(&path, true)?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn refuses_like_hwp_and_reports_wrong_names() -> Result<()> {
        let mut s = Session::new()?;
        s.run("text.insert", json!({"text": "가"}))?;
        let dir = tempfile::tempdir()?;
        assert!(s.export_docx(&dir.path().join("a.hwp"), false).is_err());
        // Captions are paragraphs of their own, numbers included.
        s.run(
            "text.insert",
            json!({"text": "
"}),
        )?;
        s.run("insert.table", json!({"rows": 1, "columns": 2}))?;
        let table = s.core.document().sections[0]
            .paragraphs
            .iter()
            .enumerate()
            .find_map(|(p, para)| {
                para.controls
                    .iter()
                    .position(|c| matches!(c, Control::Table(_)))
                    .map(|c| (p, c))
            })
            .ok_or_else(|| anyhow!("table"))?;
        s.cursor = Cursor {
            section: 0,
            paragraph: table.0,
            offset: 0,
            cell: Some(Cell {
                control: table.1,
                index: 0,
                paragraph: 0,
            }),
        };
        s.run("table.properties", json!({"props": {"hasCaption": true}}))?;
        s.run("table.caption_text", json!({"text": "예산 집행"}))?;
        let path = dir.path().join("captioned.docx");
        s.export_docx(&path, false)?;
        let document = part(&path, "word/document.xml")?;
        assert!(
            document.contains("표 ") && document.contains("예산 집행"),
            "{document}"
        );
        Ok(())
    }

    /// Links to bookmarks (a linked table of contents) become DOCX anchors that
    /// name bookmarks Word accepts.
    #[test]
    fn writes_bookmark_links_as_anchors() -> Result<()> {
        let mut s = Session::new()?;
        s.run(
            "text.insert",
            json!({"text": "앞\n첫 제목\n본문\n둘째 제목"}),
        )?;
        for paragraph in [1, 3] {
            s.cursor = Cursor {
                section: 0,
                paragraph,
                offset: 0,
                cell: None,
            };
            s.run("style.apply", json!({"id": 2}))?;
        }
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: None,
        };
        s.run("tools.toc", json!({}))?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("toc.docx");
        let omitted = s.export_docx(&path, false)?;
        assert!(omitted.is_empty(), "{omitted:?}");
        let document = part(&path, "word/document.xml")?;
        let anchors: Vec<&str> = document
            .split("w:anchor=\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .collect();
        assert_eq!(anchors.len(), 2, "{document}");
        for anchor in anchors {
            assert!(
                anchor.chars().all(|c| c.is_alphanumeric() || c == '_'),
                "{anchor}"
            );
            assert!(
                document.contains(&format!("w:name=\"{anchor}\"")),
                "{anchor}"
            );
        }
        Ok(())
    }
}
