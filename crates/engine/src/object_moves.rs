//! Owner: shell/page area. Moving several selected objects together (canvas drag with a
//! multiple selection), as one undo step.
use super::*;
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] =
    &[("object.move_by", "개체 여러 개를 함께 옮기기 (끌기)", "")];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[(
    "object.move_by",
    "addresses: 떠 있는 개체 1–64개({section, paragraph, control}; 그림·도형·글상자·수식), dx_mm, dy_mm(±600). 모든 개체를 같은 만큼 옮기고 붙은 연결선을 다시 계산합니다. 한 번 실행 취소. 글자처럼 취급한 개체·보호 개체·표는 옮기기 전에 거절합니다.",
    r#"{"addresses":[{"section":0,"paragraph":0,"control":1},{"section":0,"paragraph":0,"control":2}],"dx_mm":10.0,"dy_mm":-5.0}"#,
)];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveBy {
    addresses: Vec<ObjectAddress>,
    dx_mm: f64,
    dy_mm: f64,
}

impl Session {
    pub(crate) fn run_object_moves_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        (id == "object.move_by").then(|| {
            let p: MoveBy = serde_json::from_value(params.clone())?;
            self.move_objects_by(&p.addresses, p.dx_mm, p.dy_mm)?;
            Ok(self.info())
        })
    }

    fn move_objects_by(
        &mut self,
        addresses: &[ObjectAddress],
        dx_mm: f64,
        dy_mm: f64,
    ) -> Result<()> {
        if addresses.is_empty() || addresses.len() > 64 {
            bail!("옮길 개체는 1–64개입니다.");
        }
        if ![dx_mm, dy_mm]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 600.0)
        {
            bail!("개체 이동은 ±600 mm 이내입니다.");
        }
        let mut seen = std::collections::HashSet::new();
        let mut moves = Vec::with_capacity(addresses.len());
        for a in addresses {
            if !seen.insert((a.section, a.paragraph, a.control)) {
                bail!("같은 개체를 두 번 지정했습니다.");
            }
            let current = self.object_properties(*a)?;
            if current["kind"] == "table" {
                bail!("표는 여러 개체 옮기기에 넣을 수 없습니다.");
            }
            let props = &current["props"];
            if props["treatAsChar"] == true {
                bail!("글자처럼 취급한 개체는 끌어서 옮길 수 없습니다.");
            }
            self.guard_object_lock(*a)?;
            let hu = |mm: f64| (mm * 7200.0 / 25.4).round() as i64;
            moves.push((
                *a,
                json!({
                    "horzOffset": props["horzOffset"].as_i64().unwrap_or(0) + hu(dx_mm),
                    "vertOffset": props["vertOffset"].as_i64().unwrap_or(0) + hu(dy_mm),
                }),
            ));
        }
        // The inner property edits join this one undo step (nested transactions).
        self.transaction(|s| {
            moves
                .iter()
                .try_for_each(|(a, props)| s.set_object_properties(*a, props))
        })
    }
}
