//! 창 배열·창 전환 (C41). Every HwpCraft window is its own process (새 창 starts another
//! one), so windows find each other in the settings folder's `windows` directory: a
//! window holds an exclusive lock on `<id>.lock` while it runs and keeps its title in
//! `<id>.title`. Another window asks it to come to the front or to move by writing
//! `<id>.cmd`; a small thread notices the file and wakes the window, which reads it
//! between frames. Positions are egui points on the main monitor.
use super::*;
use std::fs::File;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How often a window looks for a request.
const WATCH: Duration = Duration::from_millis(200);
/// How long the list of other windows is reused (the ribbon asks every frame).
const LIST_AGE: Duration = Duration::from_secs(1);
/// Kept free at the bottom of the screen for the task bar.
const TASK_BAR: f32 = 48.0;

/// Another running window.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Peer {
    pub id: u32,
    pub title: String,
}

/// What a window is asked to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Request {
    Focus,
    /// The window's outer rectangle.
    Place(egui::Rect),
}

/// 창 배열 kinds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Arrangement {
    /// 계단식: overlapping, each a step down and right.
    Cascade,
    /// 왼쪽·오른쪽으로 나란히.
    SideBySide,
    /// 위·아래로 나란히.
    Stacked,
}

pub(crate) struct Windows {
    dir: PathBuf,
    id: u32,
    _lock: File,
    title: Option<String>,
    stop: Arc<AtomicBool>,
    peers: Option<(Instant, Vec<Peer>)>,
}

impl Windows {
    /// Join the window list in `dir` as `id`; `wake` is called when a request arrives.
    pub(crate) fn open(
        dir: PathBuf,
        id: u32,
        wake: impl Fn() + Send + 'static,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let lock = File::create(dir.join(format!("{id}.lock")))?;
        lock.try_lock().map_err(std::io::Error::other)?;
        let stop = Arc::new(AtomicBool::new(false));
        let request = dir.join(format!("{id}.cmd"));
        let stopped = stop.clone();
        std::thread::Builder::new()
            .name("hwpcraft-windows".into())
            .spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    if request.exists() {
                        wake();
                    }
                    std::thread::sleep(WATCH);
                }
            })?;
        Ok(Self {
            dir,
            id,
            _lock: lock,
            title: None,
            stop,
            peers: None,
        })
    }

    fn file(&self, id: u32, ext: &str) -> PathBuf {
        self.dir.join(format!("{id}.{ext}"))
    }

    /// Keep this window's title for the others' 창 전환 list.
    pub(crate) fn set_title(&mut self, title: &str) {
        if self.title.as_deref() != Some(title) {
            self.title = Some(title.to_owned());
            let _ = std::fs::write(self.file(self.id, "title"), title);
        }
    }

    /// The other running windows, oldest first. Files left by windows that ended are removed.
    pub(crate) fn peers(&mut self) -> Vec<Peer> {
        if let Some((at, peers)) = &self.peers
            && at.elapsed() < LIST_AGE
        {
            return peers.clone();
        }
        let mut found = Vec::new();
        for entry in std::fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("lock") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str()?.parse::<u32>().ok())
            else {
                continue;
            };
            if id == self.id {
                continue;
            }
            let Ok(file) = File::options().write(true).open(&path) else {
                continue;
            };
            if file.try_lock().is_ok() {
                // That window ended without cleaning up.
                drop(file);
                for ext in ["lock", "title", "cmd"] {
                    let _ = std::fs::remove_file(self.file(id, ext));
                }
                continue;
            }
            let title = std::fs::read_to_string(self.file(id, "title")).unwrap_or_default();
            let started = entry.metadata().and_then(|m| m.modified()).ok();
            found.push((started, Peer { id, title }));
        }
        found.sort_by_key(|(started, peer)| (*started, peer.id));
        let peers: Vec<Peer> = found.into_iter().map(|(_, peer)| peer).collect();
        self.peers = Some((Instant::now(), peers.clone()));
        peers
    }

    /// Ask window `id` to do `request`.
    pub(crate) fn send(&self, id: u32, request: Request) -> std::io::Result<()> {
        let text = match request {
            Request::Focus => "focus".to_owned(),
            Request::Place(r) => {
                format!("place {} {} {} {}", r.min.x, r.min.y, r.width(), r.height())
            }
        };
        // Written whole, then renamed, so the other window never reads half a request.
        let partial = self.file(id, "cmd.part");
        std::fs::write(&partial, text)?;
        std::fs::rename(&partial, self.file(id, "cmd"))
    }

    /// The request another window left for this one, if any.
    pub(crate) fn take(&self) -> Option<Request> {
        let path = self.file(self.id, "cmd");
        let text = std::fs::read_to_string(&path).ok()?;
        let _ = std::fs::remove_file(&path);
        parse_request(&text)
    }
}

impl Drop for Windows {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for ext in ["lock", "title", "cmd"] {
            let _ = std::fs::remove_file(self.file(self.id, ext));
        }
    }
}

fn parse_request(text: &str) -> Option<Request> {
    let mut words = text.split_whitespace();
    match words.next()? {
        "focus" => Some(Request::Focus),
        "place" => {
            let numbers: Vec<f32> = words.map(|w| w.parse().ok()).collect::<Option<_>>()?;
            let [x, y, w, h] = numbers[..] else {
                return None;
            };
            let sane = [x, y, w, h].iter().all(|v| v.is_finite()) && w >= 200.0 && h >= 150.0;
            sane.then(|| {
                Request::Place(egui::Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(w, h),
                ))
            })
        }
        _ => None,
    }
}

/// Outer rectangles for `count` windows in `area`, this window first.
pub(crate) fn arrange(area: egui::Rect, count: usize, how: Arrangement) -> Vec<egui::Rect> {
    let count = count.max(1);
    let n = count as f32;
    match how {
        Arrangement::Cascade => {
            const STEP: f32 = 32.0;
            let size = (area.size() * 0.75)
                .max(egui::vec2(480.0, 360.0))
                .min(area.size());
            let room = (area.size() - size).max(egui::Vec2::ZERO);
            // Steps start again from the corner once they would leave the screen.
            let steps = ((room.x.min(room.y) / STEP) as usize + 1).max(1);
            (0..count)
                .map(|i| {
                    let offset = STEP * (i % steps) as f32;
                    egui::Rect::from_min_size(area.min + egui::vec2(offset, offset), size)
                })
                .collect()
        }
        Arrangement::SideBySide => {
            let width = area.width() / n;
            (0..count)
                .map(|i| {
                    egui::Rect::from_min_size(
                        egui::pos2(area.min.x + width * i as f32, area.min.y),
                        egui::vec2(width, area.height()),
                    )
                })
                .collect()
        }
        Arrangement::Stacked => {
            let height = area.height() / n;
            (0..count)
                .map(|i| {
                    egui::Rect::from_min_size(
                        egui::pos2(area.min.x, area.min.y + height * i as f32),
                        egui::vec2(area.width(), height),
                    )
                })
                .collect()
        }
    }
}

impl HwpApp {
    /// Every frame: join the window list once, keep the title current and do what
    /// another window asked.
    pub(crate) fn windows_frame(&mut self, ctx: &egui::Context) {
        if self.ext.shell.windows.is_none() {
            // Captures, benchmarks and tests stay out of the user's window list.
            if self.ext.shell.windows_started
                || cfg!(test)
                || self.screenshot.is_some()
                || self.ext.shell.bench.is_some()
            {
                return;
            }
            self.ext.shell.windows_started = true;
            let Some(dir) = config_dir().map(|d| d.join("windows")) else {
                return;
            };
            let wake = {
                let ctx = ctx.clone();
                move || ctx.request_repaint()
            };
            match Windows::open(dir, std::process::id(), wake) {
                Ok(windows) => self.ext.shell.windows = Some(windows),
                Err(_) => return,
            }
        }
        let title = self.ext.shell.window_title.clone();
        let Some(windows) = self.ext.shell.windows.as_mut() else {
            return;
        };
        windows.set_title(&title);
        match windows.take() {
            Some(Request::Focus) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            Some(Request::Place(outer)) => place_window(ctx, outer),
            None => {}
        }
    }

    /// The other windows, or none when the list could not be joined.
    pub(crate) fn other_windows(&mut self) -> Vec<Peer> {
        self.ext
            .shell
            .windows
            .as_mut()
            .map(Windows::peers)
            .unwrap_or_default()
    }

    /// 창 배열: this window and every other one, on the main monitor.
    pub(crate) fn arrange_windows(&mut self, ctx: &egui::Context, how: Arrangement) {
        let peers = self.other_windows();
        let screen = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1280.0, 800.0));
        let area = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(screen.x, (screen.y - TASK_BAR).max(300.0)),
        );
        let rects = arrange(area, peers.len() + 1, how);
        place_window(ctx, rects[0]);
        let mut failed = 0;
        if let Some(windows) = &self.ext.shell.windows {
            for (peer, rect) in peers.iter().zip(&rects[1..]) {
                failed += usize::from(windows.send(peer.id, Request::Place(*rect)).is_err());
            }
        }
        self.message = match (peers.len(), failed) {
            (0, _) => "다른 창이 없어 이 창만 배열했습니다.".into(),
            (n, 0) => format!("창 {}개를 배열했습니다.", n + 1),
            (n, f) => format!("창 {}개 중 {f}개에 배열을 알리지 못했습니다.", n + 1),
        };
    }

    /// 창 전환: bring window `id` to the front.
    pub(crate) fn switch_to_window(&mut self, id: u32) {
        let sent = self
            .ext
            .shell
            .windows
            .as_ref()
            .is_some_and(|w| w.send(id, Request::Focus).is_ok());
        if !sent {
            self.message = "그 창에 알리지 못했습니다. 작업 표시줄에서 고르세요.".into();
        }
    }

    // ----- 창 전환 (열린 창 목록) -----
    pub(crate) fn switch_window_dialog(
        &mut self,
        ctx: &egui::Context,
        peers: &[Peer],
        selected: &mut usize,
    ) -> bool {
        let button = dialog(ctx, "창 전환", 420.0, &["전환(D)", "취소"], |ui| {
            group(ui, "열린 창");
            for (i, peer) in peers.iter().enumerate() {
                let title = if peer.title.is_empty() {
                    "HwpCraft"
                } else {
                    &peer.title
                };
                ui.radio_value(selected, i, format!("{}. {title}", i + 1));
            }
            notice(
                ui,
                "Windows가 다른 프로그램의 창을 앞으로 가져오지 못하게 막으면 작업 표시줄 단추만 깜박입니다.",
            );
        });
        match button {
            Some(0) => {
                if let Some(peer) = peers.get(*selected) {
                    self.switch_to_window(peer.id);
                }
                false
            }
            Some(_) => false,
            None => true,
        }
    }
}

/// Move and size this window to the outer rectangle `outer`.
fn place_window(ctx: &egui::Context, outer: egui::Rect) {
    let frame = ctx
        .input(|i| {
            let v = i.viewport();
            Some(v.outer_rect?.size() - v.inner_rect?.size())
        })
        .unwrap_or(egui::vec2(16.0, 39.0))
        .max(egui::Vec2::ZERO);
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(outer.min));
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
        (outer.size() - frame).max(egui::vec2(200.0, 150.0)),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("hwpcraft-windows-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn join(dir: &std::path::Path, id: u32) -> Windows {
        Windows::open(dir.to_path_buf(), id, || {}).unwrap()
    }

    #[test]
    fn windows_see_each_other_and_pass_requests() {
        let temp = dir("pass");
        let mut a = join(&temp, 101);
        let mut b = join(&temp, 202);
        b.set_title("보고서.hwpx - HwpCraft");
        assert_eq!(
            a.peers(),
            vec![Peer {
                id: 202,
                title: "보고서.hwpx - HwpCraft".into()
            }]
        );
        assert_eq!(b.peers().len(), 1);
        a.send(202, Request::Focus).unwrap();
        assert_eq!(b.take(), Some(Request::Focus));
        assert_eq!(b.take(), None, "a request is done once");
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(640.0, 480.0));
        a.send(202, Request::Place(rect)).unwrap();
        assert_eq!(b.take(), Some(Request::Place(rect)));
        // A window that ends leaves the list; its files go with it.
        drop(b);
        a.peers = None;
        assert!(a.peers().is_empty());
        assert!(!temp.join("202.title").exists());
    }

    #[test]
    fn files_of_a_window_that_died_are_cleared() {
        let temp = dir("dead");
        let mut a = join(&temp, 101);
        // Left behind by a crash: nobody holds the lock.
        std::fs::write(temp.join("303.lock"), "").unwrap();
        std::fs::write(temp.join("303.title"), "옛 창").unwrap();
        assert!(a.peers().is_empty());
        assert!(!temp.join("303.lock").exists());
        assert!(!temp.join("303.title").exists());
    }

    #[test]
    fn bad_requests_are_ignored() {
        for text in [
            "",
            "place 1 2 3",
            "place 0 0 10 10",
            "place 0 0 NaN 400",
            "jump",
        ] {
            assert_eq!(parse_request(text), None, "{text}");
        }
        assert_eq!(parse_request(" focus\n"), Some(Request::Focus));
    }

    #[test]
    fn arrangements_fill_the_screen_without_overlap_or_cascade_inside_it() {
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1032.0));
        let side = arrange(area, 3, Arrangement::SideBySide);
        assert_eq!(side.len(), 3);
        assert_eq!(side[0].width(), 640.0);
        assert_eq!(side[2].max, area.max);
        let stacked = arrange(area, 2, Arrangement::Stacked);
        assert_eq!(stacked[1].min.y, 516.0);
        assert_eq!(stacked[1].width(), 1920.0);
        let cascade = arrange(area, 40, Arrangement::Cascade);
        assert!(
            cascade.iter().all(|r| area.contains_rect(*r)),
            "{cascade:?}"
        );
        assert_eq!(cascade[1].min, egui::pos2(32.0, 32.0));
        assert_eq!(arrange(area, 0, Arrangement::Stacked), vec![area]);
    }
}
