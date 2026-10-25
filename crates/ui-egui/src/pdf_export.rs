//! Owner: format area. Desktop PDF export off the UI thread.
//!
//! RHWP's PDF conversion lays out and embeds fonts for every page and takes
//! minutes on long documents (217 pages: about 138 s), which froze the window
//! when it ran inside `Session::save`. The session only prepares the page SVGs
//! (`Session::pdf_job`, well under a second); a worker thread converts and
//! writes the file atomically, and the UI polls for the result each frame.
use super::*;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BUSY: &str = "이전 PDF 저장이 아직 진행 중입니다. 끝난 뒤 다시 저장해 주세요.";

/// One running export: the result arrives on `done`.
pub(crate) struct PdfWorker {
    path: PathBuf,
    pages: usize,
    started: Instant,
    done: mpsc::Receiver<std::result::Result<(), String>>,
}

impl PdfWorker {
    /// `convert` is `Session::pdf_job(..).convert`, which needs no session.
    pub(crate) fn spawn(
        pages: usize,
        path: PathBuf,
        convert: impl FnOnce() -> Result<Vec<u8>> + Send + 'static,
    ) -> Result<Self> {
        let (tx, done) = mpsc::channel();
        let target = path.clone();
        std::thread::Builder::new()
            .name("hwpcraft-pdf-export".into())
            .spawn(move || {
                let result = convert()
                    .and_then(|bytes| hwpcraft_formats::atomic_write(&target, &bytes))
                    .map_err(|e| format!("{e:#}"));
                // The UI may have gone away (window closed); nothing to report then.
                let _ = tx.send(result);
            })
            .map_err(|e| anyhow::anyhow!("PDF 저장 작업을 시작하지 못했습니다: {e}"))?;
        Ok(Self {
            path,
            pages,
            started: Instant::now(),
            done,
        })
    }
    /// `None` while converting; a dropped sender means the worker panicked.
    pub(crate) fn try_finish(&self) -> Option<std::result::Result<(), String>> {
        match self.done.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("PDF 변환 작업이 비정상적으로 끝났습니다.".into()))
            }
        }
    }
    fn elapsed(&self) -> u64 {
        self.started.elapsed().as_secs()
    }
}

/// At most one export at a time, plus the close request it may be holding back.
#[derive(Default)]
pub(crate) struct PdfSlot {
    worker: Option<PdfWorker>,
    close_after: bool,
    /// The progress line last written to the status bar, so it is only refreshed
    /// while no other message has replaced it.
    progress: String,
}

impl PdfSlot {
    pub(crate) fn busy(&self) -> bool {
        self.worker.is_some()
    }
    pub(crate) fn start(
        &mut self,
        pages: usize,
        path: PathBuf,
        convert: impl FnOnce() -> Result<Vec<u8>> + Send + 'static,
    ) -> Result<()> {
        if self.busy() {
            anyhow::bail!(BUSY);
        }
        self.worker = Some(PdfWorker::spawn(pages, path, convert)?);
        Ok(())
    }
    /// `start` plus the first progress line, which `poll_pdf_export` keeps
    /// refreshing while the status bar still shows it.
    pub(crate) fn begin(
        &mut self,
        pages: usize,
        path: PathBuf,
        convert: impl FnOnce() -> Result<Vec<u8>> + Send + 'static,
    ) -> Result<String> {
        self.start(pages, path, convert)?;
        self.progress = self.progress_line();
        Ok(self.progress.clone())
    }
    fn progress_line(&self) -> String {
        self.worker.as_ref().map_or_else(String::new, |w| {
            format!(
                "PDF로 저장하는 중… {}쪽 · {}초 경과 · 저장 시점의 문서로 내보냅니다 (편집은 계속할 수 있습니다)",
                w.pages,
                w.elapsed()
            )
        })
    }
}

impl HwpApp {
    /// Start a background PDF export of the whole document. True when it started.
    pub(crate) fn start_pdf_export(&mut self, path: PathBuf) -> bool {
        if self.pdf.busy() {
            self.result::<()>(Err(anyhow::anyhow!(BUSY)));
            return false;
        }
        let job = self
            .session
            .page_count()
            .and_then(|count| self.session.pdf_job(&(0..count).collect::<Vec<_>>()));
        match self.result(job) {
            Some(job) => {
                let pages = job.page_count();
                self.begin_pdf_export(pages, path, move || job.convert())
            }
            None => false,
        }
    }
    /// Start any prepared PDF (`convert` from `Session::pdf_job` or a page/block
    /// variant) in the shared slot and show its live progress in the status bar,
    /// refreshed by `poll_pdf_export`. True when it started; errors are shown.
    pub(crate) fn begin_pdf_export(
        &mut self,
        pages: usize,
        path: PathBuf,
        convert: impl FnOnce() -> Result<Vec<u8>> + Send + 'static,
    ) -> bool {
        let started = self.pdf.begin(pages, path, convert);
        match self.result(started) {
            Some(line) => {
                self.message = line;
                true
            }
            None => false,
        }
    }
    /// Called every frame: refresh progress and report the finished export.
    pub(crate) fn poll_pdf_export(&mut self, ctx: &egui::Context) {
        let Some(worker) = self.pdf.worker.as_ref() else {
            return;
        };
        let Some(result) = worker.try_finish() else {
            if self.message == self.pdf.progress {
                self.pdf.progress = self.pdf.progress_line();
                self.message = self.pdf.progress.clone();
            }
            ctx.request_repaint_after(Duration::from_millis(500));
            return;
        };
        let (path, pages, secs) = (worker.path.clone(), worker.pages, worker.elapsed());
        self.pdf.worker = None;
        match result {
            Ok(()) => {
                self.message = format!("PDF 저장 완료 · {} ({pages}쪽, {secs}초)", path.display())
            }
            Err(e) => {
                self.result::<()>(Err(anyhow::anyhow!("PDF로 저장하지 못했습니다: {e}")));
            }
        }
        if std::mem::take(&mut self.pdf.close_after) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    /// A close request during an export waits for it; a second one closes now
    /// (the PDF is then not written; the atomic write never leaves a partial file).
    /// True when this close request was held back.
    pub(crate) fn hold_close_for_pdf(&mut self, ctx: &egui::Context) -> bool {
        if !self.pdf.busy() || !ctx.input(|i| i.viewport().close_requested()) {
            return false;
        }
        if self.pdf.close_after {
            return false;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        self.pdf.close_after = true;
        self.message = "PDF 저장이 끝나면 창을 닫습니다. 지금 닫으려면 닫기를 한 번 더 누르세요 (PDF는 저장되지 않습니다).".into();
        self.pdf.progress.clear();
        true
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// A unique directory under the system temp folder, removed on drop.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "hwpcraft-pdf-export-test-{}-{n}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn start(slot: &mut PdfSlot, text: &str, path: PathBuf) -> Result<()> {
        let mut s = Session::new().unwrap();
        s.execute(hwpcraft_engine::Command::Insert(text.into()))
            .unwrap();
        let job = s.pdf_job(&[0]).unwrap();
        slot.start(job.page_count(), path, move || job.convert())
    }

    fn wait(slot: &PdfSlot) -> std::result::Result<(), String> {
        let until = Instant::now() + Duration::from_secs(120);
        loop {
            if let Some(result) = slot.worker.as_ref().unwrap().try_finish() {
                return result;
            }
            assert!(Instant::now() < until, "PDF export did not finish");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn export_runs_off_the_calling_thread_and_writes_a_pdf() {
        let dir = TempDir::new();
        let path = dir.0.as_path().join("백그라운드.pdf");
        let mut slot = PdfSlot::default();
        start(&mut slot, "PDF 백그라운드 저장", path.clone()).unwrap();
        assert!(slot.busy());
        // A second export while one runs is refused and leaves the first alone.
        let second = start(&mut slot, "두 번째", dir.0.as_path().join("second.pdf"));
        assert!(second.is_err());
        assert!(slot.busy());
        assert_eq!(wait(&slot), Ok(()));
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF"), "not a PDF");
        assert!(!dir.0.as_path().join("second.pdf").exists());
    }

    #[test]
    fn begin_reports_live_progress_until_the_export_finishes() {
        let dir = TempDir::new();
        let mut slot = PdfSlot::default();
        let (release, gate) = mpsc::channel::<()>();
        let line = slot
            .begin(3, dir.0.join("progress.pdf"), move || {
                // Held open until the test has looked at the progress line.
                let _ = gate.recv();
                Ok(b"%PDF-1.7\n".to_vec())
            })
            .unwrap();
        assert!(line.contains("3쪽"), "{line}");
        assert_eq!(slot.progress, line, "poll refreshes this line");
        assert!(slot.progress_line().contains("초 경과"));
        assert!(slot.begin(1, dir.0.join("x.pdf"), || Ok(vec![])).is_err());
        release.send(()).unwrap();
        assert_eq!(wait(&slot), Ok(()));
    }

    #[test]
    fn a_failed_write_is_reported_not_lost() {
        let dir = TempDir::new();
        // A directory path cannot be replaced by the atomic file write.
        let target = dir.0.as_path().join("occupied");
        std::fs::create_dir(&target).unwrap();
        let mut slot = PdfSlot::default();
        start(&mut slot, "실패 경로", target).unwrap();
        assert!(wait(&slot).is_err());
    }
}
