//! Owner: shell/page area. Direct manipulation of tables inside table cells (중첩 표):
//! the canvas drags a nested table's grid line like a top-level one. A nested table is
//! named by the cell paragraph that holds it (a `DocumentAddress` with at least one
//! cell step) and its control index there.
use super::*;
use crate::contexts::DocumentAddress;
use crate::ext_table::CellRange;
use hwpcraft_doc::model::table::Table;
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[(
    "context.table.move_border",
    "중첩 표 셀 경계선 옮기기 (마우스 끌기)",
    "",
)];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[(
    "context.table.move_border",
    "address: 중첩 표가 든 셀 문단({section, paragraph, path:[{control, cell, paragraph}…]}, 한 단계 이상; 본문 표는 table.move_border), control: 그 문단에서 표의 컨트롤 번호, vertical(true: 세로 경계선), boundary(1–칸 수 또는 줄 수), delta_mm, range?(그 표의 셀 블록: 안쪽 세로 경계선은 블록의 줄에서만 옮김). 한 번 실행 취소.",
    r#"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"control":0,"vertical":true,"boundary":1,"delta_mm":5.0}"#,
)];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveBorder {
    address: DocumentAddress,
    control: usize,
    vertical: bool,
    boundary: u16,
    delta_mm: f64,
    #[serde(default)]
    range: Option<CellRange>,
}

/// The table at `control` of the paragraph `address` names.
fn nested_table_mut<'a>(
    doc: &'a mut hwpcraft_doc::model::document::Document,
    address: &DocumentAddress,
    control: usize,
) -> Result<&'a mut Table> {
    let mut para = doc
        .sections
        .get_mut(address.section)
        .and_then(|s| s.paragraphs.get_mut(address.paragraph))
        .ok_or_else(|| anyhow!("본문 문단 주소가 범위를 벗어났습니다."))?;
    for (depth, step) in address.path.iter().enumerate() {
        let Some(Control::Table(table)) = para.controls.get_mut(step.control) else {
            bail!("문맥 경로 {depth}: 지정한 컨트롤이 표가 아닙니다.");
        };
        if table.common.locked {
            bail!("잠긴 표 안의 표는 고칠 수 없습니다.");
        }
        para = table
            .cells
            .get_mut(step.cell)
            .and_then(|cell| cell.paragraphs.get_mut(step.paragraph))
            .ok_or_else(|| anyhow!("문맥 경로 {depth}: 셀 문단이 없습니다."))?;
    }
    match para.controls.get_mut(control) {
        Some(Control::Table(table)) => Ok(table),
        _ => bail!("지정한 컨트롤이 표가 아닙니다."),
    }
}

impl Session {
    pub(crate) fn run_nested_table_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        (id == "context.table.move_border").then(|| {
            let p: MoveBorder = serde_json::from_value(params.clone())?;
            self.move_nested_border(&p)?;
            Ok(self.info())
        })
    }

    fn move_nested_border(&mut self, p: &MoveBorder) -> Result<()> {
        if p.address.path.is_empty() {
            bail!("본문 표는 table.move_border로 옮기세요.");
        }
        if !p.delta_mm.is_finite() || p.delta_mm.abs() > 600.0 {
            bail!("경계선 이동은 ±600 mm 이내입니다.");
        }
        let d = (p.delta_mm * 7200.0 / 25.4).round() as i64;
        if d == 0 {
            return Ok(());
        }
        self.transaction(|s| {
            let doc = s.core.document_mut();
            let t = nested_table_mut(doc, &p.address, p.control)?;
            let (cross, count) = match p.range {
                Some(r) if p.vertical => (Some((r.start_row, r.end_row)), t.row_count),
                Some(r) => (Some((r.start_col, r.end_col)), t.col_count),
                None => (None, 0),
            };
            match cross {
                Some(c) if c.0 > c.1 || c.1 >= count => {
                    bail!("셀 블록이 표 범위를 벗어났습니다.")
                }
                Some(c) if c != (0, count - 1) => {
                    if !p.vertical {
                        bail!(
                            "가로 경계선은 블록만큼 옮길 수 없습니다. 블록을 풀고 줄 전체의 경계선을 옮기세요."
                        );
                    }
                    crate::table_border::move_border_in_block(t, true, p.boundary, d, c)?;
                }
                _ => crate::table_border::move_whole_border(t, p.vertical, p.boundary, d)?,
            }
            if let Some(section) = doc.sections.get_mut(p.address.section) {
                section.raw_stream = None;
            }
            s.refresh_layout()
        })
    }
}
