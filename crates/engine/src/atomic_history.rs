//! Single-scope document/history rollback without cloning every historical document.
//!
//! Body/cell rich clipboard is reconstructed from its exact tracked copy source.
//! The document-local page clipboard is also restored. Native control-copy
//! cascade and the unused native transpose clipboard remain outside this API.
//! Mixing a live full scope with the legacy infallible checkpoint API or direct
//! native snapshot manipulation through `Session::core` is outside this contract.

use std::{
    collections::BTreeSet,
    path::PathBuf,
    rc::{Rc, Weak},
};

use anyhow::{Result, bail};

use crate::{Cursor, Session, Snapshot, ViewMarks, err};

#[derive(Default)]
pub(crate) struct HistoryScopes {
    owner: Rc<()>,
    active: Option<ActiveScope>,
    legacy: Vec<(u32, Weak<()>)>,
}

struct ActiveScope {
    lease: Weak<()>,
    pins: BTreeSet<u32>,
}

/// A single active document/history baseline, owned by its originating Session.
///
/// Consume with [`Session::rollback_full`] or [`Session::release_full`]. Dropping
/// it keeps the current work; unreferenced baseline snapshots are reclaimed at
/// the next snapshot allocation or full-scope acquisition. This is not a panic
/// recovery API, a native clipboard snapshot, or a nested transaction API.
#[must_use = "consume the baseline with rollback_full or release_full"]
pub struct FullCheckpoint {
    owner: Rc<()>,
    lease: Rc<()>,
    current: Snapshot,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    password: Option<String>,
    path: Option<PathBuf>,
    next_revision: u64,
    saved_revision: u64,
    overwrite: bool,
    typing: Option<Cursor>,
    typing_chars: usize,
    view_marks: ViewMarks,
    clipboard: crate::clipboard_history::ClipboardBaseline,
    page_clipboard: Option<Rc<hwpcraft_doc::model::document::Document>>,
}

impl Session {
    fn reclaim_legacy_history_scopes(&mut self) {
        let leases = std::mem::take(&mut self.history_scopes.legacy);
        for (id, lease) in leases {
            if lease.strong_count() == 0 {
                self.retire_history_snapshot(id);
            } else {
                self.history_scopes.legacy.push((id, lease));
            }
        }
    }

    fn reclaim_history_scopes(&mut self) {
        self.reclaim_legacy_history_scopes();
        if self
            .history_scopes
            .active
            .as_ref()
            .is_some_and(|s| s.lease.strong_count() == 0)
        {
            self.finish_history_scope();
        }
    }

    fn finish_history_scope(&mut self) {
        if let Some(scope) = self.history_scopes.active.take() {
            let live: BTreeSet<_> = self.undo.iter().chain(&self.redo).map(|s| s.id).collect();
            for id in scope.pins.difference(&live) {
                self.core.discard_snapshot_native(*id);
            }
        }
    }

    pub(crate) fn save_history_snapshot(&mut self) -> u32 {
        self.reclaim_history_scopes();
        self.core.save_snapshot_native()
    }

    pub(crate) fn retire_history_snapshot(&mut self, id: u32) {
        if !self
            .history_scopes
            .active
            .as_ref()
            .is_some_and(|s| s.pins.contains(&id))
        {
            self.core.discard_snapshot_native(id);
        }
    }

    pub(crate) fn track_legacy_checkpoint(&mut self, id: u32) -> Rc<()> {
        self.reclaim_legacy_history_scopes();
        let lease = Rc::new(());
        self.history_scopes.legacy.push((id, Rc::downgrade(&lease)));
        lease
    }

    /// Core replacement would invalidate IDs held by a live full baseline.
    pub(crate) fn ensure_history_replacement_allowed(&mut self) -> Result<()> {
        self.reclaim_history_scopes();
        if self.history_scopes.active.is_some() {
            bail!("문서 전체 rollback 범위 안에서는 문서를 교체할 수 없습니다.");
        }
        Ok(())
    }

    /// Preserve the current document, complete history and insertion state.
    /// Only one full scope is supported; acquire before executing any batch work.
    pub fn full_checkpoint(&mut self) -> Result<FullCheckpoint> {
        self.reclaim_history_scopes();
        if self.history_scopes.active.is_some() || !self.history_scopes.legacy.is_empty() {
            bail!("기존 checkpoint가 살아 있는 동안 전체 checkpoint를 만들 수 없습니다.");
        }
        if self.undo.len().saturating_add(self.redo.len()) > 40 {
            bail!("문서 실행 취소 기록이 전체 checkpoint 한도를 초과했습니다.");
        }
        let clipboard = self.snapshot_clipboard()?;
        let view_marks = self.view_marks();
        let current = self.snapshot();
        let lease = Rc::new(());
        let pins = self
            .undo
            .iter()
            .chain(&self.redo)
            .map(|s| s.id)
            .chain(std::iter::once(current.id))
            .collect();
        self.history_scopes.active = Some(ActiveScope {
            lease: Rc::downgrade(&lease),
            pins,
        });
        Ok(FullCheckpoint {
            owner: self.history_scopes.owner.clone(),
            lease,
            current,
            undo: self.undo.clone(),
            redo: self.redo.clone(),
            password: self.password.clone(),
            path: self.path.clone(),
            next_revision: self.next_revision,
            saved_revision: self.saved_revision,
            overwrite: self.overwrite,
            typing: self.typing,
            typing_chars: self.typing_chars,
            view_marks,
            clipboard,
            page_clipboard: self.page_clipboard.clone(),
        })
    }

    fn validate_full_checkpoint(&self, token: &FullCheckpoint) -> Result<()> {
        if !Rc::ptr_eq(&self.history_scopes.owner, &token.owner)
            || !self
                .history_scopes
                .active
                .as_ref()
                .is_some_and(|s| Weak::ptr_eq(&s.lease, &Rc::downgrade(&token.lease)))
        {
            bail!("다른 문서 또는 종료된 범위의 전체 checkpoint입니다.");
        }
        Ok(())
    }

    /// Restore document/history and private save state; invalidate stale generations.
    /// A native restore error consumes/cleans this scope but does not promise that
    /// the document is safe to reuse. The caller must apply its fail-closed policy.
    pub fn rollback_full(&mut self, token: FullCheckpoint) -> Result<()> {
        self.validate_full_checkpoint(&token)?;
        let Some(generation) = self.generation.checked_add(1) else {
            self.finish_history_scope();
            bail!("문서 변경 세대가 소진되어 전체 rollback을 수행할 수 없습니다.");
        };
        if let Err(error) = self
            .core
            .restore_snapshot_native(token.current.id)
            .map_err(err)
        {
            self.finish_history_scope();
            return Err(error);
        }
        if let Err(error) = self.restore_clipboard(token.clipboard, token.current.id) {
            self.finish_history_scope();
            return Err(error);
        }
        let working_undo = std::mem::replace(&mut self.undo, token.undo);
        let working_redo = std::mem::replace(&mut self.redo, token.redo);
        for snapshot in working_undo.into_iter().chain(working_redo) {
            self.retire_history_snapshot(snapshot.id);
        }
        self.cursor = token.current.cursor;
        self.anchor = token.current.anchor;
        self.span_anchor = token.current.span_anchor;
        self.revision = token.current.revision;
        self.insertion_shape = token.current.insertion_shape;
        self.page_clipboard = token.page_clipboard;
        self.password = token.password;
        self.path = token.path;
        self.next_revision = token.next_revision;
        self.saved_revision = token.saved_revision;
        self.overwrite = token.overwrite;
        self.typing = token.typing;
        self.typing_chars = token.typing_chars;
        self.set_view_marks(token.view_marks);
        self.generation = generation;
        self.clear_geometry();
        *self.page_field_previous.borrow_mut() = None;
        self.page_field_edit.set(None);
        *self.render_highlights.borrow_mut() = None;
        self.restore_visual_hint(token.current.visual_hint);
        self.finish_history_scope();
        Ok(())
    }

    /// Keep the current work and history, releasing baseline-only native IDs.
    pub fn release_full(&mut self, token: FullCheckpoint) -> Result<()> {
        self.validate_full_checkpoint(&token)?;
        self.finish_history_scope();
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::Command;
    use serde_json::json;

    fn append(s: &mut Session, text: &str) -> Result<()> {
        s.transaction(|s| s.insert(text))
    }

    fn ids(s: &Session) -> (Vec<u32>, Vec<u32>) {
        (
            s.undo.iter().map(|x| x.id).collect(),
            s.redo.iter().map(|x| x.id).collect(),
        )
    }

    #[test]
    fn old_redo_native_ids_survive_failed_edit() -> Result<()> {
        let mut s = Session::new()?;
        append(&mut s, "a")?;
        append(&mut s, "b")?;
        s.execute(Command::Undo)?;
        let before = ids(&s);
        let token = s.full_checkpoint()?;
        append(&mut s, "x")?;
        assert!(s.redo.is_empty());
        s.rollback_full(token)?;
        assert_eq!(s.text(), "a");
        assert_eq!(ids(&s), before);
        s.execute(Command::Redo)?;
        assert_eq!(s.text(), "ab");
        s.execute(Command::Undo)?;
        s.execute(Command::Undo)?;
        assert_eq!(s.text(), "");
        Ok(())
    }

    #[test]
    fn mixed_undo_redo_rollback_reactivates_original_ids() -> Result<()> {
        let mut s = Session::new()?;
        for text in ["a", "b", "c", "d"] {
            append(&mut s, text)?;
        }
        s.execute(Command::Undo)?;
        s.execute(Command::Undo)?;
        let before = ids(&s);
        let token = s.full_checkpoint()?;
        s.execute(Command::Redo)?;
        s.execute(Command::Undo)?;
        s.execute(Command::Undo)?;
        append(&mut s, "x")?;
        s.rollback_full(token)?;
        assert_eq!(ids(&s), before);
        assert_eq!(s.text(), "ab");
        for expected in ["abc", "abcd"] {
            s.execute(Command::Redo)?;
            assert_eq!(s.text(), expected);
        }
        for expected in ["abc", "ab", "a", ""] {
            s.execute(Command::Undo)?;
            assert_eq!(s.text(), expected);
        }
        Ok(())
    }

    #[test]
    fn oldest_forty_history_entries_survive_eviction_in_scope() -> Result<()> {
        let mut s = Session::new()?;
        for _ in 0..40 {
            append(&mut s, "a")?;
        }
        let before = ids(&s);
        let token = s.full_checkpoint()?;
        for _ in 0..45 {
            append(&mut s, "b")?;
        }
        s.rollback_full(token)?;
        assert_eq!(ids(&s), before);
        for remaining in (0..40).rev() {
            s.execute(Command::Undo)?;
            assert_eq!(s.text(), "a".repeat(remaining));
        }
        Ok(())
    }

    #[test]
    fn thousand_transactions_keep_pins_bounded_on_success_and_failure() -> Result<()> {
        for rollback in [false, true] {
            let mut s = Session::new()?;
            for _ in 0..40 {
                append(&mut s, "a")?;
            }
            let token = s.full_checkpoint()?;
            assert_eq!(s.history_scopes.active.as_ref().unwrap().pins.len(), 41);
            for _ in 0..1000 {
                append(&mut s, "b")?;
                assert_eq!(s.undo.len() + s.redo.len(), 40);
                assert_eq!(s.history_scopes.active.as_ref().unwrap().pins.len(), 41);
            }
            if rollback {
                s.rollback_full(token)?;
            } else {
                s.release_full(token)?;
            }
            assert!(s.history_scopes.active.is_none());
            assert_eq!(s.text().len(), if rollback { 40 } else { 1040 });
            for _ in 0..40 {
                s.execute(Command::Undo)?;
            }
            assert_eq!(
                s.text(),
                if rollback {
                    String::new()
                } else {
                    format!("{}{}", "a".repeat(40), "b".repeat(960))
                }
            );
            for _ in 0..40 {
                s.execute(Command::Redo)?;
            }
            assert_eq!(s.text().len(), if rollback { 40 } else { 1040 });
        }
        Ok(())
    }

    #[test]
    fn release_preserves_working_history_and_retires_old_baseline() -> Result<()> {
        let mut s = Session::new()?;
        append(&mut s, "a")?;
        append(&mut s, "b")?;
        s.execute(Command::Undo)?;
        let retired_redo = s.redo[0].id;
        let token = s.full_checkpoint()?;
        let retired_current = token.current.id;
        append(&mut s, "x")?;
        s.release_full(token)?;
        assert!(s.core.restore_snapshot_native(retired_redo).is_err());
        assert!(s.core.restore_snapshot_native(retired_current).is_err());
        s.execute(Command::Undo)?;
        assert_eq!(s.text(), "a");
        s.execute(Command::Redo)?;
        assert_eq!(s.text(), "ax");
        Ok(())
    }

    #[test]
    fn dropped_scope_reclaims_only_ids_not_owned_by_current_history() -> Result<()> {
        let mut s = Session::new()?;
        append(&mut s, "a")?;
        append(&mut s, "b")?;
        s.execute(Command::Undo)?;
        let retired_redo = s.redo[0].id;
        let token = s.full_checkpoint()?;
        let retired_current = token.current.id;
        append(&mut s, "x")?;
        drop(token);
        let next = s.full_checkpoint()?;
        assert!(s.core.restore_snapshot_native(retired_current).is_err());
        assert!(s.core.restore_snapshot_native(retired_redo).is_err());
        s.release_full(next)?;
        s.execute(Command::Undo)?;
        assert_eq!(s.text(), "a");
        Ok(())
    }

    #[test]
    fn admissions_reject_live_full_legacy_and_foreign_owner() -> Result<()> {
        let mut a = Session::new()?;
        let mut b = Session::new()?;
        let legacy = a.checkpoint();
        assert!(a.full_checkpoint().is_err());
        a.release(legacy);
        let dropped_legacy = a.checkpoint();
        let old_id = dropped_legacy.id;
        drop(dropped_legacy);
        let token = a.full_checkpoint()?;
        assert!(a.core.restore_snapshot_native(old_id).is_err());
        assert!(a.full_checkpoint().is_err());
        assert!(a.ensure_history_replacement_allowed().is_err());
        assert!(b.rollback_full(token).is_err());
        assert_eq!(a.text(), "");
        assert_eq!(b.text(), "");
        a.ensure_history_replacement_allowed()?;
        let token = a.full_checkpoint()?;
        a.release_full(token)?;
        Ok(())
    }

    #[test]
    fn typing_group_and_selection_state_are_restored() -> Result<()> {
        let mut s = Session::new()?;
        s.execute(Command::Insert("a".into()))?;
        let before = (s.typing, s.typing_chars, s.next_revision);
        let token = s.full_checkpoint()?;
        s.execute(Command::Insert("b".into()))?;
        s.overwrite = true;
        s.rollback_full(token)?;
        assert_eq!((s.typing, s.typing_chars, s.next_revision), before);
        assert!(!s.overwrite);
        s.execute(Command::Insert("c".into()))?;
        s.execute(Command::Undo)?;
        assert_eq!(s.text(), "");
        append(&mut s, "abcd")?;
        s.cursor.offset = 3;
        s.anchor = Some(1);
        s.overwrite = true;
        let token = s.full_checkpoint()?;
        s.execute(Command::Insert("x".into()))?;
        s.rollback_full(token)?;
        assert_eq!(s.text(), "abcd");
        assert_eq!(s.cursor.offset, 3);
        assert_eq!(s.anchor, Some(1));
        assert!(s.overwrite);
        Ok(())
    }

    #[test]
    fn pending_format_view_marks_and_generation_are_restored() -> Result<()> {
        let mut s = Session::new()?;
        append(&mut s, "abcd")?;
        s.cursor.offset = 2;
        s.execute(Command::Character(json!({"bold":true,"fontSize":1600})))?;
        let pending = s.character_properties()?;
        let marks = ViewMarks {
            paragraph_marks: true,
            ..Default::default()
        };
        s.set_view_marks(marks);
        let token = s.full_checkpoint()?;
        s.execute(Command::Insert("X".into()))?;
        s.set_view_marks(ViewMarks::default());
        let changed_generation = s.generation;
        s.rollback_full(token)?;
        assert!(s.generation > changed_generation);
        assert_eq!(s.view_marks(), marks);
        assert_eq!(s.projection_view_marks, marks);
        assert_eq!(s.character_properties()?, pending);
        assert_eq!(s.text(), "abcd");
        s.execute(Command::Insert("Y".into()))?;
        s.anchor = Some(2);
        assert_eq!(s.character_properties()?["bold"], true);
        Ok(())
    }

    #[test]
    fn cross_paragraph_selection_is_restored_after_replacement() -> Result<()> {
        let mut s = Session::new()?;
        append(&mut s, "abc\ndef")?;
        let cursor = Cursor {
            paragraph: 1,
            offset: 2,
            ..Default::default()
        };
        let anchor = Cursor {
            offset: 1,
            ..Default::default()
        };
        s.cursor = cursor;
        s.span_anchor = Some(anchor);
        let token = s.full_checkpoint()?;
        s.execute(Command::Insert("X".into()))?;
        s.rollback_full(token)?;
        assert_eq!(s.cursor, cursor);
        assert_eq!(s.span_anchor, Some(anchor));
        let text: Vec<_> = s.core.document().sections[0]
            .paragraphs
            .iter()
            .map(|p| p.text.as_str())
            .collect();
        assert_eq!(text, ["abc", "def"]);
        Ok(())
    }

    #[test]
    fn password_none_change_and_clear_restore_real_save_reopen() -> Result<()> {
        let dir = tempfile::tempdir()?;
        for extension in ["hwp", "hwpx"] {
            for (case, original, changed) in [
                ("add", None, Some("new-fixture-key")),
                (
                    "change",
                    Some("original-fixture-key"),
                    Some("new-fixture-key"),
                ),
                ("clear", Some("original-fixture-key"), None),
            ] {
                let mut s = Session::new()?;
                append(&mut s, "암호 rollback 자체 문서😀")?;
                s.set_password(original.map(str::to_owned))?;
                let before = dir.path().join(format!("{case}-before.{extension}"));
                s.save(&before)?;
                assert!(!s.dirty());
                let revisions = (s.revision, s.next_revision, s.saved_revision);
                let token = s.full_checkpoint()?;
                s.set_password(changed.map(str::to_owned))?;
                assert!(s.dirty());
                s.rollback_full(token)?;
                assert!(s.password.as_deref() == original);
                assert_eq!((s.revision, s.next_revision, s.saved_revision), revisions);
                assert_eq!(s.path.as_ref(), Some(&before));
                assert!(!s.dirty());
                let after = dir.path().join(format!("{case}-after.{extension}"));
                s.save(&after)?;
                let mut reopened = Session::new()?;
                if let Some(password) = original {
                    assert!(reopened.open(&after).is_err());
                    reopened.open_with_password(&after, password)?;
                } else {
                    reopened.open(&after)?;
                }
                assert_eq!(reopened.text(), "암호 rollback 자체 문서😀");
            }
        }
        Ok(())
    }

    #[test]
    fn restore_failure_cleans_scope_without_claiming_success() -> Result<()> {
        let mut s = Session::new()?;
        append(&mut s, "a")?;
        let token = s.full_checkpoint()?;
        s.core.discard_snapshot_native(token.current.id);
        assert!(s.rollback_full(token).is_err());
        assert!(s.history_scopes.active.is_none());
        Ok(())
    }
}
