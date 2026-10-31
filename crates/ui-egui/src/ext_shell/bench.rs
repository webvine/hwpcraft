//! `--view bench`: drive scrolling, zoom and window resizing by script and log frame
//! times, so screen performance can be measured without mouse or keyboard input.
//! `--view bench-edit`: typing (one key, then until the screen shows it), a typing burst,
//! ribbon tab switches, page jumps and typing with 찾기 open, with each frame split into
//! the parts of `HwpApp::ui` (`Laps`).
//! `--view bench-ime`: Korean typed through the IME events (조합 then 확정, a syllable per
//! frame; spaces as text, a new paragraph every 400 characters) until
//! `HWPCRAFT_BENCH_CHARS` (default 10,000), with the undo depth and page count every 500
//! characters; then a copy is saved, reopened and compared page by page.
//! The log goes to `HWPCRAFT_BENCH_LOG` (default: `hwpcraft-bench.tsv` in the temp folder).
use super::*;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Pass start and end times, filled by egui's begin/end pass callbacks.
#[derive(Default)]
struct Clock {
    begin: Option<Instant>,
    last: Option<(f64, f64)>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Open,
    Idle,
    Wheel,
    Drag,
    Zoom(usize),
    Resize,
    /// bench-edit: one key at a time, each until the screen shows it.
    Type(usize),
    /// bench-edit: a key every frame.
    Burst,
    /// bench-edit: a ribbon tab switch every frame.
    Tabs,
    /// bench-edit: 쪽 이동 to a page, until it is drawn.
    Goto(usize),
    /// bench-edit: one key at a time with 찾기 open.
    FindType(usize),
    /// bench-ime: a syllable per frame through the IME events.
    Ime,
    /// bench-ime: save a copy, reopen it and compare.
    Check,
    Done,
}

/// Keys typed one at a time (Type and FindType).
const KEYS: usize = 12;

/// Milliseconds spent in each part of one `HwpApp::ui` pass, when a bench runs:
/// formatting sync, shortcuts, toolbar (tabs, ribbon, 서식 도구 줄), status bar, side
/// panels, document views, document keyboard, dialogs.
pub(crate) const LAP_NAMES: [&str; 8] = [
    "sync", "keys", "toolbar", "status", "side", "views", "keyboard", "dialogs",
];
pub(crate) struct Laps {
    at: Option<Instant>,
    ms: [f64; 8],
    next: usize,
}
impl Laps {
    pub(crate) fn new(on: bool) -> Self {
        Self {
            at: on.then(Instant::now),
            ms: [0.0; 8],
            next: 0,
        }
    }
    /// Close the current part.
    pub(crate) fn mark(&mut self) {
        if let Some(at) = self.at {
            let now = Instant::now();
            if let Some(slot) = self.ms.get_mut(self.next) {
                *slot = (now - at).as_secs_f64() * 1000.0;
            }
            self.next += 1;
            self.at = Some(now);
        }
    }
    pub(crate) fn done(self) -> [f64; 8] {
        self.ms
    }
}

const ZOOMS: [f32; 8] = [1.0, 1.25, 1.5, 2.0, 1.0, 0.75, 0.5, 1.0];
const SIZES: [[f32; 2]; 2] = [[1501.0, 916.0], [1180.0, 800.0]];

pub(crate) struct Bench {
    clock: Arc<Mutex<Clock>>,
    start: Instant,
    phase: Phase,
    frames: u32,
    offset: f32,
    rows: Vec<String>,
    settle: Vec<String>,
    phase_start: Instant,
    /// bench-edit: the scripted run instead of scroll/zoom/resize.
    edit: bool,
    /// When the key being measured went in.
    key_at: Option<Instant>,
    /// Latency of the key that settled in this frame, for its row.
    latency: Option<f64>,
    /// bench-ime: characters to type, typed so far, and the syllable still being composed.
    ime: Option<(usize, usize, Option<String>)>,
}

/// bench-ime text, typed over and over.
const IME_TEXT: &str = "본 절에서는 사업 추진 현황과 성과를 정리하고, 하반기 세부 운영 계획과 예산 집행 방향을 함께 제시한다. ";

impl Bench {
    fn new(ctx: &egui::Context, edit: bool) -> Self {
        let clock = Arc::new(Mutex::new(Clock::default()));
        let c = clock.clone();
        ctx.on_begin_pass(
            "hwpcraft-bench",
            Arc::new(move |_| {
                if let Ok(mut c) = c.lock() {
                    c.begin = Some(Instant::now());
                }
            }),
        );
        let c = clock.clone();
        let start = Instant::now();
        ctx.on_end_pass(
            "hwpcraft-bench",
            Arc::new(move |_| {
                if let Ok(mut c) = c.lock()
                    && let Some(begin) = c.begin
                {
                    let now = Instant::now();
                    c.last = Some((
                        begin.duration_since(start).as_secs_f64() * 1000.0,
                        (now - begin).as_secs_f64() * 1000.0,
                    ));
                }
            }),
        );
        Self {
            clock,
            start,
            phase: Phase::Open,
            frames: 0,
            offset: 0.0,
            rows: vec![format!(
                "phase\tframe\tbegin_ms\tpass_ms\t{}\trasters\tsvg_ms\traster_ms\tupload_kpx\tlatency_ms\tframe_cpu_ms\tzoom\toffset",
                LAP_NAMES.join("\t")
            )],
            settle: Vec::new(),
            phase_start: start,
            edit,
            key_at: None,
            latency: None,
            ime: None,
        }
    }
}

fn epoch_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

impl HwpApp {
    pub(crate) fn start_bench(&mut self, edit: bool) {
        self.ext.shell.bench = Some(Bench::new(&self.ctx, edit));
        self.ext.shell.rasters = 0;
    }

    pub(crate) fn start_ime_bench(&mut self) {
        self.start_bench(false);
        let target = std::env::var("HWPCRAFT_BENCH_CHARS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000);
        if let Some(bench) = self.ext.shell.bench.as_mut() {
            bench.ime = Some((target, 0, None));
        }
    }

    /// bench-ime: next frame's events — the composed syllable committed, the next one
    /// composed (or a space typed), and Enter every 400 characters.
    fn bench_ime_step(&mut self, bench: &mut Bench) -> bool {
        let Some((target, typed, composing)) = bench.ime.as_mut() else {
            return true;
        };
        self.ctx
            .memory_mut(|m| m.request_focus(egui::Id::new("document-canvas")));
        let mut events = Vec::new();
        if let Some(syllable) = composing.take() {
            events.push(egui::Event::Ime(egui::ImeEvent::Commit(syllable)));
        }
        if *typed >= *target {
            self.ext.shell.bench_events = events;
            return true;
        }
        let c = IME_TEXT.chars().cycle().nth(*typed).unwrap_or(' ');
        if ('가'..='힣').contains(&c) {
            events.push(egui::Event::Ime(egui::ImeEvent::Preedit {
                text: c.to_string(),
                active_range_chars: None,
            }));
            *composing = Some(c.to_string());
        } else {
            events.push(egui::Event::Text(c.to_string()));
        }
        *typed += 1;
        if (*typed).is_multiple_of(400) {
            if let Some(syllable) = composing.take() {
                events.push(egui::Event::Ime(egui::ImeEvent::Commit(syllable)));
            }
            for pressed in [true, false] {
                events.push(egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Default::default(),
                });
            }
        }
        if (*typed).is_multiple_of(500) {
            bench.settle.push(format!(
                "ime\tchars={typed}\tundo={}\tpages={}\tgeneration={}\tms={:.0}",
                self.session.undo_depth(),
                self.session.page_count().unwrap_or(0),
                self.session.generation,
                bench.start.elapsed().as_secs_f64() * 1000.0
            ));
        }
        self.ext.shell.bench_events = events;
        false
    }

    /// bench-ime: save a copy in the document's own format, reopen it and compare pages.
    fn bench_ime_check(&mut self, bench: &mut Bench) {
        let ext = self
            .session
            .path
            .as_deref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .unwrap_or("hwpx")
            .to_ascii_lowercase();
        let path =
            std::env::temp_dir().join(format!("hwpcraft-bench-ime-{}.{ext}", std::process::id()));
        let pages = self.session.page_count().unwrap_or(0);
        let reopened = self.session.save_copy(&path).and_then(|_| {
            let mut back = hwpcraft_engine::Session::new()?;
            back.open(&path)?;
            Ok(back)
        });
        let line = match reopened {
            Ok(back) => {
                let back_pages = back.page_count().unwrap_or(0);
                let same = (0..pages.min(back_pages))
                    .filter(|p| {
                        let mine = self.session.render_page_svg(*p).ok();
                        mine.is_some() && mine == back.render_page_svg(*p).ok()
                    })
                    .count();
                format!(
                    "check\tpages={pages}\treopened={back_pages}\tsame_pages={same}\tundo={}\tfile={}",
                    self.session.undo_depth(),
                    path.display()
                )
            }
            Err(e) => format!("check\terror={e}"),
        };
        bench.settle.push(line);
    }

    /// bench-edit: put one key in as typing does, and start its clock.
    fn bench_key(&mut self, bench: &mut Bench) {
        self.typed_text("가".into());
        bench.key_at = Some(Instant::now());
    }

    /// bench-edit: the key's latency once every visible page shows it.
    fn bench_key_settled(&self, bench: &mut Bench, settled: bool) -> bool {
        if let (true, Some(at)) = (settled, bench.key_at) {
            let ms = at.elapsed().as_secs_f64() * 1000.0;
            bench.latency = Some(ms);
            bench.key_at = None;
            bench.settle.push(format!(
                "{:?}\tframes={}\tms={ms:.1}",
                bench.phase, bench.frames
            ));
            return true;
        }
        false
    }

    /// True when every page drawn in the last pass is rasterized at the current scale.
    fn bench_settled(&self) -> bool {
        let generation = self.session.generation;
        let (_, now, _) = &self.ext.shell.requested;
        !now.is_empty()
            && now.iter().all(|p| {
                self.ext.shell.page_scale.get(p).is_some_and(|s| {
                    self.ext.shell.page_drawn.get(p) == Some(&generation) && {
                        let scale = fit_texture(
                            raster_scale(self.zoom, self.ctx.pixels_per_point()),
                            self.page_sizes.1.get(*p as usize).copied(),
                            self.ctx.input(|i| i.max_texture_side),
                        );
                        *s == scale_key(scale)
                    }
                })
            })
    }

    pub(crate) fn bench_frame(&mut self, ctx: &egui::Context) {
        let Some(mut bench) = self.ext.shell.bench.take() else {
            return;
        };
        ctx.request_repaint();
        let last = bench.clock.lock().ok().and_then(|c| c.last);
        let rasters = std::mem::take(&mut self.ext.shell.rasters);
        let [svg_ms, raster_ms] = self.ext.shell.raster_ms.take();
        let upload = std::mem::take(&mut self.ext.shell.uploaded) as f64 / 1000.0;
        // The laps are the previous pass's, as is `last`.
        let laps = self.ext.shell.laps.map(|ms| format!("{ms:.2}")).join("\t");
        let latency = bench
            .latency
            .take()
            .map_or(String::new(), |ms| format!("{ms:.1}"));
        let cpu = self.ext.shell.frame_cpu_ms.unwrap_or(0.0);
        if let Some((begin, pass)) = last {
            bench.rows.push(format!(
                "{:?}\t{}\t{begin:.2}\t{pass:.2}\t{laps}\t{rasters}\t{svg_ms:.1}\t{raster_ms:.1}\t{upload:.0}\t{latency}\t{cpu:.2}\t{:.2}\t{:.0}",
                bench.phase, bench.frames, self.zoom, bench.offset
            ));
        }
        bench.frames += 1;
        let settled = self.bench_settled();
        let elapsed = bench.phase_start.elapsed().as_secs_f64() * 1000.0;
        let next = match bench.phase {
            Phase::Idle if bench.ime.is_some() && bench.frames >= 30 => {
                let _ = self.run("edit.goto", json!({"paragraph": 2}));
                Some(Phase::Ime)
            }
            Phase::Ime => self.bench_ime_step(&mut bench).then_some(Phase::Check),
            Phase::Check => {
                self.bench_ime_check(&mut bench);
                Some(Phase::Done)
            }
            Phase::Idle if bench.edit && bench.frames >= 30 => {
                // Type in the middle of the first page's body.
                let _ = self.run("edit.goto", json!({"paragraph": 2}));
                Some(Phase::Type(0))
            }
            Phase::Type(k) | Phase::FindType(k) => {
                if bench.frames == 1 {
                    self.bench_key(&mut bench);
                }
                // The pages checked in the key's own pass are from before it went in.
                let done = bench.frames > 1 && self.bench_key_settled(&mut bench, settled)
                    || bench.frames >= 240;
                done.then(|| match bench.phase {
                    Phase::Type(_) if k + 1 < KEYS => Phase::Type(k + 1),
                    Phase::Type(_) => Phase::Burst,
                    _ if k + 1 < KEYS => Phase::FindType(k + 1),
                    _ => Phase::Done,
                })
            }
            Phase::Burst => {
                self.bench_key(&mut bench);
                (bench.frames >= 60).then_some(Phase::Tabs)
            }
            Phase::Tabs => {
                self.tab = 1 + bench.frames as usize % 8;
                (bench.frames >= 48).then(|| {
                    self.tab = 1;
                    Phase::Goto(0)
                })
            }
            Phase::Goto(step) => {
                let count = self.page_sizes.1.len().max(1);
                let targets = [count / 2, count - 1, 0];
                if bench.frames == 1 {
                    let _ = self.run("edit.goto", json!({"page": targets[step] + 1}));
                }
                (bench.frames > 2 && settled || bench.frames >= 300).then(|| {
                    bench.settle.push(format!(
                        "goto page {}\tframes={}\tms={elapsed:.0}\tsettled={settled}",
                        targets[step] + 1,
                        bench.frames
                    ));
                    if step + 1 < targets.len() {
                        Phase::Goto(step + 1)
                    } else {
                        let _ = self.run("edit.goto", json!({"paragraph": 2}));
                        self.open_find(false);
                        self.find = "의".into();
                        Phase::FindType(0)
                    }
                })
            }
            Phase::Open if settled && self.session.path.is_some() => {
                bench.settle.push(format!(
                    "open\tready_epoch_ms={}\tsince_app_ms={:.0}",
                    epoch_ms(),
                    bench.start.elapsed().as_secs_f64() * 1000.0
                ));
                Some(Phase::Idle)
            }
            Phase::Idle if !bench.edit && bench.ime.is_none() && bench.frames >= 60 => {
                Some(Phase::Wheel)
            }
            Phase::Wheel => {
                // About two wheel notches per frame.
                bench.offset += 100.0;
                self.document_scroll.pending[1] = Some(bench.offset);
                (bench.frames >= 240).then_some(Phase::Drag)
            }
            Phase::Drag => {
                // Scroll-bar drag: about a page per frame.
                bench.offset += 1200.0;
                self.document_scroll.pending[1] = Some(bench.offset);
                (bench.frames >= 60).then(|| {
                    bench.offset = 0.0;
                    self.document_scroll.pending[1] = Some(0.0);
                    Phase::Zoom(0)
                })
            }
            Phase::Zoom(step) => {
                if bench.frames == 1 {
                    self.set_zoom(ZOOMS[step]);
                }
                (bench.frames > 2 && settled || bench.frames >= 300).then(|| {
                    bench.settle.push(format!(
                        "zoom {:.2}\tframes={}\tms={elapsed:.0}\tsettled={settled}",
                        ZOOMS[step], bench.frames
                    ));
                    if step + 1 < ZOOMS.len() {
                        Phase::Zoom(step + 1)
                    } else {
                        Phase::Resize
                    }
                })
            }
            Phase::Resize => {
                if bench.frames % 3 == 1 {
                    let size = SIZES[(bench.frames / 3) as usize % 2];
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size.into()));
                }
                (bench.frames >= 90).then(|| {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(SIZES[0].into()));
                    Phase::Done
                })
            }
            Phase::Done => {
                let path = std::env::var_os("HWPCRAFT_BENCH_LOG")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::env::temp_dir().join("hwpcraft-bench.tsv"));
                let mut text = bench.rows.join("\n");
                text.push_str("\n#\n");
                text.push_str(&bench.settle.join("\n"));
                text.push('\n');
                let _ = std::fs::write(path, text);
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            _ => None,
        };
        if let Some(next) = next {
            bench.phase = next;
            bench.frames = 0;
            bench.phase_start = Instant::now();
        }
        self.ext.shell.bench = Some(bench);
    }
}
