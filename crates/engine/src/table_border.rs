//! Owner: shell/page area. Moving a table border for part of the table only: Hangul moves
//! just the selected cells' edge when a cell block is chosen and a border inside it is
//! dragged, which leaves the columns (or rows) ragged.
//!
//! HWP tables are a grid in which a cell covers `span` grid columns, so a ragged edge is a
//! new thin grid column (or row) between the old and the new border position: inside the
//! block the cell on the side the border moved towards gives it up, outside the block the
//! cell on the other side does, and cells crossing the border simply span it as well.
use super::*;
use hwpcraft_doc::model::table::{Cell, Table};

/// Smallest cell width/height the edge may leave (1 mm), as in `move_border`.
const MIN_SIZE: i64 = 283;

/// (position, span, cross position, cross span) of a cell along the moving axis.
fn along(c: &Cell, vertical: bool) -> (u16, u16, u16, u16) {
    if vertical {
        (c.col, c.col_span.max(1), c.row, c.row_span.max(1))
    } else {
        (c.row, c.row_span.max(1), c.col, c.col_span.max(1))
    }
}

fn set_along(c: &mut Cell, vertical: bool, pos: u16, span: u16) {
    if vertical {
        c.col = pos;
        c.col_span = span;
    } else {
        c.row = pos;
        c.row_span = span;
    }
}

/// Grid column widths (or row heights) from where cells actually start: walking each row
/// (column) adds up the widths (heights) of the cells it meets. RHWP's `get_column_widths`
/// reads single-span cells and (patch 0027) solves a column from a merged cell only when
/// it is the merged cell's one unknown column; walking the rows needs no such luck.
pub(crate) fn grid_sizes(t: &Table, vertical: bool) -> Vec<u32> {
    let (n, m) = if vertical {
        (t.col_count, t.row_count)
    } else {
        (t.row_count, t.col_count)
    };
    let mut starts: Vec<Option<u32>> = vec![None; usize::from(n) + 1];
    for line in 0..m {
        let (mut at, mut last) = (0u32, None);
        for k in 0..n {
            let index = if vertical {
                t.cell_index_at(line, k)
            } else {
                t.cell_index_at(k, line)
            };
            let Some(i) = index.filter(|i| Some(*i) != last) else {
                continue;
            };
            last = Some(i);
            let c = &t.cells[i];
            let (pos, size) = if vertical {
                (c.col, c.width)
            } else {
                (c.row, c.height)
            };
            if pos == k {
                starts[usize::from(k)].get_or_insert(at);
            }
            at += size;
        }
        starts[usize::from(n)].get_or_insert(at);
    }
    let fallback = if vertical {
        t.get_column_widths()
    } else {
        t.get_row_heights()
    };
    (0..usize::from(n))
        .map(|k| match (starts[k], starts[k + 1]) {
            (Some(a), Some(b)) if b > a => b - a,
            _ => fallback.get(k).copied().unwrap_or(1800),
        })
        .collect()
}

/// Move border `b` (between grid columns, or rows, `b - 1` and `b`) by `d` HWPUNIT only for
/// the cells whose cross range lies in `cross` (inclusive). The rest of the table keeps
/// the border where it was.
pub(crate) fn move_border_in_block(
    t: &mut Table,
    vertical: bool,
    b: u16,
    d: i64,
    cross: (u16, u16),
) -> Result<()> {
    let count = if vertical { t.col_count } else { t.row_count };
    if b == 0 || b >= count {
        bail!(
            "블록 안 경계선은 표 안쪽 선(1–{})만 옮길 수 있습니다.",
            count - 1
        );
    }
    if d == 0 {
        return Ok(());
    }
    let mut sizes: Vec<i64> = grid_sizes(t, vertical).into_iter().map(i64::from).collect();
    let (bi, step) = (usize::from(b), d.abs());
    let shrinks = if d > 0 { bi } else { bi - 1 };
    if sizes[shrinks] - step < MIN_SIZE || step < 57 {
        bail!("경계선을 옆 선에 너무 가깝게 옮길 수 없습니다(셀은 1 mm 이상).");
    }
    let inside = |pos: u16, span: u16| -> Option<bool> {
        let end = pos + span - 1;
        if pos >= cross.0 && end <= cross.1 {
            Some(true)
        } else if end < cross.0 || pos > cross.1 {
            Some(false)
        } else {
            None
        }
    };
    // Which cells touch the border, and on which side, before anything moves.
    let mut plan = Vec::with_capacity(t.cells.len());
    for c in &t.cells {
        let (pos, span, cpos, cspan) = along(c, vertical);
        let crosses = pos < b && pos + span > b;
        let left = pos + span == b;
        let right = pos == b;
        let side = inside(cpos, cspan);
        if crosses && side != Some(false) {
            bail!("블록 안에 이 경계선을 걸친 합친 셀이 있어 옮길 수 없습니다.");
        }
        if (left || right) && side.is_none() {
            bail!("합친 셀이 블록 경계에 걸쳐 있어 경계선을 블록만큼 옮길 수 없습니다.");
        }
        plan.push((crosses, left, right, side == Some(true)));
    }
    // A new grid line at b: cells right of it move on; crossing cells span it too.
    for (c, (crosses, left, right, inside)) in t.cells.iter_mut().zip(plan) {
        let (pos, span, _, _) = along(c, vertical);
        let (mut pos, mut span) = (if pos >= b { pos + 1 } else { pos }, span);
        if crosses {
            span += 1;
        }
        // The thin new strip between the old and the new border position goes to the
        // cell the border moved away from: inside the block, the side it moved towards
        // shrinks; outside the block the strip stays with the far side.
        let takes = match (inside, d > 0) {
            (true, true) | (false, false) => left,
            (true, false) | (false, true) => right,
        };
        if takes {
            if right {
                pos -= 1;
            }
            span += 1;
        }
        set_along(c, vertical, pos, span);
    }
    sizes.insert(bi, step);
    if d > 0 {
        sizes[bi + 1] -= step;
    } else {
        sizes[bi - 1] -= step;
    }
    let sizes: Vec<u32> = sizes.into_iter().map(|v| v as u32).collect();
    for z in &mut t.zones {
        let (start, end) = if vertical {
            (&mut z.start_col, &mut z.end_col)
        } else {
            (&mut z.start_row, &mut z.end_row)
        };
        if *start >= b {
            *start += 1;
        }
        if *end >= b {
            *end += 1;
        }
    }
    if vertical {
        t.col_count += 1;
        t.set_column_widths(&sizes).map_err(|e| anyhow!(e))?;
    } else {
        t.row_count += 1;
        for c in &mut t.cells {
            let (r, span) = (usize::from(c.row), usize::from(c.row_span.max(1)));
            c.height = sizes[r..(r + span).min(sizes.len())].iter().sum();
        }
    }
    t.cells.sort_by_key(|c| (c.row, c.col));
    t.row_sizes = (0..t.row_count)
        .map(|r| t.cells.iter().filter(|c| c.row == r).count() as i16)
        .collect();
    t.rebuild_grid();
    t.update_ctrl_dimensions();
    Ok(())
}

/// Move border `b` of `t` by `d` HWPUNIT for the whole table, from the grid sizes (so a
/// ragged table keeps its other rows): a vertical border trades width with the next
/// column (the right edge widens the table), a horizontal one changes row `b - 1`.
pub(crate) fn move_whole_border(t: &mut Table, vertical: bool, b: u16, d: i64) -> Result<()> {
    let count = if vertical { t.col_count } else { t.row_count };
    if b == 0 || b > count {
        bail!("경계선 번호는 1–{count}입니다.");
    }
    let mut sizes: Vec<i64> = grid_sizes(t, vertical).into_iter().map(i64::from).collect();
    let bi = usize::from(b);
    sizes[bi - 1] += d;
    if vertical && bi < sizes.len() {
        sizes[bi] -= d;
    }
    if sizes.iter().any(|v| *v < MIN_SIZE) {
        bail!("셀 크기는 1 mm 이상이어야 합니다.");
    }
    if vertical && sizes.iter().sum::<i64>() > 170_079 {
        bail!("표 너비는 600 mm 이하입니다.");
    }
    let sizes: Vec<u32> = sizes.into_iter().map(|v| v as u32).collect();
    if vertical {
        t.set_column_widths(&sizes).map_err(|e| anyhow!(e))?;
    } else {
        for c in &mut t.cells {
            let (r, span) = (usize::from(c.row), usize::from(c.row_span.max(1)));
            c.height = sizes[r..(r + span).min(sizes.len())].iter().sum();
        }
        t.rebuild_grid();
    }
    t.update_ctrl_dimensions();
    Ok(())
}

impl Session {
    /// `table.move_border` with a cell block: the border moves only for the block's rows
    /// (a vertical border) or columns (a horizontal one). A block covering the whole
    /// table moves the whole border. One undo step.
    pub(crate) fn move_border_in(
        &mut self,
        a: ObjectAddress,
        vertical: bool,
        boundary: u16,
        delta_mm: f64,
        range: crate::ext_table::CellRange,
    ) -> Result<()> {
        if !delta_mm.is_finite() || delta_mm.abs() > 600.0 {
            bail!("경계선 이동은 ±600 mm 이내입니다.");
        }
        let t = self.table_at(a)?;
        let (cross, count) = if vertical {
            ((range.start_row, range.end_row), t.row_count)
        } else {
            ((range.start_col, range.end_col), t.col_count)
        };
        if cross.0 > cross.1 || cross.1 >= count {
            bail!("셀 블록이 표 범위를 벗어났습니다.");
        }
        if cross == (0, count - 1) {
            return self.move_border(a, vertical, boundary, delta_mm);
        }
        // Row heights follow the cell contents at layout time, so a fresh table stores
        // only minimum heights and a partial horizontal edge has no reliable size to
        // split; only vertical borders move for part of a table.
        if !vertical {
            bail!(
                "가로 경계선은 블록만큼 옮길 수 없습니다. 블록을 풀고 줄 전체의 경계선을 옮기세요."
            );
        }
        let d = (delta_mm * 7200.0 / 25.4).round() as i64;
        if d == 0 {
            return Ok(());
        }
        self.transaction(|s| {
            let doc = s.core.document_mut();
            let section = doc
                .sections
                .get_mut(a.section)
                .ok_or_else(|| anyhow!("구역이 없습니다."))?;
            let Some(Control::Table(table)) = section
                .paragraphs
                .get_mut(a.paragraph)
                .and_then(|p| p.controls.get_mut(a.control))
            else {
                bail!("표를 찾지 못했습니다.");
            };
            move_border_in_block(table, vertical, boundary, d, cross)?;
            section.raw_stream = None;
            s.refresh_layout()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: u16, cols: u16) -> Table {
        let mut t = Table {
            row_count: rows,
            col_count: cols,
            cells: (0..rows)
                .flat_map(|r| (0..cols).map(move |c| Cell::new_empty(c, r, 2000, 1000, 0)))
                .collect(),
            row_sizes: vec![cols as i16; usize::from(rows)],
            ..Default::default()
        };
        t.rebuild_grid();
        t
    }

    fn grid(t: &Table) -> Vec<(u16, u16, u16, u16, u32)> {
        t.cells
            .iter()
            .map(|c| (c.row, c.col, c.row_span, c.col_span, c.width))
            .collect()
    }

    #[test]
    fn one_row_of_a_border_moves_and_the_rest_stays() -> Result<()> {
        let mut t = table(3, 3);
        // Move the border between columns 0 and 1 right by 500 in row 1 only.
        move_border_in_block(&mut t, true, 1, 500, (1, 1))?;
        assert_eq!(t.col_count, 4);
        assert_eq!(grid_sizes(&t, true), [2000, 500, 1500, 2000]);
        let g = grid(&t);
        assert!(g.contains(&(0, 0, 1, 1, 2000)), "{g:?}");
        assert!(
            g.contains(&(0, 1, 1, 2, 2000)),
            "row 0 keeps its border: {g:?}"
        );
        assert!(g.contains(&(1, 0, 1, 2, 2500)), "row 1 grows: {g:?}");
        assert!(g.contains(&(1, 2, 1, 1, 1500)), "{g:?}");
        assert_eq!(t.row_sizes, [3, 3, 3]);
        // Back left by 300 in rows 0–2: the border sits at 2000 - 300 for all of them.
        let mut u = table(3, 3);
        move_border_in_block(&mut u, true, 2, -300, (0, 1))?;
        assert_eq!(grid_sizes(&u, true), [2000, 1700, 300, 2000]);
        assert!(grid(&u).contains(&(2, 1, 1, 2, 2000)), "{:?}", grid(&u));
        assert!(grid(&u).contains(&(0, 2, 1, 2, 2300)), "{:?}", grid(&u));
        Ok(())
    }

    #[test]
    fn merged_cells_across_the_block_edge_are_refused() -> Result<()> {
        let mut t = table(3, 3);
        t.merge_cells(0, 0, 1, 0).map_err(|e| anyhow!(e))?;
        assert!(move_border_in_block(&mut t, true, 1, 300, (1, 2)).is_err());
        assert!(
            move_border_in_block(&mut t, true, 1, 1900, (2, 2)).is_err(),
            "too far"
        );
        Ok(())
    }
}
