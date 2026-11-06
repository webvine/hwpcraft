//! One undo step for a run of edits made by someone else (the AI edit panel):
//! the edits keep their own steps while they run, and closing the group turns
//! them into one step on top of the user's own history.
//!
//! RHWP keeps at most 100 snapshots. The user's undo and redo stacks (40 each)
//! are set aside while a group runs, so the group's own steps stop at
//! `GROUP_UNDO_LIMIT`; with the group's start snapshot and those of a batch or
//! transaction in flight the total stays below RHWP's limit.

use anyhow::{Result, bail};

use crate::{Session, Snapshot};

/// Undo steps kept inside a group (the oldest are dropped; the group still
/// returns to its start in one step).
pub(crate) const GROUP_UNDO_LIMIT: usize = 12;

/// Commands refused while restricted (see [`Session::set_commands_restricted`]):
/// those that write files, read local files into the document, replace the
/// document, change the user's settings folder, set a password, or step the
/// history outside the group. Refused here by id; `picture.replace` is not listed
/// because it refuses only its `path` (a file) inside the command and takes the
/// question's pictures (`attachment`). Projections keep the restriction
/// (`Session::scratch_session`).
pub const RESTRICTED_COMMANDS: &[&str] = &[
    // Files written to disk.
    "file.save",
    "file.save_as",
    "file.pdf",
    "file.save_hml",
    "file.save_pages",
    "file.print_pdf",
    "file.save_block",
    "picture.save",
    "picture.save_all",
    "format.style.export",
    "review.history.save_last",
    "file.template.save",
    "file.template.delete",
    // The user's settings folder.
    "tools.autotext.add",
    "tools.autotext.delete",
    "tools.autotext.body.add",
    "tools.autotext.body.delete",
    "macro.save",
    "macro.delete",
    "macro.record.stop",
    "format.style.set.save",
    "format.style.set.delete",
    "table.user_style.save",
    "table.user_style.delete",
    "equation.macro.save",
    "equation.macro.delete",
    // Local files read into the document, or the document replaced.
    "file.open",
    "file.insert",
    "file.new",
    "file.template.new",
    "insert.picture",
    "picture.insert",
    "format.style.import",
    "format.bullet.picture",
    "review.compare",
    "review.merge.plan",
    "review.merge",
    "page.master.object.insert",
    // A password the user does not know.
    "file.password",
    // History outside the group.
    "edit.undo",
    "edit.redo",
];

pub(crate) struct ActiveGroup {
    start: Snapshot,
    user_undo: Vec<Snapshot>,
    user_redo: Vec<Snapshot>,
}

impl Session {
    /// Start collecting edits into one undo step. The user's undo and redo
    /// history is set aside and comes back at [`Session::end_undo_group`].
    /// Groups do not nest, and none may start inside a batch's full checkpoint.
    pub fn begin_undo_group(&mut self) -> Result<()> {
        if self.undo_group.is_some() {
            bail!("실행 취소 묶음이 이미 열려 있습니다.");
        }
        self.ensure_no_full_scope()?;
        let start = self.snapshot();
        self.undo_group = Some(ActiveGroup {
            start,
            user_undo: std::mem::take(&mut self.undo),
            user_redo: std::mem::take(&mut self.redo),
        });
        self.typing = None;
        Ok(())
    }

    /// Close the group: its steps become one undo step back to where it began,
    /// placed on the user's history. Returns whether the document changed; if it
    /// did not, the user's history (redo included) is as it was.
    pub fn end_undo_group(&mut self) -> Result<bool> {
        self.ensure_no_full_scope()?;
        let Some(group) = self.undo_group.take() else {
            bail!("열린 실행 취소 묶음이 없습니다.");
        };
        let working: Vec<Snapshot> = std::mem::take(&mut self.undo)
            .into_iter()
            .chain(std::mem::take(&mut self.redo))
            .collect();
        for snapshot in working {
            self.retire_history_snapshot(snapshot.id);
        }
        self.undo = group.user_undo;
        self.typing = None;
        let changed = self.revision != group.start.revision;
        if changed {
            for snapshot in group.user_redo {
                self.retire_history_snapshot(snapshot.id);
            }
            self.undo.push(group.start);
            if self.undo.len() > 40 {
                let old = self.undo.remove(0);
                self.retire_history_snapshot(old.id);
            }
        } else {
            self.redo = group.user_redo;
            self.retire_history_snapshot(group.start.id);
        }
        Ok(changed)
    }

    /// True between `begin_undo_group` and `end_undo_group`.
    pub fn undo_group_active(&self) -> bool {
        self.undo_group.is_some()
    }

    /// Snapshots set aside by an open group (for the RHWP snapshot budget).
    pub(crate) fn undo_group_snapshots(&self) -> usize {
        self.undo_group
            .as_ref()
            .map_or(0, |g| 1 + g.user_undo.len() + g.user_redo.len())
    }

    /// Undo steps a command may keep: fewer inside a group.
    pub(crate) fn undo_limit(&self) -> usize {
        if self.undo_group.is_some() {
            GROUP_UNDO_LIMIT
        } else {
            40
        }
    }

    /// Refuse [`RESTRICTED_COMMANDS`] (by command id, in batches and macros too)
    /// until turned off again.
    pub fn set_commands_restricted(&mut self, restricted: bool) {
        self.commands_restricted = restricted;
    }

    /// Refuse `id` while restricted. Callers that run file commands themselves
    /// (the MCP server) check here before doing so.
    pub fn guard_restricted_command(&self, id: &str) -> Result<()> {
        if self.commands_restricted && RESTRICTED_COMMANDS.contains(&id) {
            bail!(
                "AI 편집 중에는 `{id}`를 쓸 수 없습니다(파일 쓰기·읽기, 문서 바꾸기, 설정 폴더, 암호, 실행 취소 기록)."
            );
        }
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

    #[test]
    fn a_group_is_one_undo_step_on_top_of_the_users_history() -> Result<()> {
        for edits in [1, 45] {
            let mut s = Session::new()?;
            append(&mut s, "a")?;
            append(&mut s, "b")?;
            let cursor = s.cursor;
            s.begin_undo_group()?;
            for _ in 0..edits {
                append(&mut s, "x")?;
            }
            assert!(s.undo.len() <= GROUP_UNDO_LIMIT);
            let edited = s.text().to_owned();
            assert!(s.end_undo_group()?);
            s.execute(Command::Undo)?;
            assert_eq!(s.text(), "ab");
            assert_eq!(s.cursor, cursor);
            s.execute(Command::Redo)?;
            assert_eq!(s.text(), edited);
            s.execute(Command::Undo)?;
            s.execute(Command::Undo)?;
            assert_eq!(s.text(), "a");
            s.execute(Command::Undo)?;
            assert_eq!(s.text(), "");
        }
        Ok(())
    }

    #[test]
    fn a_group_without_edits_leaves_history_and_redo_alone() -> Result<()> {
        let mut s = Session::new()?;
        append(&mut s, "a")?;
        append(&mut s, "b")?;
        s.execute(Command::Undo)?;
        let undo: Vec<_> = s.undo.iter().map(|x| x.id).collect();
        let redo: Vec<_> = s.redo.iter().map(|x| x.id).collect();
        s.begin_undo_group()?;
        let _ = s.run("caret.get", json!({}))?;
        assert!(!s.end_undo_group()?);
        assert_eq!(s.undo.iter().map(|x| x.id).collect::<Vec<_>>(), undo);
        assert_eq!(s.redo.iter().map(|x| x.id).collect::<Vec<_>>(), redo);
        s.execute(Command::Redo)?;
        assert_eq!(s.text(), "ab");
        Ok(())
    }

    #[test]
    fn a_full_history_keeps_its_steps_and_the_group_drops_the_oldest() -> Result<()> {
        let mut s = Session::new()?;
        for _ in 0..40 {
            append(&mut s, "a")?;
        }
        s.begin_undo_group()?;
        append(&mut s, "x")?;
        assert!(s.end_undo_group()?);
        assert_eq!(s.undo.len(), 40);
        s.execute(Command::Undo)?;
        assert_eq!(s.text(), "a".repeat(40));
        for remaining in (1..40).rev() {
            s.execute(Command::Undo)?;
            assert_eq!(s.text(), "a".repeat(remaining));
        }
        Ok(())
    }

    #[test]
    fn atomic_batches_work_and_roll_back_inside_a_group() -> Result<()> {
        let mut s = Session::new()?;
        append(&mut s, "a")?;
        s.begin_undo_group()?;
        append(&mut s, "b")?;
        let token = s.full_checkpoint()?;
        append(&mut s, "c")?;
        s.rollback_full(token)?;
        assert_eq!(s.text(), "ab");
        let token = s.full_checkpoint()?;
        append(&mut s, "d")?;
        s.release_full(token)?;
        assert!(s.end_undo_group()?);
        assert_eq!(s.text(), "abd");
        s.execute(Command::Undo)?;
        assert_eq!(s.text(), "a");
        Ok(())
    }

    #[test]
    fn groups_do_not_nest_or_replace_the_document() -> Result<()> {
        let mut s = Session::new()?;
        assert!(s.end_undo_group().is_err());
        s.begin_undo_group()?;
        assert!(s.begin_undo_group().is_err());
        assert!(s.ensure_history_replacement_allowed().is_err());
        assert!(!s.end_undo_group()?);
        let token = s.full_checkpoint()?;
        assert!(s.begin_undo_group().is_err());
        s.release_full(token)?;
        s.begin_undo_group()?;
        assert!(!s.end_undo_group()?);
        Ok(())
    }

    #[test]
    fn restricted_commands_are_refused_by_id() -> Result<()> {
        let mut s = Session::new()?;
        append(&mut s, "a")?;
        s.set_commands_restricted(true);
        assert!(s.run("edit.undo", json!({})).is_err());
        assert!(s.run("file.save", json!({"path": "x.hwpx"})).is_err());
        assert!(s.run("file.pdf", json!({"path": "x.pdf"})).is_err());
        assert_eq!(s.text(), "a");
        s.run("text.insert", json!({"text": "c"}))?;
        assert_eq!(s.text(), "ac");
        s.set_commands_restricted(false);
        s.run("edit.undo", json!({}))?;
        assert_eq!(s.text(), "a");
        Ok(())
    }

    #[test]
    fn every_restricted_command_is_a_registered_command() {
        let ids: std::collections::BTreeSet<_> = crate::COMMANDS
            .iter()
            .chain(crate::extension_commands())
            .map(|(id, _, _)| *id)
            .collect();
        for id in RESTRICTED_COMMANDS {
            assert!(ids.contains(id), "{id} is not a command");
        }
    }
}
