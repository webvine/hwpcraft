//! Owner: shell/page area. Direct manipulation of tables inside table cells (중첩 표):
//! the canvas drags a nested table's grid line like a top-level one, and a cell block
//! of a nested table is merged, split, grown or shrunk by rows and columns and given
//! borders and backgrounds like a top-level one. A nested table is named by the cell
//! paragraph that holds it (a `DocumentAddress` with at least one cell step) and its
//! control index there.
use super::*;
use crate::contexts::DocumentAddress;
use crate::ext_table::CellRange;
use hwpcraft_doc::model::table::Table;
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "context.table.move_border",
        "중첩 표 셀 경계선 옮기기 (마우스 끌기)",
        "",
    ),
    ("context.table.merge", "중첩 표 셀 합치기", ""),
    ("context.table.split", "중첩 표 셀 나누기", ""),
    ("context.table.insert_row", "중첩 표 줄 넣기", ""),
    ("context.table.insert_column", "중첩 표 칸 넣기", ""),
    ("context.table.delete_row", "중첩 표 줄 지우기", ""),
    ("context.table.delete_column", "중첩 표 칸 지우기", ""),
    ("context.table.cells", "중첩 표 셀 블록 테두리·배경", ""),
    ("context.table.info", "중첩 표 줄·칸 수와 셀 목록", ""),
];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "context.table.move_border",
        "address: 중첩 표가 든 셀 문단({section, paragraph, path:[{control, cell, paragraph}…]}, 한 단계 이상; 본문 표는 table.move_border), control: 그 문단에서 표의 컨트롤 번호, vertical(true: 세로 경계선), boundary(1–칸 수 또는 줄 수), delta_mm, range?(그 표의 셀 블록: 안쪽 세로 경계선은 블록의 줄에서만 옮김). 한 번 실행 취소.",
        r#"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"control":0,"vertical":true,"boundary":1,"delta_mm":5.0}"#,
    ),
    (
        "context.table.merge",
        "address·control: context.table.move_border와 같음(중첩 표), range{start_row,start_col,end_row,end_col}(2셀 이상, 0부터). 보호된 셀이 있으면 거절. 한 번 실행 취소.",
        r#"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"control":0,"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":1}}"#,
    ),
    (
        "context.table.split",
        "address·control, row·col(나눌 셀), rows?(기본 1)·columns?(기본 2): 나눌 줄·칸 수(1–64), equal_height?(기본 true), merge_first?(합친 셀이면 먼저 풀기, 기본 false). 한 번 실행 취소.",
        r#"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"control":0,"row":0,"col":0,"rows":1,"columns":2}"#,
    ),
    (
        "context.table.insert_row",
        "address·control, row(기준 줄), below?(기본 true: 아래에 넣음). 한 번 실행 취소.",
        r#"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"control":0,"row":0}"#,
    ),
    (
        "context.table.insert_column",
        "address·control, col(기준 칸), right?(기본 true: 오른쪽에 넣음). 한 번 실행 취소.",
        r#"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"control":0,"col":0}"#,
    ),
    (
        "context.table.delete_row",
        "address·control, row. 마지막 한 줄은 지울 수 없음(표 지우기는 context.table.delete). 한 번 실행 취소.",
        r#"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"control":0,"row":0}"#,
    ),
    (
        "context.table.delete_column",
        "address·control, col. 마지막 한 칸은 지울 수 없음. 한 번 실행 취소.",
        r#"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"control":0,"col":0}"#,
    ),
    (
        "context.table.cells",
        "address·control, range(셀 블록), style: borderLeft|borderRight|borderTop|borderBottom{type,width,color #rrggbb}, fillType none|solid, fillColor #rrggbb(table.cell_props의 borders·fill과 같은 단위). 블록 안 셀마다 적용, 보호된 셀이 있으면 거절. 한 번 실행 취소.",
        r##"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"control":0,"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":1},"style":{"fillType":"solid","fillColor":"#dbe5f1"}}"##,
    ),
    (
        "context.table.info",
        "address·control → rows, columns, cells[{index,row,col,rowSpan,colSpan,protected}]. 조회만 합니다.",
        r#"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"control":0}"#,
    ),
];

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

/// A nested table and what to do to it (every command but move_border).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TableOp {
    address: DocumentAddress,
    control: usize,
    #[serde(default)]
    range: Option<CellRange>,
    #[serde(default)]
    row: Option<u16>,
    #[serde(default)]
    col: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
    #[serde(default)]
    columns: Option<u16>,
    #[serde(default)]
    equal_height: Option<bool>,
    #[serde(default)]
    merge_first: Option<bool>,
    #[serde(default)]
    below: Option<bool>,
    #[serde(default)]
    right: Option<bool>,
    #[serde(default)]
    style: Option<Value>,
}

/// The cells of `t` whose top-left corner lies in `r`, or an error naming the block.
fn cells_in(t: &Table, r: &CellRange) -> Result<Vec<usize>> {
    if r.start_row > r.end_row
        || r.start_col > r.end_col
        || r.end_row >= t.row_count
        || r.end_col >= t.col_count
    {
        bail!("셀 블록이 표 범위를 벗어났습니다.");
    }
    Ok(t.cells
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            (r.start_row..=r.end_row).contains(&c.row) && (r.start_col..=r.end_col).contains(&c.col)
        })
        .map(|(i, _)| i)
        .collect())
}

/// The table at `control` of the paragraph `address` names, read-only.
fn nested_table<'a>(
    doc: &'a hwpcraft_doc::model::document::Document,
    address: &DocumentAddress,
    control: usize,
) -> Result<&'a Table> {
    let mut para = doc
        .sections
        .get(address.section)
        .and_then(|s| s.paragraphs.get(address.paragraph))
        .ok_or_else(|| anyhow!("본문 문단 주소가 범위를 벗어났습니다."))?;
    for (depth, step) in address.path.iter().enumerate() {
        let Some(Control::Table(table)) = para.controls.get(step.control) else {
            bail!("문맥 경로 {depth}: 지정한 컨트롤이 표가 아닙니다.");
        };
        para = table
            .cells
            .get(step.cell)
            .and_then(|cell| cell.paragraphs.get(step.paragraph))
            .ok_or_else(|| anyhow!("문맥 경로 {depth}: 셀 문단이 없습니다."))?;
    }
    match para.controls.get(control) {
        Some(Control::Table(table)) => Ok(table),
        _ => bail!("지정한 컨트롤이 표가 아닙니다."),
    }
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
        match id {
            "context.table.move_border" => Some((|| {
                let p: MoveBorder = serde_json::from_value(params.clone())?;
                self.move_nested_border(&p)?;
                Ok(self.info())
            })()),
            "context.table.info" => Some((|| {
                let p: TableOp = serde_json::from_value(params.clone())?;
                self.nested_table_info(&p.address, p.control)
            })()),
            "context.table.merge"
            | "context.table.split"
            | "context.table.insert_row"
            | "context.table.insert_column"
            | "context.table.delete_row"
            | "context.table.delete_column"
            | "context.table.cells" => Some((|| {
                let p: TableOp = serde_json::from_value(params.clone())?;
                self.nested_table_op(id, &p)?;
                let mut v = self.info();
                v["table"] = self.nested_table_info(&p.address, p.control)?;
                Ok(v)
            })()),
            _ => None,
        }
    }

    /// Rows, columns and cells of a nested table.
    pub fn nested_table_info(&self, address: &DocumentAddress, control: usize) -> Result<Value> {
        let t = nested_table(self.core.document(), address, control)?;
        let cells: Vec<Value> = t
            .cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                json!({"index": i, "row": c.row, "col": c.col, "rowSpan": c.row_span,
                       "colSpan": c.col_span, "protected": c.cell_protect()})
            })
            .collect();
        Ok(json!({"rows": t.row_count, "columns": t.col_count, "cells": cells}))
    }

    fn nested_table_op(&mut self, id: &str, p: &TableOp) -> Result<()> {
        if p.address.path.is_empty() {
            bail!("본문 표는 table.* 명령을 쓰세요.");
        }
        self.guard_protected()?;
        let need = |v: Option<u16>, name: &str| v.ok_or_else(|| anyhow!("{name}을(를) 주세요."));
        // A border/fill style is resolved to a border-fill id before the table is borrowed.
        let style = match (id, &p.style) {
            ("context.table.cells", Some(v)) => Some(v.clone()),
            ("context.table.cells", None) => bail!("style을 주세요."),
            _ => None,
        };
        self.transaction(|s| {
            let doc = s.core.document_mut();
            let style_ids = if let Some(v) = &style {
                let t = nested_table(doc, &p.address, p.control)?;
                let range = p.range.as_ref().ok_or_else(|| anyhow!("range를 주세요."))?;
                let targets = cells_in(t, range)?;
                let bases: Vec<(usize, u16)> =
                    targets.iter().map(|&i| (i, t.cells[i].border_fill_id)).collect();
                let mut ids = Vec::new();
                for (i, base) in bases {
                    let current = base
                        .checked_sub(1)
                        .and_then(|k| doc.doc_info.border_fills.get(k as usize));
                    let bf = crate::ext_format::border_fill(v, current);
                    let id = match doc.doc_info.border_fills.iter().position(|b| *b == bf) {
                        Some(k) => (k + 1) as u16,
                        None => {
                            doc.doc_info.border_fills.push(bf);
                            doc.doc_info.raw_stream_dirty = true;
                            doc.doc_info.border_fills.len() as u16
                        }
                    };
                    ids.push((i, id));
                }
                ids
            } else {
                Vec::new()
            };
            let t = nested_table_mut(doc, &p.address, p.control)?;
            if t.common.locked {
                bail!("잠긴 표는 고칠 수 없습니다.");
            }
            let protected = |t: &Table, cells: &[usize]| cells.iter().any(|&i| t.cells[i].cell_protect());
            let r = match id {
                "context.table.merge" => {
                    let range = p.range.as_ref().ok_or_else(|| anyhow!("range를 주세요."))?;
                    if protected(t, &cells_in(t, range)?) {
                        bail!("보호된 셀이 있어 합칠 수 없습니다.");
                    }
                    t.merge_cells(range.start_row, range.start_col, range.end_row, range.end_col)
                }
                "context.table.split" => {
                    let (row, col) = (need(p.row, "row")?, need(p.col, "col")?);
                    let (rows, columns) = (p.rows.unwrap_or(1), p.columns.unwrap_or(2));
                    if !(1..=64).contains(&rows) || !(1..=64).contains(&columns) || rows * columns < 2 {
                        bail!("나눌 줄·칸 수는 1–64이고 둘 중 하나는 2 이상이어야 합니다.");
                    }
                    let cell = t
                        .cell_index_at(row, col)
                        .ok_or_else(|| anyhow!("그 자리에 셀이 없습니다."))?;
                    if t.cells[cell].cell_protect() {
                        bail!("보호된 셀은 나눌 수 없습니다.");
                    }
                    t.split_cell_into(
                        row,
                        col,
                        rows,
                        columns,
                        p.equal_height.unwrap_or(true),
                        p.merge_first.unwrap_or(false),
                    )
                }
                "context.table.insert_row" => t.insert_row(need(p.row, "row")?, p.below.unwrap_or(true)),
                "context.table.insert_column" => {
                    t.insert_column(need(p.col, "col")?, p.right.unwrap_or(true))
                }
                "context.table.delete_row" => {
                    if t.row_count <= 1 {
                        bail!("마지막 한 줄은 지울 수 없습니다. 표를 지우려면 context.table.delete를 쓰세요.");
                    }
                    t.delete_row(need(p.row, "row")?)
                }
                "context.table.delete_column" => {
                    if t.col_count <= 1 {
                        bail!("마지막 한 칸은 지울 수 없습니다. 표를 지우려면 context.table.delete를 쓰세요.");
                    }
                    t.delete_column(need(p.col, "col")?)
                }
                _ => {
                    let cells: Vec<usize> = style_ids.iter().map(|(i, _)| *i).collect();
                    if protected(t, &cells) {
                        bail!("보호된 셀이 있어 바꿀 수 없습니다.");
                    }
                    for (i, id) in &style_ids {
                        t.cells[*i].border_fill_id = *id;
                    }
                    Ok(())
                }
            };
            r.map_err(|e| anyhow!("{e}"))?;
            if let Some(section) = doc.sections.get_mut(p.address.section) {
                section.raw_stream = None;
            }
            s.refresh_layout()
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
