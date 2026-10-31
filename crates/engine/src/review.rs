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
    "path(필수): 비교할 HWP/HWPX/HML 파일. ignore_whitespace(선택): 띄어쓰기 차이 무시. password(선택): 암호 문서. 현재 문서를 수정본, 파일을 원본으로 보고 findings[]에 종류(label)·위치(section, paragraph, cursor)·내용(detail: RHWP 원문 A=현재·B=파일, text: 사용자 문구)을 돌려줍니다. 글상자·각주·머리말 안은 비교하지 않습니다.",
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

/// A finding's `detail` in the user's words: RHWP writes `A=… B=…` (A is the current
/// document, B the file) and model field names such as `para_shape_id`.
fn detail_text(detail: &str) -> String {
    static CELL: std::sync::LazyLock<Option<regex::Regex>> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\(r(\d+),c(\d+)\)").ok());
    const WORDS: [(&str, &str); 10] = [
        ("B 에만 있는 문단", "비교 파일에만 있는 문단"),
        ("A 에만 있는 문단", "현재 문서에만 있는 문단"),
        ("para_shape_id", "문단 모양 번호"),
        ("char_shape_id", "글자 모양 번호"),
        ("style_id", "스타일 번호"),
        ("컨트롤 수", "개체 수"),
        ("컨트롤 종류", "개체 종류"),
        ("영문이름", "영문 이름"),
        ("행렬", "줄×칸"),
        ("셀 주소", "셀 위치"),
    ];
    const KINDS: [(&str, &str); 14] = [
        ("sectionDef", "구역 정의"),
        ("columnDef", "단 정의"),
        ("table", "표"),
        ("shape", "도형"),
        ("picture", "그림"),
        ("header", "머리말"),
        ("footer", "꼬리말"),
        ("footnote", "각주"),
        ("endnote", "미주"),
        ("bookmark", "책갈피"),
        ("hyperlink", "하이퍼링크"),
        ("equation", "수식"),
        ("field", "필드"),
        ("form", "양식 개체"),
    ];
    let mut text = detail.to_owned();
    for (from, to) in WORDS {
        text = text.replace(from, to);
    }
    if text.starts_with("개체 종류") {
        for (from, to) in KINDS {
            text = text.replace(&format!("={from}"), &format!("={to}"));
        }
    }
    let text = match CELL.as_ref() {
        Some(cell) => cell
            .replace_all(&text, |c: &regex::Captures| {
                let n = |i: usize| {
                    c[i].parse::<u32>()
                        .map_or_else(|_| c[i].to_owned(), |v| (v + 1).to_string())
                };
                format!("{}줄 {}칸", n(1), n(2))
            })
            .into_owned(),
        None => text,
    };
    text.replace("A=", "현재 ").replace(" B=", " · 파일 ")
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
                    "text": detail_text(&f.detail),
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

#[cfg(test)]
mod tests {
    use super::detail_text;

    #[test]
    fn details_read_as_current_and_file_without_model_names() {
        assert_eq!(
            detail_text(r#"A="새 문장" B="옛 문장""#),
            r#"현재 "새 문장" · 파일 "옛 문장""#
        );
        assert_eq!(
            detail_text("para_shape_id: A=3 B=5, style_id: A=0 B=2"),
            "문단 모양 번호: 현재 3 · 파일 5, 스타일 번호: 현재 0 · 파일 2"
        );
        assert_eq!(
            detail_text("컨트롤 종류: A=table B=picture"),
            "개체 종류: 현재 표 · 파일 그림"
        );
        assert_eq!(
            detail_text("셀 주소: A=(r0,c1) B=(r2,c3)"),
            "셀 위치: 현재 1줄 2칸 · 파일 3줄 4칸"
        );
        assert_eq!(
            detail_text(r#"B 에만 있는 문단: "둘째""#),
            r#"비교 파일에만 있는 문단: "둘째""#
        );
        assert_eq!(
            detail_text("행렬: A=2x3 B=3x3"),
            "줄×칸: 현재 2x3 · 파일 3x3"
        );
    }
}
