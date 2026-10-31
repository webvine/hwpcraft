//! Explicit adapters for public automation ParameterSets. Native names and
//! HwpCraft JSON stay separate; unsupported fields fail before any edit.
use super::*;
use serde::Deserialize;
use serde_json::Map;

pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("hwp.parameters.apply", "공개 ParameterSet 인자 적용", ""),
    ("hwp.parameters.schema", "공개 인자 어댑터 지원 범위", ""),
];
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "hwp.parameters.apply",
        "id: CharShape|ParagraphShape|TableCreate|InsertText|PageSetup|PageMarginSetup|AllReplace|ExecReplace|RepeatFind|FindDlg|FindAll, parameterSet: 네이티브 필드 JSON(PageSetup은 {\"PageDef\":{…},\"ApplyTo\":3}처럼 하위 묶음). 순수 편집만 하며 입출력 액션은 거절합니다. 지원하지 않는 필드는 편집 전에 실패합니다.",
        r#"{"id":"CharShape","parameterSet":{"Height":1600,"Bold":1}}"#,
    ),
    (
        "hwp.parameters.schema",
        "id: CharShape|ParagraphShape|TableCreate|InsertText|PageSetup|PageMarginSetup|AllReplace|ExecReplace|RepeatFind|FindDlg|FindAll. 지원 필드와 한계를 알려 줍니다.",
        r#"{"id":"ParagraphShape"}"#,
    ),
];

const LANGUAGES: [&str; 7] = [
    "Hangul", "Latin", "Hanja", "Japanese", "Other", "Symbol", "User",
];
/// 찾기 / 바꾸기 actions taking the FindReplace set.
const FIND_ACTIONS: [&str; 5] = [
    "AllReplace",
    "ExecReplace",
    "RepeatFind",
    "FindDlg",
    "FindAll",
];
/// PageDef lengths (HWPUNIT) and the `page.setup` item each sets (mm).
const PAGE_DEF: [(&str, &str); 9] = [
    ("PaperWidth", "width_mm"),
    ("PaperHeight", "height_mm"),
    ("LeftMargin", "left_mm"),
    ("RightMargin", "right_mm"),
    ("TopMargin", "top_mm"),
    ("BottomMargin", "bottom_mm"),
    ("HeaderLen", "header_mm"),
    ("FooterLen", "footer_mm"),
    ("GutterLen", "gutter_mm"),
];

pub struct PreparedHwpAction {
    pub command: &'static str,
    pub params: Value,
    pub mapping: &'static str,
}

fn integer(value: &Value, field: &str) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| anyhow!("{field}: 정수 값이 필요합니다."))
}
fn flag(value: &Value, field: &str) -> Result<Value> {
    match value {
        Value::Bool(value) => Ok(json!(value)),
        value if value.as_i64() == Some(0) => Ok(json!(false)),
        value if value.as_i64() == Some(1) => Ok(json!(true)),
        _ => bail!("{field}: 0 또는 1이 필요합니다."),
    }
}
fn colorref(value: &Value, field: &str) -> Result<Value> {
    let color = integer(value, field)?;
    if !(0..=0xff_ffff).contains(&color) {
        bail!("{field}: 24비트 COLORREF 값이 필요합니다.");
    }
    Ok(json!(format!(
        "#{:02x}{:02x}{:02x}",
        color & 255,
        (color >> 8) & 255,
        (color >> 16) & 255
    )))
}
fn choice(value: &Value, field: &str, choices: &[&str]) -> Result<Value> {
    let i = integer(value, field)?;
    let value = usize::try_from(i)
        .ok()
        .and_then(|i| choices.get(i))
        .ok_or_else(|| anyhow!("{field}: 지원하지 않는 열거 값 {i}"))?;
    Ok(json!(value))
}
fn string(value: &Value, field: &str) -> Result<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("{field}: 문자열이 필요합니다."))
}
fn absolute_urc(value: &Value, field: &str) -> Result<Value> {
    let value = integer(value, field)?;
    if value & 1 != 0 {
        bail!(
            "{field}: 상대 글자 단위 URC는 아직 지원하지 않습니다. 절대 HWPUNIT 값의 2배인 짝수 URC를 지정하세요."
        );
    }
    // Match native ParagraphShape values and the stored paragraph shape units.
    Ok(json!(value))
}

/// Discover the implemented native fields. This is an explicit subset, not an
/// assertion that the entire COM ParameterSet ABI is implemented.
pub fn hwp_parameter_schema(action: &str) -> Option<Value> {
    let (set, fields): (&str, Vec<String>) = match action {
        "CharShape" => {
            let mut fields = [
                "Height",
                "Bold",
                "Italic",
                "Emboss",
                "Engrave",
                "SuperScript",
                "SubScript",
                "UseKerning",
                "TextColor",
                "ShadeColor",
                "UnderlineColor",
                "ShadowColor",
                "StrikeOutColor",
                "UnderlineType",
                "UnderlineShape",
                "OutlineType",
                "ShadowType",
                "ShadowOffsetX",
                "ShadowOffsetY",
                "DiacSymMark",
                "StrikeOutShape",
                "StrikeOutType",
            ]
            .map(str::to_owned)
            .to_vec();
            for prefix in ["FaceName", "Ratio", "Spacing", "Size", "Offset"] {
                fields.extend(
                    LANGUAGES
                        .iter()
                        .map(|language| format!("{prefix}{language}")),
                );
            }
            ("CharShape", fields)
        }
        "ParagraphShape" => (
            "ParaShape",
            [
                "LeftMargin",
                "RightMargin",
                "Indentation",
                "PrevSpacing",
                "NextSpacing",
                "LineSpacingType",
                "LineSpacing",
                "AlignType",
                "BreakLatinWord",
                "BreakNonLatinWord",
                "WidowOrphan",
                "KeepWithNext",
                "KeepLinesTogether",
                "PagebreakBefore",
                "TextAlignment",
                "FontLineHeight",
                "Level",
                "BorderConnect",
                "BorderText",
                "BorderOffsetLeft",
                "BorderOffsetRight",
                "BorderOffsetTop",
                "BorderOffsetBottom",
                "AutoSpaceEAsianEng",
                "AutoSpaceEAsianNum",
            ]
            .map(str::to_owned)
            .to_vec(),
        ),
        "TableCreate" => (
            "TableCreation",
            ["Rows", "Cols", "ColWidth"].map(str::to_owned).to_vec(),
        ),
        "InsertText" => ("InsertText", vec!["Text".to_owned()]),
        "PageSetup" | "PageMarginSetup" => (
            "SecDef",
            PAGE_DEF
                .iter()
                .map(|(field, _)| format!("PageDef.{field}"))
                .chain(
                    [
                        "PageDef.Landscape",
                        "PageDef.GutterType",
                        "ApplyTo",
                        "ApplyClass",
                    ]
                    .map(str::to_owned),
                )
                .collect(),
        ),
        id if FIND_ACTIONS.contains(&id) => (
            "FindReplace",
            [
                "FindString",
                "ReplaceString",
                "Direction",
                "MatchCase",
                "WholeWordOnly",
                "UseWildCards",
                "SeveralWords",
                "FindRegExp",
                "HanjaFromHangul",
                "FindCharShape.Bold",
                "FindCharShape.Italic",
                "FindCharShape.UnderlineType",
                "FindCharShape.Height",
                "IgnoreMessage",
                "ReplaceMode",
                "FindType",
            ]
            .map(str::to_owned)
            .to_vec(),
        ),
        _ => return None,
    };
    Some(
        json!({"name":set,"fields":fields,"complete":false,"unsupportedFields":"rejected before execution","units":"COLORREF=0x00BBGGRR; absolute URC has bit0=0; relative URC is rejected"}),
    )
}

impl Session {
    pub(crate) fn run_hwp_parameter_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        match id {
            "hwp.parameters.schema" => Some((|| {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Query {
                    id: String,
                }
                let p: Query = serde_json::from_value(params.clone())?;
                hwp_parameter_schema(&p.id)
                    .ok_or_else(|| anyhow!("{}: 공개 인자 어댑터가 아직 없습니다.", p.id))
            })()),
            "hwp.parameters.apply" => Some((|| {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Apply {
                    id: String,
                    #[serde(rename = "parameterSet")]
                    parameter_set: Value,
                }
                let p: Apply = serde_json::from_value(params.clone())?;
                if hwp_parameter_schema(&p.id).is_none() {
                    bail!(
                        "{}: 이 명령은 문서 편집용 공개 인자 어댑터만 실행합니다.",
                        p.id
                    );
                }
                if p.id == "CharShape" {
                    let native = p
                        .parameter_set
                        .as_object()
                        .ok_or_else(|| anyhow!("parameterSet은 JSON 객체여야 합니다."))?;
                    return self.apply_native_character(native);
                }
                self.execute_hwp_action(&p.id, json!({}), Some(p.parameter_set))
            })()),
            _ => None,
        }
    }
    /// Resolve and validate an action without modifying the session. Callers at
    /// an IO boundary still execute through their own path-checked dispatcher.
    pub fn prepare_hwp_action(
        &self,
        id: &str,
        params: Value,
        parameter_set: Option<Value>,
    ) -> Result<PreparedHwpAction> {
        let (command, defaults, mapping) = hwp_action(id)
            .ok_or_else(|| anyhow!("대응하는 HwpCraft 명령이 없는 한글 동작: {id}"))?;
        let given = params
            .as_object()
            .ok_or_else(|| anyhow!("params는 JSON 객체여야 합니다."))?;
        let mut merged: Map<String, Value> = serde_json::from_str::<Value>(defaults)?
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(native) = parameter_set {
            if !given.is_empty() {
                bail!("params와 parameterSet을 동시에 지정할 수 없습니다.");
            }
            let native = native
                .as_object()
                .ok_or_else(|| anyhow!("parameterSet은 JSON 객체여야 합니다."))?;
            let converted = match id {
                "CharShape" => self.native_character(native)?,
                "ParagraphShape" => self.native_paragraph(native)?,
                "TableCreate" => native_table(native)?,
                "InsertText" => native_insert_text(native)?,
                "PageSetup" | "PageMarginSetup" => native_page_setup(native)?,
                id if FIND_ACTIONS.contains(&id) => native_find(id, native)?,
                _ => bail!("{id}: 공개 ParameterSet 어댑터가 아직 없습니다."),
            };
            if id == "CharShape" {
                return Ok(PreparedHwpAction {
                    command: "hwp.parameters.apply",
                    params: json!({"id":id,"parameterSet":native}),
                    mapping,
                });
            }
            merged.extend(converted);
        } else {
            merged.extend(given.clone());
        }
        Ok(PreparedHwpAction {
            command,
            params: Value::Object(merged),
            mapping,
        })
    }

    /// The same native-parameter adapter is available outside MCP.
    pub fn execute_hwp_action(
        &mut self,
        id: &str,
        params: Value,
        parameter_set: Option<Value>,
    ) -> Result<Value> {
        let prepared = self.prepare_hwp_action(id, params, parameter_set)?;
        self.run(prepared.command, prepared.params)
    }

    fn native_character(&self, native: &Map<String, Value>) -> Result<Map<String, Value>> {
        let actual = self.character_properties()?;
        self.native_character_from(native, actual)
    }

    fn native_character_from(
        &self,
        native: &Map<String, Value>,
        actual: Value,
    ) -> Result<Map<String, Value>> {
        let mut out = Map::new();
        for (key, value) in native {
            let mut language = false;
            for (prefix, target) in [
                ("FaceName", "fontFamilies"),
                ("Ratio", "ratios"),
                ("Spacing", "spacings"),
                ("Size", "relativeSizes"),
                ("Offset", "charOffsets"),
            ] {
                if let Some(i) = LANGUAGES
                    .iter()
                    .position(|language| key == &format!("{prefix}{language}"))
                {
                    let values = out
                        .entry(target.to_owned())
                        .or_insert_with(|| actual[target].clone());
                    let values = values
                        .as_array_mut()
                        .filter(|v| v.len() == 7)
                        .ok_or_else(|| anyhow!("언어별 글자 모양을 읽지 못했습니다."))?;
                    values[i] = if prefix == "FaceName" {
                        value.clone()
                    } else {
                        json!(integer(value, key)?)
                    };
                    language = true;
                    break;
                }
            }
            if language {
                continue;
            }
            let target = match key.as_str() {
                "Height" => "fontSize",
                "Bold" => "bold",
                "Italic" => "italic",
                "Emboss" => "emboss",
                "Engrave" => "engrave",
                "SuperScript" => "superscript",
                "SubScript" => "subscript",
                "UseKerning" => "kerning",
                "TextColor" => "textColor",
                "ShadeColor" => "shadeColor",
                "UnderlineColor" => "underlineColor",
                "ShadowColor" => "shadowColor",
                "StrikeOutColor" => "strikeColor",
                "UnderlineShape" => "underlineShape",
                "OutlineType" => "outlineType",
                "ShadowType" => "shadowType",
                "ShadowOffsetX" => "shadowOffsetX",
                "ShadowOffsetY" => "shadowOffsetY",
                "DiacSymMark" => "emphasisDot",
                "StrikeOutShape" => "strikeShape",
                "UnderlineType" => {
                    let n = integer(value, key)?;
                    let kind = match n {
                        0 => "None",
                        1 => "Bottom",
                        3 => "Top",
                        _ => {
                            bail!("UnderlineType: 가운데 밑줄 등 값 {n}은 아직 지원하지 않습니다.")
                        }
                    };
                    out.insert("underlineType".to_owned(), json!(kind));
                    out.insert("underline".to_owned(), json!(n != 0));
                    continue;
                }
                "StrikeOutType" => {
                    let n = integer(value, key)?;
                    if !matches!(n, 0 | 3) {
                        bail!("StrikeOutType: 글자색 단선(3)과 없음(0)만 지원합니다.");
                    }
                    out.insert("strikethrough".to_owned(), json!(n != 0));
                    continue;
                }
                _ => bail!("CharShape: 지원하지 않는 공개 인자 {key}"),
            };
            let converted = if matches!(
                key.as_str(),
                "Bold"
                    | "Italic"
                    | "Emboss"
                    | "Engrave"
                    | "SuperScript"
                    | "SubScript"
                    | "UseKerning"
            ) {
                flag(value, key)?
            } else if key.ends_with("Color") {
                colorref(value, key)?
            } else {
                json!(integer(value, key)?)
            };
            out.insert(target.to_owned(), converted);
        }
        // Validate both supplied fields and the resulting partial patch before edit.
        validate_format(&Value::Object(out.clone()), true)?;
        Ok(out)
    }

    fn apply_native_character(&mut self, native: &Map<String, Value>) -> Result<Value> {
        let patch = self.native_character(native)?;
        if !self.has_selection() {
            self.execute(Command::Character(Value::Object(patch)))?;
            return Ok(self.info());
        }
        let mut operations = Vec::new();
        for (cursor, range) in self.selected_paragraphs()? {
            if self.cell_protected(cursor) {
                bail!("보호된 셀은 고칠 수 없습니다.");
            }
            if range.is_empty() {
                continue;
            }
            let paragraph = self.paragraph(cursor)?;
            let mut boundaries = std::collections::BTreeSet::from([range.start, range.end]);
            for shape in &paragraph.char_shapes {
                let scalar = paragraph
                    .char_offsets
                    .partition_point(|position| *position < shape.start_pos);
                if range.start < scalar && scalar < range.end {
                    boundaries.insert(scalar);
                }
            }
            let boundaries = boundaries.into_iter().collect::<Vec<_>>();
            for ends in boundaries.windows(2) {
                let shape = paragraph.char_shape_id_at(ends[0]).unwrap_or(0);
                let patch = self.native_character_from(native, self.char_shape_json(shape))?;
                operations.push((cursor, ends[0]..ends[1], Value::Object(patch)));
            }
        }
        self.transaction(|session| {
            for (cursor, range, patch) in operations {
                let patch = session.character_parameters(patch)?.to_string();
                if let Some(cell) = cursor.cell {
                    session.core.apply_char_format_in_cell_native(
                        cursor.section,
                        cursor.paragraph,
                        cell.control,
                        cell.index,
                        cell.paragraph,
                        range.start,
                        range.end,
                        &patch,
                    )
                } else {
                    session.core.apply_char_format_native(
                        cursor.section,
                        cursor.paragraph,
                        range.start,
                        range.end,
                        &patch,
                    )
                }
                .map_err(err)?;
                if range.start == 0 {
                    insertion_shape::align_selected_leading(&mut session.core, cursor)?;
                }
            }
            insertion_shape::validate_resources(session.core.document())?;
            Ok(())
        })?;
        Ok(self.info())
    }

    fn native_paragraph(&self, native: &Map<String, Value>) -> Result<Map<String, Value>> {
        let mut out = Map::new();
        let current = self.paragraph_format()?;
        let line_kind = if let Some(value) = native.get("LineSpacingType") {
            integer(value, "LineSpacingType")?
        } else {
            match current["lineSpacingType"].as_str() {
                Some("Fixed") => 1,
                Some("SpaceOnly") => 2,
                Some("Minimum") => 3,
                _ => 0,
            }
        };
        for (key, value) in native {
            let (target, converted) = match key.as_str() {
                "LeftMargin" => ("marginLeft", absolute_urc(value, key)?),
                "RightMargin" => ("marginRight", absolute_urc(value, key)?),
                "Indentation" => ("indent", absolute_urc(value, key)?),
                "PrevSpacing" => ("spacingBefore", absolute_urc(value, key)?),
                "NextSpacing" => ("spacingAfter", absolute_urc(value, key)?),
                "LineSpacingType" => (
                    "lineSpacingType",
                    choice(value, key, &["Percent", "Fixed", "SpaceOnly"])?,
                ),
                "LineSpacing" => (
                    "lineSpacing",
                    if line_kind == 0 {
                        json!(integer(value, key)?)
                    } else {
                        absolute_urc(value, key)?
                    },
                ),
                "AlignType" => (
                    "alignment",
                    choice(
                        value,
                        key,
                        &["justify", "left", "right", "center", "distribute", "split"],
                    )?,
                ),
                "BreakLatinWord" => ("englishBreakUnit", json!(integer(value, key)?)),
                "BreakNonLatinWord" => (
                    "koreanBreakUnit",
                    json!(if flag(value, key)? == true { 1 } else { 0 }),
                ),
                "TextAlignment" => ("verticalAlign", json!(integer(value, key)?)),
                "Level" => ("paraLevel", json!(integer(value, key)?)),
                "WidowOrphan" => ("widowOrphan", flag(value, key)?),
                "KeepWithNext" => ("keepWithNext", flag(value, key)?),
                "KeepLinesTogether" => ("keepLines", flag(value, key)?),
                "PagebreakBefore" => ("pageBreakBefore", flag(value, key)?),
                "FontLineHeight" => ("fontLineHeight", flag(value, key)?),
                "BorderConnect" => ("borderConnect", flag(value, key)?),
                "BorderText" => ("borderIgnoreMargin", flag(value, key)?),
                "AutoSpaceEAsianEng" => ("autoSpaceKrEn", flag(value, key)?),
                "AutoSpaceEAsianNum" => ("autoSpaceKrNum", flag(value, key)?),
                key if key.starts_with("BorderOffset") => {
                    let index = match key {
                        "BorderOffsetLeft" => 0,
                        "BorderOffsetRight" => 1,
                        "BorderOffsetTop" => 2,
                        "BorderOffsetBottom" => 3,
                        _ => bail!("ParaShape: 지원하지 않는 공개 인자 {key}"),
                    };
                    let a = out
                        .entry("borderSpacing".to_owned())
                        .or_insert_with(|| current["borderSpacing"].clone());
                    let values = a
                        .as_array_mut()
                        .filter(|a| a.len() == 4)
                        .ok_or_else(|| anyhow!("문단 테두리 간격을 읽지 못했습니다."))?;
                    values[index] = json!(integer(value, key)?);
                    continue;
                }
                _ => bail!("ParaShape: 지원하지 않는 공개 인자 {key}"),
            };
            out.insert(target.to_owned(), converted);
        }
        self.paragraph_parameters(Value::Object(out.clone()))?;
        Ok(out)
    }
}

fn native_table(native: &Map<String, Value>) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    // Published table defaults; installation-specific last-used UI values are
    // intentionally not read from or written to another application's settings.
    out.insert("rows".to_owned(), json!(5));
    out.insert("columns".to_owned(), json!(5));
    for (key, value) in native {
        match key.as_str() {
            "Rows" => {
                out.insert("rows".to_owned(), json!(integer(value, key)?));
            }
            "Cols" => {
                out.insert("columns".to_owned(), json!(integer(value, key)?));
            }
            "ColWidth" => {
                let widths = value
                    .as_array()
                    .ok_or_else(|| anyhow!("ColWidth는 HWPUNIT 정수 배열이어야 합니다."))?;
                let widths = widths
                    .iter()
                    .map(|v| {
                        let n = integer(v, key)?;
                        if n <= 0 {
                            bail!("ColWidth는 양수여야 합니다.");
                        }
                        Ok(n as f64 * 25.4 / 7200.0)
                    })
                    .collect::<Result<Vec<_>>>()?;
                out.insert("column_widths_mm".to_owned(), json!(widths));
            }
            _ => bail!("TableCreation: 지원하지 않는 공개 인자 {key}"),
        }
    }
    let rows = out["rows"].as_i64().unwrap_or(0);
    let columns = out["columns"].as_i64().unwrap_or(0);
    if !(1..=200).contains(&rows) || !(1..=64).contains(&columns) || rows * columns > 4096 {
        bail!("표 크기는 1–200줄·1–64칸·4,096셀 이하여야 합니다.");
    }
    if out
        .get("column_widths_mm")
        .is_some_and(|v| v.as_array().is_none_or(|a| a.len() != columns as usize))
    {
        bail!("ColWidth 개수는 Cols와 같아야 합니다.");
    }
    Ok(out)
}

/// InsertText: `Text` is the text to insert.
fn native_insert_text(native: &Map<String, Value>) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    for (key, value) in native {
        match key.as_str() {
            "Text" => {
                out.insert("text".to_owned(), json!(string(value, key)?));
            }
            _ => bail!("InsertText: 지원하지 않는 공개 인자 {key}"),
        }
    }
    if !out.contains_key("text") {
        bail!("InsertText: Text가 필요합니다.");
    }
    Ok(out)
}

/// SecDef for 편집 용지: the `PageDef` lengths in HWPUNIT, `Landscape` (0 좁게, 1 넓게),
/// `GutterType` (0 한쪽, 1 맞쪽, 2 위로 넘기기) and `ApplyTo` (0·2 현재 구역, 3 문서 전체).
fn native_page_setup(native: &Map<String, Value>) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    for (key, value) in native {
        match key.as_str() {
            "PageDef" => {
                let def = value.as_object().ok_or_else(|| {
                    anyhow!("PageDef는 하위 ParameterSet(JSON 객체)이어야 합니다.")
                })?;
                for (field, value) in def {
                    if let Some((_, target)) = PAGE_DEF.iter().find(|(name, _)| name == field) {
                        let n = integer(value, field)?;
                        if n < 0 {
                            bail!("PageDef.{field}: 0 이상의 HWPUNIT이 필요합니다.");
                        }
                        out.insert((*target).to_owned(), json!(n as f64 * 25.4 / 7200.0));
                        continue;
                    }
                    match field.as_str() {
                        "Landscape" => {
                            out.insert("landscape".to_owned(), flag(value, field)?);
                        }
                        "GutterType" => {
                            let n = integer(value, field)?;
                            if !(0..=2).contains(&n) {
                                bail!("PageDef.GutterType: 0–2가 필요합니다.");
                            }
                            out.insert("binding".to_owned(), json!(n));
                        }
                        _ => bail!("PageDef: 지원하지 않는 공개 인자 {field}"),
                    }
                }
            }
            "ApplyTo" => match integer(value, key)? {
                0 | 2 => {}
                3 => {
                    out.insert("apply_to".to_owned(), json!("document"));
                }
                n => bail!(
                    "ApplyTo {n}: 선택된 문자열·새 구역에 적용하는 편집 용지는 아직 지원하지 않습니다."
                ),
            },
            // The dialog's list of choices only.
            "ApplyClass" => {
                integer(value, key)?;
            }
            _ => bail!("SecDef: 지원하지 않는 공개 인자 {key}"),
        }
    }
    Ok(out)
}

/// FindReplace for the 찾기 / 바꾸기 actions. Switches HwpCraft does not have are
/// accepted only when off (0, empty), as a set filled by GetDefault carries them.
fn native_find(id: &str, native: &Map<String, Value>) -> Result<Map<String, Value>> {
    let mut query = None;
    let mut replacement = None;
    let mut options = Map::new();
    for (key, value) in native {
        let option = match key.as_str() {
            "FindString" => {
                query = Some(string(value, key)?);
                continue;
            }
            "ReplaceString" => {
                replacement = Some(string(value, key)?);
                continue;
            }
            "Direction" => {
                options.insert(
                    "direction".to_owned(),
                    choice(value, key, &["down", "up", "all"])?,
                );
                continue;
            }
            "MatchCase" => "case_sensitive",
            "WholeWordOnly" => "whole_word",
            "UseWildCards" => "wildcards",
            "SeveralWords" => "any_word",
            "FindRegExp" => "regex",
            "HanjaFromHangul" => "hanja_by_hangul",
            "FindCharShape" => {
                let shape = value.as_object().ok_or_else(|| {
                    anyhow!("FindCharShape는 하위 ParameterSet(JSON 객체)이어야 합니다.")
                })?;
                for (field, value) in shape {
                    let (target, converted) = match field.as_str() {
                        "Bold" => ("bold", flag(value, field)?),
                        "Italic" => ("italic", flag(value, field)?),
                        "UnderlineType" => ("underline", json!(integer(value, field)? != 0)),
                        "Height" => {
                            let height = integer(value, field)?;
                            if height <= 0 || height % 100 != 0 {
                                bail!("FindCharShape.Height: 1 pt(100) 단위 양수만 지원합니다.");
                            }
                            ("size_pt", json!(height / 100))
                        }
                        _ => bail!("FindCharShape: 지원하지 않는 공개 인자 {field}"),
                    };
                    options.insert(target.to_owned(), converted);
                }
                continue;
            }
            // Message boxes and dialog modes: HwpCraft shows neither.
            "IgnoreMessage" | "ReplaceMode" | "FindType" => {
                flag(value, key)?;
                continue;
            }
            "AllWordForms"
            | "AutoSpell"
            | "FindJaso"
            | "IgnoreFindString"
            | "IgnoreReplaceString" => {
                if flag(value, key)? == true {
                    bail!("FindReplace: {key}는 아직 지원하지 않습니다(0만 받습니다).");
                }
                continue;
            }
            "FindStyle" | "ReplaceStyle" => {
                if !string(value, key)?.is_empty() {
                    bail!("FindReplace: {key}는 아직 지원하지 않습니다(빈 문자열만 받습니다).");
                }
                continue;
            }
            "FindParaShape" | "ReplaceCharShape" | "ReplaceParaShape" => {
                if value.as_object().is_none_or(|set| !set.is_empty()) {
                    bail!("FindReplace: {key}는 아직 지원하지 않습니다(빈 묶음만 받습니다).");
                }
                continue;
            }
            _ => bail!("FindReplace: 지원하지 않는 공개 인자 {key}"),
        };
        options.insert(option.to_owned(), flag(value, key)?);
    }
    let query = query.ok_or_else(|| anyhow!("FindReplace: FindString이 필요합니다."))?;
    let mut out = Map::new();
    out.insert("query".to_owned(), json!(query));
    if id == "FindAll" {
        // 모두 찾기 takes case sensitivity only and always searches the whole document.
        for (key, value) in &options {
            if key != "case_sensitive" && key != "direction" && value != &json!(false) {
                bail!("FindAll: {key}는 모두 찾기에서 아직 지원하지 않습니다.");
            }
        }
        if let Some(case) = options.get("case_sensitive") {
            out.insert("case_sensitive".to_owned(), case.clone());
        }
        return Ok(out);
    }
    if matches!(id, "AllReplace" | "ExecReplace") {
        out.insert(
            "replacement".to_owned(),
            json!(replacement.unwrap_or_default()),
        );
    }
    out.insert("options".to_owned(), Value::Object(options));
    Ok(out)
}
