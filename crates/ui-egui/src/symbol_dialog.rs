//! Owner: core area. 문자표 (Ctrl+F10): Hangul 2024's four tables (사용자·유니코드·
//! 한글(HNC)·완성형(KS)) plus 이모지, 특수 문자 and 검색, backed by
//! `hwpcraft_engine::symbol_*`. Inserts through `insert.symbol`, so the CLI and MCP
//! do exactly what the dialog does.
use super::*;
use hwpcraft_engine::{SymbolGroup, SymbolSearch, SymbolSet};
use serde::{Deserialize, Serialize};

const COLS: usize = 16;
const CELL: f32 = 30.0;
const GRID_ROWS: usize = 8;
const RECENT_LIMIT: usize = 24;
const FAVORITES_LIMIT: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tab {
    User,
    Unicode,
    Hnc,
    Ks,
    Emoji,
    Special,
    Search,
}
impl Tab {
    const ALL: [Tab; 7] = [
        Tab::User,
        Tab::Unicode,
        Tab::Hnc,
        Tab::Ks,
        Tab::Emoji,
        Tab::Special,
        Tab::Search,
    ];
    fn label(self) -> &'static str {
        match self {
            Tab::User => "사용자 문자표",
            Tab::Unicode => "유니코드 문자표",
            Tab::Hnc => "한글(HNC) 문자표",
            Tab::Ks => "완성형(KS) 문자표",
            Tab::Emoji => "이모지",
            Tab::Special => "특수 문자",
            Tab::Search => "검색",
        }
    }
    fn group(self) -> Option<SymbolGroup> {
        match self {
            Tab::Unicode => Some(SymbolGroup::Unicode),
            Tab::Hnc => Some(SymbolGroup::Hnc),
            Tab::Ks => Some(SymbolGroup::Ks),
            Tab::Emoji => Some(SymbolGroup::Emoji),
            Tab::Special => Some(SymbolGroup::Special),
            Tab::User | Tab::Search => None,
        }
    }
}

/// What survives between sessions: `symbols.json` in the settings folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Saved {
    pub recent: Vec<char>,
    pub favorites: Vec<char>,
    pub font: String,
}
impl Saved {
    pub(crate) fn remember(&mut self, text: &str) {
        for c in text.chars().rev() {
            self.recent.retain(|r| *r != c);
            self.recent.insert(0, c);
        }
        self.recent.truncate(RECENT_LIMIT);
    }
    pub(crate) fn toggle_favorite(&mut self, c: char) -> bool {
        if let Some(i) = self.favorites.iter().position(|f| *f == c) {
            self.favorites.remove(i);
            false
        } else {
            self.favorites.push(c);
            self.favorites.truncate(FAVORITES_LIMIT);
            true
        }
    }
    pub(crate) fn parse(text: &str) -> Saved {
        serde_json::from_str(text).unwrap_or_default()
    }
    pub(crate) fn serialize(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
    fn path() -> Option<PathBuf> {
        ext_shell::config_dir().map(|d| d.join("symbols.json"))
    }
    fn load() -> Saved {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }
    fn store(&self) {
        if let Some(path) = Self::path() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, self.serialize());
        }
    }
}

pub(crate) struct State {
    tab: Tab,
    sets: Vec<SymbolSet>,
    /// Selected set id per tab that lists sets.
    set_id: std::collections::BTreeMap<&'static str, String>,
    chars: Vec<char>,
    chars_of: String,
    only_available: bool,
    input: String,
    /// Font for the inserted characters; empty = the text's current font.
    font: String,
    families: Vec<String>,
    search: String,
    result: SymbolSearch,
    selected: Option<char>,
    scroll_to: Option<usize>,
    saved: Saved,
    /// Characters egui's fonts can draw, cached per character.
    glyphs: std::collections::HashMap<char, bool>,
}

impl State {
    pub(crate) fn new(families: Vec<String>) -> Self {
        let saved = Saved::load();
        let mut s = Self {
            tab: Tab::Hnc,
            sets: hwpcraft_engine::symbol_sets(),
            set_id: Default::default(),
            chars: Vec::new(),
            chars_of: String::new(),
            only_available: false,
            input: String::new(),
            font: saved.font.clone(),
            families,
            search: String::new(),
            result: SymbolSearch::default(),
            selected: None,
            scroll_to: None,
            saved,
            glyphs: Default::default(),
        };
        s.load_set();
        s
    }
    fn tab_key(tab: Tab) -> &'static str {
        tab.label()
    }
    fn sets_of(&self, tab: Tab) -> Vec<&SymbolSet> {
        let Some(group) = tab.group() else {
            return Vec::new();
        };
        self.sets.iter().filter(|s| s.group == group).collect()
    }
    fn current_set(&self) -> Option<String> {
        let key = Self::tab_key(self.tab);
        self.set_id
            .get(key)
            .cloned()
            .or_else(|| self.sets_of(self.tab).first().map(|s| s.id.to_owned()))
    }
    fn load_set(&mut self) {
        let chars = match self.tab {
            Tab::User => self.saved.favorites.clone(),
            // Keyword searches show the first matching area in the grid.
            Tab::Search if self.result.chars.is_empty() => self
                .result
                .sets
                .first()
                .and_then(|s| hwpcraft_engine::symbol_chars(s.id))
                .unwrap_or_default(),
            Tab::Search => self.result.chars.clone(),
            _ => self
                .current_set()
                .and_then(|id| hwpcraft_engine::symbol_chars(&id))
                .unwrap_or_default(),
        };
        let key = match self.tab {
            Tab::User => "user".to_owned(),
            Tab::Search => format!("search:{}", self.search),
            _ => self.current_set().unwrap_or_default(),
        };
        if key != self.chars_of || chars.len() != self.chars.len() {
            self.chars = chars;
            self.chars_of = key;
            self.scroll_to = Some(0);
        }
    }
    fn select_set(&mut self, id: &str) {
        self.set_id.insert(Self::tab_key(self.tab), id.to_owned());
        self.load_set();
    }
    /// Jump to the tab/set a search result names.
    fn show_set(&mut self, set: &SymbolSet) {
        self.tab = match set.group {
            SymbolGroup::Hnc => Tab::Hnc,
            SymbolGroup::Ks => Tab::Ks,
            SymbolGroup::Unicode => Tab::Unicode,
            SymbolGroup::Emoji => Tab::Emoji,
            SymbolGroup::Special => Tab::Special,
        };
        self.select_set(set.id);
    }
    fn available(&mut self, ctx: &egui::Context, c: char) -> bool {
        if let Some(v) = self.glyphs.get(&c) {
            return *v;
        }
        let font = egui::FontId::proportional(18.0);
        let v = ctx.fonts_mut(|f| f.has_glyph(&font, c));
        self.glyphs.insert(c, v);
        v
    }
    fn visible_chars(&mut self, ctx: &egui::Context) -> Vec<char> {
        if !self.only_available {
            return self.chars.clone();
        }
        let chars = self.chars.clone();
        chars
            .into_iter()
            .filter(|c| self.available(ctx, *c))
            .collect()
    }
    /// Start-up view: a tab name, optionally `search:<query>`.
    pub(crate) fn show_view(&mut self, name: &str) {
        if let Some(query) = name.strip_prefix("search:") {
            self.search = query.to_owned();
            self.result = hwpcraft_engine::symbol_search(query);
            self.tab = Tab::Search;
        } else {
            self.tab = match name {
                "user" => Tab::User,
                "unicode" => Tab::Unicode,
                "ks" => Tab::Ks,
                "emoji" => Tab::Emoji,
                "special" => Tab::Special,
                "search" => Tab::Search,
                _ => Tab::Hnc,
            };
        }
        self.load_set();
    }
    fn insert_params(&self, text: &str) -> serde_json::Value {
        if self.font.trim().is_empty() {
            json!({ "text": text })
        } else {
            json!({ "text": text, "font": self.font })
        }
    }
}

/// Code labels Hangul shows under the grid: Unicode, KS X 1001 and the block name.
pub(crate) fn code_labels(c: char) -> (String, String, String) {
    let unicode = format!("U+{:04X} ({})", c as u32, c as u32);
    let ks = hwpcraft_engine::ks_code(c)
        .map(|b| format!("{:02X}{:02X}", b[0], b[1]))
        .unwrap_or_else(|| "—".into());
    let block = hwpcraft_engine::block_of(c).unwrap_or("").to_owned();
    (unicode, ks, block)
}

impl HwpApp {
    pub(crate) fn open_symbol_dialog(&mut self) {
        let families = self.renderer.font_families();
        self.symbol_dialog = Some(State::new(families));
    }
    pub(crate) fn symbol_dialog_open(&self) -> bool {
        self.symbol_dialog.is_some()
    }
    pub(crate) fn symbol_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.symbol_dialog.take() else {
            return;
        };
        let mut close = false;
        let mut insert: Option<String> = None;
        chrome::modal("symbol-dialog").show(ctx, |ui| {
            ui.set_width(COLS as f32 * CELL + 310.0);
            if chrome::title(ui, "문자표") {
                close = true;
            }
            // Tabs (Hangul: 사용자 / 유니코드 / 한글(HNC) / 완성형(KS); HwpCraft adds three).
            ui.horizontal(|ui| {
                for tab in Tab::ALL {
                    if ui.selectable_label(st.tab == tab, tab.label()).clicked() && st.tab != tab {
                        st.tab = tab;
                        st.load_set();
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut st.search)
                            .hint_text("검색: 이름·U+코드·키워드")
                            .desired_width(190.0),
                    );
                    if r.changed() {
                        st.result = hwpcraft_engine::symbol_search(&st.search);
                        if !st.search.trim().is_empty() {
                            st.tab = Tab::Search;
                        }
                        st.load_set();
                    }
                });
            });
            ui.separator();
            ui.horizontal_top(|ui| {
                // Left: 문자 영역 list.
                ui.vertical(|ui| {
                    ui.set_width(190.0);
                    ui.label("문자 영역");
                    egui::ScrollArea::vertical()
                        .id_salt("symbol-sets")
                        .max_height(CELL * GRID_ROWS as f32 + 40.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| match st.tab {
                            Tab::User => {
                                ui.label(format!("등록한 문자 {}개", st.saved.favorites.len()));
                                ui.label(
                                    egui::RichText::new(
                                        "다른 탭에서 문자를 고른 뒤 [등록]을 누르면 여기에 모입니다.",
                                    )
                                    .weak(),
                                );
                            }
                            Tab::Search => {
                                let sets: Vec<SymbolSet> = st.result.sets.clone();
                                ui.label(format!("문자 {}개 · 영역 {}개", st.result.chars.len(), sets.len()));
                                for set in &sets {
                                    if ui
                                        .selectable_label(false, format!("{} ({})", set.label, set.count))
                                        .clicked()
                                    {
                                        st.show_set(set);
                                    }
                                }
                            }
                            _ => {
                                let current = st.current_set().unwrap_or_default();
                                let sets: Vec<(String, String)> = st
                                    .sets_of(st.tab)
                                    .iter()
                                    .map(|s| (s.id.to_owned(), format!("{} ({})", s.label, s.count)))
                                    .collect();
                                for (id, label) in sets {
                                    if ui.selectable_label(current == id, label).clicked() {
                                        st.select_set(&id);
                                    }
                                }
                            }
                        });
                });
                ui.separator();
                // Middle: grid.
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut st.only_available, "글꼴에 있는 문자만");
                        ui.label(egui::RichText::new(format!("{}자", st.chars.len())).weak());
                    });
                    let chars = st.visible_chars(ctx);
                    let rows = chars.len().div_ceil(COLS);
                    let mut area = egui::ScrollArea::vertical()
                        .id_salt("symbol-grid")
                        .max_height(CELL * GRID_ROWS as f32)
                        .auto_shrink([false, false]);
                    if let Some(row) = st.scroll_to.take() {
                        area = area.vertical_scroll_offset(row as f32 * CELL);
                    }
                    let mut picked = None;
                    let mut double = None;
                    area.show_rows(ui, CELL, rows, |ui, range| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for row in range {
                            ui.horizontal(|ui| {
                                for c in chars.iter().skip(row * COLS).take(COLS) {
                                    let ok = st.available(ctx, *c);
                                    let r = cell(ui, *c, st.selected == Some(*c), ok);
                                    if r.clicked() {
                                        picked = Some(*c);
                                    }
                                    if r.double_clicked() {
                                        double = Some(*c);
                                    }
                                }
                            });
                        }
                    });
                    if chars.is_empty() {
                        ui.label(egui::RichText::new("표시할 문자가 없습니다.").weak());
                    }
                    if let Some(c) = picked {
                        st.selected = Some(c);
                        st.input.push(c);
                    }
                    if let Some(c) = double {
                        st.selected = Some(c);
                        insert = Some(c.to_string());
                    }
                    ui.add_space(4.0);
                    // Codes of the selected character.
                    if let Some(c) = st.selected {
                        let (u, ks, block) = code_labels(c);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(c.to_string()).size(20.0));
                            ui.label(format!("유니코드 {u}"));
                            ui.label(format!("완성형 {ks}"));
                            if !block.is_empty() {
                                ui.label(egui::RichText::new(block).weak());
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("입력 문자");
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut st.input).desired_width(300.0),
                        );
                        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !st.input.is_empty()
                        {
                            insert = Some(st.input.clone());
                        }
                        if ui.button("지우기").clicked() {
                            st.input.clear();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("최근 사용");
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        let recent = st.saved.recent.clone();
                        for c in recent.iter().take(COLS) {
                            let ok = st.available(ctx, *c);
                            let r = cell(ui, *c, false, ok);
                            if r.clicked() {
                                st.selected = Some(*c);
                                st.input.push(*c);
                            }
                            if r.double_clicked() {
                                insert = Some(c.to_string());
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("선택 글꼴");
                        let shown = if st.font.is_empty() {
                            "(현재 글꼴)".to_owned()
                        } else {
                            st.font.clone()
                        };
                        egui::ComboBox::from_id_salt("symbol-font")
                            .selected_text(shown)
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(st.font.is_empty(), "(현재 글꼴)").clicked() {
                                    st.font.clear();
                                }
                                let families = st.families.clone();
                                for f in families {
                                    if ui.selectable_label(st.font == f, &f).clicked() {
                                        st.font = f;
                                    }
                                }
                            });
                        let registered = st.selected.is_some_and(|c| st.saved.favorites.contains(&c));
                        if ui
                            .add_enabled(
                                st.selected.is_some(),
                                egui::Button::new(if registered { "등록 해제" } else { "등록" }),
                            )
                            .clicked()
                            && let Some(c) = st.selected
                        {
                            st.saved.toggle_favorite(c);
                            st.saved.store();
                            if st.tab == Tab::User {
                                st.chars_of.clear();
                                st.load_set();
                            }
                        }
                    });
                });
                ui.separator();
                // Right: 넣기 / 닫기 like Hangul's right-hand buttons.
                ui.vertical(|ui| {
                    let (ok, cancel) = chrome::actions(ui, !st.input.is_empty(), "넣기");
                    if ok {
                        insert = Some(st.input.clone());
                    }
                    if cancel {
                        close = true;
                    }
                });
            });
        });
        if let Some(text) = insert {
            let params = st.insert_params(&text);
            if self.run("insert.symbol", params) {
                st.saved.remember(&text);
                st.saved.font = st.font.clone();
                st.saved.store();
                st.input.clear();
            }
        }
        if !close {
            self.symbol_dialog = Some(st);
        }
    }
}

/// One grid cell: the character, greyed when the UI fonts cannot draw it.
fn cell(ui: &mut egui::Ui, c: char, selected: bool, available: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(CELL), egui::Sense::click());
    let fill = if selected {
        chrome::Tokens::ACCENT
    } else if response.hovered() {
        Color32::from_rgb(233, 240, 255)
    } else {
        Color32::WHITE
    };
    ui.painter().rect(
        rect,
        0.0,
        fill,
        Stroke::new(1.0, Color32::from_gray(220)),
        egui::StrokeKind::Inside,
    );
    let color = if selected {
        Color32::WHITE
    } else if available {
        Color32::from_gray(30)
    } else {
        Color32::from_gray(170)
    };
    let shown =
        if c.is_whitespace() || matches!(c as u32, 0x00AD | 0x200B..=0x200F | 0x2060 | 0xFEFF) {
            "␣".to_owned()
        } else {
            c.to_string()
        };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        shown,
        egui::FontId::proportional(18.0),
        color,
    );
    response.on_hover_text(code_labels(c).0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_round_trips_and_keeps_recent_order() {
        let mut s = Saved::default();
        s.remember("★☆");
        s.remember("★");
        assert_eq!(s.recent, vec!['★', '☆']);
        assert!(s.toggle_favorite('※'));
        assert!(!s.toggle_favorite('※'));
        s.toggle_favorite('①');
        s.font = "Segoe UI Symbol".into();
        let text = s.serialize();
        assert_eq!(Saved::parse(&text), s);
        assert_eq!(Saved::parse("not json"), Saved::default());
        let (u, ks, block) = code_labels('가');
        assert_eq!(u, "U+AC00 (44032)");
        assert_eq!(ks, "B0A1");
        assert_eq!(block, "한글 음절");
    }
}
