//! Geometry-based table pen. One stroke is one transaction shared by UI and CLI.
use super::*;
use crate::ext_table::{CellRange, MAX_CELLS, MAX_COLUMNS, MAX_ROWS};
use hwpcraft_doc::model::table::{Cell as ModelCell, Table};
use serde::Deserialize;

pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("table.draw_create", "마우스로 표 외곽 그리기", ""),
    ("table.pen", "표 구분선 그리기 / 지우기", ""),
    ("table.move", "표 위치 이동", ""),
];
const HU: f64 = 7200.0 / 25.4;
const SNAP: f64 = 170.0; // 0.6 mm: make near-boundary strokes deterministic.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Draw {
    width_mm: f64,
    height_mm: f64,
    x_mm: f64,
    y_mm: f64,
    #[serde(default = "line_default")]
    line: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pen {
    table: ObjectAddress,
    start: [f64; 2],
    end: [f64; 2], // millimetres from table top-left
    #[serde(default)]
    erase: bool,
    #[serde(default = "line_default")]
    line: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveTable {
    table: ObjectAddress,
    page: u32,
    dx_mm: f64,
    dy_mm: f64,
}
fn line_default() -> Value {
    json!({"type":1,"width":0,"color":"#000000"})
}
fn edges(sizes: Vec<u32>) -> Vec<f64> {
    let mut v = vec![0.0];
    for n in sizes {
        v.push(v.last().copied().unwrap_or(0.0) + f64::from(n));
    }
    v
}
fn range(c: &ModelCell) -> CellRange {
    CellRange {
        start_row: c.row,
        end_row: c.row + c.row_span - 1,
        start_col: c.col,
        end_col: c.col + c.col_span - 1,
    }
}
fn bounds(t: &Table) -> Result<(Vec<f64>, Vec<f64>)> {
    if t.cells.len() > MAX_CELLS
        || usize::from(t.row_count) > MAX_ROWS
        || usize::from(t.col_count) > MAX_COLUMNS
        || t.cell_spacing != 0
    {
        bail!("표 그리기는 셀 간격 0인 200줄·64칸·4096셀 이하 표에서 지원합니다.");
    }
    for c in &t.cells {
        if c.row_span == 0
            || c.col_span == 0
            || u32::from(c.row) + u32::from(c.row_span) > u32::from(t.row_count)
            || u32::from(c.col) + u32::from(c.col_span) > u32::from(t.col_count)
        {
            bail!("손상된 표 격자입니다.");
        }
    }
    Ok((edges(t.get_column_widths()), edges(t.get_row_heights())))
}
impl Session {
    pub(crate) fn run_drawing_ext(&mut self, id: &str, p: &Value) -> Option<Result<Value>> {
        match id {
            "table.move" => Some((|| {
                let p: MoveTable = serde_json::from_value(p.clone())?;
                self.move_table(p)?;
                Ok(self.info())
            })()),
            "table.draw_create" => Some((|| {
                let p: Draw = serde_json::from_value(p.clone())?;
                self.draw_table(p)
            })()),
            "table.pen" => Some((|| {
                let p: Pen = serde_json::from_value(p.clone())?;
                self.table_pen(p)?;
                Ok(self.info())
            })()),
            _ => None,
        }
    }
    fn move_table(&mut self, p: MoveTable) -> Result<()> {
        if [p.dx_mm, p.dy_mm]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1000.0)
        {
            bail!("표 이동 거리가 범위를 벗어났습니다.");
        }
        let a = p.table;
        let find = |s: &Self| -> Result<Value> {
            let layout: Value =
                serde_json::from_str(&s.core.get_page_control_layout_native(p.page).map_err(err)?)?;
            layout["controls"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|t| {
                    t["type"] == "table"
                        && t["secIdx"] == a.section
                        && t["paraIdx"] == a.paragraph
                        && t["controlIdx"] == a.control
                })
                .cloned()
                .ok_or_else(|| anyhow!("현재 쪽에서 표를 찾지 못했습니다."))
        };
        let before = find(self)?;
        // A split table cannot be moved as if its fragment were the entire object.
        for page in 0..self.core.page_count() {
            if page == p.page {
                continue;
            }
            let l: Value = serde_json::from_str(
                &self
                    .core
                    .get_page_control_layout_native(page)
                    .map_err(err)?,
            )?;
            if l["controls"].as_array().into_iter().flatten().any(|t| {
                t["type"] == "table"
                    && t["secIdx"] == a.section
                    && t["paraIdx"] == a.paragraph
                    && t["controlIdx"] == a.control
            }) {
                bail!("여러 쪽에 걸친 표는 표/셀 속성에서 위치를 조절하세요.");
            }
        }
        let x = before["x"]
            .as_f64()
            .ok_or_else(|| anyhow!("표 위치 오류"))?
            + p.dx_mm * 96.0 / 25.4;
        let y = before["y"]
            .as_f64()
            .ok_or_else(|| anyhow!("표 위치 오류"))?
            + p.dy_mm * 96.0 / 25.4;
        let paper: Value =
            serde_json::from_str(&self.core.get_page_info_native(p.page).map_err(err)?)?;
        if x < -0.1
            || y < -0.1
            || x + before["w"].as_f64().unwrap_or(0.0)
                > paper["width"].as_f64().unwrap_or(0.0) + 0.1
            || y + before["h"].as_f64().unwrap_or(0.0)
                > paper["height"].as_f64().unwrap_or(0.0) + 0.1
        {
            bail!("용지 안으로 표를 이동하세요.");
        }
        self.table_at(a)?;
        if p.dx_mm.abs() < 0.001 && p.dy_mm.abs() < 0.001 {
            return Ok(());
        }
        self.transaction(|s| {
            let inline = s.table_at(a)?.common.treat_as_char;
            let mut props = json!({"treatAsChar":false,"horzRelTo":"Paper","vertRelTo":"Paper","horzAlign":"Left","vertAlign":"Top","horzOffset":(x*75.0).round() as i32,"vertOffset":(y*75.0).round() as i32});
            if inline { props["textWrap"] = json!("TopAndBottom"); }
            s.core.set_table_properties_native(a.section,a.paragraph,a.control,&props.to_string()).map_err(err)?;
            // Account for outer margins/anchor layout instead of jumping at the first drag.
            let placed = find(s)?;
            props["horzOffset"] = json!((x*75.0).round() as i64 + ((x-placed["x"].as_f64().unwrap_or(x))*75.0).round() as i64);
            props["vertOffset"] = json!((y*75.0).round() as i64 + ((y-placed["y"].as_f64().unwrap_or(y))*75.0).round() as i64);
            s.core.set_table_properties_native(a.section,a.paragraph,a.control,&props.to_string()).map_err(err)?;
            Ok(())
        })
    }
    fn draw_table(&mut self, p: Draw) -> Result<Value> {
        if [p.width_mm, p.height_mm, p.x_mm, p.y_mm]
            .iter()
            .any(|n| !n.is_finite())
            || !(2.0..=600.0).contains(&p.width_mm)
            || !(2.0..=600.0).contains(&p.height_mm)
            || p.x_mm < 0.0
            || p.y_mm < 0.0
            || p.x_mm + p.width_mm > 1000.0
            || p.y_mm + p.height_mm > 1000.0
        {
            bail!("표 크기는 2–600 mm이며 쪽 안에서 그리세요.");
        }
        let page = self.page_setup()?;
        let (pw, ph) = if page.landscape {
            (page.height_mm, page.width_mm)
        } else {
            (page.width_mm, page.height_mm)
        };
        if p.x_mm + p.width_mm > pw || p.y_mm + p.height_mm > ph {
            bail!("용지 안에서 표를 그리세요.");
        }
        if self.cursor.cell.is_some() || self.has_selection() {
            bail!("표 밖 본문에 커서를 놓고 그리세요.");
        }
        self.transaction(|s|{
            let c=s.cursor;
            let v:Value=serde_json::from_str(&s.core.create_table_native(c.section,c.paragraph,c.offset,1,1).map_err(err)?)?;
            let a=ObjectAddress{section:c.section,paragraph:v["paraIdx"].as_u64().ok_or_else(||anyhow!("표 주소 오류"))? as usize,control:v["controlIdx"].as_u64().ok_or_else(||anyhow!("표 주소 오류"))? as usize};
            let mut t=s.table_at(a)?.clone();t.cells[0].width=(p.width_mm*HU).round()as u32;t.cells[0].height=(p.height_mm*HU).round()as u32;
            s.install_pen_table(a,t)?;
            s.core.set_table_properties_native(a.section,a.paragraph,a.control,&json!({"treatAsChar":false,"horzRelTo":"Paper","vertRelTo":"Paper","horzAlign":"Left","vertAlign":"Top","horzOffset":(p.x_mm*HU).round()as i32,"vertOffset":(p.y_mm*HU).round()as i32,"textWrap":"TopAndBottom"}).to_string()).map_err(err)?;
            let borders=json!({"borderLeft":p.line,"borderRight":p.line,"borderTop":p.line,"borderBottom":p.line});
            s.cell_properties_raw(a,CellRange{start_row:0,start_col:0,end_row:0,end_col:0},&borders,None)?;
            s.reseat(a,0,0)
        })?;
        self.table_layout(self.table_target(None)?, None)
    }
    fn install_pen_table(&mut self, a: ObjectAddress, mut t: Table) -> Result<()> {
        t.cells.sort_by_key(|c| (c.row, c.col));
        t.row_sizes = (0..t.row_count)
            .map(|r| t.cells.iter().filter(|c| c.row == r).count() as i16)
            .collect();
        t.rebuild_grid();
        t.update_ctrl_dimensions();
        let widths = t.get_column_widths();
        let sec = self
            .core
            .document_mut()
            .sections
            .get_mut(a.section)
            .ok_or_else(|| anyhow!("구역 없음"))?;
        sec.raw_stream = None;
        let slot = sec
            .paragraphs
            .get_mut(a.paragraph)
            .and_then(|p| p.controls.get_mut(a.control))
            .ok_or_else(|| anyhow!("표 없음"))?;
        *slot = Control::Table(Box::new(t));
        self.core
            .set_table_column_widths_native(a.section, a.paragraph, a.control, widths)
            .map_err(err)?;
        self.core.reflow_linesegs_on_demand();
        Ok(())
    }
    fn table_pen(&mut self, p: Pen) -> Result<()> {
        if p.start
            .iter()
            .chain(p.end.iter())
            .any(|n| !n.is_finite() || *n < -1.0 || *n > 2000.0)
        {
            bail!("표 선 좌표가 범위를 벗어났습니다.");
        }
        // Reuse strict edge validation before any mutation.
        let o = p
            .line
            .as_object()
            .ok_or_else(|| anyhow!("선 속성이 필요합니다."))?;
        if o.len() != 3
            || !p.line["type"].as_u64().is_some_and(|n| n <= 17)
            || !p.line["width"].as_u64().is_some_and(|n| n <= 15)
            || !p.line["color"].as_str().is_some_and(|s| {
                s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
            })
        {
            bail!("올바른 선 종류·굵기·색을 지정하세요.");
        }
        let a = p.table;
        let t = self.table_at(a)?.clone();
        let (xs, ys) = bounds(&t)?;
        let start = p.start.map(|n| n * HU);
        let end = p.end.map(|n| n * HU);
        let dx = (start[0] - end[0]).abs();
        let dy = (start[1] - end[1]).abs();
        if dx.max(dy) < 283.0 {
            bail!("1 mm 이상 선을 그리세요.");
        }
        let vertical = dy > dx;
        if dx > SNAP && dy > SNAP {
            return self.pen_diagonal(a, &t, &xs, &ys, start, end, p.erase, &p.line);
        }
        let (axis, perp) = if vertical { (&xs, &ys) } else { (&ys, &xs) };
        let k = usize::from(!vertical);
        let mut pos = (start[k] + end[k]) / 2.0;
        if let Some(n) = axis.iter().find(|n| (**n - pos).abs() <= SNAP) {
            pos = *n;
        }
        let lo = start[1 - k].min(end[1 - k]);
        let hi = start[1 - k].max(end[1 - k]);
        let mut targets = Vec::new();
        for (i, c) in t.cells.iter().enumerate() {
            let (begin, span, other, other_span) = if vertical {
                (c.col, c.col_span, c.row, c.row_span)
            } else {
                (c.row, c.row_span, c.col, c.col_span)
            };
            let left = axis[begin as usize];
            let right = axis[(begin + span) as usize];
            let top = perp[other as usize];
            let bottom = perp[(other + other_span) as usize];
            if pos >= left - SNAP && pos <= right + SNAP && hi > top + SNAP && lo < bottom - SNAP {
                if lo > top + SNAP || hi < bottom - SNAP {
                    bail!("선의 양 끝을 셀 테두리까지 이어 주세요.");
                }
                if c.cell_protect() {
                    bail!("보호된 셀은 그리거나 지울 수 없습니다.");
                }
                targets.push(i);
            }
        }
        if targets.is_empty() {
            bail!("그리거나 지울 셀 경계를 찾지 못했습니다.");
        }
        let cuts: Vec<usize> = targets
            .iter()
            .copied()
            .filter(|i| {
                let c = &t.cells[*i];
                let (b, n) = if vertical {
                    (c.col, c.col_span)
                } else {
                    (c.row, c.row_span)
                };
                pos > axis[b as usize] + SNAP && pos < axis[(b + n) as usize] - SNAP
            })
            .collect();
        if p.erase && !cuts.is_empty() {
            bail!("지울 선 위를 끌어 주세요.");
        }
        if !cuts.is_empty() && !t.zones.is_empty() {
            bail!("영역 속성이 있는 표의 그리기 분할은 아직 지원하지 않습니다.");
        }
        self.transaction(|s| {
            let mut work = t.clone();
            if !cuts.is_empty() {
                cut_grid(&mut work, &xs, &ys, &cuts, vertical, pos)?;
                s.install_pen_table(a, work)?;
            } else if p.erase {
                // Only rectangular adjacent pairs can merge; T junctions become invisible edges.
                let mut pairs = Vec::new();
                for &i in &targets {
                    for &j in &targets {
                        if i >= j {
                            continue;
                        }
                        let c = &t.cells[i];
                        let d = &t.cells[j];
                        let adjacent = if vertical {
                            c.row == d.row
                                && c.row_span == d.row_span
                                && ((c.col + c.col_span == d.col
                                    && (xs[d.col as usize] - pos).abs() < SNAP)
                                    || (d.col + d.col_span == c.col
                                        && (xs[c.col as usize] - pos).abs() < SNAP))
                        } else {
                            c.col == d.col
                                && c.col_span == d.col_span
                                && ((c.row + c.row_span == d.row
                                    && (ys[d.row as usize] - pos).abs() < SNAP)
                                    || (d.row + d.row_span == c.row
                                        && (ys[c.row as usize] - pos).abs() < SNAP))
                        };
                        if adjacent {
                            pairs.push((
                                c.row.min(d.row),
                                c.col.min(d.col),
                                (c.row + c.row_span).max(d.row + d.row_span) - 1,
                                (c.col + c.col_span).max(d.col + d.col_span) - 1,
                            ));
                        }
                    }
                }
                for (r, c, er, ec) in pairs {
                    work.merge_cells(r, c, er, ec).map_err(|e| anyhow!(e))?;
                }
                s.install_pen_table(a, work)?;
            }
            let after = s.table_at(a)?.clone();
            let (ax, ay) = bounds(&after)?;
            let (axis, perp) = if vertical { (&ax, &ay) } else { (&ay, &ax) };
            let mut line = p.line.clone();
            if p.erase {
                line["type"] = json!(0);
            }
            for c in &after.cells {
                let (b, n, o, on) = if vertical {
                    (c.col, c.col_span, c.row, c.row_span)
                } else {
                    (c.row, c.row_span, c.col, c.col_span)
                };
                if perp[o as usize] < lo - SNAP || perp[(o + on) as usize] > hi + SNAP {
                    continue;
                }
                let mut props = json!({});
                if (axis[b as usize] - pos).abs() <= SNAP {
                    props[if vertical { "borderLeft" } else { "borderTop" }] = line.clone();
                }
                if (axis[(b + n) as usize] - pos).abs() <= SNAP {
                    props[if vertical {
                        "borderRight"
                    } else {
                        "borderBottom"
                    }] = line.clone();
                }
                if !props.as_object().is_none_or(|m| m.is_empty()) {
                    s.cell_properties_raw(a, range(c), &props, None)?;
                }
            }
            let first = &t.cells[targets[0]];
            s.reseat(a, first.row, first.col)
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn pen_diagonal(
        &mut self,
        a: ObjectAddress,
        t: &Table,
        xs: &[f64],
        ys: &[f64],
        start: [f64; 2],
        end: [f64; 2],
        erase: bool,
        line: &Value,
    ) -> Result<()> {
        let c = t
            .cells
            .iter()
            .find(|c| {
                let x = xs[c.col as usize];
                let r = xs[(c.col + c.col_span) as usize];
                let y = ys[c.row as usize];
                let b = ys[(c.row + c.row_span) as usize];
                (start[0].min(end[0]) - x).abs() <= SNAP
                    && (start[0].max(end[0]) - r).abs() <= SNAP
                    && (start[1].min(end[1]) - y).abs() <= SNAP
                    && (start[1].max(end[1]) - b).abs() <= SNAP
            })
            .ok_or_else(|| anyhow!("대각선은 한 셀의 서로 마주 보는 모서리를 이어 주세요."))?;
        if c.cell_protect() {
            bail!("보호된 셀입니다.");
        }
        let key = if (end[0] - start[0]) * (end[1] - start[1]) > 0.0 {
            "diagonalBackSlash"
        } else {
            "diagonalSlash"
        };
        self.set_cell_properties(a,Some(range(c)),&json!({key:if erase{0}else{1},"diagonalLine":line["type"],"diagonalWidth":line["width"],"diagonalColor":line["color"]}),None)
    }
}
fn cut_grid(
    t: &mut Table,
    xs: &[f64],
    ys: &[f64],
    targets: &[usize],
    vertical: bool,
    pos: f64,
) -> Result<()> {
    let axis = if vertical { xs } else { ys };
    let boundary = axis.iter().position(|n| (*n - pos).abs() < 1.0);
    let index = boundary.unwrap_or_else(|| axis.partition_point(|v| *v < pos));
    if index == 0 || index >= axis.len() {
        bail!("표 안쪽에 선을 그리세요.");
    }
    let added = usize::from(boundary.is_none());
    if usize::from(t.col_count) + if vertical { added } else { 0 } > MAX_COLUMNS
        || usize::from(t.row_count) + if vertical { 0 } else { added } > MAX_ROWS
        || t.cells.len() + targets.len() > MAX_CELLS
    {
        bail!("표의 줄·칸·셀 제한을 넘습니다.");
    }
    for &i in targets {
        let c = &t.cells[i];
        let (b, n) = if vertical {
            (c.col, c.col_span)
        } else {
            (c.row, c.row_span)
        };
        if pos - axis[b as usize] < 283.0 || axis[(b + n) as usize] - pos < 283.0 {
            bail!("나눈 셀의 크기를 1 mm 이상 남겨 주세요.");
        }
    }
    if added == 1 {
        for c in &mut t.cells {
            let (b, n) = if vertical {
                (&mut c.col, &mut c.col_span)
            } else {
                (&mut c.row, &mut c.row_span)
            };
            if *b as usize >= index {
                *b += 1;
            } else if usize::from(*b) + usize::from(*n) >= index {
                *n += 1;
            }
        }
        if vertical {
            t.col_count += 1;
        } else {
            t.row_count += 1;
        }
    }
    let mut extra = Vec::new();
    for &i in targets {
        let template = t.cells[i].clone();
        let original_begin = if vertical { template.col } else { template.row } as usize;
        let old_begin = if added == 1 && original_begin >= index {
            original_begin - 1
        } else {
            original_begin
        };
        let first = (pos - axis[old_begin]).round() as u32;
        let mut new = ModelCell::new_from_template(
            template.col,
            template.row,
            template.width,
            template.height,
            &template,
        );
        if vertical {
            new.col = index as u16;
            new.col_span = template.col + template.col_span - index as u16;
            new.row_span = template.row_span;
            new.width = template.width.saturating_sub(first);
            t.cells[i].col_span = index as u16 - template.col;
            t.cells[i].width = first;
        } else {
            new.row = index as u16;
            new.row_span = template.row + template.row_span - index as u16;
            new.col_span = template.col_span;
            new.height = template.height.saturating_sub(first);
            t.cells[i].row_span = index as u16 - template.row;
            t.cells[i].height = first;
        }
        for p in &mut t.cells[i].paragraphs {
            p.line_segs.clear();
        }
        for p in &mut new.paragraphs {
            p.line_segs.clear();
        }
        extra.push(new);
    }
    t.cells.extend(extra);
    Ok(())
}
