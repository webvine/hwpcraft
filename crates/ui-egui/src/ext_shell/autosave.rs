//! 자동 복구: while a document has unsaved changes, a copy is written to the settings
//! folder's `recovery` directory every [`INTERVAL`] once the user has paused for
//! [`IDLE`] (writing a long document takes up to a second, so never mid-typing).
//! A normal exit, a save or discarding the changes removes the copy. Each window holds
//! an exclusive lock on `<pid>.lock`; at start-up, copies whose lock is free belong to a
//! window that ended without cleaning up, and the user is offered to recover them.
use super::*;
use std::fs::File;
use std::time::{Duration, Instant};

pub(crate) const INTERVAL: Duration = Duration::from_secs(300);

fn settings_default_minutes() -> u32 {
    (INTERVAL.as_secs() / 60) as u32
}
const IDLE: Duration = Duration::from_secs(2);

/// One copy left by a window that did not end normally.
#[derive(Clone, Debug)]
pub(crate) struct Orphan {
    pub copy: PathBuf,
    /// The document's own file, if it had one.
    pub original: Option<PathBuf>,
    pub saved: std::time::SystemTime,
    lock: PathBuf,
}

pub(crate) struct Recovery {
    dir: PathBuf,
    _lock: File,
    lock_path: PathBuf,
    /// This window's copy and its note, once written.
    copy: Option<(PathBuf, PathBuf)>,
    /// Document generation the copy holds.
    saved_generation: Option<u64>,
    last_save: Instant,
    last_input: Instant,
}

impl Recovery {
    /// Take this window's lock in `dir` (created if needed).
    pub(crate) fn open(dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let lock_path = dir.join(format!("{}.lock", std::process::id()));
        let lock = File::create(&lock_path)?;
        lock.try_lock().map_err(std::io::Error::other)?;
        Ok(Self {
            dir,
            _lock: lock,
            lock_path,
            copy: None,
            saved_generation: None,
            last_save: Instant::now(),
            last_input: Instant::now(),
        })
    }

    /// Copies whose window lock is free (that window ended without cleaning up).
    pub(crate) fn orphans(&self) -> Vec<Orphan> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("lock") || path == self.lock_path {
                continue;
            }
            let free = File::options()
                .write(true)
                .open(&path)
                .is_ok_and(|f| f.try_lock().is_ok());
            if !free {
                continue;
            }
            let pid = path.file_stem().map(|s| s.to_string_lossy().into_owned());
            let Some(pid) = pid else { continue };
            for ext in ["hwpx", "hwp"] {
                let copy = self.dir.join(format!("{pid}.{ext}"));
                if let Ok(meta) = std::fs::metadata(&copy) {
                    let note = std::fs::read_to_string(self.dir.join(format!("{pid}.txt")))
                        .ok()
                        .map(|t| t.trim().to_owned())
                        .filter(|t| !t.is_empty())
                        .map(PathBuf::from);
                    out.push(Orphan {
                        copy,
                        original: note,
                        saved: meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                        lock: path.clone(),
                    });
                }
            }
            if !out.iter().any(|o| o.lock == path) {
                // A window that never needed a copy: its lock is just left over.
                let _ = std::fs::remove_file(&path);
            }
        }
        out.sort_by_key(|o| std::cmp::Reverse(o.saved));
        out
    }

    fn remove_copy(&mut self) {
        if let Some((copy, note)) = self.copy.take() {
            let _ = std::fs::remove_file(copy);
            let _ = std::fs::remove_file(note);
        }
        self.saved_generation = None;
    }
}

impl Drop for Recovery {
    /// A normal exit: the document was saved or its changes were given up.
    fn drop(&mut self) {
        self.remove_copy();
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

/// Remove an orphan's copy, note and lock.
pub(crate) fn discard(orphan: &Orphan) {
    let _ = std::fs::remove_file(&orphan.copy);
    let _ = std::fs::remove_file(orphan.copy.with_extension("txt"));
    let _ = std::fs::remove_file(&orphan.lock);
}

/// Move an orphan's copy out of the recovery folder to `recovered/` under a readable
/// name, so it can be opened and saved like any document.
pub(crate) fn recover(orphan: &Orphan, dir: &std::path::Path) -> std::io::Result<PathBuf> {
    let recovered = dir.join("recovered");
    std::fs::create_dir_all(&recovered)?;
    let stem = orphan
        .original
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "새 문서".into());
    let ext = orphan
        .copy
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_else(|| "hwpx".into());
    let mut target = recovered.join(format!("{stem} (복구).{ext}"));
    let mut n = 2;
    while target.exists() {
        target = recovered.join(format!("{stem} (복구 {n}).{ext}"));
        n += 1;
    }
    std::fs::copy(&orphan.copy, &target)?;
    discard(orphan);
    Ok(target)
}

impl HwpApp {
    /// Every frame: write or remove this window's copy (see the module notes).
    pub(crate) fn autosave_frame(&mut self, ctx: &egui::Context) {
        if self.ext.shell.recovery.is_none() {
            if self.ext.shell.recovery_started {
                return;
            }
            self.ext.shell.recovery_started = true;
            let Some(dir) = config_dir().map(|d| d.join("recovery")) else {
                return;
            };
            match Recovery::open(dir) {
                Ok(recovery) => {
                    let orphans = recovery.orphans();
                    if !orphans.is_empty() && self.ext.shell.dialog.is_none() {
                        self.ext.shell.dialog = Some(Dialog::Recover {
                            orphans,
                            selected: 0,
                        });
                    }
                    self.ext.shell.recovery = Some(recovery);
                }
                Err(_) => return,
            }
        }
        let typing = ctx.input(|i| !i.events.is_empty() || i.pointer.any_down());
        let dirty = self.session.dirty();
        let generation = self.session.generation;
        let password = self.session.has_password();
        let path = self.session.path.clone();
        let Some(recovery) = self.ext.shell.recovery.as_mut() else {
            return;
        };
        if typing {
            recovery.last_input = Instant::now();
        }
        if !dirty {
            recovery.remove_copy();
            recovery.last_save = Instant::now();
            return;
        }
        if recovery.saved_generation == Some(generation) {
            return;
        }
        let minutes = self.ext.shell.settings.autosave_minutes;
        if minutes == 0 {
            return;
        }
        let Some(recovery) = self.ext.shell.recovery.as_mut() else {
            return;
        };
        let interval = if minutes == settings_default_minutes() {
            INTERVAL
        } else {
            Duration::from_secs(u64::from(minutes) * 60)
        };
        let due = recovery.last_save + interval;
        let idle = recovery.last_input + IDLE;
        let now = Instant::now();
        if now < due || now < idle {
            ctx.request_repaint_after(due.max(idle).saturating_duration_since(now));
            return;
        }
        // A password document stays encrypted: HWP with its password.
        let ext = if password { "hwp" } else { "hwpx" };
        let copy = recovery.dir.join(format!("{}.{ext}", std::process::id()));
        let note = copy.with_extension("txt");
        recovery.last_save = now;
        let written = self.session.save_copy(&copy);
        let Some(recovery) = self.ext.shell.recovery.as_mut() else {
            return;
        };
        match written {
            Ok(()) => {
                let _ = std::fs::write(
                    &note,
                    path.map(|p| p.display().to_string()).unwrap_or_default(),
                );
                recovery.copy = Some((copy, note));
                recovery.saved_generation = Some(generation);
            }
            Err(e) => {
                self.message = format!("자동 복구용 사본을 쓰지 못했습니다: {e}");
            }
        }
    }

    /// 문서 복구: copies left by windows that ended unexpectedly.
    pub(crate) fn recover_dialog(
        &mut self,
        ctx: &egui::Context,
        orphans: &mut Vec<Orphan>,
        selected: &mut usize,
    ) -> bool {
        let button = dialog(
            ctx,
            "문서 복구",
            520.0,
            &["복구해서 열기(D)", "지우기", "나중에"],
            |ui| {
                ui.label("HwpCraft가 정상적으로 끝나지 않아 저장하지 못한 문서가 있습니다.");
                group(ui, "복구할 수 있는 문서");
                for (i, o) in orphans.iter().enumerate() {
                    let name = o
                        .original
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "저장한 적 없는 새 문서".into());
                    let when = o
                        .saved
                        .elapsed()
                        .map(|d| format!("{}분 전", d.as_secs() / 60))
                        .unwrap_or_default();
                    ui.selectable_value(selected, i, format!("{name}    {when}"));
                }
                notice(
                    ui,
                    "복구한 문서는 설정 폴더의 recovered 폴더에 사본으로 열립니다. \
                     원래 파일은 바꾸지 않으니 [다른 이름으로 저장]으로 저장하세요. \
                     [나중에]를 누르면 다음에 시작할 때 다시 묻습니다.",
                );
            },
        );
        let Some(orphan) = orphans.get(*selected).cloned() else {
            return false;
        };
        match button {
            Some(0) => {
                let dir = self.ext.shell.recovery.as_ref().map(|r| r.dir.clone());
                let target = dir
                    .as_deref()
                    .and_then(|d| d.parent())
                    .ok_or_else(|| anyhow::anyhow!("설정 폴더가 없습니다."))
                    .and_then(|d| recover(&orphan, d).map_err(anyhow::Error::from));
                if let Some(target) = self.result(target) {
                    if self.session.dirty() {
                        self.open_new_window(Some(target));
                    } else {
                        self.request(Pending::Open(target), ctx);
                        self.message =
                            "복구한 문서입니다. [다른 이름으로 저장]으로 저장하세요.".into();
                    }
                }
                orphans.remove(*selected);
                *selected = 0;
                !orphans.is_empty()
            }
            Some(1) => {
                discard(&orphan);
                orphans.remove(*selected);
                *selected = 0;
                !orphans.is_empty()
            }
            Some(_) => false,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("hwpcraft-recovery-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn copies_of_a_live_window_are_not_offered_but_a_dead_ones_are() -> anyhow::Result<()> {
        let d = dir("orphans");
        let mine = Recovery::open(d.clone())?;
        // A window that is still running holds its lock.
        let live = d.join("111.lock");
        let held = File::create(&live)?;
        held.try_lock().map_err(std::io::Error::other)?;
        std::fs::write(d.join("111.hwpx"), b"x")?;
        // A window that ended: lock file left, nobody holds it.
        std::fs::write(d.join("222.lock"), b"")?;
        std::fs::write(d.join("222.hwpx"), b"y")?;
        // The original's path in this platform's form (a Windows path has no folders on Linux).
        let original = std::env::temp_dir().join("docs").join("보고서.hwpx");
        std::fs::write(d.join("222.txt"), original.to_string_lossy().as_bytes())?;
        // A window that ended without a copy: only its lock is cleaned up.
        std::fs::write(d.join("333.lock"), b"")?;
        let orphans = mine.orphans();
        assert_eq!(orphans.len(), 1, "{orphans:?}");
        assert_eq!(orphans[0].copy, d.join("222.hwpx"));
        assert_eq!(orphans[0].original.as_deref(), Some(original.as_path()));
        assert!(!d.join("333.lock").exists());
        let target = recover(&orphans[0], &d)?;
        assert!(target.ends_with(std::path::Path::new("recovered").join("보고서 (복구).hwpx")));
        assert_eq!(std::fs::read(&target)?, b"y");
        assert!(!d.join("222.hwpx").exists() && !d.join("222.lock").exists());
        drop(held);
        drop(mine);
        assert!(
            !d.join(format!("{}.lock", std::process::id())).exists(),
            "own lock removed"
        );
        let _ = std::fs::remove_dir_all(&d);
        Ok(())
    }

    #[test]
    fn a_changed_document_gets_a_copy_that_saving_removes() -> anyhow::Result<()> {
        let d = dir("copy");
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        a.ext.shell.recovery_started = true;
        a.ext.shell.recovery = Some(Recovery::open(d.clone())?);
        let ctx = a.ctx.clone();
        a.session.execute(Command::Insert("복구할 글".into()))?;
        // Not yet due: nothing is written.
        a.autosave_frame(&ctx);
        let copy = d.join(format!("{}.hwpx", std::process::id()));
        assert!(!copy.exists());
        if let Some(r) = a.ext.shell.recovery.as_mut() {
            r.last_save -= INTERVAL;
            r.last_input -= IDLE;
        }
        a.autosave_frame(&ctx);
        assert!(copy.exists(), "written once due and idle");
        let mut reopened = Session::new()?;
        reopened.open(&copy)?;
        assert!(reopened.text().contains("복구할 글"));
        assert!(
            a.session.dirty() && a.session.path.is_none(),
            "the document itself is untouched"
        );
        // Saved: the copy goes.
        let saved = d.join("doc.hwpx");
        a.session.save(&saved)?;
        a.autosave_frame(&ctx);
        assert!(!copy.exists());
        a.ext.shell.recovery = None;
        let _ = std::fs::remove_dir_all(&d);
        Ok(())
    }
}
