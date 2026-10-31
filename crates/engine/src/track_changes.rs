//! Owner: format area. 변경 추적 기록의 상태와 저장 보호.
//!
//! HwpCraft cannot keep Hangul's change tracking. HWP 5 documents keep it in
//! DocInfo records (TRACK_CHANGE_AUTHOR 97, TRACK_CHANGE 96), FileHeader bit 14
//! and paragraph range tags of kinds 16–19, while RHWP edits and saves the
//! shown result (`ViewText`) as the body; HWPX `insertBegin`/`deleteBegin`
//! marks are not read at all. Saving such a document as HWP/HWPX would merge
//! the changes into one text under stale tracking records, so it is refused
//! unless the caller accepts the merge, and an accepted save drops the tracking
//! records so the file is consistent.
use super::*;
use std::io::Read;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[(
    "document.track_changes",
    "변경 추적 기록 상태 (저장하면 결과본으로 합쳐지는지)",
    "",
)];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[(
    "document.track_changes",
    "매개 변수 없음. tracked(변경 추적 기록 여부), format(hwp/hwpx), authors·changes(HWP 기록 수), ranges(HWP 변경 범위 수), marks(HWPX 삽입·삭제 표식 수), message. tracked면 HWP/HWPX 저장은 기본으로 거절되고, file.save·save_document의 acceptTrackChanges:true로만 저장합니다(변경 내용이 지금 보이는 결과로 합쳐지고 변경 추적 기록은 지워집니다).",
    r#"{}"#,
)];

/// HWP 5 DocInfo record tags (HWPTAG_BEGIN 16 + 80 / + 81).
const TRACK_CHANGE: u16 = 96;
const TRACK_CHANGE_AUTHOR: u16 = 97;
/// FileHeader property bit 14: 변경 추적 문서.
const HEADER_TRACKED: u32 = 1 << 14;
/// Paragraph range-tag kinds found only in change-tracked HWP 5 documents.
const TRACK_RANGE_KINDS: std::ops::RangeInclusive<u32> = 16..=19;

pub const REFUSED: &str = "변경 추적 기록이 있는 문서입니다. HwpCraft는 변경 추적 기록을 보존하지 못해, HWP/HWPX/DOCX로 저장하면 변경 내용이 지금 보이는 결과본으로 합쳐지고 변경 기록은 사라집니다. 그래도 저장하려면 결과본으로 합쳐 저장하세요(MCP save_document acceptTrackChanges:true, CLI --accept-track-changes, 데스크톱 확인 창). 쪽·블록 저장은 그렇게 저장한 뒤에 하세요.";

/// Insert/delete marks in an HWPX package (sections only), read when it opens.
pub(crate) fn hwpx_track_marks(path: &Path) -> usize {
    let Ok(file) = std::fs::File::open(path) else {
        return 0;
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return 0;
    };
    let names: Vec<String> = zip
        .file_names()
        .filter(|n| n.starts_with("Contents/section") && n.ends_with(".xml"))
        .map(str::to_owned)
        .collect();
    let mut marks = 0;
    for name in names {
        let Ok(entry) = zip.by_name(&name) else {
            continue;
        };
        let mut xml = String::new();
        // Section XML is bounded by the package itself; 512 MB guards a bomb.
        if entry.take(512 << 20).read_to_string(&mut xml).is_err() {
            continue;
        }
        marks += xml.matches(":insertBegin").count() + xml.matches(":deleteBegin").count();
    }
    marks
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_track_changes_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        (id == "document.track_changes").then(|| {
            if params.as_object().is_some_and(|p| !p.is_empty()) {
                bail!("document.track_changes에는 매개 변수가 없습니다.");
            }
            Ok(self.track_changes())
        })
    }

    /// The document's change-tracking state.
    pub fn track_changes(&self) -> Value {
        let doc = self.core.document();
        let count = |tag| {
            doc.doc_info
                .extra_records
                .iter()
                .filter(|r| r.tag_id == tag)
                .count()
        };
        let (authors, changes) = (count(TRACK_CHANGE_AUTHOR), count(TRACK_CHANGE));
        let ranges: usize = doc
            .sections
            .iter()
            .flat_map(|s| s.paragraphs.iter())
            .map(|p| {
                p.range_tags
                    .iter()
                    .filter(|r| TRACK_RANGE_KINDS.contains(&(r.tag >> 24)))
                    .count()
            })
            .sum();
        let header = doc.header.flags & HEADER_TRACKED != 0;
        let marks = self.track_marks;
        let tracked = header || authors + changes + ranges + marks > 0;
        let format = if marks > 0 { "hwpx" } else { "hwp" };
        json!({
            "tracked": tracked,
            "format": format,
            "authors": authors,
            "changes": changes,
            "ranges": ranges,
            "marks": marks,
            "message": if tracked { REFUSED } else { "" },
        })
    }

    pub fn has_unkept_track_changes(&self) -> bool {
        self.track_changes()["tracked"] == true
    }

    /// Save as HWP/HWPX accepting that the tracked changes merge into the shown
    /// text: the tracking records are dropped first so the file is consistent.
    pub fn save_accepting_track_changes(&mut self, path: &Path) -> Result<()> {
        self.drop_track_changes();
        self.save(path)
    }

    /// Drop the tracking records so the shown text is the whole document.
    pub(crate) fn drop_track_changes(&mut self) {
        let doc = self.core.document_mut();
        doc.header.flags &= !HEADER_TRACKED;
        if let Some(raw) = doc.header.raw_data.as_mut()
            && raw.len() >= 40
        {
            let flags = u32::from_le_bytes([raw[36], raw[37], raw[38], raw[39]]) & !HEADER_TRACKED;
            raw[36..40].copy_from_slice(&flags.to_le_bytes());
        }
        let before = doc.doc_info.extra_records.len();
        doc.doc_info
            .extra_records
            .retain(|r| !matches!(r.tag_id, TRACK_CHANGE | TRACK_CHANGE_AUTHOR));
        if doc.doc_info.extra_records.len() != before {
            doc.doc_info.raw_stream = None;
            doc.doc_info.raw_stream_dirty = true;
        }
        for section in &mut doc.sections {
            let mut touched = false;
            for paragraph in &mut section.paragraphs {
                let before = paragraph.range_tags.len();
                paragraph
                    .range_tags
                    .retain(|r| !TRACK_RANGE_KINDS.contains(&(r.tag >> 24)));
                touched |= paragraph.range_tags.len() != before;
            }
            if touched {
                section.raw_stream = None;
            }
        }
        self.track_marks = 0;
    }

    /// Refuse an HWP/HWPX save that would carry tracking records it cannot keep.
    pub(crate) fn refuse_tracked_save(&self, path: &Path) -> Result<()> {
        let editable = path.extension().and_then(|x| x.to_str()).is_some_and(|x| {
            ["hwp", "hwpx", "docx"]
                .iter()
                .any(|e| x.eq_ignore_ascii_case(e))
        });
        if editable && self.has_unkept_track_changes() {
            bail!(REFUSED);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "../../../rhwp/samples/issue5169_viewtext_changetracking.hwp";

    #[test]
    fn tracked_hwp_refuses_plain_save_and_accepting_save_is_clean() -> Result<()> {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE);
        if !sample.exists() {
            return Ok(());
        }
        let mut s = Session::new()?;
        s.open(&sample)?;
        let state = s.run("document.track_changes", json!({}))?;
        assert_eq!(state["tracked"], true, "{state}");
        let dir = tempfile::tempdir()?;
        let plain = dir.path().join("plain.hwp");
        let error = s.save(&plain).err().map(|e| e.to_string());
        assert!(
            error.as_deref().is_some_and(|e| e.contains("변경 추적")),
            "{error:?}"
        );
        assert!(!plain.exists());
        // Page and block saves would write the same stale records.
        let pages = s.page_count()?;
        let whole = dir.path().join("pages.hwp");
        assert!(
            s.run("file.save_pages", json!({"path":whole,"from":1,"to":pages}))
                .is_err()
        );
        assert!(!whole.exists());
        // PDF and TXT exports are not affected.
        s.save(&dir.path().join("out.txt"))?;
        let accepted = dir.path().join("accepted.hwp");
        s.save_accepting_track_changes(&accepted)?;
        let mut reopened = Session::new()?;
        reopened.open(&accepted)?;
        assert_eq!(reopened.track_changes()["tracked"], false);
        // Once accepted, the document saves normally.
        s.save(&dir.path().join("again.hwpx"))?;
        Ok(())
    }

    #[test]
    fn untracked_documents_save_as_before() -> Result<()> {
        let mut s = Session::new()?;
        s.run("text.insert", json!({"text":"보통 문서"}))?;
        assert_eq!(s.track_changes()["tracked"], false);
        let dir = tempfile::tempdir()?;
        s.save(&dir.path().join("a.hwp"))?;
        s.save(&dir.path().join("a.hwpx"))?;
        assert!(s.run("document.track_changes", json!({"x":1})).is_err());
        Ok(())
    }
}
