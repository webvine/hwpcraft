//! Ctrl+click on a hyperlink follows it, as in Hangul: a link into the document (a table
//! of contents entry, `?bookmark`) moves the caret there; a web or mail address opens in
//! the system browser or mail program.
use super::*;

impl HwpApp {
    /// Page pointer hook: a Ctrl+click on a link follows it (true = handled).
    pub(crate) fn shell_link_canvas(
        &mut self,
        page: u32,
        response: &egui::Response,
        rect: Rect,
    ) -> bool {
        let ctrl = response
            .ctx
            .input(|i| i.modifiers.command && !i.modifiers.shift);
        let Some(p) = response
            .interact_pointer_pos()
            .filter(|_| response.clicked() && ctrl)
        else {
            return false;
        };
        let at = (p - rect.min) / self.zoom;
        self.follow_link_at(page, Pos2::new(at.x, at.y))
    }

    /// Follow the link under `at` (page units) on `page`; false when there is none there,
    /// so the click selects as usual.
    pub(crate) fn follow_link_at(&mut self, page: u32, at: Pos2) -> bool {
        let Ok(cursor) = self
            .session
            .hit_test(page, f64::from(at.x), f64::from(at.y))
        else {
            return false;
        };
        let before = (self.session.cursor, self.session.anchor);
        if self.session.select(cursor, false).is_err() {
            return false;
        }
        if !matches!(self.session.hyperlink_at_caret(), Ok(Some(_))) {
            (self.session.cursor, self.session.anchor) = before;
            return false;
        }
        match self.session.run("hyperlink.jump", json!({})) {
            Ok(r) if r["jumped"] == true => {
                self.follow_cursor();
                self.message = "링크가 가리키는 곳으로 옮겼습니다.".into();
            }
            Ok(r) => self.open_outside_link(r["uri"].as_str().unwrap_or_default()),
            Err(e) => self.message = format!("링크를 따라가지 못했습니다: {e}"),
        }
        true
    }

    /// A link out of the document: web and mail addresses go to the system, the rest
    /// (files, scripts, other schemes) only get a message.
    fn open_outside_link(&mut self, uri: &str) {
        if openable(uri) {
            self.ctx.open_url(egui::OpenUrl::new_tab(uri.trim()));
            self.message = format!("링크를 엽니다: {uri}");
        } else {
            self.message = format!("이 링크는 열 수 없습니다: {uri}");
        }
    }
}

/// Addresses a click may hand to the system: web pages and mail only. Files, scripts
/// and other schemes are never opened from a document.
fn openable(uri: &str) -> bool {
    let uri = uri.trim().to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| uri.starts_with(scheme))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(paragraph: usize, offset: usize) -> hwpcraft_doc::Cursor {
        hwpcraft_doc::Cursor {
            section: 0,
            paragraph,
            offset,
            cell: None,
        }
    }

    /// Page position of the caret at `cursor`.
    fn point(a: &HwpApp, cursor: hwpcraft_doc::Cursor) -> (u32, Pos2) {
        let c = a.session.caret_at(cursor).unwrap();
        let page = c["pageIndex"].as_u64().unwrap() as u32;
        let x = c["x"].as_f64().unwrap() as f32 + 2.0;
        let y = c["y"].as_f64().unwrap() as f32 + c["height"].as_f64().unwrap() as f32 / 2.0;
        (page, Pos2::new(x, y))
    }

    #[test]
    fn ctrl_click_on_a_contents_entry_goes_to_its_heading() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        a.session
            .execute(Command::Insert("차례 자리\n서론\n본문".into()))?;
        a.session.select(at(1, 0), false)?;
        a.session.run("style.apply", json!({"id": 2}))?;
        a.session.select(at(0, 0), false)?;
        a.session.run("tools.toc", json!({}))?;
        let texts: Vec<String> = a.session.core.document().sections[0]
            .paragraphs
            .iter()
            .map(|p| p.text.clone())
            .collect();
        let entry = texts.iter().position(|t| t.contains("서론\t")).unwrap();
        let heading = texts.iter().rposition(|t| t == "서론").unwrap();
        let (page, p) = point(&a, at(entry, 1));
        assert!(a.follow_link_at(page, p), "{texts:?}");
        assert_eq!(a.session.cursor.paragraph, heading);
        // Plain text is not a link: the click is left to select as usual.
        let (page, p) = point(&a, at(heading, 1));
        let before = a.session.cursor;
        assert!(!a.follow_link_at(page, p));
        assert_eq!(a.session.cursor, before);
        Ok(())
    }

    #[test]
    fn only_web_and_mail_addresses_are_opened() {
        for uri in [
            "https://example.com",
            "HTTP://x.kr/a",
            "mailto:a@b.kr",
            " https://x",
        ] {
            assert!(openable(uri), "{uri}");
        }
        for uri in [
            "file:///C:/Windows/System32/cmd.exe",
            "javascript:alert(1)",
            "ftp://x",
            r"\\server\share",
            r"C:\a.exe",
            "data:text/html,x",
            "?책갈피",
            "",
        ] {
            assert!(!openable(uri), "{uri}");
        }
    }

    /// The OpenUrl commands one follow of the link at `cursor` asks of the system.
    fn opened(a: &mut HwpApp, cursor: hwpcraft_doc::Cursor) -> Vec<String> {
        let (page, p) = point(a, cursor);
        let ctx = a.ctx.clone();
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
            assert!(a.follow_link_at(page, p));
        });
        output.textures_delta.clear();
        output
            .platform_output
            .commands
            .iter()
            .filter_map(|c| match c {
                egui::OutputCommand::OpenUrl(u) => Some(u.url.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_web_link_opens_and_a_file_or_script_link_does_not() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        a.session.execute(Command::Insert("웹 링크 글".into()))?;
        a.session.select(at(0, 0), false)?;
        a.session.select(at(0, 2), true)?;
        a.session.insert_hyperlink("https://example.com/a", None)?;
        assert_eq!(opened(&mut a, at(0, 1)), ["https://example.com/a"]);
        // HwpCraft makes only web links, but a document may bring others.
        for uri in ["file:///C:/a.exe", "javascript:alert(1)", "ftp://x/y"] {
            let ctx = a.ctx.clone();
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
                a.open_outside_link(uri);
            });
            output.textures_delta.clear();
            let any = output
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::OpenUrl(_)));
            assert!(!any, "{uri} is not opened");
            assert!(a.message.contains("열 수 없습니다"), "{}", a.message);
        }
        Ok(())
    }
}
