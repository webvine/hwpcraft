use super::*;

/// Validate before RHWP's permissive JSON parser can truncate or ignore input.
pub fn validate_format(value: &Value, character: bool) -> Result<()> {
    let fields = value
        .as_object()
        .ok_or_else(|| anyhow!("서식은 JSON 객체여야 합니다."))?;
    for (key, v) in fields {
        let boolean = if character {
            [
                "bold",
                "italic",
                "underline",
                "strikethrough",
                "subscript",
                "superscript",
                "emboss",
                "engrave",
                "kerning",
            ]
            .contains(&key.as_str())
        } else {
            [
                "widowOrphan",
                "keepWithNext",
                "keepLines",
                "pageBreakBefore",
                "fontLineHeight",
                "singleLine",
                "autoSpaceKrEn",
                "autoSpaceKrNum",
                "tabAutoLeft",
                "tabAutoRight",
                "borderConnect",
                "borderIgnoreMargin",
            ]
            .contains(&key.as_str())
        };
        if boolean {
            if !v.is_boolean() {
                bail!("{key}: 참/거짓 값이 필요합니다.");
            }
            continue;
        }
        if character && key == "fontFamily" {
            if !v.as_str().is_some_and(|s| {
                !s.trim().is_empty() && s.len() <= 200 && !s.chars().any(char::is_control)
            }) {
                bail!("올바른 글꼴 이름을 입력하세요.");
            }
            continue;
        }
        if character && key == "fontFamilies" {
            let names = v
                .as_array()
                .filter(|a| a.len() == 7)
                .ok_or_else(|| anyhow!("언어별 글꼴은 7개가 필요합니다."))?;
            for name in names {
                validate_format(&json!({"fontFamily":name}), true)?;
            }
            continue;
        }
        if character
            && [
                "textColor",
                "shadeColor",
                "underlineColor",
                "strikeColor",
                "shadowColor",
            ]
            .contains(&key.as_str())
            || ["fillColor", "patternColor"].contains(&key.as_str())
        {
            color(v, key)?;
            continue;
        }
        if BORDER_KEYS.contains(&key.as_str()) {
            border_line(v, key)?;
            continue;
        }
        if !character && key == "tabStops" {
            tab_stops(v)?;
            continue;
        }
        if !character && key == "borderSpacing" {
            let a = v.as_array().filter(|a| a.len() == 4).ok_or_else(|| {
                anyhow!("borderSpacing: 왼쪽·오른쪽·위·아래 4개 값이 필요합니다.")
            })?;
            for x in a {
                number(x, key, 0, 30000)?;
            }
            continue;
        }
        let choices: &[&str] = match (character, key.as_str()) {
            (true, "underlineType") => &["None", "Bottom", "Top"],
            (false, "alignment") => &[
                "left",
                "center",
                "right",
                "justify",
                "distribute",
                "split",
                "division",
            ],
            (false, "lineSpacingType") => &["Percent", "Fixed", "SpaceOnly", "Minimum"],
            (false, "headType") => &["None", "Outline", "Number", "Bullet"],
            (_, "fillType") => &["none", "solid"],
            _ => &[],
        };
        if !choices.is_empty() {
            if !v.as_str().is_some_and(|s| choices.contains(&s)) {
                bail!("{key}: 지원하지 않는 설정입니다.");
            }
            continue;
        }
        let range = match (character, key.as_str()) {
            (true, "fontSize") => Some((100, 409600)),
            (true, "outlineType") => Some((0, 6)),
            (true, "shadowType") => Some((0, 2)),
            (true, "shadowOffsetX" | "shadowOffsetY") => Some((-100, 100)),
            (true, "emphasisDot") => Some((0, 7)),
            (true, "underlineShape" | "strikeShape") => Some((0, 17)),
            (false, "marginLeft" | "marginRight" | "indent") => Some((-100000, 100000)),
            (false, "spacingBefore" | "spacingAfter" | "lineSpacing") => Some((0, 200000)),
            (false, "verticalAlign" | "englishBreakUnit") => Some((0, 3)),
            (false, "koreanBreakUnit") => Some((0, 1)),
            (false, "paraLevel") => Some((0, 6)),
            (false, "numberingId") => Some((0, 65535)),
            (_, "patternType") => Some((-1, 6)),
            _ => None,
        };
        if let Some((min, max)) = range {
            number(v, key, min, max)?;
            continue;
        }
        let array_range = match (character, key.as_str()) {
            // Hangul limits: 장평 50–200 %, 자간 −50–50 %, 상대 크기 10–250 %, 위치 ±100 %.
            (true, "ratios") => Some((50, 200)),
            (true, "relativeSizes") => Some((10, 250)),
            (true, "spacings") => Some((-50, 50)),
            (true, "charOffsets") => Some((-100, 100)),
            _ => None,
        };
        if let Some((min, max)) = array_range {
            let a = v
                .as_array()
                .filter(|a| a.len() == 7)
                .ok_or_else(|| anyhow!("{key}: 7개 언어별 값이 필요합니다."))?;
            for x in a {
                number(x, key, min, max)?;
            }
            continue;
        }
        bail!("지원하지 않는 서식 항목: {key}");
    }
    if character && value["subscript"] == true && value["superscript"] == true {
        bail!("위 첨자와 아래 첨자를 동시에 설정할 수 없습니다.");
    }
    Ok(())
}
/// Border sides as RHWP's character / paragraph border-fill JSON names them.
const BORDER_KEYS: [&str; 4] = ["borderLeft", "borderRight", "borderTop", "borderBottom"];
/// Keys that make RHWP create a new BorderFill; a partial set would reset the
/// omitted sides, so changed border/fill values are always sent together.
pub(crate) const BORDER_FILL_KEYS: [&str; 8] = [
    "borderLeft",
    "borderRight",
    "borderTop",
    "borderBottom",
    "fillType",
    "fillColor",
    "patternColor",
    "patternType",
];
fn color(v: &Value, key: &str) -> Result<()> {
    if !v.as_str().is_some_and(|s| {
        s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
    }) {
        bail!("{key}: #RRGGBB 색상이 필요합니다.");
    }
    Ok(())
}
/// `{"type":0–17,"width":0–15,"color":"#RRGGBB"}`: HWP line type (표 27) and width index.
fn border_line(v: &Value, key: &str) -> Result<()> {
    let o = v
        .as_object()
        .filter(|o| o.len() == 3)
        .ok_or_else(|| anyhow!("{key}: type·width·color가 필요합니다."))?;
    number(
        o.get("type").unwrap_or(&Value::Null),
        &format!("{key}.type"),
        0,
        17,
    )?;
    number(
        o.get("width").unwrap_or(&Value::Null),
        &format!("{key}.width"),
        0,
        15,
    )?;
    color(
        o.get("color").unwrap_or(&Value::Null),
        &format!("{key}.color"),
    )
}
/// Sorted, unique stops. Positions use the same HWP units the paragraph query returns.
fn tab_stops(v: &Value) -> Result<()> {
    let stops = v
        .as_array()
        .filter(|a| a.len() <= 64)
        .ok_or_else(|| anyhow!("tabStops: 탭은 64개 이하의 배열이어야 합니다."))?;
    let mut previous = -1;
    for stop in stops {
        let o = stop
            .as_object()
            .filter(|o| o.len() == 3)
            .ok_or_else(|| anyhow!("tabStops: position·type·fill이 필요합니다."))?;
        let position = o.get("position").unwrap_or(&Value::Null);
        number(position, "tabStops.position", 0, 2_000_000)?;
        number(o.get("type").unwrap_or(&Value::Null), "tabStops.type", 0, 3)?;
        number(
            o.get("fill").unwrap_or(&Value::Null),
            "tabStops.fill",
            0,
            11,
        )?;
        let position = position.as_f64().unwrap_or(0.0) as i64;
        if position <= previous {
            bail!("tabStops: 탭 위치는 겹치지 않게 오름차순이어야 합니다.");
        }
        previous = position;
    }
    Ok(())
}
fn number(v: &Value, key: &str, min: i64, max: i64) -> Result<()> {
    if !v
        .as_f64()
        .is_some_and(|n| n.is_finite() && n.fract() == 0.0 && n >= min as f64 && n <= max as f64)
    {
        bail!("{key}: {min}–{max} 범위의 정수가 필요합니다.");
    }
    Ok(())
}

impl Session {
    pub(super) fn character_parameters(&mut self, value: Value) -> Result<Value> {
        validate_format(&value, true)?;
        let current = BORDER_FILL_KEYS
            .iter()
            .any(|k| value.get(*k).is_some())
            .then(|| self.character_properties())
            .transpose()?;
        self.character_parameters_from(value, current)
    }
    /// `character_parameters` with the formatting that fills omitted border/fill keys
    /// supplied by the caller (needed when the target is not the caret paragraph).
    pub(crate) fn character_parameters_from(
        &mut self,
        mut value: Value,
        current: Option<Value>,
    ) -> Result<Value> {
        validate_format(&value, true)?;
        normalize_numbers(&mut value);
        complete_border_fill(&mut value, || {
            current.ok_or_else(|| anyhow!("현재 글자 모양을 읽지 못했습니다."))
        })?;
        if value["superscript"] == true {
            value["subscript"] = json!(false);
        }
        if value["subscript"] == true {
            value["superscript"] = json!(false);
        }
        let names = if let Some(name) = value["fontFamily"].as_str() {
            Some(vec![name.to_owned(); 7])
        } else {
            value["fontFamilies"].as_array().map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
        };
        if let Some(names) = names {
            let mut ids = Vec::new();
            for (language, name) in names.iter().enumerate() {
                let id = self.core.find_or_create_font_id_for_lang(language, name);
                ids.push(u16::try_from(id).map_err(|_| anyhow!("글꼴을 등록할 수 없습니다."))?);
            }
            value["fontIds"] = json!(ids);
        }
        Ok(value)
    }
    /// Read/write format values use the same raw units. RHWP's UI query uses px,
    /// but paragraph margins/indent use 200 units/pt and spacing uses 100 units/pt.
    pub fn paragraph_format(&self) -> Result<Value> {
        let mut value = self.paragraph_properties()?;
        let para = self.paragraph(self.cursor)?;
        let shape = self
            .core
            .document()
            .doc_info
            .para_shapes
            .get(para.para_shape_id as usize)
            .ok_or_else(|| anyhow!("문단 서식을 찾을 수 없습니다."))?;
        for (key, raw) in [
            ("marginLeft", shape.margin_left),
            ("marginRight", shape.margin_right),
            ("indent", shape.indent),
            ("spacingBefore", shape.spacing_before),
            ("spacingAfter", shape.spacing_after),
            ("lineSpacing", shape.line_spacing),
        ] {
            value[key] = json!(raw);
        }
        Ok(value)
    }
}

impl Session {
    /// Paragraph counterpart of `character_parameters`: validated, integer-normalized
    /// and with a complete border/fill set.
    pub(crate) fn paragraph_parameters(&self, value: Value) -> Result<Value> {
        self.paragraph_parameters_from(value, || self.paragraph_properties())
    }
    /// `paragraph_parameters` with the current formatting supplied by the caller.
    pub(crate) fn paragraph_parameters_from(
        &self,
        mut value: Value,
        current: impl FnOnce() -> Result<Value>,
    ) -> Result<Value> {
        validate_format(&value, false)?;
        normalize_numbers(&mut value);
        complete_border_fill(&mut value, current)?;
        Ok(value)
    }
}

/// RHWP builds a BorderFill from scratch, so an omitted side would become a default
/// solid line. Fill every missing border/fill key from the current formatting.
fn complete_border_fill(value: &mut Value, current: impl FnOnce() -> Result<Value>) -> Result<()> {
    if !BORDER_FILL_KEYS.iter().any(|k| value.get(*k).is_some()) {
        return Ok(());
    }
    let current = current()?;
    for key in BORDER_FILL_KEYS {
        if value.get(key).is_none()
            && let Some(v) = current.get(key)
        {
            value[key] = v.clone();
        }
    }
    normalize_numbers(value);
    Ok(())
}

// RHWP's small JSON array parser accepts integer tokens only (100, not 100.0).
pub(super) fn normalize_numbers(value: &mut Value) {
    match value {
        Value::Number(n) if n.is_f64() => {
            if let Some(n) = n.as_f64() {
                *value = json!(n as i64);
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_numbers(item);
            }
        }
        Value::Object(fields) => {
            for item in fields.values_mut() {
                normalize_numbers(item);
            }
        }
        _ => {}
    }
}
