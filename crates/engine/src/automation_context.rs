//! Additive headless selection and shared presentation-geometry queries.
//! Legacy selection.set and Cursor serialization retain their existing contracts.
use super::*;
use serde::Deserialize;

pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("selection.range", "문단 사이의 명시적 범위 선택", ""),
    ("page.layout.get", "표시 쪽 정보·개체 배치 조회", ""),
    ("caret.get", "현재 또는 지정한 캐럿의 표시 좌표 조회", ""),
];

pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "selection.range",
        "start/end Cursor, expected_generation 필수. 유니코드 스칼라 오프셋이며 같은 구역 본문 또는 같은 1단계 셀 안이어야 합니다. 선택 방향을 유지합니다.",
        r#"{"start":{"section":0,"paragraph":0,"offset":0},"end":{"section":0,"paragraph":1,"offset":2},"expected_generation":0}"#,
    ),
    (
        "page.layout.get",
        "page: 1부터, includeControls(선택, 기본 false). 화면과 같은 pageInfo와 (선택) controlLayout을 돌려주며 캐럿은 바뀌지 않습니다.",
        r#"{"page":1,"includeControls":true}"#,
    ),
    (
        "caret.get",
        "cursor(선택, 유니코드 스칼라 Cursor). 생략하면 지금 보이는 캐럿. 결과: generation과 배율 없는 쪽 픽셀 좌표(pageIndex는 0부터).",
        r#"{}"#,
    ),
];

// The new endpoints reject unrepresentable paths rather than allowing serde's
// legacy Cursor decoder to discard a caller's deeper context information.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputCell {
    control: usize,
    index: usize,
    paragraph: usize,
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputCursor {
    section: usize,
    paragraph: usize,
    offset: usize,
    #[serde(default)]
    cell: Option<InputCell>,
}
impl From<InputCursor> for Cursor {
    fn from(value: InputCursor) -> Self {
        Self {
            section: value.section,
            paragraph: value.paragraph,
            offset: value.offset,
            cell: value.cell.map(|cell| Cell {
                control: cell.control,
                index: cell.index,
                paragraph: cell.paragraph,
            }),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionRange {
    start: InputCursor,
    end: InputCursor,
    expected_generation: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageLayout {
    page: u32,
    #[serde(default, rename = "includeControls")]
    include_controls: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Caret {
    #[serde(default)]
    cursor: Option<InputCursor>,
}

fn same_container(start: Cursor, end: Cursor) -> bool {
    start.section == end.section
        && match (start.cell, end.cell) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                start.paragraph == end.paragraph && a.control == b.control && a.index == b.index
            }
            _ => false,
        }
}

impl Session {
    pub(crate) fn run_automation_context_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        Some(match id {
            "selection.range" => serde_json::from_value(params.clone())
                .map_err(anyhow::Error::from)
                .and_then(|value| self.select_explicit_range(value)),
            "page.layout.get" => serde_json::from_value(params.clone())
                .map_err(anyhow::Error::from)
                .and_then(|value| self.query_page_layout(value)),
            "caret.get" => serde_json::from_value(params.clone())
                .map_err(anyhow::Error::from)
                .and_then(|value| self.query_caret(value)),
            _ => return None,
        })
    }

    fn checked_automation_cursor(&self, cursor: Cursor) -> Result<Cursor> {
        let paragraph = self.paragraph(cursor)?;
        if cursor.offset > paragraph.text.chars().count() {
            bail!("캐럿 위치가 문단의 Unicode 문자 범위를 벗어났습니다.");
        }
        Ok(cursor)
    }

    fn select_explicit_range(&mut self, request: SelectionRange) -> Result<Value> {
        if request.expected_generation != self.generation {
            bail!("문서가 바뀌었습니다. 현재 generation으로 선택 범위를 다시 확인하세요.");
        }
        let start = self.checked_automation_cursor(request.start.into())?;
        let end = self.checked_automation_cursor(request.end.into())?;
        if !same_container(start, end) {
            bail!("선택 범위는 같은 구역의 본문 또는 같은 1단계 표 셀 안에 있어야 합니다.");
        }
        // Both endpoints and the container are checked before selection clears
        // pending formatting, visual affinity or an existing span.
        self.select(start, false)?;
        self.select(end, start != end)?;
        let mut result = self.selection_json();
        result["generation"] = json!(self.generation);
        result["offsetUnit"] = json!("unicode_scalar");
        Ok(result)
    }

    fn query_page_layout(&self, request: PageLayout) -> Result<Value> {
        let pages = self.page_count()?;
        if request.page == 0 || request.page > pages {
            bail!("page는 1부터 전체 쪽수 사이여야 합니다.");
        }
        let index = request.page - 1;
        let mut result = json!({"generation":self.generation,"page":request.page,
            "pages":pages,"pageInfo":self.page_info(index)?,"unit":"page_px"});
        if request.include_controls {
            result["controlLayout"] = serde_json::from_str(&self.page_control_layout(index)?)?;
        }
        Ok(result)
    }

    fn query_caret(&self, request: Caret) -> Result<Value> {
        let cursor =
            self.checked_automation_cursor(request.cursor.map_or(self.cursor, Cursor::from))?;
        Ok(json!({"generation":self.generation,"cursor":cursor,
            "geometry":self.caret_at(cursor)?,"unit":"page_px","offsetUnit":"unicode_scalar"}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(session: &mut Session, command: &str, params: Value) -> Result<Value> {
        session
            .run_automation_context_ext(command, &params)
            .ok_or_else(|| anyhow!("test command was not handled"))?
    }

    fn state(session: &Session) -> Result<Value> {
        let hint = |hint: Option<VisualHint>| {
            hint.map(|h| {
                json!({
            "cursor":h.cursor,"page":h.page,"generation":h.generation,
            "affinity":format!("{:?}",h.affinity)})
            })
        };
        Ok(json!({"document":format!("{:?}",session.core.document()),
            "selection":session.selection_json(),"generation":session.generation,
            "pending":session.insertion_shape.has_pending(),
            "pendingRevision":session.insertion_shape.revision(),
            "character":session.character_properties()?,
            "visualHint":hint(session.visual_hint.get()),"hitHint":hint(session.hit_hint.get()),
            "undo":session.can_undo(),"redo":session.can_redo()}))
    }

    #[test]
    fn absolute_body_selection_keeps_direction_and_unicode_offsets() -> Result<()> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("😀가a\n둘째🚀\n셋".into()))?;
        let before = format!("{:?}", session.core.document());
        let generation = session.generation;
        let start = json!({"section":0,"paragraph":0,"offset":1});
        let end = json!({"section":0,"paragraph":1,"offset":2});
        let selected = run(
            &mut session,
            "selection.range",
            json!({
            "start":start,"end":end,"expected_generation":generation}),
        )?;
        assert_eq!(selected["text"], "가a\n둘째");
        assert_eq!(selected["cursor"]["paragraph"], 1);
        assert_eq!(selected["spanAnchor"]["offset"], 1);
        let reversed = run(
            &mut session,
            "selection.range",
            json!({
            "start":end,"end":start,"expected_generation":generation}),
        )?;
        assert_eq!(reversed["text"], selected["text"]);
        assert_eq!(reversed["cursor"]["paragraph"], 0);
        assert_eq!(reversed["spanAnchor"]["paragraph"], 1);
        let collapsed = run(
            &mut session,
            "selection.range",
            json!({
            "start":start,"end":start,"expected_generation":generation}),
        )?;
        assert_eq!(collapsed["hasSelection"], false);
        assert!(collapsed["anchor"].is_null());
        assert!(collapsed["spanAnchor"].is_null());
        assert_eq!(session.generation, generation);
        assert_eq!(format!("{:?}", session.core.document()), before);
        Ok(())
    }

    #[test]
    fn range_supports_multiple_paragraphs_inside_one_cell() -> Result<()> {
        let mut session = Session::new()?;
        session.run("table.create", json!({"rows":1,"columns":2}))?;
        session.execute(Command::Insert("😀가\n나🚀".into()))?;
        let cell = session
            .cursor
            .cell
            .ok_or_else(|| anyhow!("missing fixture cell"))?;
        let start = Cursor {
            offset: 1,
            cell: Some(Cell {
                paragraph: 0,
                ..cell
            }),
            ..session.cursor
        };
        let end = Cursor {
            offset: 1,
            cell: Some(Cell {
                paragraph: 1,
                ..cell
            }),
            ..session.cursor
        };
        let generation = session.generation;
        let selected = run(
            &mut session,
            "selection.range",
            json!({
            "start":start,"end":end,"expected_generation":generation}),
        )?;
        assert_eq!(selected["text"], "가\n나");
        let before = state(&session)?;
        let other = Cursor {
            cell: Some(Cell {
                index: 1,
                paragraph: 0,
                ..cell
            }),
            offset: 0,
            ..end
        };
        assert!(
            run(
                &mut session,
                "selection.range",
                json!({
            "start":start,"end":other,"expected_generation":generation})
            )
            .is_err()
        );
        assert_eq!(state(&session)?, before);
        assert!(
            run(
                &mut session,
                "selection.range",
                json!({
            "start":Cursor::default(),"end":end,"expected_generation":generation})
            )
            .is_err()
        );
        assert_eq!(state(&session)?, before);
        Ok(())
    }

    #[test]
    fn invalid_stale_or_unrepresentable_ranges_preserve_all_session_state() -> Result<()> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("😀첫 문단\n둘 문단".into()))?;
        session.select(
            Cursor {
                offset: 1,
                ..Default::default()
            },
            false,
        )?;
        session.execute(Command::Character(json!({"bold":true})))?;
        session.set_visual_hint(session.cursor, 0, Affinity::Downstream);
        let before = state(&session)?;
        let generation = session.generation;
        for params in [
            json!({"start":Cursor::default(),"end":{"section":0,"paragraph":1,"offset":999},"expected_generation":generation}),
            json!({"start":Cursor::default(),"end":Cursor::default(),"expected_generation":generation+1}),
            json!({"start":Cursor::default(),"end":Cursor::default()}),
            json!({"start":{"section":0,"paragraph":0,"offset":0,"path":[{"control":0,"cell":0,"paragraph":0}]},"end":Cursor::default(),"expected_generation":generation}),
        ] {
            assert!(run(&mut session, "selection.range", params).is_err());
            assert_eq!(state(&session)?, before);
        }
        let mut document = session.core.document().clone();
        document.sections.push(document.sections[0].clone());
        session.core.set_document(document);
        session.generation += 1;
        let before = state(&session)?;
        let generation = session.generation;
        assert!(
            run(
                &mut session,
                "selection.range",
                json!({"start":Cursor::default(),
            "end":{"section":1,"paragraph":0,"offset":0},"expected_generation":generation})
            )
            .is_err()
        );
        assert_eq!(state(&session)?, before);
        Ok(())
    }

    #[test]
    fn geometry_queries_use_display_core_and_are_independent_read_operations() -> Result<()> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("현재 ".into()))?;
        session.run("insert.auto_text", json!({"kind":"page_of_total"}))?;
        session.execute(Command::Insert(" 끝".into()))?;
        session.run("page.new_number", json!({"start":12345}))?;
        session.execute(Command::Character(json!({"italic":true})))?;
        session.set_visual_hint(session.cursor, 0, Affinity::Upstream);
        let caret = session.caret()?;
        session.hit_test(
            0,
            caret["x"]
                .as_f64()
                .ok_or_else(|| anyhow!("geometry x must be numeric"))?,
            caret["y"]
                .as_f64()
                .ok_or_else(|| anyhow!("geometry y must be numeric"))?,
        )?;
        let before = state(&session)?;
        let layout = run(
            &mut session,
            "page.layout.get",
            json!({"page":1,"includeControls":true}),
        )?;
        assert_eq!(layout["pageInfo"]["pageNumber"], 12345);
        assert_eq!(layout["pages"], session.page_count()?);
        assert!(layout["controlLayout"].is_object());
        assert_eq!(
            run(&mut session, "caret.get", json!({}))?["geometry"],
            caret
        );
        let a = run(
            &mut session,
            "caret.get",
            json!({"cursor":{"section":0,"paragraph":0,"offset":3}}),
        )?;
        let b = run(
            &mut session,
            "caret.get",
            json!({"cursor":{"section":0,"paragraph":0,"offset":4}}),
        )?;
        assert!(
            b["geometry"]["x"]
                .as_f64()
                .ok_or_else(|| anyhow!("geometry x must be numeric"))?
                - a["geometry"]["x"]
                    .as_f64()
                    .ok_or_else(|| anyhow!("geometry x must be numeric"))?
                > 30.0
        );
        let simple = run(&mut session, "page.layout.get", json!({"page":1}))?;
        assert!(simple.get("controlLayout").is_none());
        assert_eq!(state(&session)?, before);
        for (command, params) in [
            ("page.layout.get", json!({"page":0})),
            ("page.layout.get", json!({"page":2})),
            (
                "caret.get",
                json!({"cursor":{"section":0,"paragraph":0,"offset":999}}),
            ),
            (
                "caret.get",
                json!({"cursor":{"section":0,"paragraph":0,"offset":0,"path":[]}}),
            ),
        ] {
            assert!(run(&mut session, command, params).is_err());
            assert_eq!(state(&session)?, before);
        }
        Ok(())
    }
}
