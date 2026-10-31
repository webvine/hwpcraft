//! State-preserving HwpCraft meanings for historical Hangul UI shortcut aliases.
//! These aliases are not claims that HAction.Run accepts the same identifiers.

use super::*;
use hwpcraft_doc::model::page::{ColumnDef, ColumnType};
use serde::Deserialize;

pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("page.number.format", "기존 쪽 번호의 모양만 변경", ""),
    ("page.columns.count", "기존 다단 설정의 단 수만 변경", ""),
];
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "page.number.format",
        "format: HWP 번호 모양 코드. 캐럿 구역에 이미 있는 쪽 번호 위치 컨트롤의 모양만 바꿉니다. 위치·장식·시작 번호는 그대로입니다. 컨트롤이 없으면 거절하니 먼저 page.number로 위치를 지정하세요.",
        r#"{"format":2}"#,
    ),
    (
        "page.columns.count",
        "count: 1–6. 이미 있는 첫 같은 너비 단 정의의 단 수만 바꿉니다. 간격·구분선·방향·종류는 그대로입니다. 단 정의가 여럿이거나 너비가 달라 다시 나눠야 하면 거절합니다.",
        r#"{"count":2}"#,
    ),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NumberFormat {
    format: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ColumnCount {
    count: u16,
}

impl Session {
    pub(crate) fn run_hwp_action_defaults_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        match id {
            "page.number.format" => Some((|| {
                let p: NumberFormat = serde_json::from_value(params.clone())?;
                self.patch_page_number_format(p.format)
            })()),
            "page.columns.count" => Some((|| {
                let p: ColumnCount = serde_json::from_value(params.clone())?;
                self.patch_column_count(p.count)
            })()),
            _ => None,
        }
    }

    fn patch_page_number_format(&mut self, format: u8) -> Result<Value> {
        if self.cursor.cell.is_some() {
            bail!("쪽 번호 모양은 본문에서 변경하세요.");
        }
        if !ext_page::NUMBER_FORMATS
            .iter()
            .any(|(code, _)| *code == format)
        {
            bail!("지원하지 않는 쪽 번호 모양입니다: {format}");
        }
        let section = self.cursor.section;
        let found = self
            .core
            .document()
            .sections
            .get(section)
            .ok_or_else(|| anyhow!("현재 구역이 없습니다."))?
            .paragraphs
            .iter()
            .enumerate()
            .find_map(|(pi, paragraph)| {
                paragraph
                    .controls
                    .iter()
                    .enumerate()
                    .find_map(|(ci, control)| {
                        if let Control::PageNumberPos(number) = control {
                            Some((pi, ci, number.format))
                        } else {
                            None
                        }
                    })
            });
        let (paragraph, control, previous) = found.ok_or_else(|| {
            anyhow!(
                "현재 구역에 쪽 번호가 없습니다. page.number에 position을 명시하여 먼저 배치하세요."
            )
        })?;
        if previous != format {
            self.transaction(|s| {
                let sec = &mut s.core.document_mut().sections[section];
                let Control::PageNumberPos(number) =
                    &mut sec.paragraphs[paragraph].controls[control]
                else {
                    bail!("쪽 번호 컨트롤이 변경되었습니다.");
                };
                number.format = format;
                sec.raw_stream = None;
                s.core.set_section_def_native(section, "{}").map_err(err)?;
                Ok(())
            })?;
        }
        self.run("page.number.get", json!({}))
    }

    fn column_target(&self) -> Result<(usize, usize, ColumnDef)> {
        let section = self
            .core
            .document()
            .sections
            .get(self.cursor.section)
            .ok_or_else(|| anyhow!("현재 구역이 없습니다."))?;
        let mut definitions = section.paragraphs.iter().enumerate().flat_map(|(pi, p)| {
            p.controls
                .iter()
                .enumerate()
                .filter_map(move |(ci, control)| {
                    if let Control::ColumnDef(column) = control {
                        Some((pi, ci, column.clone()))
                    } else {
                        None
                    }
                })
        });
        let target = definitions.next().ok_or_else(|| {
            anyhow!(
                "현재 구역에 단 정의가 없습니다. page.columns에 count와 spacing_mm를 명시하세요."
            )
        })?;
        if definitions.next().is_some() {
            bail!("구역 안에 단 정의가 여러 개여서 단 수만 일괄 변경할 수 없습니다.");
        }
        Ok(target)
    }

    fn patch_column_count(&mut self, count: u16) -> Result<Value> {
        if self.cursor.cell.is_some() {
            bail!("단 수는 본문에서 변경하세요.");
        }
        if !(1..=6).contains(&count) {
            bail!("단 수는 1–6 범위입니다.");
        }
        let section = self.cursor.section;
        let (paragraph, control, original) = self.column_target()?;
        if original.column_count == count {
            return Ok(column_result(&original));
        }
        if !original.same_width {
            bail!(
                "너비가 다른 다단은 단별 너비 재분배를 지정해야 합니다. 기존 설정을 보존했습니다."
            );
        }
        if original.spacing < 0 {
            bail!("음수 단 간격은 단 수만 변경하는 명령에서 지원하지 않습니다.");
        }
        let page = self.page_setup()?;
        let width_mm = if page.landscape {
            page.height_mm
        } else {
            page.width_mm
        } - page.left_mm
            - page.right_mm
            - page.gutter_mm;
        let gap_mm = f64::from(original.spacing) * 25.4 / 7200.0;
        if (width_mm - gap_mm * f64::from(count - 1)) / f64::from(count) < 10.0 {
            bail!("현재 단 간격을 보존하면 각 단 너비가 10 mm 미만입니다.");
        }
        let mut changed = original.clone();
        changed.column_count = count;
        if changed.raw_attr != 0 {
            // HWP table 141: only bits 2..9 encode the count. Retain reserved
            // bits as well as type/direction/equal-width flags.
            changed.raw_attr = (changed.raw_attr & !(0xff << 2)) | (count << 2);
        }
        self.transaction(|s| {
            let kind = match original.column_type {
                ColumnType::Normal => 0,
                ColumnType::Distribute => 1,
                ColumnType::Parallel => 2,
            };
            // Public primitive performs actual old/new body-width reflow. Then
            // restore every source property except the explicitly changed count.
            s.core
                .set_column_def_native(section, count, kind, true, original.spacing)
                .map_err(err)?;
            let sec = &mut s.core.document_mut().sections[section];
            let slot = &mut sec.paragraphs[paragraph].controls[control];
            if !matches!(slot, Control::ColumnDef(_)) {
                bail!("단 정의 위치가 변경되었습니다.");
            }
            *slot = Control::ColumnDef(changed.clone());
            sec.raw_stream = None;
            s.core.set_section_def_native(section, "{}").map_err(err)?;
            Ok(())
        })?;
        Ok(column_result(&changed))
    }
}

fn column_result(column: &ColumnDef) -> Value {
    json!({"count":column.column_count,"spacing_mm":f64::from(column.spacing)*25.4/7200.0,
        "column":column})
}

#[cfg(test)]
mod tests {
    use super::*;
    use hwpcraft_doc::model::{control::PageNumberPos, page::ColumnDirection};

    fn number(session: &Session) -> Result<PageNumberPos> {
        session.core.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .find_map(|control| {
                if let Control::PageNumberPos(number) = control {
                    Some(number.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| anyhow!("fixture page number missing"))
    }

    fn number_fixture(decorations: bool) -> Result<Session> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("자체 쪽 번호 본문 😀".into()))?;
        session.run(
            "page.number",
            json!({"position":"outside_top","format":0,"dash":true,"start":7}),
        )?;
        if decorations {
            for paragraph in &mut session.core.document_mut().sections[0].paragraphs {
                for control in &mut paragraph.controls {
                    if let Control::PageNumberPos(number) = control {
                        number.user_symbol = '※';
                        number.prefix_char = '[';
                        number.suffix_char = ']';
                        number.dash_char = '~';
                    }
                }
            }
        }
        session.core.document_mut().sections[0].raw_stream = None;
        session.core.set_section_def_native(0, "{}").map_err(err)?;
        Ok(Session::with_core(session.core, None))
    }

    fn columns_fixture() -> Result<Session> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("자체 다단 본문 ".repeat(30)))?;
        session.run("page.columns", json!({"count":2,"spacing_mm":6.25}))?;
        for paragraph in &mut session.core.document_mut().sections[0].paragraphs {
            for control in &mut paragraph.controls {
                if let Control::ColumnDef(column) = control {
                    column.column_type = ColumnType::Distribute;
                    column.direction = ColumnDirection::Mirror;
                    column.separator_type = 2;
                    column.separator_width = 3;
                    column.separator_color = 0x335577;
                    column.raw_attr = 0x8000 | 1 | (2 << 2) | (2 << 10) | (1 << 12);
                }
            }
        }
        session.core.document_mut().sections[0].raw_stream = None;
        session.core.set_section_def_native(0, "{}").map_err(err)?;
        Ok(Session::with_core(session.core, None))
    }

    #[test]
    fn shape_aliases_apply_only_format_with_pure_preparation_and_undo_redo() -> Result<()> {
        for (action, format) in [
            ("PageNumberShapeDigit", 0),
            ("PageNumberShapeCicledDigit", 1),
            ("PageNumberShapeRomanCapital", 2),
            ("PageNumberShapeRomanSmall", 3),
            ("PageNumberShapeLatinCapital", 4),
            ("PageNumberShapeHangulSyllable", 8),
            ("PageNumberShapeIdeograph", 13),
        ] {
            let mut session = number_fixture(true)?;
            let before = number(&session)?;
            let generation = session.generation;
            let cursor = session.cursor;
            let doc_info = format!("{:?}", session.core.document().doc_info);
            let prepared = session.prepare_hwp_action(action, json!({}), None)?;
            assert_eq!(prepared.command, "page.number.format");
            assert_eq!(prepared.params, json!({"format":format}));
            assert_eq!(prepared.mapping, "partial");
            assert_eq!(session.generation, generation);
            assert!(!session.dirty());
            let result = session.execute_hwp_action(action, json!({}), None)?;
            assert_eq!(result["format"], format);
            assert_eq!(result["start"], 7);
            let mut expected = before.clone();
            expected.format = format;
            assert_eq!(
                serde_json::to_value(number(&session)?)?,
                serde_json::to_value(&expected)?
            );
            assert_eq!(session.cursor, cursor);
            assert_eq!(format!("{:?}", session.core.document().doc_info), doc_info);
            if format == before.format {
                assert_eq!(session.generation, generation);
                assert!(!session.can_undo());
            } else {
                session.execute(Command::Undo)?;
                assert_eq!(
                    serde_json::to_value(number(&session)?)?,
                    serde_json::to_value(before)?
                );
                session.execute(Command::Redo)?;
                assert_eq!(
                    serde_json::to_value(number(&session)?)?,
                    serde_json::to_value(expected)?
                );
            }
        }
        Ok(())
    }

    #[test]
    fn missing_number_and_invalid_inputs_preserve_document_and_history() -> Result<()> {
        let mut session = Session::new()?;
        let before = format!("{:?}", session.core.document());
        let info = session.info();
        let generation = session.generation;
        let error = session
            .execute_hwp_action("PageNumberShapeRomanCapital", json!({}), None)
            .err()
            .ok_or_else(|| anyhow!("missing number silently inserted"))?;
        assert!(error.to_string().contains("position"));
        assert!(
            session
                .run("page.number.format", json!({"format":255}))
                .is_err()
        );
        assert!(
            session
                .run(
                    "page.number.format",
                    json!({"format":2,"position":"bottom_center"})
                )
                .is_err()
        );
        assert_eq!(format!("{:?}", session.core.document()), before);
        assert_eq!(session.info(), info);
        assert_eq!(session.generation, generation);
        assert!(!session.can_undo());
        assert!(!session.can_redo());
        Ok(())
    }

    #[test]
    fn column_aliases_keep_gap_separator_direction_type_and_reserved_bits() -> Result<()> {
        for (action, count) in [
            ("MultiColumnOne", 1),
            ("MultiColumnTwo", 2),
            ("MultiColumnThree", 3),
        ] {
            let mut session = columns_fixture()?;
            let original = session.column_target()?.2;
            let before = serde_json::to_value(&original)?;
            let generation = session.generation;
            let prepared = session.prepare_hwp_action(action, json!({}), None)?;
            assert_eq!(prepared.command, "page.columns.count");
            assert_eq!(prepared.params, json!({"count":count}));
            assert_eq!(prepared.mapping, "partial");
            let result = session.execute_hwp_action(action, json!({}), None)?;
            assert_eq!(result["count"], count);
            let mut expected = original.clone();
            expected.column_count = count;
            expected.raw_attr = (expected.raw_attr & !(0xff << 2)) | (count << 2);
            assert_eq!(
                serde_json::to_value(session.column_target()?.2)?,
                serde_json::to_value(&expected)?
            );
            if count == original.column_count {
                assert_eq!(session.generation, generation);
                assert!(!session.can_undo());
            } else {
                session.execute(Command::Undo)?;
                assert_eq!(serde_json::to_value(session.column_target()?.2)?, before);
                session.execute(Command::Redo)?;
                assert_eq!(
                    serde_json::to_value(session.column_target()?.2)?,
                    serde_json::to_value(expected)?
                );
            }
        }
        Ok(())
    }

    #[test]
    fn unsupported_column_redistribution_is_atomic() -> Result<()> {
        for kind in ["unequal", "multiple", "missing", "negative_gap"] {
            let mut session = columns_fixture()?;
            let (paragraph, control, original) = session.column_target()?;
            let section = &mut session.core.document_mut().sections[0];
            match kind {
                "multiple" => section.paragraphs[paragraph]
                    .controls
                    .push(Control::ColumnDef(original)),
                "missing" => {
                    section.paragraphs[paragraph].controls.remove(control);
                }
                _ => {
                    if let Control::ColumnDef(column) =
                        &mut section.paragraphs[paragraph].controls[control]
                    {
                        if kind == "negative_gap" {
                            column.spacing = -1;
                        } else {
                            column.same_width = false;
                            column.widths = vec![10000, 20000];
                            column.gaps = vec![500, 0];
                        }
                    }
                }
            }
            let before = format!("{:?}", session.core.document());
            let generation = session.generation;
            let dirty = session.dirty();
            assert!(
                session
                    .execute_hwp_action("MultiColumnThree", json!({}), None)
                    .is_err(),
                "{kind}"
            );
            assert!(
                session
                    .run("page.columns.count", json!({"count":0}))
                    .is_err()
            );
            assert!(
                session
                    .run("page.columns.count", json!({"count":7}))
                    .is_err()
            );
            assert_eq!(format!("{:?}", session.core.document()), before, "{kind}");
            assert_eq!(session.generation, generation);
            assert_eq!(session.dirty(), dirty);
            assert!(!session.can_undo());
        }
        Ok(())
    }

    #[test]
    fn semantic_aliases_roundtrip_hwp_and_hwpx_with_standard_fields() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut session = number_fixture(false)?;
        session.run("page.columns", json!({"count":2,"spacing_mm":5.5}))?;
        session.execute_hwp_action("PageNumberShapeHangulSyllable", json!({}), None)?;
        session.execute_hwp_action("MultiColumnThree", json!({}), None)?;
        let expected_number = serde_json::to_value(number(&session)?)?;
        let gap = session.column_target()?.2.spacing;
        for extension in ["hwp", "hwpx"] {
            let path = directory.path().join(format!("own-shortcuts.{extension}"));
            session.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            assert_eq!(
                serde_json::to_value(number(&reopened)?)?,
                expected_number,
                "{extension}"
            );
            assert_eq!(reopened.core.document().sections[0].section_def.page_num, 7);
            let column = reopened.column_target()?.2;
            assert_eq!(column.column_count, 3);
            assert_eq!(column.spacing, gap);
            assert!(reopened.render_page_svg(0)?.contains("svg"));
            assert!(reopened.render_pdf(&[0])?.starts_with(b"%PDF-"));
        }
        // HWP supports the extra decoration fields absent from the standard
        // HWPX pageNum element. Verify that our edit does not clear those either.
        let mut session = number_fixture(true)?;
        session.execute_hwp_action("PageNumberShapeRomanCapital", json!({}), None)?;
        let expected = serde_json::to_value(number(&session)?)?;
        let path = directory.path().join("own-decoration.hwp");
        session.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(serde_json::to_value(number(&reopened)?)?, expected);
        Ok(())
    }
}
