//! Picture effects: shadow, glow (네온), soft edge and reflection.
//!
//! Values are stored as OWPML writes them (HWPUNIT lengths, 0–1 transparency);
//! commands speak millimetres, 0–100 % transparency, degrees and `#RRGGBB`.
//! HWP5 picture-effect fields are not written yet, so an HWP save refuses a
//! document whose pictures carry modelled effects instead of dropping them.

use anyhow::{Result, anyhow, bail, ensure};
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::document::Document;
use hwpcraft_doc::model::image::{
    EffectColor, EffectPoint, EffectRange, EffectRgb, PictureEffects, PictureGlow,
    PictureReflection, PictureShadow, PictureSoftEdge,
};
use hwpcraft_doc::model::paragraph::Paragraph;
use hwpcraft_doc::model::shape::ShapeObject;
use serde_json::{Map, Value, json};
use std::path::Path;

use crate::{ObjectAddress, Session, err};

const HU_PER_MM: f64 = 7200.0 / 25.4;

fn mm_to_hu(mm: f64) -> String {
    format!("{}", (mm * HU_PER_MM).round())
}

fn hu_to_mm(hu: Option<&str>) -> f64 {
    let hu = hu.and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0);
    (hu / HU_PER_MM * 100.0).round() / 100.0
}

/// 0–100 % transparency ⇄ OWPML alpha 0–1.
fn percent_to_alpha(percent: f64) -> String {
    let alpha = (percent / 100.0).clamp(0.0, 1.0);
    format!("{}", (alpha * 1000.0).round() / 1000.0)
}

fn alpha_to_percent(alpha: Option<&str>, default: f64) -> f64 {
    alpha
        .and_then(|v| v.trim().parse::<f64>().ok())
        .map_or(default, |a| (a.clamp(0.0, 1.0) * 1000.0).round() / 10.0)
}

fn color_value(color: Option<&EffectColor>) -> String {
    let channel = |v: Option<&String>| {
        v.and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(0)
            .min(255)
    };
    color
        .and_then(|c| c.rgb.as_ref())
        .map_or("#000000".into(), |rgb| {
            format!(
                "#{:02X}{:02X}{:02X}",
                channel(rgb.r.as_ref()),
                channel(rgb.g.as_ref()),
                channel(rgb.b.as_ref())
            )
        })
}

fn parse_color(value: &Value, key: &str) -> Result<EffectColor> {
    let text = value
        .as_str()
        .ok_or_else(|| anyhow!("{key}: \"#RRGGBB\" 색이 필요합니다."))?;
    let hex = text.strip_prefix('#').unwrap_or(text);
    ensure!(
        hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()),
        "{key}: \"#RRGGBB\" 색이 필요합니다."
    );
    let part = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map(|v| v.to_string());
    Ok(EffectColor {
        color_type: Some("RGB".into()),
        scheme_idx: Some("-1".into()),
        system_idx: Some("-1".into()),
        preset_idx: Some("-1".into()),
        rgb: Some(EffectRgb {
            r: Some(part(0)?),
            g: Some(part(2)?),
            b: Some(part(4)?),
        }),
        transforms: Vec::new(),
    })
}

/// Read a number field, check its range and return it (or `default`).
fn number(map: &Map<String, Value>, key: &str, range: (f64, f64), default: f64) -> Result<f64> {
    let Some(value) = map.get(key) else {
        return Ok(default);
    };
    let v = value
        .as_f64()
        .ok_or_else(|| anyhow!("{key}: 숫자가 필요합니다."))?;
    ensure!(
        v.is_finite() && (range.0..=range.1).contains(&v),
        "{key}: {}–{} 범위여야 합니다.",
        range.0,
        range.1
    );
    Ok(v)
}

fn fields<'a>(value: &'a Value, effect: &str, allowed: &[&str]) -> Result<&'a Map<String, Value>> {
    let map = value
        .as_object()
        .ok_or_else(|| anyhow!("{effect}: 객체 또는 null이 필요합니다."))?;
    if let Some(key) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
        bail!("{effect}: 알 수 없는 항목 {key}");
    }
    Ok(map)
}

fn identity_point() -> EffectPoint {
    EffectPoint {
        x: Some("0".into()),
        y: Some("0".into()),
    }
}

fn unit_scale() -> EffectPoint {
    EffectPoint {
        x: Some("1".into()),
        y: Some("1".into()),
    }
}

fn shadow_from(value: &Value) -> Result<PictureShadow> {
    let m = fields(
        value,
        "shadow",
        &[
            "style",
            "transparency",
            "blur_mm",
            "distance_mm",
            "direction",
            "color",
        ],
    )?;
    let style = match m.get("style").and_then(Value::as_str).unwrap_or("outside") {
        "outside" => "OUTSIDE",
        "inside" => "INSIDE",
        _ => bail!("shadow.style은 outside 또는 inside입니다."),
    };
    let color = match m.get("color") {
        Some(c) => parse_color(c, "shadow.color")?,
        None => parse_color(&json!("#000000"), "shadow.color")?,
    };
    Ok(PictureShadow {
        style: Some(style.into()),
        alpha: Some(percent_to_alpha(number(
            m,
            "transparency",
            (0.0, 100.0),
            50.0,
        )?)),
        radius: Some(mm_to_hu(number(m, "blur_mm", (0.0, 50.0), 1.0)?)),
        direction: Some(format!(
            "{}",
            number(m, "direction", (0.0, 360.0), 45.0)?.round()
        )),
        distance: Some(mm_to_hu(number(m, "distance_mm", (0.0, 50.0), 2.0)?)),
        align_style: Some("CENTER".into()),
        rotation_style: Some("0".into()),
        skew: Some(identity_point()),
        scale: Some(unit_scale()),
        color: Some(color),
    })
}

fn glow_from(value: &Value) -> Result<PictureGlow> {
    let m = fields(value, "glow", &["transparency", "size_mm", "color"])?;
    let color = match m.get("color") {
        Some(c) => parse_color(c, "glow.color")?,
        None => parse_color(&json!("#FFC000"), "glow.color")?,
    };
    Ok(PictureGlow {
        alpha: Some(percent_to_alpha(number(
            m,
            "transparency",
            (0.0, 100.0),
            60.0,
        )?)),
        radius: Some(mm_to_hu(number(m, "size_mm", (0.0, 50.0), 2.0)?)),
        color: Some(color),
    })
}

fn soft_edge_from(value: &Value) -> Result<PictureSoftEdge> {
    let m = fields(value, "soft_edge", &["size_mm"])?;
    Ok(PictureSoftEdge {
        radius: Some(mm_to_hu(number(m, "size_mm", (0.0, 50.0), 1.5)?)),
    })
}

fn reflection_from(value: &Value) -> Result<PictureReflection> {
    let m = fields(
        value,
        "reflection",
        &[
            "transparency_start",
            "transparency_end",
            "size",
            "distance_mm",
            "blur_mm",
        ],
    )?;
    Ok(PictureReflection {
        align_style: Some("BOTTOM_LEFT".into()),
        radius: Some(mm_to_hu(number(m, "blur_mm", (0.0, 50.0), 0.5)?)),
        direction: Some("90".into()),
        distance: Some(mm_to_hu(number(m, "distance_mm", (0.0, 50.0), 0.0)?)),
        rotation_style: Some("0".into()),
        fade_direction: Some("90".into()),
        skew: Some(identity_point()),
        scale: Some(EffectPoint {
            x: Some("1".into()),
            y: Some("-1".into()),
        }),
        alpha: Some(EffectRange {
            start: Some(percent_to_alpha(number(
                m,
                "transparency_start",
                (0.0, 100.0),
                50.0,
            )?)),
            end: Some(percent_to_alpha(number(
                m,
                "transparency_end",
                (0.0, 100.0),
                100.0,
            )?)),
        }),
        pos: Some(EffectRange {
            start: Some("0".into()),
            end: Some(format!(
                "{}",
                number(m, "size", (1.0, 100.0), 50.0)?.round()
            )),
        }),
    })
}

/// The command-shaped view of a picture's effects (`null` when absent).
pub(crate) fn effects_json(effects: &PictureEffects) -> Value {
    json!({
        "shadow": effects.shadow.as_ref().map(|s| json!({
            "style": if s.style.as_deref() == Some("INSIDE") { "inside" } else { "outside" },
            "transparency": alpha_to_percent(s.alpha.as_deref(), 0.0),
            "blur_mm": hu_to_mm(s.radius.as_deref()),
            "distance_mm": hu_to_mm(s.distance.as_deref()),
            "direction": s.direction.as_deref().and_then(|d| d.trim().parse::<f64>().ok()).unwrap_or(0.0),
            "color": color_value(s.color.as_ref()),
        })),
        "glow": effects.glow.as_ref().map(|g| json!({
            "transparency": alpha_to_percent(g.alpha.as_deref(), 0.0),
            "size_mm": hu_to_mm(g.radius.as_deref()),
            "color": color_value(g.color.as_ref()),
        })),
        "soft_edge": effects.soft_edge.as_ref().map(|e| json!({
            "size_mm": hu_to_mm(e.radius.as_deref()),
        })),
        "reflection": effects.reflection.as_ref().map(|r| json!({
            "transparency_start": alpha_to_percent(r.alpha.as_ref().and_then(|a| a.start.as_deref()), 50.0),
            "transparency_end": alpha_to_percent(r.alpha.as_ref().and_then(|a| a.end.as_deref()), 100.0),
            "size": r.pos.as_ref().and_then(|p| p.end.as_deref()).and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(100.0),
            "distance_mm": hu_to_mm(r.distance.as_deref()),
            "blur_mm": hu_to_mm(r.radius.as_deref()),
        })),
    })
}

/// Apply a `picture.effects` request to `effects`: a present key sets the
/// effect, `null` removes it, an absent key keeps it; `clear` removes all first.
fn apply_request(effects: &mut PictureEffects, params: &Map<String, Value>) -> Result<()> {
    if params.get("clear").and_then(Value::as_bool) == Some(true) {
        *effects = PictureEffects::default();
    }
    if let Some(v) = params.get("shadow") {
        effects.shadow = (!v.is_null()).then(|| shadow_from(v)).transpose()?;
    }
    if let Some(v) = params.get("glow") {
        effects.glow = (!v.is_null()).then(|| glow_from(v)).transpose()?;
    }
    if let Some(v) = params.get("soft_edge") {
        effects.soft_edge = (!v.is_null()).then(|| soft_edge_from(v)).transpose()?;
    }
    if let Some(v) = params.get("reflection") {
        effects.reflection = (!v.is_null()).then(|| reflection_from(v)).transpose()?;
    }
    Ok(())
}

pub const PICTURE_EFFECTS_REFUSED: &str = "그림 효과(그림자·네온·부드러운 가장자리·반사)는 아직 HWP(.hwp)로 저장할 수 없습니다. HWPX로 저장하거나, 효과를 빼고 저장하세요(MCP save_document dropPictureEffects:true, CLI --drop-picture-effects, 데스크톱 확인 창).";

fn effect_count(e: &PictureEffects) -> usize {
    usize::from(e.shadow.is_some())
        + usize::from(e.glow.is_some())
        + usize::from(e.soft_edge.is_some())
        + usize::from(e.reflection.is_some())
}

/// Modelled effects on every picture (body, table cell, group, text box).
/// Effects of pictures read from .hwp stay raw bytes and are not counted.
pub(crate) fn count_picture_effects(document: &Document) -> usize {
    fn paragraphs(list: &[Paragraph]) -> usize {
        list.iter().flat_map(|p| &p.controls).map(control).sum()
    }
    fn shape(s: &ShapeObject) -> usize {
        match s {
            ShapeObject::Picture(p) => effect_count(&p.effects),
            ShapeObject::Group(g) => g.children.iter().map(shape).sum(),
            other => other
                .drawing()
                .and_then(|d| d.text_box.as_ref())
                .map_or(0, |t| paragraphs(&t.paragraphs)),
        }
    }
    fn control(c: &Control) -> usize {
        match c {
            Control::Picture(p) => effect_count(&p.effects),
            Control::Table(t) => t
                .cells
                .iter()
                .map(|cell| paragraphs(&cell.paragraphs))
                .sum(),
            Control::Shape(s) => shape(s),
            _ => 0,
        }
    }
    document
        .sections
        .iter()
        .map(|section| paragraphs(&section.paragraphs))
        .sum()
}

/// Remove every modelled picture effect; returns how many were removed.
fn strip_picture_effects(document: &mut Document) -> usize {
    fn take(e: &mut PictureEffects) -> usize {
        let n = effect_count(e);
        *e = PictureEffects::default();
        n
    }
    fn paragraphs(list: &mut [Paragraph]) -> usize {
        list.iter_mut()
            .flat_map(|p| &mut p.controls)
            .map(control)
            .sum()
    }
    fn shape(s: &mut ShapeObject) -> usize {
        match s {
            ShapeObject::Picture(p) => take(&mut p.effects),
            ShapeObject::Group(g) => g.children.iter_mut().map(shape).sum(),
            other => other
                .drawing_mut()
                .and_then(|d| d.text_box.as_mut())
                .map_or(0, |t| paragraphs(&mut t.paragraphs)),
        }
    }
    fn control(c: &mut Control) -> usize {
        match c {
            Control::Picture(p) => take(&mut p.effects),
            Control::Table(t) => t
                .cells
                .iter_mut()
                .map(|cell| paragraphs(&mut cell.paragraphs))
                .sum(),
            Control::Shape(s) => shape(s),
            _ => 0,
        }
    }
    let mut total = 0;
    for section in &mut document.sections {
        let n = paragraphs(&mut section.paragraphs);
        if n > 0 {
            section.raw_stream = None;
        }
        total += n;
    }
    total
}

fn is_hwp5(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("hwp"))
}

impl Session {
    /// How many picture effects a save to `path` would have to drop: HWP5
    /// picture-effect fields are not written yet, HWPX keeps them.
    pub fn picture_effects_blocking_save(&self, path: &Path) -> usize {
        if is_hwp5(path) {
            count_picture_effects(self.core.document())
        } else {
            0
        }
    }

    /// Refuse an .hwp save that would silently drop picture effects.
    pub(crate) fn refuse_picture_effects_save(&self, path: &Path) -> Result<()> {
        if self.picture_effects_blocking_save(path) > 0 {
            bail!(PICTURE_EFFECTS_REFUSED);
        }
        Ok(())
    }

    /// Save, first removing the picture effects an .hwp file cannot hold (one undo
    /// step); returns how many were removed. A failed save puts them back.
    /// `accept_track_changes` merges tracked changes as `save_accepting_track_changes`.
    pub fn save_dropping_picture_effects(
        &mut self,
        path: &Path,
        accept_track_changes: bool,
    ) -> Result<usize> {
        let save = |s: &mut Self| {
            if accept_track_changes {
                s.save_accepting_track_changes(path)
            } else {
                s.save(path)
            }
        };
        if self.picture_effects_blocking_save(path) == 0 {
            return save(self).map(|()| 0);
        }
        self.guard_protected()?;
        let mut dropped = 0;
        self.transaction(|s| {
            dropped = strip_picture_effects(s.core.document_mut());
            s.refresh_layout()
        })?;
        if let Err(e) = save(self) {
            self.execute(crate::Command::Undo)?;
            return Err(e);
        }
        Ok(dropped)
    }

    fn picture_effects_at(&self, a: ObjectAddress) -> Result<&PictureEffects> {
        match self.object(a)? {
            Control::Picture(p) => Ok(&p.effects),
            _ => bail!("그림이 아닙니다."),
        }
    }

    pub(crate) fn picture_effects(&self, a: ObjectAddress) -> Result<Value> {
        Ok(effects_json(self.picture_effects_at(a)?))
    }

    /// Set, remove or clear a picture's effects as one undo step.
    pub(crate) fn set_picture_effects(
        &mut self,
        a: ObjectAddress,
        params: &Map<String, Value>,
    ) -> Result<()> {
        let mut effects = self.picture_effects_at(a)?.clone();
        apply_request(&mut effects, params)?;
        self.guard_protected()?;
        self.transaction(|s| {
            let section = &mut s.core.document_mut().sections[a.section];
            section.raw_stream = None;
            let Some(Control::Picture(p)) = section
                .paragraphs
                .get_mut(a.paragraph)
                .and_then(|p| p.controls.get_mut(a.control))
            else {
                bail!("그림이 아닙니다.");
            };
            p.effects = effects;
            // Effects change only drawing; this also tells RHWP the paragraph changed.
            s.core
                .relayout_paragraph_native(a.section, a.paragraph, None)
                .map_err(err)?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(effects: &mut PictureEffects, request: Value) -> Result<()> {
        let request = request.as_object().ok_or_else(|| anyhow!("object"))?;
        apply_request(effects, request)
    }

    #[test]
    fn request_values_map_to_owpml_and_back() -> Result<()> {
        let mut effects = PictureEffects::default();
        let request = json!({
            "shadow": {"transparency": 80, "blur_mm": 1.41, "distance_mm": 3.53, "direction": 45, "color": "#102030"},
            "glow": {"size_mm": 2, "color": "#FFC000"},
            "soft_edge": {"size_mm": 1.5},
            "reflection": {"size": 40},
        });
        apply(&mut effects, request)?;
        let shadow = effects.shadow.as_ref().ok_or_else(|| anyhow!("shadow"))?;
        assert_eq!(shadow.alpha.as_deref(), Some("0.8"));
        assert_eq!(shadow.radius.as_deref(), Some("400"));
        assert_eq!(shadow.distance.as_deref(), Some("1001"));
        let view = effects_json(&effects);
        assert_eq!(view["shadow"]["color"], "#102030");
        assert_eq!(view["shadow"]["transparency"], 80.0);
        assert_eq!(view["reflection"]["size"], 40.0);
        apply(&mut effects, json!({"glow": null}))?;
        assert!(effects.glow.is_none() && effects.shadow.is_some());
        apply(&mut effects, json!({"clear": true}))?;
        assert!(effects.is_empty());
        assert!(apply(&mut effects, json!({"shadow": {"bogus": 1}})).is_err());
        assert!(apply(&mut effects, json!({"glow": {"color": "red"}})).is_err());
        Ok(())
    }
}
