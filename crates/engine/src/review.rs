//! Owner: review-tools area. 검토: 문서 비교 (current document against a file) through RHWP's
//! `docdiff` engine. Other areas must not edit this file; request changes through the owner.
//!
//! The current document is treated as the edited copy and the file as the original, so a
//! paragraph only in the current document is reported as added and one only in the file as
//! removed. Findings carry a cursor when they point into the current document.
use super::*;
use hwpcraft_doc::docdiff::{DiffOptions, FindingKind, PathStep, diff_documents};
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[(
    "review.compare",
    "문서 비교 (현재 문서와 파일: 추가·삭제·변경 문단, 표·개체·스타일 차이)",
    "",
)];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[(
    "review.compare",
    "path(필수): 비교할 HWP/HWPX/HML 파일. ignore_whitespace(선택): 띄어쓰기 차이 무시. password(선택): 암호 문서. 현재 문서를 수정본, 파일을 원본으로 보고 findings[]에 종류(label)·위치(section, paragraph, cursor)·내용(detail)을 돌려줍니다. 글상자·각주·머리말 안은 비교하지 않습니다.",
    r#"{"path":"original.hwpx","ignore_whitespace":true}"#,
)];

/// Upper bound on reported findings so huge documents stay responsive.
const MAX_FINDINGS: usize = 2_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Compare {
    path: PathBuf,
    /// Treat runs of spaces as one and ignore leading/trailing spaces.
    #[serde(default)]
    ignore_whitespace: bool,
    #[serde(default)]
    password: Option<String>,
}

/// Korean label from the current document's point of view.
fn label(kind: FindingKind) -> &'static str {
    match kind {
        FindingKind::SectionCountChanged => "구역 수 변경",
        // A is the current document, so an A-only paragraph was added here.
        FindingKind::ParagraphRemoved => "문단 추가 (현재 문서에만 있음)",
        FindingKind::ParagraphAdded => "문단 삭제 (비교 파일에만 있음)",
        FindingKind::TextChanged => "내용 변경",
        FindingKind::ParagraphStyleChanged => "문단 모양 변경",
        FindingKind::TableShapeChanged => "표 모양 변경",
        FindingKind::ControlCountChanged => "개체 수 변경",
        FindingKind::ControlKindChanged => "개체 종류 변경",
        FindingKind::StyleCountChanged => "스타일 수 변경",
        FindingKind::StyleChanged => "스타일 정의 변경",
    }
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_review_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        Some(match id {
            "review.compare" => serde_json::from_value(params.clone())
                .map_err(anyhow::Error::from)
                .and_then(|p| self.compare_with_file(p)),
            _ => Err(anyhow!("알 수 없는 검토 명령: {id}")),
        })
    }

    /// Cursor in the current document for a finding path, when the path points into it.
    fn cursor_for_steps(&self, steps: &[PathStep]) -> Option<Cursor> {
        let mut it = steps.iter();
        let Some(PathStep::Section(section)) = it.next() else {
            return None;
        };
        let Some(PathStep::Paragraph(paragraph)) = it.next() else {
            return None;
        };
        let mut cursor = Cursor {
            section: *section,
            paragraph: *paragraph,
            ..Default::default()
        };
        let para = self.paragraph(cursor).ok()?;
        match (it.next(), it.next(), it.next()) {
            (None, _, _) => Some(cursor),
            (
                Some(PathStep::Control(control)),
                Some(PathStep::TableCell { row, col }),
                cell_paragraph,
            ) => {
                let Some(Control::Table(table)) = para.controls.get(*control) else {
                    return Some(cursor);
                };
                let index = table
                    .cells
                    .iter()
                    .position(|c| c.row == *row && c.col == *col)?;
                let paragraph = match cell_paragraph {
                    Some(PathStep::Paragraph(k)) => *k,
                    _ => 0,
                };
                cursor.cell = Some(Cell {
                    control: *control,
                    index,
                    paragraph,
                });
                self.paragraph(cursor).ok()?;
                Some(cursor)
            }
            _ => Some(cursor),
        }
    }

    fn compare_with_file(&mut self, p: Compare) -> Result<Value> {
        // Under automation roots, refuse before probing: a missing-file message must
        // not tell an agent what exists outside its read root.
        hwpcraft_formats::file_access::check_read(&p.path.to_string_lossy())
            .map_err(anyhow::Error::msg)?;
        if !p.path.is_file() {
            bail!("비교할 파일이 없습니다: {}", p.path.display());
        }
        let other = match p.password.as_deref() {
            Some(pw) => hwpcraft_formats::open_with_password(&p.path, pw)?,
            None => hwpcraft_formats::open(&p.path)?,
        };
        let options = DiffOptions::default()
            .ignore_whitespace(p.ignore_whitespace)
            .max_findings(MAX_FINDINGS);
        let diff = diff_documents(self.core.document(), other.document(), &options);
        let findings: Vec<Value> = diff
            .findings
            .iter()
            .enumerate()
            .map(|(index, f)| {
                let cursor = match f.kind {
                    // Only-in-file paragraphs have no place in the current document.
                    FindingKind::ParagraphAdded => None,
                    _ => self.cursor_for_steps(f.path.steps()),
                };
                json!({
                    "index": index,
                    "kind": f.kind.label(),
                    "label": label(f.kind),
                    "path": f.path.to_string(),
                    "section": f.path.section(),
                    "paragraph": f.path.paragraph(),
                    "detail": f.detail,
                    "cursor": cursor,
                })
            })
            .collect();
        let summary: Vec<Value> = diff
            .summary()
            .by_kind
            .iter()
            .map(|(kind, count)| json!({"kind": kind.label(), "label": label(*kind), "count": count}))
            .collect();
        Ok(json!({
            "path": p.path,
            "identical": diff.identical,
            "truncated": diff.truncated,
            "total": diff.findings.len(),
            "summary": summary,
            "findings": findings,
        }))
    }
}
