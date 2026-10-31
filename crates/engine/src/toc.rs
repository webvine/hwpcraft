//! Owner: format area. 차례 만들기·새로 고침 (Hangul 도구 > 차례/찾아보기).
//!
//! Entries are the outline paragraphs (개요 문단) in document order, written as
//! ordinary body paragraphs in the "차례 제목" / "차례 N" styles that Hangul
//! documents carry, with the page number after a right-aligned leader tab. Those
//! styles identify the table later: `tools.toc.update` rewrites the first run of
//! such paragraphs in place. Writing the table moves the headings it lists, so
//! page numbers are re-read after layout until they stop changing.
//!
//! 표 차례 / 그림 차례 (`kind`) list captioned tables or pictures the same way, as
//! ordinary paragraphs in HwpCraft's own "표 차례 제목"/"표 차례" and "그림 차례
//! 제목"/"그림 차례" styles: each line is the caption as shown ("표 1 예산 집행")
//! and its page. Hangul's own table-of-contents field is not written.
//!
//! With `links` (the default) each entry's text is a hyperlink to a bookmark at its
//! heading, table or picture paragraph, in the form a Hangul-saved document uses for
//! a link to a bookmark: command `?<name>;0;0;0;` with the HWPX parameters Category
//! HWPHYPERLINK_TYPE_HWP, TargetType HWPHYPERLINK_TARGET_BOOKMARK and DocOpenType
//! HWPHYPERLINK_JUMP_CURRENTTAB. A paragraph's existing bookmark is reused.
use super::*;
use hwpcraft_doc::model::control::{FieldType, Parameter, ParameterList};
use hwpcraft_doc::model::{
    document::Document,
    style::{ParaShapeMods, Style, TabDef, TabItem},
};
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "tools.toc",
        "차례 만들기 (개요 문단, 또는 캡션 달린 표·그림을 쪽 번호와 함께 차례로 넣기)",
        "",
    ),
    ("tools.toc.update", "차례 새로 고침", ""),
];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "tools.toc",
        "본문 캐럿 위치에 차례를 넣습니다. levels(1–10, 기본 3): 넣을 개요 수준. title(기본 \"차례\", null이면 제목 없음). leader: dot(기본)·dash·solid·none — 쪽 번호 앞 채움 선. page_numbers(기본 true). links(기본 true): 항목 글을 그 제목·표·그림 문단의 책갈피로 가는 하이퍼링크로 만듦(hyperlink.jump로 이동). numbers(기본 true): 개요 번호를 함께 적음. 차례 문단은 \"차례 제목\"·\"차례 N\" 스타일이며 문서에 없으면 만듭니다. kind: headings(기본, 제목 차례) | tables(표 차례) | pictures(그림 차례) — tables·pictures는 캡션 달린 표·그림의 캡션 글(예: \"표 1 예산 집행\")과 쪽을 \"표 차례\"·\"그림 차례\" 스타일 문단으로 넣고, 제목 기본값은 \"표 차례\"·\"그림 차례\"이며 levels·numbers·styles는 받지 않습니다. 결과: entries(항목 수), section, first(첫 차례 문단), paragraphs(차례 문단 수).",
        r#"{"levels":3,"title":"차례","leader":"dot"}"#,
    ),
    (
        "tools.toc.update",
        "첫 차례(\"차례 제목\"·\"차례 N\" 스타일 문단의 연속)를 지금 개요와 쪽 번호로 다시 씁니다. 매개 변수를 생략하면 기존 차례의 수준·제목·채움 선·쪽 번호 표시를 따르고, tools.toc와 같은 이름으로 바꿀 수 있습니다. 매개 변수 없이 부르면 제목 차례·표 차례·그림 차례 가운데 문서에 있는 것을 모두 한 번(실행 취소 한 단계)에 새로 고치고 결과에 lists[{kind, entries, …}]를 더합니다. kind를 주면 그 차례만 고칩니다.",
        r#"{}"#,
    ),
];

const MAX_LEVELS: u8 = 10;
const TITLE_STYLE: (&str, &str) = ("차례 제목", "TOC Heading");
/// Each level below the first indents by 10 pt (paragraph margins use 200 units/pt).
const LEVEL_INDENT: i32 = 2_000;
/// Page numbers move when the table pushes headings down; settle within this many passes.
const PAGE_PASSES: usize = 3;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Leader {
    #[default]
    Dot,
    Dash,
    Solid,
    None,
}

impl Leader {
    /// HWP tab fill type: 3 점선, 2 파선, 1 실선, 0 없음.
    fn fill(self) -> u8 {
        match self {
            Leader::Dot => 3,
            Leader::Dash => 2,
            Leader::Solid => 1,
            Leader::None => 0,
        }
    }
    fn from_fill(fill: u8) -> Self {
        match fill {
            3 => Leader::Dot,
            2 => Leader::Dash,
            1 => Leader::Solid,
            _ => Leader::None,
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TocParams {
    levels: Option<u8>,
    #[serde(default, with = "explicit")]
    title: Option<Option<String>>,
    leader: Option<Leader>,
    page_numbers: Option<bool>,
    numbers: Option<bool>,
    /// Collect paragraphs in these styles (level 1, 2, ...) instead of outline paragraphs.
    styles: Option<Vec<StyleRef>>,
    /// Which table: headings (default), or captioned tables or pictures.
    kind: Option<Kind>,
    /// Entries link to what they list (default true).
    links: Option<bool>,
}

impl TocParams {
    fn is_empty(&self) -> bool {
        self.levels.is_none()
            && self.title.is_none()
            && self.leader.is_none()
            && self.page_numbers.is_none()
            && self.numbers.is_none()
            && self.styles.is_none()
            && self.kind.is_none()
            && self.links.is_none()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Headings,
    Tables,
    Pictures,
}

const LISTS: [Kind; 2] = [Kind::Tables, Kind::Pictures];

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Headings => "headings",
            Kind::Tables => "tables",
            Kind::Pictures => "pictures",
        }
    }
    /// The list's default title, also its entry style's name.
    fn title(self) -> &'static str {
        match self {
            Kind::Tables => "표 차례",
            Kind::Pictures => "그림 차례",
            Kind::Headings => "차례",
        }
    }
    /// (title style, entry style) as (Korean, English) names.
    fn styles(self) -> ((&'static str, &'static str), (&'static str, &'static str)) {
        match self {
            Kind::Pictures => (
                ("그림 차례 제목", "Figure List Heading"),
                ("그림 차례", "Figure List"),
            ),
            _ => (
                ("표 차례 제목", "Table List Heading"),
                ("표 차례", "Table List"),
            ),
        }
    }
}

/// How a 표 차례 / 그림 차례 looks.
#[derive(Clone, Debug, PartialEq)]
struct ListOptions {
    title: Option<String>,
    leader: Leader,
    page_numbers: bool,
    links: bool,
}

impl ListOptions {
    fn merged(mut self, kind: Kind, p: TocParams) -> Result<Self> {
        if p.levels.is_some() || p.numbers.is_some() || p.styles.is_some() {
            bail!(
                "{}에는 levels·numbers·styles를 쓰지 않습니다.",
                kind.title()
            );
        }
        if let Some(title) = p.title {
            self.title = title.map(|t| clean(&t)).filter(|t| !t.is_empty());
        }
        self.leader = p.leader.unwrap_or(self.leader);
        self.page_numbers = p.page_numbers.unwrap_or(self.page_numbers);
        self.links = p.links.unwrap_or(self.links);
        Ok(self)
    }
}

/// Whether a written entry paragraph links somewhere.
fn links_out(paragraph: &hwpcraft_doc::model::paragraph::Paragraph) -> bool {
    paragraph
        .controls
        .iter()
        .any(|c| matches!(c, Control::Field(f) if f.field_type == FieldType::Hyperlink))
}

/// Hangul's HWPX parameters of a link to a bookmark in the same document.
fn bookmark_link_parameters(command: &str) -> ParameterList {
    use super::cross_ref::string;
    ParameterList {
        name: Some(String::new()),
        items: vec![
            Parameter::Integer {
                name: Some("Prop".into()),
                value: 0,
            },
            string("Command", command),
            string("Category", "HWPHYPERLINK_TYPE_HWP"),
            string("TargetType", "HWPHYPERLINK_TARGET_BOOKMARK"),
            string("DocOpenType", "HWPHYPERLINK_JUMP_CURRENTTAB"),
        ],
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StyleRef {
    Id(usize),
    Name(String),
}

/// Where entries come from: outline paragraphs, or paragraphs in given styles
/// (Hangul's 개요 문단으로 / 스타일로 모으기).
#[derive(Clone, Debug, Default, PartialEq)]
enum Source {
    #[default]
    Outline,
    /// Style id per level, level 1 first.
    Styles(Vec<usize>),
}

/// Tell an omitted `title` (keep the default) from `"title": null` (no title).
mod explicit {
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Option<String>>, D::Error> {
        Option::<String>::deserialize(d).map(Some)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TocOptions {
    levels: u8,
    title: Option<String>,
    leader: Leader,
    page_numbers: bool,
    numbers: bool,
    source: Source,
    links: bool,
}

impl Default for TocOptions {
    fn default() -> Self {
        Self {
            levels: 3,
            title: Some("차례".into()),
            leader: Leader::Dot,
            page_numbers: true,
            numbers: true,
            source: Source::Outline,
            links: true,
        }
    }
}

impl TocOptions {
    fn merged(mut self, p: TocParams, styles: &[Style]) -> Result<Self> {
        if let Some(list) = p.styles {
            if list.is_empty() || list.len() > usize::from(MAX_LEVELS) {
                bail!("styles에는 수준별 스타일을 1–{MAX_LEVELS}개 적으세요.");
            }
            let ids = list
                .iter()
                .map(|r| match r {
                    StyleRef::Id(id) if *id < styles.len() => Ok(*id),
                    StyleRef::Name(name) => styles
                        .iter()
                        .position(|s| s.local_name == *name || s.english_name == *name)
                        .ok_or_else(|| anyhow!("스타일을 찾지 못했습니다: {name}")),
                    StyleRef::Id(id) => Err(anyhow!("스타일을 찾지 못했습니다: {id}")),
                })
                .collect::<Result<Vec<_>>>()?;
            self.levels = ids.len() as u8;
            self.source = Source::Styles(ids);
            // Hand-numbered headings carry their numbers in the text.
            self.numbers = false;
        }
        if let Some(levels) = p.levels {
            self.levels = levels;
        }
        if !(1..=MAX_LEVELS).contains(&self.levels) {
            bail!("차례 수준은 1–{MAX_LEVELS}입니다.");
        }
        if let Some(title) = p.title {
            self.title = title.map(|t| clean(&t)).filter(|t| !t.is_empty());
        }
        self.leader = p.leader.unwrap_or(self.leader);
        self.page_numbers = p.page_numbers.unwrap_or(self.page_numbers);
        self.numbers = p.numbers.unwrap_or(self.numbers);
        self.links = p.links.unwrap_or(self.links);
        Ok(self)
    }
}

#[derive(Deserialize)]
struct Outline {
    outline: Vec<Heading>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct Heading {
    pub(crate) level: u8,
    number: String,
    pub(crate) title: String,
    pub(crate) page: u32,
    /// Where the heading is (the table's paragraph for a heading in a cell).
    #[serde(default)]
    section: usize,
    #[serde(default)]
    paragraph: usize,
}

/// One paragraph of text: tabs and line breaks would change the table's shape.
pub(crate) fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// The written table: where it is and which levels its entries have.
struct Block {
    section: usize,
    first: usize,
    title: bool,
    levels: Vec<u8>,
}

impl Block {
    fn paragraphs(&self) -> usize {
        usize::from(self.title) + self.levels.len()
    }
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_toc_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        let parsed = serde_json::from_value::<TocParams>(params.clone())
            .map_err(|e| anyhow!("{id}: 매개 변수 오류 ({e})"));
        Some(parsed.and_then(|p| match (id, p.kind) {
            ("tools.toc", Some(kind @ (Kind::Tables | Kind::Pictures))) => {
                self.insert_list(kind, p)
            }
            ("tools.toc", _) => self.insert_toc(p),
            (_, Some(kind @ (Kind::Tables | Kind::Pictures))) => self.update_list(kind, p),
            (_, Some(Kind::Headings)) => self.update_toc(p),
            (_, None) if p.is_empty() => self.update_all(),
            _ => self.update_toc(p),
        }))
    }

    pub(crate) fn outline_headings(&self, levels: u8) -> Result<Vec<Heading>> {
        let raw = self.core.get_outline_navigation_native().map_err(err)?;
        let outline: Outline = serde_json::from_str(&raw)?;
        Ok(outline
            .outline
            .into_iter()
            // An empty outline paragraph has nothing to list.
            .filter(|h| (1..=levels).contains(&h.level) && !clean(&h.title).is_empty())
            .collect())
    }

    /// Entries for `options`, in document order, with one-based physical pages.
    fn toc_headings(&self, options: &TocOptions) -> Result<Vec<Heading>> {
        let Source::Styles(ids) = &options.source else {
            return self.outline_headings(options.levels);
        };
        let starts = self.page_starts()?;
        let mut headings = Vec::new();
        for (s, section) in self.core.document().sections.iter().enumerate() {
            for (p, paragraph) in section.paragraphs.iter().enumerate() {
                let Some(level) = ids
                    .iter()
                    .position(|id| *id == usize::from(paragraph.style_id))
                else {
                    continue;
                };
                let title = clean(&paragraph.text);
                if title.is_empty() || level >= usize::from(options.levels) {
                    continue;
                }
                let page = starts.partition_point(|start| *start <= (s, p)).max(1);
                headings.push(Heading {
                    level: level as u8 + 1,
                    number: String::new(),
                    title,
                    page: page as u32,
                    section: s,
                    paragraph: p,
                });
            }
        }
        Ok(headings)
    }

    /// (section, paragraph) where each page starts, in page order.
    fn page_starts(&self) -> Result<Vec<(usize, usize)>> {
        let pages = self.page_count()?;
        (0..pages)
            .map(|page| {
                let raw = self
                    .core
                    .get_position_of_page_native(page as usize)
                    .map_err(err)?;
                let v: Value = serde_json::from_str(&raw)?;
                let n = |k: &str| v[k].as_u64().map_or(0, |x| x as usize);
                Ok((n("sec"), n("para")))
            })
            .collect()
    }

    fn insert_toc(&mut self, p: TocParams) -> Result<Value> {
        let options = TocOptions::default().merged(p, &self.core.document().doc_info.styles)?;
        if self.cursor.cell.is_some() {
            bail!("차례는 본문에 넣을 수 있습니다. 표 밖에 캐럿을 두세요.");
        }
        let headings = self.toc_headings(&options)?;
        if headings.is_empty() {
            bail!(
                "차례에 넣을 문단이 없습니다. 제목 문단에 개요 번호를 적용하거나 styles로 제목 스타일을 지정하세요."
            );
        }
        let at = self.cursor;
        let mut written = None;
        self.transaction(|s| {
            s.clear_selection();
            written = Some(s.write_toc(at, &options, &headings)?);
            Ok(())
        })?;
        let block = written.ok_or_else(|| anyhow!("차례를 쓰지 못했습니다."))?;
        Ok(
            json!({"entries":block.levels.len(),"section":block.section,"first":block.first,
            "paragraphs":block.paragraphs()}),
        )
    }

    fn update_toc(&mut self, p: TocParams) -> Result<Value> {
        let styles = self.toc_style_ids();
        let (block, existing) = self
            .find_toc(&styles)
            .ok_or_else(|| anyhow!("새로 고칠 차례가 없습니다. 먼저 차례를 만드세요."))?;
        let mut existing = existing;
        let entries: Vec<String> = {
            let paragraphs = &self.core.document().sections[block.section].paragraphs;
            let from = block.first + usize::from(block.title);
            paragraphs[from..from + block.levels.len()]
                .iter()
                .map(|p| p.text.split('\t').next().unwrap_or("").to_owned())
                .collect()
        };
        let outline = self.outline_headings(existing.levels)?;
        if let Some(first) = outline.first() {
            // Entries written with outline numbers start with their heading's number.
            let number = clean(&first.number);
            existing.numbers = !number.is_empty()
                && entries
                    .first()
                    .is_some_and(|e| e.starts_with(&format!("{number} ")));
        } else if let Some(ids) = self.styles_behind(&block, &entries, &styles) {
            existing.numbers = false;
            existing.source = Source::Styles(ids);
        }
        let options = existing.merged(p, &self.core.document().doc_info.styles)?;
        let headings = self.toc_headings(&options)?;
        if headings.is_empty() {
            bail!("차례에 넣을 개요 문단이 없습니다.");
        }
        let block = self.rewrite_block(&block, |s, at| s.write_toc(at, &options, &headings))?;
        Ok(
            json!({"entries":block.levels.len(),"section":block.section,"first":block.first,
            "paragraphs":block.paragraphs()}),
        )
    }

    /// Replace a written table in one undo step: empty it down to one paragraph,
    /// write the new one before it, then fold that leftover paragraph into the new
    /// table's last line.
    fn rewrite_block(
        &mut self,
        block: &Block,
        write: impl FnOnce(&mut Self, Cursor) -> Result<Block>,
    ) -> Result<Block> {
        let mut written = None;
        self.transaction(|s| {
            s.clear_selection();
            let section = block.section;
            let last = block.first + block.paragraphs() - 1;
            // Deleting a link's text leaves an empty field behind; take links off first.
            s.deferred_layout(|s| {
                for paragraph in block.first..=last {
                    let ids: Vec<u32> = s.core.document().sections[section].paragraphs[paragraph]
                        .controls
                        .iter()
                        .filter_map(|c| match c {
                            Control::Field(f) if f.field_type == FieldType::Hyperlink => {
                                Some(f.field_id)
                            }
                            _ => None,
                        })
                        .collect();
                    let target = hwpcraft_doc::HyperlinkTarget::body(section, paragraph);
                    for id in ids {
                        s.core.remove_hyperlink_native(&target, id).map_err(err)?;
                    }
                }
                Ok(())
            })?;
            let end = s.body_paragraph_len(section, last)?;
            s.core
                .delete_range_native(section, block.first, 0, last, end, None)
                .map_err(err)?;
            let at = Cursor {
                section,
                paragraph: block.first,
                offset: 0,
                cell: None,
            };
            let new = write(s, at)?;
            let leftover = new.first + new.paragraphs();
            if s.body_paragraph_len(section, leftover)? == 0 {
                s.core
                    .merge_paragraph_native(section, leftover)
                    .map_err(err)?;
            }
            s.cursor = at;
            written = Some(new);
            Ok(())
        })?;
        written.ok_or_else(|| anyhow!("차례를 쓰지 못했습니다."))
    }

    /// Every table this module wrote, refreshed in one undo step.
    fn update_all(&mut self) -> Result<Value> {
        let has_toc = self.find_toc(&self.toc_style_ids()).is_some();
        let kinds: Vec<Kind> = LISTS
            .into_iter()
            .filter(|k| self.find_list(*k).is_some())
            .collect();
        if !has_toc && kinds.is_empty() {
            bail!("새로 고칠 차례가 없습니다. 먼저 차례를 만드세요.");
        }
        let mut out = json!({});
        self.transaction(|s| {
            let mut lists = Vec::new();
            for kind in &kinds {
                lists.push(s.update_list(*kind, TocParams::default())?);
            }
            if has_toc {
                out = s.update_toc(TocParams::default())?;
            }
            // The rewritten contents can move the tables and pictures listed.
            for kind in &kinds {
                if let Some((block, options)) = s.find_list(*kind)
                    && options.page_numbers
                {
                    s.settle_list_pages(*kind, &block)?;
                }
            }
            out["lists"] = json!(lists);
            Ok(())
        })?;
        Ok(out)
    }

    /// Captioned tables or pictures in the body, in document order: (caption as
    /// shown, one-based physical page).
    fn caption_entries(&self, kind: Kind) -> Vec<(String, u32)> {
        self.caption_targets(kind)
            .into_iter()
            .map(|(text, page, _)| (text, page))
            .collect()
    }

    /// [`Self::caption_entries`] with each caption's (section, paragraph).
    fn caption_targets(&self, kind: Kind) -> Vec<(String, u32, (usize, usize))> {
        let mut entries = Vec::new();
        for (s, section) in self.core.document().sections.iter().enumerate() {
            for (p, para) in section.paragraphs.iter().enumerate() {
                for control in &para.controls {
                    let caption = match (kind, control) {
                        (Kind::Tables, Control::Table(t)) => t.caption.as_ref(),
                        (Kind::Pictures, Control::Picture(pic)) => pic.caption.as_ref(),
                        _ => None,
                    };
                    let Some(text) = caption
                        .map(super::cross_ref::caption_text)
                        .filter(|t| !t.is_empty())
                    else {
                        continue;
                    };
                    let page = self.physical_page(s, p, 0).unwrap_or(1) as u32;
                    entries.push((text, page, (s, p)));
                }
            }
        }
        entries
    }

    /// Style ids of a list's (title, entry) styles, when present.
    fn list_style_ids(&self, kind: Kind) -> (Option<usize>, Option<usize>) {
        let styles = &self.core.document().doc_info.styles;
        let find = |name: &str| styles.iter().position(|s| s.local_name == name);
        let ((title, _), (entry, _)) = kind.styles();
        (find(title), find(entry))
    }

    /// The first run of paragraphs in the list's styles and the options behind it.
    fn find_list(&self, kind: Kind) -> Option<(Block, ListOptions)> {
        let (title_id, entry_id) = self.list_style_ids(kind);
        let entry_id = entry_id?;
        let document = self.core.document();
        let is = |p: &hwpcraft_doc::model::paragraph::Paragraph, id: Option<usize>| {
            id == Some(usize::from(p.style_id))
        };
        for (s, section) in document.sections.iter().enumerate() {
            let Some(first) = section
                .paragraphs
                .iter()
                .position(|p| is(p, title_id) || is(p, Some(entry_id)))
            else {
                continue;
            };
            let title = is(&section.paragraphs[first], title_id);
            let entries: Vec<_> = section.paragraphs[first + usize::from(title)..]
                .iter()
                .take_while(|p| is(p, Some(entry_id)))
                .collect();
            let leader = entries
                .first()
                .and_then(|p| {
                    document
                        .doc_info
                        .para_shapes
                        .get(usize::from(p.para_shape_id))
                })
                .and_then(|shape| {
                    document
                        .doc_info
                        .tab_defs
                        .get(usize::from(shape.tab_def_id))
                })
                .and_then(|def| def.tabs.last())
                .map_or(Leader::None, |t| Leader::from_fill(t.fill_type));
            let options = ListOptions {
                title: title.then(|| section.paragraphs[first].text.clone()),
                leader,
                page_numbers: entries.iter().any(|p| p.text.contains('\t')),
                links: entries.first().is_some_and(|p| links_out(p)),
            };
            let block = Block {
                section: s,
                first,
                title,
                levels: vec![1; entries.len()],
            };
            return Some((block, options));
        }
        None
    }

    /// Create the list's styles a document lacks, after "차례 제목" / "차례 1"
    /// when the document has them.
    fn ensure_list_styles(&mut self, kind: Kind) -> Result<(usize, usize)> {
        let (title_id, entry_id) = self.list_style_ids(kind);
        let toc = self.toc_style_ids();
        let ((title_name, title_english), (entry_name, entry_english)) = kind.styles();
        let make = |s: &mut Self, base: Option<usize>, name: &str, english: &str| {
            let style = &s.core.document().doc_info.styles[base.unwrap_or(0)];
            let (para, chars) = (style.para_shape_id, style.char_shape_id);
            s.create_style_now(ext_format::NewStyle::plain(name, english, para, chars))
        };
        let title = match title_id {
            Some(id) => id,
            None => make(self, toc[0], title_name, title_english)?,
        };
        let entry = match entry_id {
            Some(id) => id,
            None => make(self, toc[1], entry_name, entry_english)?,
        };
        Ok((title, entry))
    }

    fn insert_list(&mut self, kind: Kind, p: TocParams) -> Result<Value> {
        let options = ListOptions {
            title: Some(kind.title().to_owned()),
            leader: Leader::Dot,
            page_numbers: true,
            links: true,
        }
        .merged(kind, p)?;
        if self.cursor.cell.is_some() {
            bail!(
                "{}는 본문에 넣을 수 있습니다. 표 밖에 캐럿을 두세요.",
                kind.title()
            );
        }
        let entries = self.caption_entries(kind);
        if entries.is_empty() {
            bail!(
                "{}에 넣을 캡션 달린 {}이(가) 없습니다. 먼저 캡션을 넣으세요.",
                kind.title(),
                if kind == Kind::Tables {
                    "표"
                } else {
                    "그림"
                }
            );
        }
        let at = self.cursor;
        let mut written = None;
        self.transaction(|s| {
            s.clear_selection();
            written = Some(s.write_list(at, kind, &options, &entries)?);
            Ok(())
        })?;
        let block = written.ok_or_else(|| anyhow!("차례를 쓰지 못했습니다."))?;
        Ok(Self::list_result(kind, &block))
    }

    fn update_list(&mut self, kind: Kind, p: TocParams) -> Result<Value> {
        let (block, existing) = self
            .find_list(kind)
            .ok_or_else(|| anyhow!("새로 고칠 {}가 없습니다. 먼저 만드세요.", kind.title()))?;
        let options = existing.merged(kind, p)?;
        let entries = self.caption_entries(kind);
        if entries.is_empty() {
            bail!("{}에 넣을 캡션이 없습니다.", kind.title());
        }
        let block =
            self.rewrite_block(&block, |s, at| s.write_list(at, kind, &options, &entries))?;
        Ok(Self::list_result(kind, &block))
    }

    fn list_result(kind: Kind, block: &Block) -> Value {
        json!({"kind": kind.name(), "entries": block.levels.len(), "section": block.section,
               "first": block.first, "paragraphs": block.paragraphs()})
    }

    fn write_list(
        &mut self,
        at: Cursor,
        kind: Kind,
        options: &ListOptions,
        entries: &[(String, u32)],
    ) -> Result<Block> {
        let (title_style, entry_style) = self.ensure_list_styles(kind)?;
        let mut lines = Vec::new();
        if let Some(title) = &options.title {
            lines.push((title.clone(), title_style, false));
        }
        for (caption, page) in entries {
            let line = if options.page_numbers {
                format!("{caption}\t{page}")
            } else {
                caption.clone()
            };
            lines.push((line, entry_style, options.page_numbers));
        }
        let block = Block {
            section: at.section,
            first: at.paragraph + usize::from(at.offset > 0),
            title: options.title.is_some(),
            levels: vec![1; entries.len()],
        };
        self.write_lines(at, block.first, &lines, options.leader)?;
        if options.page_numbers {
            self.settle_list_pages(kind, &block)?;
        }
        if options.links {
            let targets: Vec<_> = self
                .caption_targets(kind)
                .into_iter()
                .map(|(_, _, at)| at)
                .collect();
            self.link_entries(&block, &targets)?;
        }
        self.cursor = Cursor {
            section: at.section,
            paragraph: block.first + block.paragraphs(),
            offset: 0,
            cell: None,
        };
        Ok(block)
    }

    /// Make each entry's text (before the page tab) a link to a bookmark at its
    /// target paragraph, reusing a bookmark already there. One layout pass.
    fn link_entries(&mut self, block: &Block, targets: &[(usize, usize)]) -> Result<()> {
        if targets.len() != block.levels.len() {
            bail!("차례 항목과 대상 수가 맞지 않습니다.");
        }
        let mut at_paragraph = std::collections::HashMap::new();
        let mut names = std::collections::HashSet::new();
        for b in self.bookmarks()?.as_array().into_iter().flatten() {
            let n = |k: &str| b[k].as_u64().unwrap_or(0) as usize;
            if let Some(name) = b["name"].as_str() {
                names.insert(name.to_owned());
                at_paragraph
                    .entry((n("sec"), n("para")))
                    .or_insert_with(|| name.to_owned());
            }
        }
        let first_entry = block.first + usize::from(block.title);
        self.deferred_layout(|s| {
            for (i, (section, paragraph)) in targets.iter().copied().enumerate() {
                let name = match at_paragraph.get(&(section, paragraph)) {
                    Some(name) => name.clone(),
                    None => {
                        let base = format!("hwpcraft-ref-{section}-{paragraph}");
                        let mut name = base.clone();
                        let mut n = 1;
                        while names.contains(&name) {
                            n += 1;
                            name = format!("{base}-{n}");
                        }
                        let added: Value = serde_json::from_str(
                            &s.core
                                .add_bookmark_native(section, paragraph, 0, &name)
                                .map_err(err)?,
                        )?;
                        if added["ok"] == false {
                            bail!(
                                "{}",
                                added["error"]
                                    .as_str()
                                    .unwrap_or("책갈피를 달지 못했습니다.")
                            );
                        }
                        names.insert(name.clone());
                        at_paragraph.insert((section, paragraph), name.clone());
                        name
                    }
                };
                let entry = first_entry + i;
                let text = &s.core.document().sections[block.section].paragraphs[entry].text;
                let end = text
                    .chars()
                    .position(|c| c == '\t')
                    .unwrap_or_else(|| text.chars().count());
                if end == 0 {
                    continue;
                }
                let target = hwpcraft_doc::HyperlinkTarget::body(block.section, entry);
                let field_id = s
                    .core
                    .insert_hyperlink_native(&target, 0, end, super::cross_ref::SCAFFOLD_URI)
                    .map_err(err)?;
                let command = format!("?{name};0;0;0;");
                s.set_field_identity(block.section, entry, field_id, |f| {
                    f.parameters = bookmark_link_parameters(&command);
                    f.command = command.clone();
                    f.hyperlink_format = None;
                    f.raw_parameters_xml = None;
                })?;
            }
            Ok(())
        })?;
        self.refresh_layout()
    }

    fn settle_list_pages(&mut self, kind: Kind, block: &Block) -> Result<()> {
        let count = block.levels.len();
        self.settle_pages(block, &|s: &Self| {
            let entries = s.caption_entries(kind);
            if entries.len() != count {
                bail!("{}를 넣는 동안 캡션 수가 바뀌었습니다.", kind.title());
            }
            Ok(entries.into_iter().map(|(_, page)| page).collect())
        })
    }

    /// For a table made from styled headings: the style of the paragraph each
    /// entry names, per level, when every level resolves to one style.
    fn styles_behind(
        &self,
        block: &Block,
        entries: &[String],
        toc_styles: &[Option<usize>],
    ) -> Option<Vec<usize>> {
        let levels = usize::from(*block.levels.iter().max()?);
        let mut ids: Vec<Option<usize>> = vec![None; levels];
        for section in &self.core.document().sections {
            for paragraph in &section.paragraphs {
                let style = usize::from(paragraph.style_id);
                if toc_styles.contains(&Some(style)) {
                    continue;
                }
                let title = clean(&paragraph.text);
                if let Some(i) = entries.iter().position(|e| clean(e) == title) {
                    ids[usize::from(block.levels[i]) - 1].get_or_insert(style);
                }
            }
        }
        ids.into_iter().collect()
    }

    fn body_paragraph_len(&self, section: usize, paragraph: usize) -> Result<usize> {
        self.core
            .document()
            .sections
            .get(section)
            .and_then(|s| s.paragraphs.get(paragraph))
            .map(|p| p.text.chars().count())
            .ok_or_else(|| anyhow!("차례 문단을 찾지 못했습니다."))
    }

    /// Style ids of "차례 제목" (index 0) and "차례 1".."차례 10" (index n), when present.
    fn toc_style_ids(&self) -> Vec<Option<usize>> {
        let styles = &self.core.document().doc_info.styles;
        let find = |name: &str| styles.iter().position(|s| s.local_name == name);
        std::iter::once(find(TITLE_STYLE.0))
            .chain((1..=MAX_LEVELS).map(|n| find(&format!("차례 {n}"))))
            .collect()
    }

    /// The first run of body paragraphs in table-of-contents styles, and the
    /// options that reproduce it.
    fn find_toc(&self, styles: &[Option<usize>]) -> Option<(Block, TocOptions)> {
        let level_of = |style: u8| styles.iter().position(|id| *id == Some(usize::from(style)));
        let document = self.core.document();
        for (s, section) in document.sections.iter().enumerate() {
            let Some(first) = section
                .paragraphs
                .iter()
                .position(|p| level_of(p.style_id).is_some())
            else {
                continue;
            };
            let mut block = Block {
                section: s,
                first,
                title: false,
                levels: Vec::new(),
            };
            let mut options = TocOptions {
                title: None,
                page_numbers: false,
                ..TocOptions::default()
            };
            for (i, paragraph) in section.paragraphs[first..].iter().enumerate() {
                match level_of(paragraph.style_id) {
                    Some(0) if i == 0 => {
                        block.title = true;
                        options.title = Some(paragraph.text.clone());
                    }
                    Some(level) if level > 0 => {
                        let level = level as u8;
                        if block.levels.is_empty() {
                            let tab = document
                                .doc_info
                                .para_shapes
                                .get(usize::from(paragraph.para_shape_id))
                                .and_then(|shape| {
                                    document
                                        .doc_info
                                        .tab_defs
                                        .get(usize::from(shape.tab_def_id))
                                })
                                .and_then(|def| def.tabs.last());
                            options.leader = Leader::from_fill(tab.map_or(0, |t| t.fill_type));
                        }
                        options.page_numbers |= paragraph.text.contains('\t');
                        if block.levels.is_empty() {
                            options.links = links_out(paragraph);
                        }
                        block.levels.push(level);
                    }
                    _ => break,
                }
            }
            options.levels = block.levels.iter().copied().max().unwrap_or(3);
            return Some((block, options));
        }
        None
    }

    /// Create the "차례 제목" / "차례 N" styles a document lacks. Level N copies
    /// level N-1 (or 바탕글) indented by one more step.
    fn ensure_toc_styles(&mut self, levels: u8, title: bool) -> Result<Vec<Option<usize>>> {
        let mut ids = self.toc_style_ids();
        let base = |ids: &[Option<usize>], level: usize, doc: &Document| {
            let from = (1..level).rev().find_map(|l| ids[l]).unwrap_or(0);
            let style = &doc.doc_info.styles[from];
            (style.para_shape_id, style.char_shape_id)
        };
        if title && ids[0].is_none() {
            let (para, chars) = base(&ids, 0, self.core.document());
            ids[0] = Some(self.create_style_now(ext_format::NewStyle::plain(
                TITLE_STYLE.0,
                TITLE_STYLE.1,
                para,
                chars,
            ))?);
        }
        for level in 1..=usize::from(levels) {
            if ids[level].is_some() {
                continue;
            }
            let document = self.core.document();
            let (para, chars) = base(&ids, level, document);
            let margin = document
                .doc_info
                .para_shapes
                .get(usize::from(para))
                .map_or(0, |shape| shape.margin_left);
            let has_parent = (1..level).any(|l| ids[l].is_some());
            let mods = ParaShapeMods {
                margin_left: Some(margin + if has_parent { LEVEL_INDENT } else { 0 }),
                ..ParaShapeMods::default()
            };
            let para = self
                .core
                .document_mut()
                .find_or_create_para_shape(para, &mods);
            ids[level] = Some(self.create_style_now(ext_format::NewStyle::plain(
                &format!("차례 {level}"),
                &format!("TOC {level}"),
                para,
                chars,
            ))?);
        }
        Ok(ids)
    }

    fn toc_line(options: &TocOptions, heading: &Heading) -> String {
        let mut line = String::new();
        let number = clean(&heading.number);
        if options.numbers && !number.is_empty() {
            line.push_str(&number);
            line.push(' ');
        }
        line.push_str(&clean(&heading.title));
        if options.page_numbers {
            line.push('\t');
            line.push_str(&heading.page.to_string());
        }
        line
    }

    /// Write the table as new paragraphs at `at` (body), styled and with the
    /// page-number tab, then correct the page numbers after layout.
    fn write_toc(
        &mut self,
        at: Cursor,
        options: &TocOptions,
        headings: &[Heading],
    ) -> Result<Block> {
        let styles = self.ensure_toc_styles(options.levels, options.title.is_some())?;
        let section = at.section;
        let mut lines: Vec<(String, usize, bool)> = Vec::new();
        if let Some(title) = &options.title {
            lines.push((title.clone(), styles[0].unwrap_or(0), false));
        }
        for heading in headings {
            let style = styles[usize::from(heading.level)].unwrap_or(0);
            lines.push((
                Self::toc_line(options, heading),
                style,
                options.page_numbers,
            ));
        }
        let block = Block {
            section,
            first: at.paragraph + usize::from(at.offset > 0),
            title: options.title.is_some(),
            levels: headings.iter().map(|h| h.level).collect(),
        };
        self.write_lines(at, block.first, &lines, options.leader)?;
        if options.page_numbers {
            self.settle_toc_pages(&block, options)?;
        }
        if options.links {
            // Read again: the table moved the headings after it.
            let targets: Vec<_> = self
                .toc_headings(options)?
                .iter()
                .map(|h| (h.section, h.paragraph))
                .collect();
            self.link_entries(&block, &targets)?;
        }
        self.cursor = Cursor {
            section,
            paragraph: block.first + block.paragraphs(),
            offset: 0,
            cell: None,
        };
        Ok(block)
    }

    /// Write `lines` (text, style, page-number tab) as new body paragraphs from
    /// `first` on, splitting the caret's paragraph when the caret is inside it.
    pub(crate) fn write_lines(
        &mut self,
        at: Cursor,
        first: usize,
        lines: &[(String, usize, bool)],
        leader: Leader,
    ) -> Result<()> {
        let section = at.section;
        let mut shapes = std::collections::HashMap::new();
        self.deferred_layout(|s| {
            if at.offset > 0 {
                let split = s.split_offset(at)?;
                s.core
                    .split_paragraph_native(section, at.paragraph, split, None)
                    .map_err(err)?;
            }
            for (i, (line, style, tabbed)) in lines.iter().enumerate() {
                let paragraph = first + i;
                // Write before whatever follows the caret, then cut the line off,
                // so a section's first paragraph keeps its section definition.
                s.core
                    .insert_text_native(section, paragraph, 0, line)
                    .map_err(err)?;
                s.core
                    .split_paragraph_native(section, paragraph, line.chars().count(), None)
                    .map_err(err)?;
                // A fresh paragraph takes its style's shapes outright. RHWP's
                // apply_style_native rebuilds the section per call (7 ms each on
                // a 381-page document), so the shapes are set here and the
                // document is laid out once below.
                let tabbed = *tabbed;
                let shape = match shapes.get(&(*style, tabbed)) {
                    Some(shape) => *shape,
                    None => {
                        let shape = s.toc_para_shape(section, *style, tabbed.then_some(leader));
                        shapes.insert((*style, tabbed), shape);
                        shape
                    }
                };
                let char_shape = u32::from(s.core.document().doc_info.styles[*style].char_shape_id);
                let paragraph = &mut s.core.document_mut().sections[section].paragraphs[paragraph];
                paragraph.style_id = *style as u8;
                paragraph.para_shape_id = shape;
                paragraph.set_single_char_shape(char_shape);
            }
            s.core.document_mut().sections[section].raw_stream = None;
            Ok(())
        })?;
        self.refresh_layout()
    }

    /// The style's paragraph shape; with `leader`, plus a right-aligned tab
    /// at the right edge of the paragraph's text area for the page number.
    fn toc_para_shape(&mut self, section: usize, style: usize, leader: Option<Leader>) -> u16 {
        let document = self.core.document_mut();
        let base = document.doc_info.styles[style].para_shape_id;
        let Some(leader) = leader else {
            return base;
        };
        let page = &document.sections[section].section_def.page_def;
        let body = i64::from(page.width)
            - i64::from(page.margin_left)
            - i64::from(page.margin_right)
            - i64::from(page.margin_gutter);
        let (left, right) = document
            .doc_info
            .para_shapes
            .get(usize::from(base))
            .map_or((0, 0), |s| {
                (i64::from(s.margin_left), i64::from(s.margin_right))
            });
        // Tab positions, like paragraph margins, are stored at twice HWPUNIT.
        let position = (body * 2 - left - right).clamp(0, i64::from(u32::MAX)) as u32;
        let tab = document.find_or_create_tab_def(TabDef {
            raw_data: None,
            attr: 0,
            tabs: vec![TabItem {
                position,
                tab_type: 1,
                fill_type: leader.fill(),
            }],
            auto_tab_left: false,
            auto_tab_right: false,
        });
        let mods = ParaShapeMods {
            tab_def_id: Some(tab),
            ..ParaShapeMods::default()
        };
        document.find_or_create_para_shape(base, &mods)
    }

    /// Rewrite each entry's page number from the current layout until stable.
    fn settle_toc_pages(&mut self, block: &Block, options: &TocOptions) -> Result<()> {
        let count = block.levels.len();
        self.settle_pages(block, &|s: &Self| {
            let headings = s.toc_headings(options)?;
            if headings.len() != count {
                bail!("차례를 넣는 동안 개요 문단 수가 바뀌었습니다.");
            }
            Ok(headings.iter().map(|h| h.page).collect())
        })
    }

    /// Rewrite the page after each entry's tab with `pages` (one per entry, read
    /// from the current layout) until they stop changing.
    fn settle_pages(
        &mut self,
        block: &Block,
        pages: &dyn Fn(&Self) -> Result<Vec<u32>>,
    ) -> Result<()> {
        let first_entry = block.first + usize::from(block.title);
        for _ in 0..PAGE_PASSES {
            let expected = pages(self)?;
            let paragraphs = &self.core.document().sections[block.section].paragraphs;
            let stale: Vec<(usize, usize, usize, String)> = expected
                .iter()
                .enumerate()
                .filter_map(|(i, page)| {
                    let paragraph = first_entry + i;
                    let text = &paragraphs[paragraph].text;
                    let tab = text.chars().position(|c| c == '\t')?;
                    let page = page.to_string();
                    let current: String = text.chars().skip(tab + 1).collect();
                    (current != page).then(|| (paragraph, tab + 1, text.chars().count(), page))
                })
                .collect();
            if stale.is_empty() {
                return Ok(());
            }
            // One layout pass for the whole correction, not one per entry.
            self.deferred_layout(|s| {
                for (paragraph, start, end, page) in &stale {
                    s.core
                        .delete_range_native(
                            block.section,
                            *paragraph,
                            *start,
                            *paragraph,
                            *end,
                            None,
                        )
                        .map_err(err)?;
                    s.core
                        .insert_text_native(block.section, *paragraph, *start, page)
                        .map_err(err)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Result<Session> {
        let mut s = Session::new()?;
        s.run(
            "text.insert",
            json!({"text":"머리 문단\n서론\n본문 하나\n본론\n세부 항목\n본문 둘"}),
        )?;
        for (paragraph, level) in [(1, 1), (3, 1), (4, 2)] {
            s.cursor = Cursor {
                section: 0,
                paragraph,
                offset: 0,
                cell: None,
            };
            s.run("style.apply", json!({"id": 1 + level}))?;
        }
        // Push 본론 to the second page: break at the end of the paragraph before it.
        s.cursor = Cursor {
            section: 0,
            paragraph: 2,
            offset: s.body_paragraph_len(0, 2)?,
            cell: None,
        };
        s.run("insert.page_break", json!({}))?;
        Ok(s)
    }

    fn texts(s: &Session) -> Vec<String> {
        s.core.document().sections[0]
            .paragraphs
            .iter()
            .map(|p| p.text.clone())
            .collect()
    }

    #[test]
    fn inserts_styled_entries_with_settled_page_numbers() -> Result<()> {
        let mut s = report()?;
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: None,
        };
        let out = s.run("tools.toc", json!({}))?;
        assert_eq!(out["entries"], 3);
        let text = texts(&s);
        assert_eq!(text[0], "차례");
        assert!(text[1].ends_with("서론\t1"), "{text:?}");
        assert!(text[2].ends_with("본론\t2"), "{text:?}");
        assert!(text[3].contains("세부 항목\t2"), "{text:?}");
        assert_eq!(text[4], "머리 문단");
        let styles = s.toc_style_ids();
        let paragraphs = &s.core.document().sections[0].paragraphs;
        assert_eq!(Some(usize::from(paragraphs[0].style_id)), styles[0]);
        assert_eq!(Some(usize::from(paragraphs[1].style_id)), styles[1]);
        assert_eq!(Some(usize::from(paragraphs[3].style_id)), styles[2]);
        // The entry carries a right tab with a dotted leader.
        let doc = s.core.document();
        let shape = &doc.doc_info.para_shapes[usize::from(paragraphs[1].para_shape_id)];
        let tab = doc.doc_info.tab_defs[usize::from(shape.tab_def_id)]
            .tabs
            .last()
            .cloned();
        assert_eq!(tab.map(|t| (t.tab_type, t.fill_type)), Some((1, 3)));
        // One undo removes the whole table.
        s.run("edit.undo", json!({}))?;
        assert_eq!(texts(&s)[0], "머리 문단");
        Ok(())
    }

    #[test]
    fn update_rewrites_the_table_after_headings_change() -> Result<()> {
        let mut s = report()?;
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: None,
        };
        s.run("tools.toc", json!({"levels":1,"leader":"dash"}))?;
        let before = texts(&s).len();
        // A new level-1 heading at the end.
        let last = s.core.document().sections[0].paragraphs.len() - 1;
        s.cursor = Cursor {
            section: 0,
            paragraph: last,
            offset: s.body_paragraph_len(0, last)?,
            cell: None,
        };
        s.run("text.insert", json!({"text":"\n결론"}))?;
        s.run("style.apply", json!({"id": 2}))?;
        let out = s.run("tools.toc.update", json!({}))?;
        assert_eq!(out["entries"], 3);
        let text = texts(&s);
        // Same title, one more entry, nothing left over from the old table.
        assert_eq!(text.len(), before + 2, "{text:?}");
        assert_eq!(text[0], "차례");
        assert!(text[3].contains("결론\t2"), "{text:?}");
        assert_eq!(text[4], "머리 문단");
        // The leader and level choice survived the update.
        let doc = s.core.document();
        let paragraphs = &doc.sections[0].paragraphs;
        let shape = &doc.doc_info.para_shapes[usize::from(paragraphs[1].para_shape_id)];
        let tab = doc.doc_info.tab_defs[usize::from(shape.tab_def_id)]
            .tabs
            .last()
            .cloned();
        assert_eq!(tab.map(|t| t.fill_type), Some(2));
        assert!(!text.iter().any(|t| t.contains("세부 항목\t")), "{text:?}");
        Ok(())
    }

    #[test]
    fn first_paragraph_keeps_its_section_definition_through_save() -> Result<()> {
        let mut s = report()?;
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: None,
        };
        s.run("tools.toc", json!({"title":null,"numbers":false}))?;
        s.run("tools.toc.update", json!({}))?;
        let dir = tempfile::tempdir()?;
        for name in ["toc.hwpx", "toc.hwp"] {
            let path = dir.path().join(name);
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            let text = texts(&reopened);
            assert!(text[0].starts_with("서론\t1"), "{name}: {text:?}");
            assert_eq!(reopened.page_count()?, s.page_count()?, "{name}");
            let page = &reopened.core.document().sections[0].section_def.page_def;
            let original = &s.core.document().sections[0].section_def.page_def;
            assert_eq!(
                (page.width, page.height),
                (original.width, original.height),
                "{name}"
            );
            // The reopened table is still found and refreshed in place.
            reopened.run("tools.toc.update", json!({}))?;
            assert_eq!(texts(&reopened).len(), text.len(), "{name}");
        }
        Ok(())
    }

    #[test]
    fn refuses_cells_missing_outline_and_bad_levels() -> Result<()> {
        let mut s = Session::new()?;
        s.run("text.insert", json!({"text":"개요 없음"}))?;
        assert!(s.run("tools.toc", json!({})).is_err());
        assert!(s.run("tools.toc.update", json!({})).is_err());
        let mut s = report()?;
        assert!(s.run("tools.toc", json!({"levels":0})).is_err());
        assert!(s.run("tools.toc", json!({"levels":11})).is_err());
        assert!(s.run("tools.toc", json!({"unknown":1})).is_err());
        assert!(
            s.run("tools.toc", json!({"styles":["없는 스타일"]}))
                .is_err()
        );
        assert!(s.run("tools.toc", json!({"styles":[]})).is_err());
        Ok(())
    }

    /// Hand-numbered headings in custom styles, as many public documents use.
    #[test]
    fn collects_styled_headings_and_refreshes_them_without_parameters() -> Result<()> {
        let mut s = Session::new()?;
        s.run(
            "text.insert",
            json!({"text":"앞 문단\nⅠ. 개관\n본문\n1. 목적\n본문\nⅡ. 절차"}),
        )?;
        for (name, paragraphs) in [("++장", [1, 5]), ("++절", [3, 3])] {
            s.cursor = Cursor {
                section: 0,
                paragraph: paragraphs[0],
                offset: 0,
                cell: None,
            };
            let id = s.run("format.style.create", json!({"name":name}))?["id"].clone();
            for paragraph in paragraphs {
                s.cursor.paragraph = paragraph;
                s.run("style.apply", json!({"id":id}))?;
            }
        }
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: None,
        };
        let out = s.run("tools.toc", json!({"styles":["++장","++절"],"title":null}))?;
        assert_eq!(out["entries"], 3);
        let text = texts(&s);
        assert_eq!(&text[..3], ["Ⅰ. 개관\t1", "1. 목적\t1", "Ⅱ. 절차\t1"]);
        // A new section heading appears; refresh finds the styles from the entries.
        let last = s.core.document().sections[0].paragraphs.len() - 1;
        s.cursor = Cursor {
            section: 0,
            paragraph: last,
            offset: s.body_paragraph_len(0, last)?,
            cell: None,
        };
        s.run("text.insert", json!({"text":"\n2. 범위"}))?;
        let id = s
            .core
            .document()
            .doc_info
            .styles
            .iter()
            .position(|st| st.local_name == "++절")
            .ok_or_else(|| anyhow!("style"))?;
        s.run("style.apply", json!({"id":id}))?;
        s.run("tools.toc.update", json!({}))?;
        let text = texts(&s);
        assert_eq!(text[3], "2. 범위\t1", "{text:?}");
        assert_eq!(text[4], "앞 문단", "{text:?}");
        Ok(())
    }

    fn captioned_table(s: &mut Session, caption: &str) -> Result<()> {
        s.run("caret.move", json!({"motion": "document_end"}))?;
        s.run("text.insert", json!({"text": "\n"}))?;
        let made = s.run("insert.table", json!({"rows": 1, "columns": 1}))?;
        s.run(
            "table.properties",
            json!({"table": made["table"], "props": {"hasCaption": true}}),
        )?;
        s.run(
            "table.caption_text",
            json!({"table": made["table"], "text": caption}),
        )?;
        Ok(())
    }

    fn list_lines(s: &Session, title: &str) -> Vec<String> {
        let text = texts(s);
        let Some(first) = text.iter().position(|t| t == title) else {
            return Vec::new();
        };
        text[first + 1..]
            .iter()
            .take_while(|t| t.contains('\t'))
            .cloned()
            .collect()
    }

    /// 표 차례 and 그림 차례: captions as shown with their pages, refreshed together
    /// with the contents in one undo step, the same after an HWP or HWPX reopen.
    #[test]
    fn table_and_figure_lists_follow_the_captions() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let png = dir.path().join("figure.png");
        image::RgbaImage::from_pixel(8, 6, image::Rgba([68, 114, 196, 255])).save(&png)?;
        let mut s = report()?;
        captioned_table(&mut s, "예산")?;
        s.run("caret.move", json!({"motion": "document_end"}))?;
        s.run("insert.page_break", json!({}))?;
        captioned_table(&mut s, "결산")?;
        s.run("caret.move", json!({"motion": "document_end"}))?;
        s.run("text.insert", json!({"text": "\n"}))?;
        let picture = s.run(
            "picture.insert",
            json!({"path": png, "width_mm": 20.0, "treat_as_char": true}),
        )?;
        s.run(
            "object.properties",
            json!({"address": picture["address"], "props": {"hasCaption": true}}),
        )?;
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: None,
        };
        s.run("tools.toc", json!({}))?;
        let tables = s.run("tools.toc", json!({"kind": "tables"}))?;
        assert_eq!(tables["entries"], 2, "{tables}");
        let pictures = s.run("tools.toc", json!({"kind": "pictures", "leader": "dash"}))?;
        assert_eq!(pictures["entries"], 1, "{pictures}");
        let pages = s.page_count()?;
        assert_eq!(
            list_lines(&s, "표 차례"),
            [
                format!("표 1 예산\t{}", pages - 1),
                format!("표 2 결산\t{pages}")
            ]
        );
        assert_eq!(list_lines(&s, "그림 차례"), [format!("그림 1\t{pages}")]);
        // The headings' contents does not take the lists in.
        assert!(!list_lines(&s, "차례").iter().any(|l| l.contains("표 1")));
        // A third table; one update refreshes all three tables of contents.
        captioned_table(&mut s, "추가")?;
        let updated = s.run("tools.toc.update", json!({}))?;
        assert_eq!(updated["entries"], 3, "{updated}");
        assert_eq!(updated["lists"][0]["kind"], "tables", "{updated}");
        assert_eq!(updated["lists"][0]["entries"], 3, "{updated}");
        assert_eq!(updated["lists"][1]["entries"], 1, "{updated}");
        let three = list_lines(&s, "표 차례");
        assert_eq!(three.len(), 3, "{three:?}");
        assert!(three[2].starts_with("표 3 추가\t"), "{three:?}");
        // The leader chosen for the figure list survived the update.
        let doc = s.core.document();
        let first = texts(&s)
            .iter()
            .position(|t| t == "그림 차례")
            .ok_or_else(|| anyhow!("list"))?;
        let paragraph = &doc.sections[0].paragraphs[first + 1];
        let shape = &doc.doc_info.para_shapes[usize::from(paragraph.para_shape_id)];
        let tab = doc.doc_info.tab_defs[usize::from(shape.tab_def_id)]
            .tabs
            .last()
            .map(|t| t.fill_type);
        assert_eq!(tab, Some(2));
        // Saved and reopened, both lists read the same and refresh in place.
        let lists = (list_lines(&s, "표 차례"), list_lines(&s, "그림 차례"));
        for name in ["lists.hwp", "lists.hwpx"] {
            let path = dir.path().join(name);
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            let read = (
                list_lines(&reopened, "표 차례"),
                list_lines(&reopened, "그림 차례"),
            );
            assert_eq!(read, lists, "{name}");
            let length = texts(&reopened).len();
            reopened.run("tools.toc.update", json!({}))?;
            assert_eq!(texts(&reopened).len(), length, "{name}");
            assert_eq!(list_lines(&reopened, "표 차례"), lists.0, "{name}");
        }
        // One undo takes the whole refresh back.
        s.run("edit.undo", json!({}))?;
        assert_eq!(list_lines(&s, "표 차례").len(), 2);
        Ok(())
    }

    #[test]
    fn lists_refuse_heading_options_and_missing_captions() -> Result<()> {
        let mut s = report()?;
        assert!(s.run("tools.toc", json!({"kind": "tables"})).is_err());
        assert!(
            s.run("tools.toc.update", json!({"kind": "pictures"}))
                .is_err()
        );
        captioned_table(&mut s, "예산")?;
        assert!(
            s.run("tools.toc", json!({"kind": "tables", "levels": 2}))
                .is_err()
        );
        assert!(s.run("tools.toc", json!({"kind": "figures"})).is_err());
        Ok(())
    }

    /// (link command, linked text) of each hyperlink in a paragraph.
    fn links(s: &Session, paragraph: usize) -> Vec<(String, String)> {
        let para = &s.core.document().sections[0].paragraphs[paragraph];
        para.field_ranges
            .iter()
            .filter_map(|r| match para.controls.get(r.control_idx) {
                Some(Control::Field(f)) if f.field_type == FieldType::Hyperlink => Some((
                    f.command.clone(),
                    para.text
                        .chars()
                        .skip(r.start_char_idx)
                        .take(r.end_char_idx - r.start_char_idx)
                        .collect(),
                )),
                _ => None,
            })
            .collect()
    }

    /// Entries link to bookmarks at what they list, in Hangul's bookmark-link form;
    /// following one lands on the heading, also after an HWP or HWPX reopen and
    /// after a refresh, which reuses the bookmarks instead of adding more.
    #[test]
    fn entries_link_to_their_headings_and_survive_saves() -> Result<()> {
        let mut s = report()?;
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: None,
        };
        s.run("tools.toc", json!({}))?;
        let entry = links(&s, 2);
        assert_eq!(entry.len(), 1, "{entry:?}");
        let (command, text) = &entry[0];
        assert!(text.ends_with("본론") && !text.contains('\t'), "{text}");
        let name = command
            .strip_prefix('?')
            .and_then(|c| c.strip_suffix(";0;0;0;"))
            .ok_or_else(|| anyhow!("{command}"))?
            .to_owned();
        let field = s.core.document().sections[0].paragraphs[2]
            .controls
            .iter()
            .find_map(|c| match c {
                Control::Field(f) if f.field_type == FieldType::Hyperlink => Some(f.clone()),
                _ => None,
            })
            .ok_or_else(|| anyhow!("field"))?;
        let params = field.parameters.render_xml("parameters");
        assert!(
            params.contains("HWPHYPERLINK_TARGET_BOOKMARK")
                && params.contains("HWPHYPERLINK_TYPE_HWP"),
            "{params}"
        );
        let bookmarks = s.bookmarks()?.as_array().cloned().unwrap_or_default().len();
        // Following the link lands on 본론.
        assert!(s.follow_internal_link(&format!("?{name}"))?);
        assert_eq!(
            s.core.document().sections[0].paragraphs[s.cursor.paragraph].text,
            "본론"
        );
        assert!(!s.follow_internal_link("https://example.com")?);
        // A refresh links again without new bookmarks.
        s.run("tools.toc.update", json!({}))?;
        assert_eq!(links(&s, 2), entry);
        assert_eq!(
            s.bookmarks()?.as_array().cloned().unwrap_or_default().len(),
            bookmarks
        );
        let dir = tempfile::tempdir()?;
        for file in ["links.hwp", "links.hwpx"] {
            let path = dir.path().join(file);
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            assert_eq!(links(&reopened, 2), entry, "{file}");
            assert!(
                reopened.follow_internal_link(&format!("?{name}"))?,
                "{file}"
            );
            let at = reopened.cursor.paragraph;
            assert_eq!(
                reopened.core.document().sections[0].paragraphs[at].text,
                "본론",
                "{file}"
            );
            // Refreshed after reopening, the links are found and rewritten.
            reopened.run("tools.toc.update", json!({}))?;
            assert_eq!(links(&reopened, 2), entry, "{file}");
        }
        // links: false writes plain entries, and a refresh keeps them plain.
        s.run("edit.undo", json!({}))?;
        s.run("edit.undo", json!({}))?;
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: None,
        };
        s.run("tools.toc", json!({"links": false}))?;
        assert!(links(&s, 2).is_empty());
        s.run("tools.toc.update", json!({}))?;
        assert!(links(&s, 2).is_empty());
        Ok(())
    }
}
