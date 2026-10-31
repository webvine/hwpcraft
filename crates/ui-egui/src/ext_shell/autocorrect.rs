//! 빠른 교정: a user list of words replaced as they are typed. It applies only to text
//! typed on the desktop canvas; pasted text, MCP, CLI and macro playback never pass
//! through here, so text an AI wrote is not changed behind its back. The replacement
//! and the space that triggered it are one undo step, so one Ctrl+Z gives the typed
//! word back.
use super::*;

/// Most entries kept, and the longest word on either side.
pub(crate) const MAX_ENTRIES: usize = 500;
pub(crate) const MAX_WORD: usize = 40;

/// 빠른 교정 dialog draft: the list being edited and the two input fields.
#[derive(Default)]
pub(crate) struct AutoCorrectDraft {
    pub enabled: bool,
    pub entries: Vec<(String, String)>,
    pub from: String,
    pub to: String,
    pub selected: Option<usize>,
}

/// A word that may be a 빠른 교정 entry: 1–40 characters without spaces or controls.
pub(crate) fn valid_word(word: &str) -> bool {
    let n = word.chars().count();
    (1..=MAX_WORD).contains(&n) && !word.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The replacement for the word that ends at `caret` in `text`, as (start, replacement).
fn correction<'a>(
    entries: &'a [(String, String)],
    text: &str,
    caret: usize,
) -> Option<(usize, &'a str)> {
    let chars: Vec<char> = text.chars().take(caret).collect();
    let start = chars
        .iter()
        .rposition(|c| c.is_whitespace())
        .map_or(0, |i| i + 1);
    let word: String = chars[start..].iter().collect();
    entries
        .iter()
        .find(|(from, _)| *from == word)
        .map(|(_, to)| (start, to.as_str()))
}

impl HwpApp {
    /// Text typed on the canvas: a space after a listed word replaces the word first.
    pub(crate) fn typed_text(&mut self, text: String) {
        let s = &self.ext.shell.settings;
        let active = s.autocorrect_enabled
            && !s.autocorrect.is_empty()
            && !self.session.has_selection()
            && text.ends_with(' ');
        if !active {
            self.command(Command::Insert(text));
            return;
        }
        // An IME commit may carry the last syllables together with the space.
        let head = &text[..text.len() - 1];
        if !head.is_empty() {
            self.command(Command::Insert(head.to_owned()));
        }
        let caret = self.session.cursor.offset;
        let found = correction(
            &self.ext.shell.settings.autocorrect,
            self.session.text(),
            caret,
        )
        .map(|(start, to)| (start, format!("{to} ")));
        match found {
            Some((start, replacement)) => {
                // Selecting the word makes the insert replace it: one undo step.
                let mut from = self.session.cursor;
                from.offset = start;
                let r = self.session.select(from, false).and_then(|()| {
                    let mut to = self.session.cursor;
                    to.offset = caret;
                    self.session.select(to, true)
                });
                if self.result(r).is_some() {
                    self.command(Command::Insert(replacement));
                } else {
                    self.command(Command::Insert(" ".into()));
                }
            }
            None => self.command(Command::Insert(" ".into())),
        }
    }

    /// 빠른 교정 dialog: the user list and whether it applies while typing.
    pub(crate) fn autocorrect_dialog(
        &mut self,
        ctx: &egui::Context,
        draft: &mut AutoCorrectDraft,
    ) -> bool {
        let button = dialog(ctx, "빠른 교정", 520.0, &["설정(D)", "취소"], |ui| {
            ui.checkbox(&mut draft.enabled, "입력할 때 빠른 교정 쓰기");
            group(ui, "바꿀 말");
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    if draft.entries.is_empty() {
                        ui.label(RichText::new("아직 없습니다.").weak());
                    }
                    for (i, (from, to)) in draft.entries.iter().enumerate() {
                        if ui
                            .selectable_label(draft.selected == Some(i), format!("{from}  →  {to}"))
                            .clicked()
                        {
                            draft.selected = Some(i);
                        }
                    }
                });
            ui.horizontal(|ui| {
                chrome::first_focus(
                    &ui.add(
                        egui::TextEdit::singleline(&mut draft.from)
                            .hint_text("입력한 말")
                            .char_limit(MAX_WORD)
                            .desired_width(140.0),
                    ),
                );
                ui.label("→");
                ui.add(
                    egui::TextEdit::singleline(&mut draft.to)
                        .hint_text("바꿀 말")
                        .char_limit(MAX_WORD)
                        .desired_width(140.0),
                );
                let (from, to) = (draft.from.trim().to_owned(), draft.to.trim().to_owned());
                let ok = valid_word(&from)
                    && !to.is_empty()
                    && to.chars().count() <= MAX_WORD
                    && !to.chars().any(char::is_control)
                    && draft.entries.len() < MAX_ENTRIES;
                if ui.add_enabled(ok, egui::Button::new("넣기")).clicked() {
                    draft.entries.retain(|(f, _)| *f != from);
                    draft.entries.push((from, to));
                    draft.entries.sort();
                    draft.from.clear();
                    draft.to.clear();
                    draft.selected = None;
                }
                if ui
                    .add_enabled(draft.selected.is_some(), egui::Button::new("지우기"))
                    .clicked()
                    && let Some(i) = draft.selected.take()
                    && i < draft.entries.len()
                {
                    draft.entries.remove(i);
                }
            });
            notice(
                ui,
                "띄어쓰기를 치면 바로 앞 낱말이 목록에 있을 때 바꿉니다. 바로 Ctrl+Z를 누르면 \
                 친 그대로 돌아갑니다. 데스크톱에서 친 글에만 쓰고, 붙여넣기·MCP·CLI·매크로로 \
                 들어온 글은 바꾸지 않습니다.",
            );
        });
        match button {
            Some(0) => {
                let s = &mut self.ext.shell.settings;
                s.autocorrect_enabled = draft.enabled;
                s.autocorrect = std::mem::take(&mut draft.entries);
                s.save();
                self.message = "빠른 교정을 저장했습니다.".into();
                false
            }
            Some(_) => false,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listed_word_before_the_caret_is_found() {
        let entries = vec![("teh".to_owned(), "the".to_owned())];
        assert_eq!(correction(&entries, "go teh", 6), Some((3, "the")));
        assert_eq!(correction(&entries, "teh", 3), Some((0, "the")));
        assert_eq!(correction(&entries, "go tehx", 7), None);
        assert_eq!(correction(&entries, "teh go", 6), None);
        assert!(valid_word("teh") && !valid_word("a b") && !valid_word(""));
    }

    #[test]
    fn typing_a_space_replaces_the_word_and_one_undo_restores_it() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        a.ext.shell.settings.autocorrect_enabled = true;
        a.ext.shell.settings.autocorrect = vec![("ㄱㅅ".into(), "감사합니다".into())];
        for t in ["오늘도", " ", "ㄱㅅ", " "] {
            a.typed_text(t.into());
        }
        assert_eq!(a.session.text(), "오늘도 감사합니다 ");
        a.session.execute(Command::Undo)?;
        assert_eq!(a.session.text(), "오늘도 ㄱㅅ");
        // Off, or with a selection, nothing changes.
        a.session.execute(Command::Redo)?;
        a.ext.shell.settings.autocorrect_enabled = false;
        a.typed_text("ㄱㅅ".into());
        a.typed_text(" ".into());
        assert_eq!(a.session.text(), "오늘도 감사합니다 ㄱㅅ ");
        Ok(())
    }
}
