//! Owner: shell/page area. Moving an object treated as a character (글자처럼 취급) to another
//! place in the body text, as Hangul's drag of an inline object does: the control leaves its
//! 8-unit slot and opens one in front of the target character, in one undo step. The text
//! itself is not touched (a space HwpCraft reserved after an inserted object stays).
//!
//! Field and memo boundaries follow RHWP patch 0019: at a field's start the object goes in
//! behind the field-begin slot (inside the field), at a field's end it stays outside.
use super::*;
use crate::control_slots::{insert_control, remove_control};
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[(
    "object.move_inline",
    "글자처럼 취급한 개체를 다른 글자 자리로 옮기기 (끌기)",
    "",
)];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[(
    "object.move_inline",
    "address: 글자처럼 취급한 본문 개체(그림·도형·글상자·수식·표), to: 그 개체를 앞에 둘 본문 캐럿 위치({section, paragraph, offset}, 캐럿처럼 유니코드 스칼라). 글은 그대로이며 한 번 실행 취소. 메모·양식 필드 시작에서는 필드 안, 끝에서는 바깥에 둡니다(RHWP 0019 규칙). 개체의 새 주소를 돌려줍니다.",
    r#"{"address":{"section":0,"paragraph":0,"control":2},"to":{"section":0,"paragraph":1,"offset":3}}"#,
)];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveInline {
    address: ObjectAddress,
    to: Cursor,
}

impl Session {
    pub(crate) fn run_inline_moves_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        (id == "object.move_inline").then(|| {
            let p: MoveInline = serde_json::from_value(params.clone())?;
            let address = self.move_inline_object(p.address, p.to)?;
            Ok(json!({ "address": address }))
        })
    }

    /// Move inline object `a` in front of the body position `to`; returns its new address.
    pub fn move_inline_object(&mut self, a: ObjectAddress, to: Cursor) -> Result<ObjectAddress> {
        if to.cell.is_some() {
            bail!("글자처럼 취급한 개체는 본문 글자 자리로만 옮길 수 있습니다.");
        }
        let inline = match self.object(a)? {
            Control::Table(t) => t.common.treat_as_char,
            Control::Picture(_) | Control::Shape(_) | Control::Equation(_) => {
                self.object_properties(a)?["props"]["treatAsChar"] == true
            }
            _ => bail!("그림·도형·글상자·수식·표만 옮길 수 있습니다."),
        };
        if !inline {
            bail!("글자처럼 취급한 개체가 아닙니다. 떠 있는 개체는 위치를 옮기세요.");
        }
        self.guard_object_lock(a)?;
        let target = self.paragraph(to)?;
        let len = target.text.chars().count();
        if to.offset > len {
            bail!("옮길 자리가 문단 범위를 벗어났습니다.");
        }
        let source = self.paragraph(Cursor {
            section: a.section,
            paragraph: a.paragraph,
            offset: 0,
            cell: None,
        })?;
        let from = source
            .control_text_positions()
            .get(a.control)
            .copied()
            .unwrap_or(0);
        let same = (a.section, a.paragraph) == (to.section, to.paragraph);
        if same && from == to.offset {
            return Ok(a);
        }
        let mut moved = a;
        self.transaction(|s| {
            let doc = s.core.document_mut();
            let (control, record) = remove_control(
                &mut doc.sections[a.section].paragraphs[a.paragraph],
                a.control,
            );
            doc.sections[a.section].raw_stream = None;
            let section = &mut doc.sections[to.section];
            section.raw_stream = None;
            let ci = insert_control(
                &mut section.paragraphs[to.paragraph],
                to.offset,
                control,
                record,
            );
            moved = ObjectAddress {
                section: to.section,
                paragraph: to.paragraph,
                control: ci,
            };
            s.cursor = Cursor {
                offset: to.offset,
                ..to
            };
            s.anchor = None;
            s.refresh_layout()
        })?;
        Ok(moved)
    }
}
