//! Distribute physical cells in the selected block without resizing other rows
//! or columns. Rebuild the grid from their resulting edges, including T junctions.
use super::*;
use std::collections::BTreeMap;

const SPLIT: &str = "여러 쪽에 걸친 표는 일부 칸의 셀 높이만 같게 맞출 수 없습니다. 표 너비 전체를 블록으로 잡으세요.";

impl Session {
    pub(super) fn equalize_block_cells(
        &mut self,
        a: ObjectAddress,
        r: CellRange,
        rows: bool,
    ) -> Result<()> {
        let mut table = self.table_at(a)?.clone();
        if table.cell_spacing != 0 || !table.zones.is_empty() {
            bail!(
                "셀 간격이나 영역 속성이 있는 표의 부분 셀 크기 맞추기는 아직 지원하지 않습니다."
            );
        }
        let drawn = if rows {
            Some(self.drawn_table(a, SPLIT)?)
        } else {
            None
        };
        if let Some((heights, _)) = &drawn {
            crate::table_border::pin_row_heights(&mut table, heights);
        }
        let sizes = crate::table_border::grid_sizes(&table, !rows);
        let mut edges = vec![0u64];
        for size in sizes {
            edges.push(edges.last().copied().unwrap_or(0) + u64::from(size));
        }
        let axis = |c: &hwpcraft_doc::model::table::Cell| {
            if rows {
                (c.row, c.row_span, c.col, c.col_span)
            } else {
                (c.col, c.col_span, c.row, c.row_span)
            }
        };
        let mut bounds: Vec<(u64, u64)> = table
            .cells
            .iter()
            .map(|c| {
                let (start, span, _, _) = axis(c);
                let left = edges.get(usize::from(start));
                let right = edges.get(usize::from(start) + usize::from(span));
                match (left, right) {
                    (Some(&left), Some(&right))
                        if right > left && right - left <= u64::from(u32::MAX) =>
                    {
                        Ok((left, right))
                    }
                    _ => bail!("셀 경계가 올바르지 않아 크기를 맞출 수 없습니다."),
                }
            })
            .collect::<Result<_>>()?;
        if table.cells.iter().zip(&bounds).any(|(c, (left, right))| {
            right - left != u64::from(if rows { c.height } else { c.width })
        }) {
            bail!("셀 크기와 표 격자가 일치하지 않아 부분 크기를 맞출 수 없습니다.");
        }
        let original = bounds.clone();
        let selected = Self::block_cells(&table, r);
        let mut groups: BTreeMap<(u16, u16), Vec<usize>> = BTreeMap::new();
        for &index in &selected {
            let (_, _, cross, span) = axis(&table.cells[index]);
            groups.entry((cross, span)).or_default().push(index);
        }
        let mut has_pair = false;
        for indices in groups.values_mut() {
            indices.sort_by_key(|i| bounds[*i].0);
            if indices.len() < 2 {
                continue;
            }
            has_pair = true;
            if indices
                .windows(2)
                .any(|pair| bounds[pair[0]].1 != bounds[pair[1]].0)
            {
                bail!("합친 셀 때문에 연속하지 않은 블록의 크기를 같게 맞출 수 없습니다.");
            }
            let left = bounds[indices[0]].0;
            let total = bounds[indices[indices.len() - 1]].1 - left;
            let count = indices.len() as u64;
            if total / count < MIN_CELL_HU as u64 {
                bail!("셀 크기를 1 mm 이상 남겨 주세요.");
            }
            for (n, &i) in indices.iter().enumerate() {
                bounds[i] = (
                    left + total * n as u64 / count,
                    left + total * (n as u64 + 1) / count,
                );
            }
        }
        if !has_pair {
            bail!("같게 맞출 연속된 셀을 두 개 이상 선택하세요.");
        }
        if bounds == original {
            return Ok(());
        }
        let mut edges: Vec<u64> = bounds.iter().flat_map(|(l, r)| [*l, *r]).collect();
        edges.sort_unstable();
        edges.dedup();
        let count = edges.len() - 1;
        if count > if rows { MAX_ROWS } else { MAX_COLUMNS } {
            bail!("크기를 맞춘 뒤 표의 줄·칸 제한을 넘습니다.");
        }
        for (cell, &(left, right)) in table.cells.iter_mut().zip(&bounds) {
            let start = edges
                .binary_search(&left)
                .map_err(|_| anyhow!("셀 시작 경계 없음"))? as u16;
            let end = edges
                .binary_search(&right)
                .map_err(|_| anyhow!("셀 끝 경계 없음"))? as u16;
            if rows {
                cell.row = start;
                cell.row_span = end - start;
                cell.height = (right - left) as u32;
            } else {
                let changed = cell.width != (right - left) as u32;
                cell.col = start;
                cell.col_span = end - start;
                cell.width = (right - left) as u32;
                if changed {
                    for p in &mut cell.paragraphs {
                        p.line_segs.clear();
                    }
                }
            }
        }
        if rows {
            table.row_count = count as u16;
        } else {
            table.col_count = count as u16;
        }
        table.row_sizes = (0..table.row_count)
            .map(|r| table.cells.iter().filter(|c| c.row == r).count() as i16)
            .collect();
        table.rebuild_grid();
        if !rows
            && table.get_column_widths()
                != edges
                    .windows(2)
                    .map(|e| (e[1] - e[0]) as u32)
                    .collect::<Vec<_>>()
        {
            bail!("이 병합 구조에서는 새 칸 너비를 계산할 수 없습니다.");
        }
        table.update_ctrl_dimensions();
        let first = &table.cells[selected[0]];
        let seat = (first.row, first.col);
        let inside = self.cursor_in(a).is_some();
        self.transaction(|s| {
            let section = &mut s.core.document_mut().sections[a.section];
            section.raw_stream = None;
            section.paragraphs[a.paragraph].controls[a.control] = Control::Table(Box::new(table));
            s.refresh_layout()?;
            s.core.reflow_linesegs_on_demand();
            if let Some((_, height)) = drawn
                && (s.drawn_table(a, SPLIT)?.1 - height).abs() > 1.0
            {
                bail!("셀 내용보다 작게 높이를 줄일 수 없습니다.");
            }
            if inside {
                s.reseat(a, seat.0, seat.1)?;
            }
            Ok(())
        })
    }
}
