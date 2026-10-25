//! Owner: core area. Hyperlink listing, editing and removal through RHWP's
//! hyperlink API (`hyperlinks_native`, `update_hyperlink_native`,
//! `replace_hyperlink_text_native`, `remove_hyperlink_with_format_native`).
//! Insertion stays in `features.rs`.
use super::*;
use hwpcraft_doc::{HyperlinkInfo, HyperlinkTarget};
use serde::Deserialize;

/// `hyperlink.update` parameters: the field to change and what to change.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HyperlinkEdit {
    pub field_id: u32,
    /// New HTTP/HTTPS address; `None` keeps the current one.
    #[serde(default)]
    pub uri: Option<String>,
    /// New display text (one line); `None` keeps the current one.
    #[serde(default)]
    pub text: Option<String>,
}

fn validate_uri(uri: &str) -> Result<()> {
    if uri.len() > 8192 {
        bail!("주소는 8,192바이트 이하로 입력하세요.");
    }
    if uri.trim().is_empty() {
        bail!("주소를 입력하세요.");
    }
    Ok(())
}
fn validate_text(text: &str) -> Result<()> {
    if text.len() > 1_000_000 || text.contains(['\n', '\r']) || text.chars().any(char::is_control) {
        bail!("링크 표시 문자열은 1 MB 이하의 한 문단이어야 합니다.");
    }
    if text.trim().is_empty() {
        bail!("표시할 문자열을 입력하세요.");
    }
    Ok(())
}

impl Session {
    /// RHWP address of the caret paragraph (body or first-level table cell).
    pub fn hyperlink_target(&self) -> HyperlinkTarget {
        let c = self.cursor;
        HyperlinkTarget {
            section: c.section,
            para: c.paragraph,
            cell_path: c
                .cell
                .map(|t| vec![(t.control, t.index, t.paragraph)])
                .unwrap_or_default(),
        }
    }
    /// Links in the caret paragraph, in text order.
    pub fn hyperlinks(&self) -> Result<Vec<HyperlinkInfo>> {
        self.paragraph(self.cursor)?;
        self.core
            .hyperlinks_native(&self.hyperlink_target())
            .map_err(err)
    }
    /// The link under the caret (or touching the selection), if any.
    pub fn hyperlink_at_caret(&self) -> Result<Option<HyperlinkInfo>> {
        let r = self.selection();
        let at = self.cursor.offset;
        Ok(self
            .hyperlinks()?
            .into_iter()
            .find(|l| (l.start <= at && at <= l.end) || (r.start < l.end && l.start < r.end)))
    }
    /// Change a link's address and/or display text in one undo step.
    /// Nothing changes when both are `None` or equal to the current values.
    pub fn update_hyperlink(&mut self, edit: HyperlinkEdit) -> Result<()> {
        self.guard_protected()?;
        if edit.uri.is_none() && edit.text.is_none() {
            bail!("바꿀 주소나 표시 문자열을 지정하세요.");
        }
        if let Some(uri) = &edit.uri {
            validate_uri(uri)?;
        }
        if let Some(text) = &edit.text {
            validate_text(text)?;
        }
        let current = self
            .hyperlinks()?
            .into_iter()
            .find(|l| l.field_id == edit.field_id)
            .ok_or_else(|| anyhow!("현재 문단에 그 하이퍼링크가 없습니다."))?;
        let target = self.hyperlink_target();
        self.transaction(|s| {
            if let Some(uri) = edit.uri.as_deref().filter(|u| *u != current.uri) {
                s.core
                    .update_hyperlink_native(&target, edit.field_id, uri)
                    .map_err(err)?;
            }
            if let Some(text) = edit.text.as_deref().filter(|t| *t != current.text) {
                s.core
                    .replace_hyperlink_text_native(&target, edit.field_id, text)
                    .map_err(err)?;
                // The caret follows the end of the renamed link.
                s.cursor.offset = current.start + text.chars().count();
                s.clear_selection();
            }
            Ok(())
        })
    }
    /// Remove a link's markers, keeping its text. `restore_format` puts the
    /// colour/underline the link covered back the way Hangul's 하이퍼링크 지우기 does.
    pub fn remove_hyperlink(&mut self, field_id: u32, restore_format: bool) -> Result<()> {
        self.guard_protected()?;
        if !self.hyperlinks()?.iter().any(|l| l.field_id == field_id) {
            bail!("현재 문단에 그 하이퍼링크가 없습니다.");
        }
        let target = self.hyperlink_target();
        self.transaction(|s| {
            s.core
                .remove_hyperlink_with_format_native(&target, field_id, restore_format)
                .map_err(err)?;
            s.clear_selection();
            Ok(())
        })
    }
}
