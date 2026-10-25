use super::*;
use hwpcraft_doc::{HyperlinkTarget, model::table::Table};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TableEdit {
    RowAbove,
    RowBelow,
    ColumnLeft,
    ColumnRight,
    DeleteRow,
    DeleteColumn,
    FitPage,
    MergeRight,
    SplitMerged,
}

impl Session {
    pub fn header_footer_text(&self, is_header: bool) -> Result<String> {
        let info: Value = serde_json::from_str(
            &self
                .core
                .get_header_footer_native(self.cursor.section, is_header, 0)
                .map_err(err)?,
        )?;
        if info["exists"] != true {
            return Ok(String::new());
        }
        let pi = info["paraIndex"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| anyhow!("머리말/꼬리말 주소 오류"))?;
        let ci = info["controlIndex"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| anyhow!("머리말/꼬리말 주소 오류"))?;
        let ctrl = self
            .core
            .document()
            .sections
            .get(self.cursor.section)
            .and_then(|s| s.paragraphs.get(pi))
            .and_then(|p| p.controls.get(ci));
        let paragraphs = match ctrl {
            Some(Control::Header(h)) => &h.paragraphs,
            Some(Control::Footer(f)) => &f.paragraphs,
            _ => bail!("머리말/꼬리말을 찾지 못했습니다."),
        };
        if paragraphs.len() != 1
            || paragraphs.iter().any(|p| {
                !p.controls.is_empty()
                    || !p.field_ranges.is_empty()
                    || p.text.chars().any(char::is_control)
            })
        {
            bail!(
                "이 머리말/꼬리말에는 여러 문단·필드·개체가 있어 단순 텍스트 대체를 지원하지 않습니다."
            );
        }
        Ok(paragraphs
            .first()
            .map(|p| p.text.clone())
            .unwrap_or_default())
    }
    pub fn set_header_footer(&mut self, is_header: bool, text: &str) -> Result<()> {
        if text.len() > 10_000 || text.chars().any(char::is_control) {
            bail!("머리말/꼬리말은 제어 문자 없는 한 줄, 10 KB 이하로 입력하세요.");
        }
        let old = self.header_footer_text(is_header)?;
        self.transaction(|s| {
            let section = s.cursor.section;
            let info: Value = serde_json::from_str(
                &s.core
                    .get_header_footer_native(section, is_header, 0)
                    .map_err(err)?,
            )?;
            if info["exists"] != true {
                s.core
                    .create_header_footer_native(section, is_header, 0)
                    .map_err(err)?;
            }
            if !old.is_empty() {
                s.core
                    .delete_text_in_header_footer_native(
                        section,
                        is_header,
                        0,
                        0,
                        0,
                        old.chars().count(),
                    )
                    .map_err(err)?;
            }
            if !text.is_empty() {
                s.core
                    .insert_text_in_header_footer_native(section, is_header, 0, 0, 0, text)
                    .map_err(err)?;
            }
            Ok(())
        })
    }
    pub fn insert_page_number(&mut self) -> Result<()> {
        let info: Value = serde_json::from_str(
            &self
                .core
                .get_header_footer_native(self.cursor.section, false, 0)
                .map_err(err)?,
        )?;
        if info["exists"] == true {
            bail!(
                "현재 구역에 꼬리말이 이미 있습니다. 기존 꼬리말을 보존하기 위해 새 쪽 번호를 추가하지 않았습니다."
            );
        }
        self.transaction(|s| {
            let section = s.cursor.section;
            s.core
                .create_header_footer_native(section, false, 0)
                .map_err(err)?;
            s.core
                .apply_hf_template_native(section, false, 0, 2)
                .map_err(err)?;
            Ok(())
        })
    }
    pub fn set_columns(&mut self, count: u16, spacing_mm: f64) -> Result<()> {
        if self.cursor.cell.is_some() {
            bail!("단 설정은 본문에서 사용하세요.");
        }
        if !(1..=6).contains(&count)
            || !spacing_mm.is_finite()
            || !(0.0..=50.0).contains(&spacing_mm)
        {
            bail!("단 수는 1–6, 간격은 0–50 mm입니다.");
        }
        let p = self.page_setup()?;
        let width = if p.landscape { p.height_mm } else { p.width_mm }
            - p.left_mm
            - p.right_mm
            - p.gutter_mm;
        if (width - spacing_mm * f64::from(count - 1)) / f64::from(count) < 10.0 {
            bail!("각 단의 너비를 10 mm 이상 남겨 주세요.");
        }
        self.transaction(|s| {
            s.core
                .set_column_def_native(
                    s.cursor.section,
                    count,
                    0,
                    true,
                    (spacing_mm * 7200.0 / 25.4).round() as i16,
                )
                .map_err(err)?;
            Ok(())
        })
    }
    pub fn column_break(&mut self) -> Result<()> {
        if self.cursor.cell.is_some() {
            bail!("단 나누기는 본문에서 사용하세요.");
        }
        self.transaction(|s| {
            s.delete_range(s.selection())?;
            let c = s.cursor;
            // RHWP's split_at counts inline controls, like the page-break path.
            let offset = s.split_offset(c)?;
            s.core
                .insert_column_break_native(c.section, c.paragraph, offset)
                .map_err(err)?;
            s.cursor.paragraph += 1;
            s.cursor.offset = 0;
            s.anchor = None;
            Ok(())
        })
    }
    /// Kept for the `insert.picture` protocol command: RHWP default placement, natural size.
    pub fn insert_png(&mut self, path: &Path) -> Result<()> {
        self.insert_picture(path, None, false).map(|_| ())
    }
    /// PNG/JPG/BMP/GIF at the caret (body or a table cell). The picture keeps its
    /// aspect ratio; `width_mm` defaults to 96 dpi size capped by the text width.
    /// RHWP places body pictures as floating 어울림 and cell pictures beside the table.
    pub fn insert_picture(
        &mut self,
        path: &Path,
        width_mm: Option<f64>,
        treat_as_char: bool,
    ) -> Result<Option<ObjectAddress>> {
        hwpcraft_formats::file_access::check_read(&path.to_string_lossy())
            .map_err(anyhow::Error::msg)?;
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(20_000_001)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 20_000_000 {
            bail!("그림 파일은 20 MB 이하입니다.");
        }
        let format = image::guess_format(&bytes)
            .map_err(|_| anyhow!("그림 형식을 알 수 없습니다. PNG, JPG, BMP, GIF를 지원합니다."))?;
        let extension = match format {
            image::ImageFormat::Png => "png",
            image::ImageFormat::Jpeg => "jpg",
            image::ImageFormat::Bmp => "bmp",
            image::ImageFormat::Gif => "gif",
            other => bail!("{other:?} 그림은 지원하지 않습니다. PNG, JPG, BMP, GIF를 넣으세요."),
        };
        let reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
        let (w, h) = reader.into_dimensions()?;
        if w == 0 || h == 0 || w > 8192 || h > 8192 || u64::from(w) * u64::from(h) > 16_000_000 {
            bail!("그림은 8192픽셀 및 1,600만 화소 이하입니다.");
        }
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(128_000_000);
        reader.limits(limits);
        reader.decode()?;
        let p = self.page_setup()?;
        let body = (if p.landscape { p.height_mm } else { p.width_mm })
            - p.left_mm
            - p.right_mm
            - p.gutter_mm;
        let width_mm = match width_mm {
            Some(v) if v.is_finite() && (1.0..=600.0).contains(&v) => v,
            Some(_) => bail!("그림 너비는 1–600 mm입니다."),
            None => (f64::from(w) * 25.4 / 96.0).min(body).min(140.0),
        };
        let height_mm = width_mm * f64::from(h) / f64::from(w);
        if !(0.1..=600.0).contains(&height_mm) {
            bail!("그림 높이가 0.1–600 mm 범위를 벗어납니다.");
        }
        let (width, height) = (
            (width_mm * 7200.0 / 25.4).round().max(1.0) as u32,
            (height_mm * 7200.0 / 25.4).round().max(1.0) as u32,
        );
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().chars().take(200).collect::<String>())
            .unwrap_or_default();
        let mut inserted = None;
        self.transaction(|s| {
            s.delete_selection()?;
            // Body pictures keep the caret after the object, like other inline objects.
            let c = if s.cursor.cell.is_none() {
                s.reserve_object_position()?
            } else {
                s.cursor
            };
            let cell_path: Vec<(usize, usize, usize)> = c
                .cell
                .map(|t| vec![(t.control, t.index, t.paragraph)])
                .unwrap_or_default();
            let response = s
                .core
                .insert_picture_native(
                    c.section,
                    c.paragraph,
                    c.offset,
                    &cell_path,
                    &bytes,
                    width,
                    height,
                    w,
                    h,
                    extension,
                    &name,
                    None,
                    None,
                )
                .map_err(err)?;
            let v: Value = serde_json::from_str(&response)?;
            let index = |k: &str| {
                v[k].as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| anyhow!("그림 삽입 결과에 {k}가 없습니다: {v}"))
            };
            let a = ObjectAddress {
                section: c.section,
                paragraph: index("paraIdx")?,
                control: index("controlIdx")?,
            };
            if treat_as_char {
                s.core
                    .set_picture_properties_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        r#"{"treatAsChar":true}"#,
                    )
                    .map_err(err)?;
            }
            inserted = Some(a);
            s.clear_selection();
            Ok(())
        })?;
        Ok(inserted)
    }
    fn current_table(&self) -> Result<&Table> {
        let c = self.cursor;
        let cell = c.cell.ok_or_else(|| anyhow!("먼저 표 셀을 선택하세요."))?;
        match self
            .core
            .document()
            .sections
            .get(c.section)
            .and_then(|s| s.paragraphs.get(c.paragraph))
            .and_then(|p| p.controls.get(cell.control))
        {
            Some(Control::Table(t)) => Ok(t),
            _ => bail!("표를 찾지 못했습니다."),
        }
    }
    pub fn table_info(&self) -> Result<Value> {
        let t = self.current_table()?;
        let c = self
            .cursor
            .cell
            .ok_or_else(|| anyhow!("표 셀을 선택하세요."))?;
        let cell = t
            .cells
            .get(c.index)
            .ok_or_else(|| anyhow!("셀 범위를 벗어났습니다."))?;
        Ok(json!({"rows":t.row_count,"columns":t.col_count,"row":cell.row,"column":cell.col}))
    }
    pub fn edit_table(&mut self, edit: TableEdit) -> Result<()> {
        let c = self.cursor;
        let address = c.cell.ok_or_else(|| anyhow!("먼저 표 셀을 선택하세요."))?;
        let t = self.current_table()?;
        let cell = t
            .cells
            .get(address.index)
            .ok_or_else(|| anyhow!("셀 범위를 벗어났습니다."))?;
        let (row, col) = (cell.row, cell.col);
        let (rows, cols) = (t.row_count, t.col_count);
        let merge_end = if matches!(edit, TableEdit::MergeRight) {
            let next_col = col.saturating_add(cell.col_span);
            let right = t
                .cells
                .iter()
                .find(|v| v.row == row && v.col == next_col && v.row_span == cell.row_span)
                .ok_or_else(|| anyhow!("같은 높이의 오른쪽 셀이 없습니다."))?;
            (
                row.saturating_add(cell.row_span).saturating_sub(1),
                right.col.saturating_add(right.col_span).saturating_sub(1),
            )
        } else {
            (row, col)
        };
        if matches!(edit, TableEdit::SplitMerged) && (cell.col_span == 1 && cell.row_span == 1) {
            bail!("병합된 셀을 선택하세요.");
        }
        if matches!(edit, TableEdit::SplitMerged) && usize::from(rows) * usize::from(cols) > 4096 {
            bail!("셀 나누기는 4,096칸 이하의 표에서 지원합니다.");
        }
        let new_rows = usize::from(rows)
            + usize::from(matches!(edit, TableEdit::RowAbove | TableEdit::RowBelow));
        let new_cols = usize::from(cols)
            + usize::from(matches!(
                edit,
                TableEdit::ColumnLeft | TableEdit::ColumnRight
            ));
        if matches!(
            edit,
            TableEdit::RowAbove
                | TableEdit::RowBelow
                | TableEdit::ColumnLeft
                | TableEdit::ColumnRight
        ) && (new_rows > 200 || new_cols > 64 || new_rows * new_cols > 4096)
        {
            bail!("표 확장은 최대 200행, 64열, 4,096칸까지 지원합니다.");
        }
        if (matches!(edit, TableEdit::DeleteRow) && rows <= 1)
            || (matches!(edit, TableEdit::DeleteColumn) && cols <= 1)
        {
            bail!("마지막 줄이나 칸은 삭제할 수 없습니다.");
        }
        self.transaction(|s| {
            let r = match edit {
                TableEdit::RowAbove | TableEdit::RowBelow => s.core.insert_table_row_native(
                    c.section,
                    c.paragraph,
                    address.control,
                    row,
                    matches!(edit, TableEdit::RowBelow),
                ),
                TableEdit::ColumnLeft | TableEdit::ColumnRight => {
                    s.core.insert_table_column_native(
                        c.section,
                        c.paragraph,
                        address.control,
                        col,
                        matches!(edit, TableEdit::ColumnRight),
                    )
                }
                TableEdit::DeleteRow => {
                    s.core
                        .delete_table_row_native(c.section, c.paragraph, address.control, row)
                }
                TableEdit::DeleteColumn => {
                    s.core
                        .delete_table_column_native(c.section, c.paragraph, address.control, col)
                }
                TableEdit::FitPage => {
                    s.core
                        .fit_table_to_page_native(c.section, c.paragraph, address.control)
                }
                TableEdit::MergeRight => s.core.merge_table_cells_native(
                    c.section,
                    c.paragraph,
                    address.control,
                    row,
                    col,
                    merge_end.0,
                    merge_end.1,
                ),
                TableEdit::SplitMerged => s.core.split_table_cell_native(
                    c.section,
                    c.paragraph,
                    address.control,
                    row,
                    col,
                ),
            };
            r.map_err(err)?;
            let t = s.current_table()?;
            let row = row.min(t.row_count.saturating_sub(1));
            let col = col.min(t.col_count.saturating_sub(1));
            let index = t
                .cells
                .iter()
                .position(|v| {
                    v.row <= row
                        && row < v.row.saturating_add(v.row_span)
                        && v.col <= col
                        && col < v.col.saturating_add(v.col_span)
                })
                .ok_or_else(|| anyhow!("편집 후 셀을 찾지 못했습니다."))?;
            s.cursor.cell = Some(Cell {
                index,
                paragraph: 0,
                ..address
            });
            s.cursor.offset = 0;
            s.anchor = None;
            Ok(())
        })
    }
    /// Tab / Shift+Tab inside a table: Hangul's TableRightCellAppend / TableLeftCell.
    pub fn move_cell(&mut self, backward: bool) -> Result<()> {
        let a = self.table_target(None)?;
        self.move_in_table(a, if backward { "prev" } else { "next" })?;
        Ok(())
    }
    pub fn bookmarks(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self.core.get_bookmarks_native().map_err(err)?,
        )?)
    }
    pub fn add_bookmark(&mut self, name: &str) -> Result<()> {
        if self.cursor.cell.is_some() {
            bail!("책갈피 추가는 현재 본문에서만 지원합니다.");
        }
        if name.trim().is_empty()
            || name.chars().count() > 128
            || name.chars().any(char::is_control)
        {
            bail!("책갈피 이름은 제어 문자 없이 1–128글자로 입력하세요.");
        }
        self.transaction(|s| {
            let c = s.cursor;
            let value: Value = serde_json::from_str(
                &s.core
                    .add_bookmark_native(c.section, c.paragraph, c.offset, name)
                    .map_err(err)?,
            )?;
            if value["ok"] != true {
                bail!("{}", value["error"].as_str().unwrap_or("책갈피 추가 실패"));
            }
            Ok(())
        })
    }
    fn bookmark(&self, name: &str) -> Result<(Cursor, usize)> {
        let all = self.bookmarks()?;
        let v = all
            .as_array()
            .and_then(|a| a.iter().find(|x| x["name"] == name))
            .ok_or_else(|| anyhow!("책갈피를 찾지 못했습니다."))?;
        let number = |key: &str| -> Result<usize> {
            v[key]
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| anyhow!("책갈피 주소가 잘못되었습니다."))
        };
        Ok((
            Cursor {
                section: number("sec")?,
                paragraph: number("para")?,
                offset: number("charPos")?,
                cell: None,
            },
            number("ctrlIdx")?,
        ))
    }
    pub fn goto_bookmark(&mut self, name: &str) -> Result<()> {
        let (c, _) = self.bookmark(name)?;
        self.select(c, false)
    }
    pub fn delete_bookmark(&mut self, name: &str) -> Result<()> {
        let (c, index) = self.bookmark(name)?;
        self.transaction(|s| {
            let value: Value = serde_json::from_str(
                &s.core
                    .delete_bookmark_native(c.section, c.paragraph, index)
                    .map_err(err)?,
            )?;
            if value["ok"] != true {
                bail!("책갈피 삭제 실패");
            }
            Ok(())
        })
    }
    pub fn insert_hyperlink(&mut self, uri: &str, text: Option<&str>) -> Result<()> {
        if uri.len() > 8192 {
            bail!("주소는 8,192바이트 이하로 입력하세요.");
        }
        if let Some(t) = text
            && (t.len() > 1_000_000 || t.contains(['\n', '\r']))
        {
            bail!("링크 표시 문자열은 1 MB 이하의 한 문단이어야 합니다.");
        }
        self.transaction(|s| {
            let mut r = s.selection();
            if r.is_empty() {
                let start = s.cursor.offset;
                s.insert(text.filter(|v| !v.is_empty()).unwrap_or(uri))?;
                r = start..s.cursor.offset;
            }
            let c = s.cursor;
            let target = HyperlinkTarget {
                section: c.section,
                para: c.paragraph,
                cell_path: c
                    .cell
                    .map(|t| vec![(t.control, t.index, t.paragraph)])
                    .unwrap_or_default(),
            };
            s.core
                .insert_hyperlink_native(&target, r.start, r.end, uri)
                .map_err(err)?;
            s.anchor = None;
            Ok(())
        })
    }
}
