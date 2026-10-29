//! Replay-based preservation of the body/cell rich clipboard owned by Session.
//!
//! The replay retains source resource IDs verbatim. Restoring via a live native
//! snapshot after replay also restores layout/reflow provenance, unlike setting
//! a cloned live Document back. No private native ClipboardData is accessed.
//! Control-copy cascade and the separate page/transpose clipboards are not this
//! module's contract; direct native clipboard writes bypass its tracking.

use std::rc::Rc;

use anyhow::{Result, bail};
use hwpcraft_doc::{DocumentCore, model::document::Document};
use serde_json::Value;

use crate::{Session, err};

enum CopyRange {
    Body {
        section: usize,
        start_paragraph: usize,
        start_offset: usize,
        end_paragraph: usize,
        end_offset: usize,
    },
    Cell {
        section: usize,
        paragraph: usize,
        control: usize,
        cell: usize,
        start_paragraph: usize,
        start_offset: usize,
        end_paragraph: usize,
        end_offset: usize,
    },
}

/// An immutable copy source and the exact public API arguments used by copy().
/// Rc retention at a full baseline shares this source rather than cloning it.
pub(crate) struct ClipboardReplay {
    source: Document,
    range: CopyRange,
}

impl ClipboardReplay {
    /// Prepare before the native copy. Install this replay only if that copy
    /// succeeds; an unsuccessful native copy leaves the previous replay intact.
    pub(crate) fn prepare(session: &Session) -> Result<Self> {
        if !session.has_selection() {
            bail!("복사할 선택 영역이 없습니다.");
        }
        let selected = session.selected_paragraphs()?;
        let (Some((first, start)), Some((last, end))) = (selected.first(), selected.last()) else {
            bail!("복사할 문단을 찾지 못했습니다.");
        };
        let range = if let Some(cell) = first.cell {
            CopyRange::Cell {
                section: first.section,
                paragraph: first.paragraph,
                control: cell.control,
                cell: cell.index,
                start_paragraph: cell.paragraph,
                start_offset: start.start,
                end_paragraph: last.cell.map_or(last.paragraph, |c| c.paragraph),
                end_offset: end.end,
            }
        } else {
            CopyRange::Body {
                section: first.section,
                start_paragraph: first.paragraph,
                start_offset: start.start,
                end_paragraph: last.paragraph,
                end_offset: end.end,
            }
        };
        Ok(Self {
            source: session.core.document().clone(),
            range,
        })
    }

    pub(crate) fn copy_into(&self, core: &mut DocumentCore) -> Result<()> {
        let response = match self.range {
            CopyRange::Body {
                section,
                start_paragraph,
                start_offset,
                end_paragraph,
                end_offset,
            } => core.copy_selection_native(
                section,
                start_paragraph,
                start_offset,
                end_paragraph,
                end_offset,
            ),
            CopyRange::Cell {
                section,
                paragraph,
                control,
                cell,
                start_paragraph,
                start_offset,
                end_paragraph,
                end_offset,
            } => core.copy_selection_in_cell_native(
                section,
                paragraph,
                control,
                cell,
                start_paragraph,
                start_offset,
                end_paragraph,
                end_offset,
            ),
        }
        .map_err(err)?;
        let response: Value = serde_json::from_str(&response)?;
        if response["ok"] != true {
            bail!("원래의 서식 포함 클립보드를 재생성하지 못했습니다.");
        }
        Ok(())
    }
}

pub(crate) struct ClipboardBaseline {
    replay: Option<Rc<ClipboardReplay>>,
    text: Option<String>,
}

impl Session {
    pub(crate) fn snapshot_clipboard(&self) -> Result<ClipboardBaseline> {
        let replay = if self.core.has_internal_clipboard_native() {
            Some(self.clipboard_replay.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "추적되지 않은 native 클립보드는 전체 checkpoint로 보존할 수 없습니다."
                )
            })?)
        } else {
            None
        };
        Ok(ClipboardBaseline {
            replay,
            text: self.clipboard_text.clone(),
        })
    }

    /// full_native_id must still be owned by the active FullCheckpoint. On any
    /// failure the caller must fail closed: metadata is deliberately not updated.
    pub(crate) fn restore_clipboard(
        &mut self,
        baseline: ClipboardBaseline,
        full_native_id: u32,
    ) -> Result<()> {
        // The native snapshot restore never touches RHWP's internal clipboard, and
        // only Session::copy (which installs a new replay) or this function write it.
        // An unchanged replay therefore means an unchanged clipboard: skip the
        // replay's full document rebuild (rollback 683–715 ms → one restore).
        let unchanged = match (&baseline.replay, &self.clipboard_replay) {
            (Some(old), Some(now)) => Rc::ptr_eq(old, now),
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            self.clipboard_text = baseline.text;
            return Ok(());
        }
        if let Some(replay) = &baseline.replay {
            self.core.set_document(replay.source.clone());
            let copied = replay.copy_into(&mut self.core);
            // Always put the full baseline Document/reflow back, including when
            // the replay's public copy reports an error.
            self.core
                .restore_snapshot_native(full_native_id)
                .map_err(err)?;
            copied?;
        } else {
            self.core.clear_clipboard_native();
        }
        self.clipboard_text = baseline.text;
        self.clipboard_replay = baseline.replay;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{Cell, Command, Cursor};
    use hwpcraft_doc::model::control::Control;
    use serde_json::json;

    fn body_text(s: &Session) -> Vec<String> {
        s.core.document().sections[0]
            .paragraphs
            .iter()
            .map(|p| p.text.clone())
            .collect()
    }

    fn end_body(s: &mut Session) -> Result<()> {
        let section = &s.core.document().sections[0];
        let paragraph = section.paragraphs.len() - 1;
        let offset = section.paragraphs[paragraph].text.chars().count();
        s.select(
            Cursor {
                paragraph,
                offset,
                ..Default::default()
            },
            false,
        )
    }

    fn replace_clipboard(s: &mut Session) -> Result<()> {
        s.select(Cursor::default(), false)?;
        s.select(
            Cursor {
                offset: 1,
                ..Default::default()
            },
            true,
        )?;
        s.copy()?;
        Ok(())
    }

    fn roundtrip(s: &mut Session, verify: impl Fn(&Session) -> Result<()>) -> Result<()> {
        let dir = tempfile::tempdir()?;
        for extension in ["hwp", "hwpx"] {
            let path = dir.path().join(format!("clipboard.{extension}"));
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            verify(&reopened)?;
        }
        Ok(())
    }

    #[test]
    fn empty_clipboard_returns_to_empty_after_atomic_copy() -> Result<()> {
        let mut s = Session::new()?;
        let token = s.full_checkpoint()?;
        s.execute(Command::Insert("새 복사".into()))?;
        s.select_all()?;
        s.copy()?;
        assert!(s.core.has_internal_clipboard_native());
        s.rollback_full(token)?;
        assert!(!s.core.has_internal_clipboard_native());
        assert!(s.clipboard_text.is_none());
        assert!(s.clipboard_replay.is_none());
        assert!(s.run("edit.paste", json!({})).is_err());
        assert_eq!(s.text(), "");
        Ok(())
    }

    #[test]
    fn styled_multiple_paragraphs_replay_paste_and_roundtrip() -> Result<()> {
        let mut s = Session::new()?;
        s.execute(Command::Insert("굵은😀 문장\n둘째 문단".into()))?;
        s.select_all()?;
        s.execute(Command::Character(json!({"bold":true,"fontSize":1600})))?;
        s.copy()?;
        let original = s.clipboard_text.clone();
        end_body(&mut s)?;
        let token = s.full_checkpoint()?;
        let document = format!("{:?}", s.core.document());
        let layout = s.core.get_page_text_layout_native(0).map_err(err)?;
        replace_clipboard(&mut s)?;
        s.execute(Command::Insert("수정".into()))?;
        s.rollback_full(token)?;
        assert_eq!(s.clipboard_text, original);
        assert_eq!(format!("{:?}", s.core.document()), document);
        assert_eq!(s.core.get_page_text_layout_native(0).map_err(err)?, layout);
        end_body(&mut s)?;
        s.execute(Command::Split)?;
        s.run("edit.paste", json!({}))?;
        let expected = vec!["굵은😀 문장", "둘째 문단", "굵은😀 문장", "둘째 문단"];
        assert_eq!(body_text(&s), expected);
        roundtrip(&mut s, |r| {
            assert_eq!(body_text(r), expected);
            for paragraph in &r.core.document().sections[0].paragraphs {
                let id = paragraph.char_shape_id_at(0).unwrap_or(0) as usize;
                assert!(r.core.document().doc_info.char_shapes[id].bold);
            }
            Ok(())
        })
    }

    #[test]
    fn rich_tables_pictures_and_shapes_survive_replay_and_save() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("self-created.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([17, 40, 200, 255])).save(&path)?;
        let mut s = Session::new()?;
        s.execute(Command::Insert("본문 개체😀".into()))?;
        s.run(
            "picture.insert",
            json!({"path":path,"width_mm":12.0,"treat_as_char":true}),
        )?;
        s.run(
            "shape.insert",
            json!({"kind":"rectangle","width_mm":12.0,"height_mm":7.0}),
        )?;
        s.execute(Command::Table {
            rows: 1,
            columns: 1,
        })?;
        end_body(&mut s)?;
        s.select_all()?;
        s.copy()?;
        assert!(s.core.clipboard_has_control_native());
        let token = s.full_checkpoint()?;
        let before = format!("{:?}", s.core.document());
        replace_clipboard(&mut s)?;
        s.rollback_full(token)?;
        assert_eq!(format!("{:?}", s.core.document()), before);
        assert!(s.core.clipboard_has_control_native());
        end_body(&mut s)?;
        s.execute(Command::Split)?;
        s.run("edit.paste", json!({}))?;
        let verify = |r: &Session| -> Result<()> {
            let mut counts = [0, 0, 0];
            for control in r.core.document().sections[0]
                .paragraphs
                .iter()
                .flat_map(|p| &p.controls)
            {
                match control {
                    Control::Table(_) => counts[0] += 1,
                    Control::Picture(_) => counts[1] += 1,
                    Control::Shape(_) => counts[2] += 1,
                    _ => {}
                }
            }
            assert_eq!(counts, [2, 2, 2]);
            let bytes = r.core.get_bin_data_image_data_native(1).map_err(err)?;
            assert!(!bytes.is_empty());
            assert_eq!(
                image::load_from_memory(&bytes)?.to_rgba8().get_pixel(0, 0),
                &image::Rgba([17, 40, 200, 255])
            );
            assert_eq!(
                body_text(r)
                    .iter()
                    .filter(|p| p.contains("본문 개체😀"))
                    .count(),
                2
            );
            Ok(())
        };
        verify(&s)?;
        roundtrip(&mut s, verify)
    }

    #[test]
    fn cell_selection_replay_preserves_original_layout_and_protection() -> Result<()> {
        let mut s = Session::new()?;
        s.execute(Command::Table {
            rows: 1,
            columns: 1,
        })?;
        let cursor = s
            .entries()
            .into_iter()
            .find(|(c, _)| c.cell.is_some())
            .unwrap()
            .0;
        s.select(cursor, false)?;
        s.execute(Command::Insert("셀 첫 문단😀\n둘째".into()))?;
        s.select_all()?;
        s.execute(Command::Character(json!({"italic":true})))?;
        s.copy()?;
        let original_text = s.clipboard_text.clone();
        let token = s.full_checkpoint()?;
        let before = format!("{:?}", s.core.document());
        let layout = s.core.get_page_text_layout_native(0).map_err(err)?;
        s.execute(Command::Insert("다른 내용".into()))?;
        s.select_all()?;
        s.copy()?;
        s.run(
            "table.cell_properties",
            json!({"props":{"cellProtect":true}}),
        )?;
        s.rollback_full(token)?;
        assert_eq!(format!("{:?}", s.core.document()), before);
        assert_eq!(s.core.get_page_text_layout_native(0).map_err(err)?, layout);
        assert_eq!(s.clipboard_text, original_text);
        assert!(!s.cell_protected(s.cursor));
        let current = s.cursor;
        let cell = current.cell.unwrap();
        s.select(
            Cursor {
                offset: 2,
                cell: Some(Cell {
                    paragraph: 1,
                    ..cell
                }),
                ..current
            },
            false,
        )?;
        s.execute(Command::Split)?;
        s.run("edit.paste", json!({}))?;
        let verify = |r: &Session| -> Result<()> {
            let table = r.core.document().sections[0]
                .paragraphs
                .iter()
                .flat_map(|p| &p.controls)
                .find_map(|c| {
                    if let Control::Table(t) = c {
                        Some(t)
                    } else {
                        None
                    }
                })
                .unwrap();
            let paras = &table.cells[0].paragraphs;
            assert_eq!(
                paras.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(),
                ["셀 첫 문단😀", "둘째", "셀 첫 문단😀", "둘째"]
            );
            for para in paras {
                let id = para.char_shape_id_at(0).unwrap_or(0) as usize;
                assert!(r.core.document().doc_info.char_shapes[id].italic);
            }
            Ok(())
        };
        verify(&s)?;
        roundtrip(&mut s, verify)
    }

    #[test]
    fn failed_native_copy_preserves_native_replay_and_none_text_pair() -> Result<()> {
        let mut s = Session::new()?;
        s.execute(Command::Insert("복사 원문".into()))?;
        s.select_all()?;
        s.copy()?;
        // Match copy()'s existing native-Err fallback: keep the old native data
        // and replay, but disable the text-match rich-paste shortcut.
        assert!(s.core.copy_selection_native(999, 0, 0, 0, 1).is_err());
        s.clipboard_text = None;
        let token = s.full_checkpoint()?;
        replace_clipboard(&mut s)?;
        s.rollback_full(token)?;
        assert!(s.clipboard_text.is_none());
        assert_eq!(s.core.get_clipboard_text_native(), "복사 원문");
        assert!(s.clipboard_replay.is_some());
        Ok(())
    }

    #[test]
    fn untracked_clipboard_is_rejected_before_full_acquire() -> Result<()> {
        let mut s = Session::new()?;
        s.execute(Command::Insert("native 직접 복사".into()))?;
        s.core.copy_selection_native(0, 0, 0, 0, 3).map_err(err)?;
        let generation = s.generation;
        let before = format!("{:?}", s.core.document());
        assert!(s.full_checkpoint().is_err());
        assert_eq!(s.generation, generation);
        assert_eq!(format!("{:?}", s.core.document()), before);
        assert_eq!(s.core.get_clipboard_text_native(), "nat");
        Ok(())
    }

    #[test]
    fn replay_copy_error_still_restores_full_native_document() -> Result<()> {
        let mut s = Session::new()?;
        s.execute(Command::Insert("보존할 문서".into()))?;
        let id = s.save_history_snapshot();
        let before = format!("{:?}", s.core.document());
        let replay = ClipboardReplay {
            source: s.core.document().clone(),
            range: CopyRange::Body {
                section: 999,
                start_paragraph: 0,
                start_offset: 0,
                end_paragraph: 0,
                end_offset: 1,
            },
        };
        let baseline = ClipboardBaseline {
            replay: Some(Rc::new(replay)),
            text: Some("대입되면 안 됨".into()),
        };
        assert!(s.restore_clipboard(baseline, id).is_err());
        assert_eq!(format!("{:?}", s.core.document()), before);
        assert!(s.clipboard_text.is_none());
        assert!(s.clipboard_replay.is_none());
        s.retire_history_snapshot(id);
        Ok(())
    }
}
