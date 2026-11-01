//! AI 편집 패널 (0.21): a chat panel on the right where a locally installed
//! Claude Code or Codex edits the open document through this window's MCP
//! connection.
//!
//! The shell draws the dock (`Panel::right`, width, open state) and calls
//! `ai_panel_ui` inside it; it reads `ai_busy` to lock document editing while a
//! question runs, `ai_panel_take_close` when the panel's ✕ was pressed, and
//! calls `ai_stop` before the window closes.
use super::*;
use context::Context;
use runner::{Output, Request, Running};
use stream::{Engine, Turn};

mod changes;
mod context;
mod history;
mod markdown;
mod models;
mod runner;
mod sample;
mod stream;
mod view;

/// A question that runs longer is stopped (PLAN §8.6).
const TIME_LIMIT: Duration = Duration::from_secs(600);

/// Standing instructions for the AI (PLAN §8.5).
const SYSTEM: &str = "너는 HwpCraft 데스크톱 창에 열린 한글 문서를 `hwpcraft` MCP 도구로만 편집한다.
- 대상은 이 창의 문서 하나다. document_id 인자가 있는 도구에는 \"{document_id}\"를 쓰고, 그 인자가 없는 도구(list_commands 등)에는 넣지 않는다.
- 새 문서를 만들거나, 다른 파일을 열거나, 저장하지 않는다. 저장은 사람이 한다.
- 사용자가 '이 부분', '여기', '선택한 곳'이라고 하면 [맥락]의 영역을 뜻한다. 필요하면 그 밖도 고칠 수 있다.
- 편집을 마치면 무엇을 바꿨는지 한두 문장의 한국어로 답한다.";

/// Engines in the order the panel lists them.
const ENGINES: [Engine; 2] = [Engine::Claude, Engine::Codex];

impl Engine {
    /// Index into per-engine arrays.
    fn slot(self) -> usize {
        match self {
            Engine::Claude => 0,
            Engine::Codex => 1,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Engine::Claude => "Claude Code",
            Engine::Codex => "Codex",
        }
    }
}

/// One installed CLI, or why it cannot be used.
#[derive(Clone, Debug)]
struct Cli {
    program: Option<PathBuf>,
    /// Why the engine is greyed out.
    unusable: Option<String>,
}

/// One bubble in the conversation.
#[derive(Clone, Debug)]
pub(crate) struct Message {
    pub user: bool,
    pub text: String,
    /// The region sent with a question (chip text).
    pub context: Option<String>,
    /// The AI's turn while it runs and after.
    pub turn: Option<Turn>,
    /// The folded command line is open.
    pub expanded: bool,
    /// Edits the question made (one undo step).
    pub changed: bool,
    pub stopped: bool,
    pub error: Option<String>,
    /// The question's place in the history and what it changed.
    pub link: Option<history::Link>,
}

impl Message {
    fn user(text: String, context: Option<String>) -> Self {
        Self {
            user: true,
            text,
            context,
            turn: None,
            expanded: false,
            changed: false,
            stopped: false,
            error: None,
            link: None,
        }
    }
    fn ai() -> Self {
        Self {
            user: false,
            turn: Some(Turn::default()),
            ..Self::user(String::new(), None)
        }
    }
    fn failed(error: String) -> Self {
        Self {
            turn: None,
            error: Some(error),
            ..Self::ai()
        }
    }
}

/// The question being answered.
struct Run {
    running: Running,
    /// Index of its AI bubble.
    message: usize,
    engine: Engine,
    /// Stdout closed (the CLI is done or dying).
    ended: bool,
    stderr: String,
    timed_out: bool,
    /// When the CLI process was first seen ended while its output stayed open
    /// (a helper it started may hold stdout).
    exited_at: Option<std::time::Instant>,
    /// The document's paragraphs before the question (the history's change set).
    before: Vec<changes::Para>,
    /// The run's document id and token: never written to the history.
    secrets: Vec<String>,
}

/// Panel state, stored in `HwpApp::ext.ai`.
#[derive(Default)]
pub(crate) struct State {
    close_requested: bool,
    engine: Engine,
    input: String,
    messages: Vec<Message>,
    /// Conversation to continue, per engine (its session/thread id).
    conversation: Option<(Engine, String)>,
    run: Option<Run>,
    /// CLI detection running on a thread (it starts processes).
    detecting: Option<std::sync::mpsc::Receiver<Detected>>,
    /// Installed CLIs, found when the panel is first shown.
    clis: Option<Vec<(Engine, Cli)>>,
    /// The next question goes without the context chip (its ✕ was pressed).
    skip_context: bool,
    /// The panel was closed while a question ran: release the MCP connection
    /// the panel started once the question ends.
    release_after_run: bool,
    /// Tests shorten `TIME_LIMIT`.
    time_limit: Option<Duration>,
    /// Models each engine offers (newest first), read with the CLIs.
    models: [Vec<models::Model>; 2],
    /// Chosen model (index into `models`) and reasoning level, per engine;
    /// kept while the app runs. Index 0, the newest model, is the default.
    model_pick: [usize; 2],
    effort_pick: [Option<String>; 2],
    /// The conversation history of the open document.
    hist: history::Hist,
    /// The context chip's text, kept until the caret, the selection or the
    /// document changes (a large selection is costly to read every frame).
    chip: Option<(ChipKey, Option<String>)>,
    /// `--view ai-sample-busy`: the message shown as running (no CLI runs);
    /// 중지 clears it.
    sample_busy: Option<usize>,
    /// The history list's settings (switches, delete-all) are shown.
    history_settings: bool,
    #[cfg(test)]
    busy_for_test: bool,
}

/// What the detection thread finds: the CLIs and each engine's models.
type Detected = (Vec<(Engine, Cli)>, [Vec<models::Model>; 2]);

/// What the context chip depends on: document generation, caret, selection
/// anchor, and whether the caret is in a nested table or text box.
type ChipKey = (u64, Cursor, Option<Cursor>, bool);

/// Where the AI's helper files live: `<settings>/ai-work/` (never the
/// document's folder).
fn work_root() -> PathBuf {
    ext_shell::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("ai-work")
}

/// Find both CLIs; Codex must also run with its shell off (PLAN §8.6). Without
/// `check_codex` (screen captures) that check, which writes the panel's Codex
/// settings, is left for a real run.
fn detect(check_codex: bool) -> Vec<(Engine, Cli)> {
    detect_with(runner::find_cli, check_codex)
}

fn detect_with(find: impl Fn(Engine) -> Option<PathBuf>, check_codex: bool) -> Vec<(Engine, Cli)> {
    ENGINES
        .into_iter()
        .map(|engine| {
            let program = find(engine);
            let unusable = match (&program, engine) {
                (None, _) => Some(format!(
                    "{}가 설치되어 있지 않습니다. 설치하고 로그인한 뒤 패널을 다시 여세요.",
                    engine.name()
                )),
                (Some(p), Engine::Codex) if check_codex => {
                    runner::codex_shell_off(p, &work_root().join("codex-home"))
                        .err()
                        .map(|e| e.to_string())
                }
                _ => None,
            };
            (engine, Cli { program, unusable })
        })
        .collect()
}

/// The line of a CLI's stderr that says why it stopped: the last one that is
/// not a warning or chatter (Codex warns about PATH aliases on every run).
fn cause(stderr: &str) -> Option<&str> {
    stderr.lines().map(str::trim).rev().find(|l| {
        !l.is_empty() && !l.starts_with("WARNING") && !l.starts_with("Reading additional input")
    })
}

/// What a question is sent with.
enum Region {
    Context(Context),
    /// The user pressed the chip's ✕: this question is not about that region.
    Dropped,
    /// Nothing to send (the caret is in a nested table or a text box).
    Unknown,
}

/// The question as sent: the region block, then the request.
fn prompt(region: &Region, question: &str) -> String {
    match region {
        Region::Context(c) => format!("{}\n[요청]\n{question}", c.block()),
        // The AI can read the caret over MCP; it must not quietly use the
        // place the user just left out.
        Region::Dropped => format!(
            "[맥락]\n사용자가 영역을 빼고 보냈다. 요청은 문서 전체를 두고 판단한다. '이 부분', '여기' 같은 말이 무엇을 가리키는지 분명하지 않으면 캐럿이나 선택 위치로 짐작하지 말고, 편집하지 말고 되묻는다.\n\n[요청]\n{question}"
        ),
        Region::Unknown => question.to_owned(),
    }
}

impl HwpApp {
    /// True while an AI question runs; document editing is locked meanwhile.
    pub(crate) fn ai_busy(&self) -> bool {
        #[cfg(test)]
        if self.ext.ai.busy_for_test {
            return true;
        }
        self.ext.ai.run.is_some() || self.ext.ai.sample_busy.is_some()
    }
    /// Tests of the editing lock: pretend a question is running.
    #[cfg(test)]
    pub(crate) fn ai_set_busy_for_test(&mut self, busy: bool) {
        self.ext.ai.busy_for_test = busy;
    }
    /// True once after the panel's ✕ was pressed; the shell then hides the dock.
    pub(crate) fn ai_panel_take_close(&mut self) -> bool {
        let close = std::mem::take(&mut self.ext.ai.close_requested);
        if close {
            if self.ai_busy() {
                self.ext.ai.release_after_run = true;
            } else {
                self.ai_panel_release_connection();
                // A CLI installed or signed in meanwhile is found on reopening.
                self.ext.ai.clis = None;
            }
        }
        close
    }
    /// The context chip's text (cached, see `State::chip`).
    fn ai_chip_text(&mut self) -> Option<String> {
        let key = (
            self.session.generation,
            self.session.cursor,
            self.session.anchor_cursor(),
            self.session.nested_caret().is_some(),
        );
        if let Some((k, chip)) = &self.ext.ai.chip
            && *k == key
        {
            return chip.clone();
        }
        let chip = Context::of(&self.session).map(|c| c.chip());
        self.ext.ai.chip = Some((key, chip.clone()));
        chip
    }
    /// Why the chosen engine cannot run, if it cannot.
    fn ai_unusable(&self) -> Option<String> {
        let engine = self.ext.ai.engine;
        let Some(clis) = &self.ext.ai.clis else {
            return Some("Claude Code·Codex를 확인하는 중입니다…".to_owned());
        };
        clis.iter()
            .find(|(e, _)| *e == engine)
            .and_then(|(_, c)| c.unusable.clone())
    }
    /// Find the CLIs and models on a thread (it runs `codex features list`),
    /// then take the result on a later frame.
    fn ai_detect(&mut self, ctx: &egui::Context) {
        if self.ext.ai.clis.is_some() {
            return;
        }
        if let Some(rx) = &self.ext.ai.detecting {
            if let Ok((clis, models)) = rx.try_recv() {
                self.ext.ai.clis = Some(clis);
                self.ext.ai.models = models;
                self.ext.ai.detecting = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            return;
        }
        // Screen captures leave the settings folder alone: no Codex check
        // (it writes the panel's Codex settings).
        let check_codex = self.screenshot.is_none();
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let clis = detect(check_codex);
            let home = runner::CodexLogin::user_home();
            let models = ENGINES.map(|e| models::list(e, home.as_deref()));
            let _ = tx.send((clis, models));
            ctx.request_repaint();
        });
        self.ext.ai.detecting = Some(rx);
    }

    /// Load each engine's models (tests; the panel loads them on its thread).
    #[cfg(test)]
    fn ai_load_models(&mut self) {
        let codex_home = runner::CodexLogin::user_home();
        for engine in ENGINES {
            self.ext.ai.models[engine.slot()] = models::list(engine, codex_home.as_deref());
        }
    }
    /// The chosen model and reasoning level for `engine`. A level the model does
    /// not take falls back to the model's default.
    fn ai_choice(&self, engine: Engine) -> (Option<models::Model>, Option<String>) {
        let i = engine.slot();
        let list = &self.ext.ai.models[i];
        let model = list
            .get(self.ext.ai.model_pick[i])
            .or(list.first())
            .cloned();
        let effort = self.ext.ai.effort_pick[i]
            .clone()
            .filter(|e| model.as_ref().is_some_and(|m| m.efforts.contains(e)));
        (model, effort)
    }

    /// Stop the running question now (the 중지 button, closing the window).
    /// Edits made so far stay, as one undo step.
    pub(crate) fn ai_stop(&mut self) {
        if let Some(i) = self.ext.ai.sample_busy.take()
            && let Some(m) = self.ext.ai.messages.get_mut(i)
        {
            m.stopped = true;
        }
        if let Some(mut run) = self.ext.ai.run.take() {
            run.running.stop();
            if let Some(m) = self.ext.ai.messages.get_mut(run.message) {
                m.stopped = true;
            }
            self.ai_finish(run);
        }
    }

    /// Send the input box's question.
    fn ai_send(&mut self, ctx: &egui::Context) {
        let question = self.ext.ai.input.trim().to_owned();
        if question.is_empty() || self.ai_busy() {
            return;
        }
        let engine = self.ext.ai.engine;
        let cli = self
            .ext
            .ai
            .clis
            .as_ref()
            .and_then(|c| c.iter().find(|(e, _)| *e == engine))
            .map(|(_, c)| c.clone());
        let region = if std::mem::take(&mut self.ext.ai.skip_context) {
            Region::Dropped
        } else {
            Context::of(&self.session).map_or(Region::Unknown, Region::Context)
        };
        self.ext.ai.input.clear();
        let chip = match &region {
            Region::Context(c) => Some(c.chip()),
            _ => None,
        };
        self.ext
            .ai
            .messages
            .push(Message::user(question.clone(), chip));
        let refuse = |s: &mut Self, why: String| s.ext.ai.messages.push(Message::failed(why));
        if self.session.is_distribution() {
            return refuse(self, "배포용 문서는 AI가 편집할 수 없습니다.".into());
        }
        let program = match cli {
            Some(Cli {
                program: Some(p),
                unusable: None,
            }) => p,
            Some(Cli {
                unusable: Some(why),
                ..
            }) => return refuse(self, why),
            _ => return refuse(self, format!("{}를 찾지 못했습니다.", engine.name())),
        };
        let before = changes::snapshot(&self.session);
        // A loaded conversation the CLI cannot continue: its turns go as a
        // summary (kept until the question really starts).
        let seed = self.ext.ai.hist.seed.clone();
        let endpoint = match self.ai_turn_begin(ctx) {
            Ok(e) => e,
            Err(e) => return refuse(self, format!("MCP 연결을 준비하지 못했습니다: {e}")),
        };
        let (model, effort) = self.ai_choice(engine);
        let model = model.as_ref();
        let root = work_root();
        let secrets = vec![endpoint.document_id.clone(), endpoint.token.clone()];
        let request = Request {
            engine,
            program,
            prompt: match &seed {
                Some(s) => format!("{s}\n{}", prompt(&region, &question)),
                None => prompt(&region, &question),
            },
            system: SYSTEM.replace("{document_id}", &endpoint.document_id),
            resume: self
                .ext
                .ai
                .conversation
                .as_ref()
                .filter(|(e, _)| *e == engine)
                .map(|(_, id)| id.clone()),
            url: endpoint.url,
            token: endpoint.token,
            work_dir: root.join("run"),
            codex_home: root.join("codex-home"),
            model: model.and_then(|m| m.id.clone()),
            effort,
        };
        match runner::start(&request) {
            Ok(running) => {
                self.ext.ai.hist.seed = None;
                self.ext.ai.messages.push(Message::ai());
                self.ext.ai.run = Some(Run {
                    running,
                    message: self.ext.ai.messages.len() - 1,
                    engine,
                    ended: false,
                    stderr: String::new(),
                    timed_out: false,
                    exited_at: None,
                    before,
                    secrets,
                });
                ctx.request_repaint();
            }
            Err(e) => {
                self.ai_turn_end();
                refuse(self, e.to_string());
            }
        }
    }

    /// Every frame: read the running CLI's output and finish the turn.
    pub(crate) fn ai_panel_tick(&mut self, ctx: &egui::Context) {
        let Some(run) = self.ext.ai.run.as_mut() else {
            return;
        };
        while let Ok(out) = run.running.events.try_recv() {
            match out {
                Output::Event(e) => {
                    if let Some(t) = self
                        .ext
                        .ai
                        .messages
                        .get_mut(run.message)
                        .and_then(|m| m.turn.as_mut())
                    {
                        t.apply(e);
                    }
                }
                Output::Exited { stderr } => {
                    run.ended = true;
                    run.stderr = stderr;
                }
            }
        }
        let limit = self.ext.ai.time_limit.unwrap_or(TIME_LIMIT);
        if !run.ended && run.running.started.elapsed() > limit {
            // Like 중지: stop the tree and close the question now.
            run.timed_out = true;
            if let Some(mut run) = self.ext.ai.run.take() {
                run.running.stop();
                self.ai_finish(run);
            }
            return;
        }
        if !run.ended && run.running.has_exited() {
            let at = *run.exited_at.get_or_insert_with(std::time::Instant::now);
            if at.elapsed() > Duration::from_secs(2) {
                run.ended = true;
            }
        }
        if run.ended {
            if let Some(run) = self.ext.ai.run.take() {
                self.ai_finish(run);
            }
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    /// Close a finished or stopped question: clean up the process, end the
    /// undo group, fill in the bubble, remember the conversation.
    fn ai_finish(&mut self, run: Run) {
        let Run {
            running,
            message,
            engine,
            stderr,
            timed_out,
            before,
            secrets,
            ..
        } = run;
        // The token stops working and the undo group closes first: a helper
        // that outlived the CLI can edit nothing more.
        let changed = self.ai_turn_end();
        let login = running.finish();
        if std::mem::take(&mut self.ext.ai.release_after_run) {
            self.ai_panel_release_connection();
        }
        let Some(m) = self.ext.ai.messages.get_mut(message) else {
            return;
        };
        m.changed = changed;
        let turn = m.turn.clone().unwrap_or_default();
        if let Some(id) = &turn.session {
            self.ext.ai.conversation = Some((engine, id.clone()));
        }
        m.error = if timed_out {
            Some(format!(
                "{}분이 지나 중지했습니다. 그때까지의 편집은 남아 있습니다.",
                self.ext.ai.time_limit.unwrap_or(TIME_LIMIT).as_secs() / 60
            ))
        } else if m.stopped {
            None
        } else if let Some(e) = turn.error {
            Some(e)
        } else if !turn.done {
            let last = cause(&stderr);
            Some(match last {
                Some(l) => format!("응답 없이 끝났습니다: {l}"),
                None => "응답 없이 끝났습니다.".into(),
            })
        } else {
            None
        };
        if let Err(e) = login {
            let note = format!("Codex 로그인 정보를 정리하지 못했습니다: {e}");
            m.error = Some(match m.error.take() {
                Some(prev) => format!("{prev}\n{note}"),
                None => note,
            });
        }
        self.ai_record(message, engine, &before, &secrets);
    }

    /// Start a question on the MCP side: a one-time token for the CLI, file
    /// commands refused, and an undo group (one Ctrl+Z per question).
    fn ai_turn_begin(&mut self, ctx: &egui::Context) -> anyhow::Result<Endpoint> {
        let run = self.ai_run_begin(ctx)?;
        Ok(Endpoint {
            url: run.url,
            token: run.token,
            document_id: run.document_id,
        })
    }
    /// End the question: the token stops working and its edits become one undo
    /// step. Whether the document changed.
    fn ai_turn_end(&mut self) -> bool {
        match self.ai_run_end() {
            Ok(changed) => changed,
            Err(e) => {
                self.message = format!("AI 편집을 마무리하지 못했습니다: {e}");
                // Assume it changed: the undo chip is then shown either way.
                true
            }
        }
    }
}

/// Where the CLI reaches this window's document.
struct Endpoint {
    url: String,
    token: String,
    document_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_question_follows_its_context_block() {
        let mut s = Session::new().unwrap();
        s.execute(Command::Insert("가나다".into())).unwrap();
        let c = Context::of(&s).unwrap();
        let p = prompt(&Region::Context(c), "굵게 해 줘");
        assert!(p.starts_with("[맥락]\n"), "{p}");
        assert!(p.ends_with("\n[요청]\n굵게 해 줘"), "{p}");
        assert_eq!(prompt(&Region::Unknown, "굵게 해 줘"), "굵게 해 줘");
        // After the chip's ✕ the AI is told not to guess "this part" from the caret.
        let dropped = prompt(&Region::Dropped, "이 부분을 지워 줘");
        assert!(dropped.contains("짐작하지 말고"), "{dropped}");
        assert!(
            dropped.ends_with(
                "[요청]
이 부분을 지워 줘"
            ),
            "{dropped}"
        );
    }

    #[test]
    fn a_question_that_cannot_start_is_answered_with_the_reason() {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.ext.ai.clis = Some(vec![(
            Engine::Claude,
            Cli {
                program: None,
                unusable: Some("설치 안 됨".into()),
            },
        )]);
        app.ext.ai.input = "  표 넣어 줘 ".into();
        let ctx = app.ctx.clone();
        app.ai_send(&ctx);
        assert!(!app.ai_busy());
        assert!(app.ext.ai.input.is_empty());
        let m = &app.ext.ai.messages;
        assert_eq!(m.len(), 2);
        assert!(m[0].user && m[0].text == "표 넣어 줘");
        assert!(m[0].context.as_deref().unwrap().starts_with("커서 문단"));
        assert_eq!(m[1].error.as_deref(), Some("설치 안 됨"));
        // The context chip's ✕ drops the region for the next question only.
        app.ext.ai.skip_context = true;
        app.ext.ai.input = "다시".into();
        app.ai_send(&ctx);
        assert!(app.ext.ai.messages[2].context.is_none());
        assert!(!app.ext.ai.skip_context);
    }

    fn frame(app: &mut HwpApp, events: Vec<egui::Event>) {
        let ctx = app.ctx.clone();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
                events,
                ..Default::default()
            },
            |ui| {
                egui::Panel::right("ai-panel-test")
                    .default_size(320.0)
                    .show(ui, |ui| app.ai_panel_ui(ui));
            },
        );
        out.textures_delta.clear();
    }

    #[test]
    fn the_panel_draws_a_conversation_and_sends_on_enter() {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        // An engine that cannot run: Enter does not send (the reason is shown).
        app.ext.ai.clis = Some(vec![(
            Engine::Claude,
            Cli {
                program: Some(PathBuf::from("claude")),
                unusable: Some("설치 안 됨".into()),
            },
        )]);
        let mut done = Turn::default();
        for e in [
            stream::Event::ToolStarted {
                id: "1".into(),
                label: "table.create".into(),
            },
            stream::Event::ToolFinished {
                id: "1".into(),
                error: None,
            },
            stream::Event::ToolStarted {
                id: "2".into(),
                label: "type_text".into(),
            },
            stream::Event::ToolFinished {
                id: "2".into(),
                error: Some("거절".into()),
            },
            stream::Event::Done {
                answer: Some("표를 넣었습니다.".into()),
                error: None,
            },
        ] {
            done.apply(e);
        }
        app.ext.ai.messages = vec![
            Message::user(
                "표 넣어 줘".into(),
                Some("커서 문단: 구역 1 문단 1 · (빈 문단)".into()),
            ),
            Message {
                turn: Some(done),
                expanded: true,
                changed: true,
                ..Message::ai()
            },
            Message::failed("설치 안 됨".into()),
        ];
        frame(&mut app, vec![]);
        frame(&mut app, vec![]);
        // Type a question and press Enter in the input box.
        app.ctx
            .memory_mut(|m| m.request_focus(egui::Id::new("ai-panel-input-box")));
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::Text("굵게 해 줘".into())]);
        assert_eq!(app.ext.ai.input, "굵게 해 줘");
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.ext.ai.input, "굵게 해 줘", "kept: nothing was sent");
        assert_eq!(app.ext.ai.messages.len(), 3);
        // A usable engine: Enter sends. A 배포용 document is refused before the
        // real MCP connection (a port, the settings folder) is touched.
        app.ext.ai.clis = Some(vec![(
            Engine::Claude,
            Cli {
                program: Some(PathBuf::from("claude")),
                unusable: None,
            },
        )]);
        app.session.core.document_mut().header.distribution = true;
        let enter = || {
            vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        frame(&mut app, vec![]);
        frame(&mut app, enter());
        assert!(app.ext.ai.input.is_empty(), "{:?}", app.ext.ai.input);
        let m = &app.ext.ai.messages;
        assert_eq!(m[m.len() - 2].text, "굵게 해 줘");
        assert_eq!(
            m[m.len() - 1].error.as_deref(),
            Some("배포용 문서는 AI가 편집할 수 없습니다.")
        );
        frame(&mut app, vec![]);
    }

    /// Whole body text, one line per paragraph.
    fn body(app: &HwpApp) -> String {
        let lines: Vec<String> = app.session.entries().into_iter().map(|(_, t)| t).collect();
        lines.join(
            "
",
        )
    }

    /// The whole path with a real CLI and model: question → MCP edits on this
    /// window's document → one Ctrl+Z back. Run with HWPCRAFT_CONFIG_DIR set to a
    /// scratch folder (the bridge keeps its token there) and AI_E2E_ENGINE=codex
    /// for Codex:
    /// `cargo test -p hwpcraft-ui-egui --lib -- --ignored real_cli`.
    #[test]
    #[ignore = "runs the local Claude Code / Codex CLI against a real model"]
    fn a_real_cli_question_edits_the_document_and_undoes_in_one_step() {
        assert!(
            std::env::var_os("HWPCRAFT_CONFIG_DIR").is_some(),
            "set HWPCRAFT_CONFIG_DIR to a scratch folder"
        );
        let engine = match std::env::var("AI_E2E_ENGINE").as_deref() {
            Ok("codex") => Engine::Codex,
            _ => Engine::Claude,
        };
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.session
            .execute(Command::Insert(
                "첫 문단은 그대로 둔다
본 사업은 6개월간 수행한다"
                    .into(),
            ))
            .unwrap();
        let before = body(&app);
        let ctx = app.ctx.clone();
        app.ext.ai.clis = Some(detect(true));
        app.ext.ai.engine = engine;
        // The newest model, passed explicitly, at the lightest level.
        app.ai_load_models();
        let (model, _) = app.ai_choice(engine);
        println!("model = {:?}", model.as_ref().and_then(|m| m.id.clone()));
        app.ext.ai.effort_pick[engine.slot()] = Some("low".into());
        app.ext.ai.input = "이 문단 끝에 ' (AI 추가)'라는 글을 덧붙여 줘.".into();
        app.ai_send(&ctx);
        assert!(app.ai_busy(), "{:?}", app.ext.ai.messages.last());
        let started = std::time::Instant::now();
        while app.ai_busy() && started.elapsed() < Duration::from_secs(300) {
            app.serve_mcp_bridge(&ctx);
            app.ai_panel_tick(&ctx);
            std::thread::sleep(Duration::from_millis(30));
        }
        let m = app.ext.ai.messages.last().unwrap().clone();
        println!(
            "tools = {:?}",
            m.turn
                .as_ref()
                .map(|t| t.tools.iter().map(|x| x.1.clone()).collect::<Vec<_>>())
        );
        println!(
            "answer = {:?}",
            m.turn.as_ref().and_then(|t| t.answer.clone())
        );
        let after = body(&app);
        println!(
            "before = {before:?}
after = {after:?}"
        );
        assert!(m.error.is_none(), "{:?}", m.error);
        assert!(m.changed);
        assert!(after.contains("6개월간 수행한다 (AI 추가)"), "{after}");
        assert!(
            after.starts_with(
                "첫 문단은 그대로 둔다
"
            ),
            "{after}"
        );
        // The conversation can continue.
        assert!(
            app.ext
                .ai
                .conversation
                .as_ref()
                .is_some_and(|(e, _)| *e == engine)
        );
        // One Ctrl+Z takes the whole question back.
        app.session.execute(Command::Undo).unwrap();
        assert_eq!(body(&app), before);
        app.ai_panel_release_connection();
        // The panel's Codex home keeps the conversation (with the document text).
        let _ = std::fs::remove_dir_all(work_root());
    }

    #[test]
    fn typing_in_the_panel_does_not_reach_the_document() {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.session.execute(Command::Insert("본문".into())).unwrap();
        app.ext.ai.clis = Some(vec![]);
        let ctx = app.ctx.clone();
        let run = |app: &mut HwpApp, events: Vec<egui::Event>| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::Panel::right("ai-panel-test").show(ui, |ui| app.ai_panel_ui(ui));
                    app.keyboard(ui.ctx());
                },
            );
            out.textures_delta.clear();
        };
        run(&mut app, vec![]);
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("ai-panel-input-box")));
        run(&mut app, vec![]);
        run(
            &mut app,
            vec![
                egui::Event::Text("표".into()),
                egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "넣".into(),
                    active_range_chars: None,
                }),
                egui::Event::Ime(egui::ImeEvent::Commit("넣".into())),
                egui::Event::Key {
                    key: Key::Backspace,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::Text("어".into()),
            ],
        );
        assert_eq!(app.ext.ai.input, "표어");
        assert_eq!(
            app.session.text(),
            "본문",
            "panel typing leaked into the document"
        );
    }

    /// After the chip's ✕, "this part" must not be taken from the caret (found
    /// with Codex). Same setup as `real_cli`.
    #[test]
    #[ignore = "runs the local Claude Code / Codex CLI against a real model"]
    fn a_real_cli_question_without_its_region_does_not_guess_this_part() {
        assert!(std::env::var_os("HWPCRAFT_CONFIG_DIR").is_some());
        let engine = match std::env::var("AI_E2E_ENGINE").as_deref() {
            Ok("codex") => Engine::Codex,
            _ => Engine::Claude,
        };
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.session
            .execute(Command::Insert(
                "첫 문단
둘째 문단
셋째 문단"
                    .into(),
            ))
            .unwrap();
        let before = body(&app);
        let ctx = app.ctx.clone();
        app.ext.ai.clis = Some(detect(true));
        app.ext.ai.engine = engine;
        app.ext.ai.skip_context = true;
        app.ext.ai.input = "이 부분을 지워 줘.".into();
        app.ai_send(&ctx);
        let started = std::time::Instant::now();
        while app.ai_busy() && started.elapsed() < Duration::from_secs(300) {
            app.serve_mcp_bridge(&ctx);
            app.ai_panel_tick(&ctx);
            std::thread::sleep(Duration::from_millis(30));
        }
        let m = app.ext.ai.messages.last().unwrap().clone();
        let t = m.turn.clone().unwrap_or_default();
        println!(
            "tools = {:?}",
            t.tools.iter().map(|x| (&x.1, &x.3)).collect::<Vec<_>>()
        );
        println!("answer = {:?}", t.answer);
        assert!(m.error.is_none(), "{:?}", m.error);
        assert_eq!(body(&app), before, "{:?}", t.answer);
        assert!(!m.changed);
        // No call failed for a document_id the tool does not take.
        assert_eq!(t.failed(), 0, "{:?}", t.tools);
        app.ai_panel_release_connection();
        let _ = std::fs::remove_dir_all(work_root());
    }

    /// 중지 during a real run: the run ends at once, the token is revoked, the
    /// edits so far undo in one step and the run's files are gone.
    #[test]
    #[ignore = "runs the local Claude Code / Codex CLI against a real model"]
    fn a_real_cli_question_stopped_midway_undoes_in_one_step() {
        assert!(std::env::var_os("HWPCRAFT_CONFIG_DIR").is_some());
        let engine = match std::env::var("AI_E2E_ENGINE").as_deref() {
            Ok("codex") => Engine::Codex,
            _ => Engine::Claude,
        };
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.session.execute(Command::Insert("원문".into())).unwrap();
        let before = body(&app);
        let ctx = app.ctx.clone();
        app.ext.ai.clis = Some(detect(true));
        app.ext.ai.engine = engine;
        app.ext.ai.input =
            "문서 끝에 1부터 40까지 숫자를 한 문단에 하나씩, 도구를 한 번에 한 문단씩 불러 차례로 넣어 줘."
                .into();
        app.ai_send(&ctx);
        assert!(app.ai_busy(), "{:?}", app.ext.ai.messages.last());
        let started = std::time::Instant::now();
        // Let it make a few edits, then press 중지.
        while app.ai_busy() && started.elapsed() < Duration::from_secs(120) {
            app.serve_mcp_bridge(&ctx);
            app.ai_panel_tick(&ctx);
            if body(&app).lines().count() >= 3 {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        let stop = std::time::Instant::now();
        app.ai_stop();
        println!(
            "stopped after {:?}, stop took {:?}",
            started.elapsed(),
            stop.elapsed()
        );
        assert!(
            stop.elapsed() < Duration::from_secs(5),
            "stopping held the window"
        );
        assert!(!app.ai_busy());
        let m = app.ext.ai.messages.last().unwrap().clone();
        assert!(m.stopped);
        println!("after stop: {:?}", body(&app));
        // Nothing reaches the document after 중지.
        let after = body(&app);
        for _ in 0..50 {
            app.serve_mcp_bridge(&ctx);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(body(&app), after);
        if m.changed {
            app.session.execute(Command::Undo).unwrap();
        }
        assert_eq!(body(&app), before);
        // The run's own files are gone.
        let run = work_root().join("run");
        let left: Vec<_> = std::fs::read_dir(&run)
            .map(|d| d.flatten().map(|e| e.file_name()).collect())
            .unwrap_or_default();
        assert!(left.is_empty(), "{left:?}");
        assert!(!work_root().join("codex-home").join("auth.json").exists());
        assert!(!work_root().join("codex-home").join("run.lock").exists());
        app.ai_panel_release_connection();
        let _ = std::fs::remove_dir_all(work_root());
    }

    #[test]
    fn a_question_past_the_time_limit_is_stopped() {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        let ctx = app.ctx.clone();
        app.ext.ai.messages.push(Message::ai());
        app.ext.ai.run = Some(Run {
            running: Running::idle_for_test(),
            message: 0,
            engine: Engine::Claude,
            ended: false,
            stderr: String::new(),
            timed_out: false,
            exited_at: None,
            before: Vec::new(),
            secrets: Vec::new(),
        });
        app.ext.ai.time_limit = Some(Duration::ZERO);
        let started = std::time::Instant::now();
        while app.ai_busy() && started.elapsed() < Duration::from_secs(20) {
            app.ai_panel_tick(&ctx);
            std::thread::sleep(Duration::from_millis(20));
        }
        // Stopped well before the stand-in's 60 s, the bubble says why, and the
        // turn was closed (no run left; its undo group ends in `ai_finish`).
        assert!(!app.ai_busy());
        assert!(started.elapsed() < Duration::from_secs(20));
        let m = &app.ext.ai.messages[0];
        assert!(
            m.error.as_deref().unwrap().contains("지나 중지했습니다"),
            "{:?}",
            m.error
        );
    }

    #[test]
    fn without_any_cli_both_engines_are_greyed_out_with_an_install_hint() {
        let clis = detect_with(|_| None, true);
        assert_eq!(clis.len(), 2);
        for (engine, cli) in &clis {
            assert!(cli.program.is_none());
            let why = cli.unusable.as_deref().unwrap();
            assert!(why.contains(engine.name()) && why.contains("설치"), "{why}");
        }
        // The search itself: nothing in no folders, the CLI in a folder holding it.
        assert_eq!(runner::find_cli_in(Engine::Claude, vec![]), None);
        let dir = std::env::temp_dir().join(format!("ai-find-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = if cfg!(windows) { "codex.exe" } else { "codex" };
        std::fs::write(dir.join(name), b"").unwrap();
        assert_eq!(
            runner::find_cli_in(Engine::Codex, vec![dir.clone()]),
            Some(dir.join(name))
        );
        assert_eq!(runner::find_cli_in(Engine::Claude, vec![dir.clone()]), None);
        let _ = std::fs::remove_dir_all(dir);
        // The panel then refuses to send and says why.
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.ext.ai.clis = Some(clis);
        app.ext.ai.input = "표 넣어 줘".into();
        let ctx = app.ctx.clone();
        app.ai_send(&ctx);
        let last = app.ext.ai.messages.last().unwrap();
        assert!(
            last.error
                .as_deref()
                .unwrap()
                .contains("설치되어 있지 않습니다")
        );
        frame(&mut app, vec![]);
    }

    #[test]
    fn the_error_bubble_skips_warnings_in_stderr() {
        let stderr = "real cause: quota exceeded
WARNING: proceeding, even though we could not create PATH aliases
Reading additional input from stdin...
";
        assert_eq!(cause(stderr), Some("real cause: quota exceeded"));
        assert_eq!(cause("WARNING: only a warning"), None);
    }

    #[test]
    fn every_row_fits_the_panel_from_its_narrowest_to_its_widest() {
        let long =
            "본 사업은 계약일로부터 6개월간 수행하며 각 단계별 산출물을 제출한다. ".repeat(6);
        for width in [260.0_f32, 320.0, 560.0] {
            let mut app =
                HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
            app.session.execute(Command::Insert(long.clone())).unwrap();
            app.ext.ai.clis = Some(vec![(
                Engine::Codex,
                Cli {
                    program: None,
                    unusable: Some(format!("길고 긴 사유입니다. {long}")),
                },
            )]);
            app.ext.ai.engine = Engine::Codex;
            let mut done = Turn::default();
            for e in [
                stream::Event::ToolStarted {
                    id: "1".into(),
                    label: format!("hwp_action {}", "MoveDocEnd".repeat(8)),
                },
                stream::Event::ToolFinished {
                    id: "1".into(),
                    error: Some(long.clone()),
                },
                stream::Event::Done {
                    answer: Some(long.clone()),
                    error: None,
                },
            ] {
                done.apply(e);
            }
            app.ext.ai.messages = vec![
                Message::user(
                    long.clone(),
                    Some(format!("커서 문단: 구역 1 문단 1 · '{long}'")),
                ),
                Message {
                    turn: Some(done),
                    expanded: true,
                    changed: true,
                    link: Some(history::Link {
                        changes: (0..8)
                            .map(|i| changes::Change {
                                kind: [
                                    changes::Kind::Modify,
                                    changes::Kind::Insert,
                                    changes::Kind::Delete,
                                ][i % 3],
                                before: Some(long.clone()),
                                after: Some(long.clone()),
                                at_before: None,
                                at_after: None,
                                follows: None,
                                prev: None,
                                next: None,
                                controls: false,
                            })
                            .collect(),
                        tracked: Some("AI · Claude Code".into()),
                        track_note: Some(long.clone()),
                        ..history::Link::default()
                    }),
                    ..Message::ai()
                },
                Message::failed(long.clone()),
            ];
            // The history list open, with a long title and every note.
            let dir =
                std::env::temp_dir().join(format!("hwpcraft-ai-width-{}", std::process::id()));
            app.ext.ai.hist.root = Some(dir.clone());
            app.ai_history_follow();
            let h = &mut app.ext.ai.hist;
            h.open = true;
            h.notice = Some(long.clone());
            h.confirm = Some(history::Doomed::Document);
            h.found = vec![(
                history::Conversation {
                    id: "a".into(),
                    title: long.clone(),
                    engine: "Claude Code".into(),
                    session: None,
                    created: 0,
                    updated: 0,
                    turns: 12,
                    path: None,
                    fingerprint: String::new(),
                },
                history::Relation::Moved,
            )];
            let ctx = app.ctx.clone();
            let mut used = None;
            for _ in 0..3 {
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1400.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        egui::Panel::right("ai-panel-width-test")
                            .exact_size(width)
                            .resizable(false)
                            .show(ui, |ui| {
                                let inner = ui.max_rect();
                                app.ai_panel_ui(ui);
                                used = Some((inner, ui.min_rect()));
                            });
                    },
                );
                out.textures_delta.clear();
            }
            let (inner, used) = used.unwrap();
            assert!(
                used.width() <= inner.width() + 0.5 && used.left() >= inner.left() - 0.5,
                "width {width}: content {used:?} spills out of {inner:?}"
            );
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn the_newest_model_is_chosen_unless_another_one_is() {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.ext.ai.models[0] = models::list(Engine::Claude, None);
        let (m, e) = app.ai_choice(Engine::Claude);
        assert_eq!(m.unwrap().id.as_deref(), Some("fable"));
        assert_eq!(e, None, "no level chosen: the model's own default");
        app.ext.ai.model_pick[0] = 2;
        app.ext.ai.effort_pick[0] = Some("high".into());
        let (m, e) = app.ai_choice(Engine::Claude);
        assert_eq!(
            (m.unwrap().id.as_deref(), e.as_deref()),
            (Some("sonnet"), Some("high"))
        );
        // A level the model does not take is dropped; an index past the list
        // falls back to the newest model.
        app.ext.ai.effort_pick[0] = Some("ultra".into());
        app.ext.ai.model_pick[0] = 99;
        let (m, e) = app.ai_choice(Engine::Claude);
        assert_eq!((m.unwrap().id.as_deref(), e), (Some("fable"), None));
        // Codex with no model list: no model is passed, the CLI decides.
        let (m, e) = app.ai_choice(Engine::Codex);
        assert!(m.is_none() && e.is_none());
    }

    #[test]
    fn the_clis_are_found_on_a_thread_without_holding_the_frame() {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        let ctx = app.ctx.clone();
        app.ai_detect(&ctx);
        // Until the thread answers, sending is held with a reason.
        assert!(app.ext.ai.clis.is_none() && app.ai_unusable().is_some());
        let started = std::time::Instant::now();
        while app.ext.ai.clis.is_none() && started.elapsed() < Duration::from_secs(30) {
            app.ai_detect(&ctx);
            std::thread::sleep(Duration::from_millis(20));
        }
        let clis = app.ext.ai.clis.clone().expect("detection finished");
        assert_eq!(clis.len(), 2);
        assert!(app.ext.ai.detecting.is_none());
        // Every engine is either usable or says why not.
        for (_, cli) in clis {
            assert!(cli.unusable.is_some() || cli.program.is_some());
        }
    }

    /// A Markdown answer with long list lines must not push the panel's
    /// contents past its width: the dock then stops reserving its space and the
    /// document covers the panel until the conversation is cleared (seen with a
    /// real Claude Code answer, 0.21).
    #[test]
    fn a_long_markdown_answer_keeps_the_dock_beside_the_document() {
        let line = "기존 문장은 그대로 두고, 행정은 도입 전에 검증하고 기업은 공공 실증 실적을 확보한다는 설명을 한 줄 더 넣었습니다";
        let answer = format!(
            "첫 번째 표의 **빈칸**을 모두 채웠습니다.\n\n- **개념**: {line}\n- **범위**: {line}\n1. `table.cell_text` {line}\n## {line}"
        );
        let mut t = Turn::default();
        t.apply(stream::Event::Done {
            answer: Some(answer),
            error: None,
        });
        let document_right = |messages: Vec<Message>| {
            let mut app =
                HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
            app.ext.shell.settings.ai_panel_open = true;
            app.ext.shell.settings.ai_panel_width = 560;
            app.ext.ai.clis = Some(vec![]);
            app.ext.ai.messages = messages;
            let ctx = app.ctx.clone();
            let mut left = Rect::NOTHING;
            for _ in 0..4 {
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1916.0, 1040.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        app.toolbar(ui);
                        app.status_bar(ui);
                        left = ui.available_rect_before_wrap();
                    },
                );
                out.textures_delta.clear();
            }
            left.right()
        };
        let empty = document_right(Vec::new());
        assert!(empty < 1916.0 - 500.0, "the dock takes its width: {empty}");
        let answered = document_right(vec![Message {
            turn: Some(t),
            ..Message::ai()
        }]);
        assert_eq!(answered, empty, "the answer pushed the dock out");
    }

    #[test]
    fn the_busy_flag_for_lock_tests() {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        assert!(!app.ai_busy());
        app.ai_set_busy_for_test(true);
        assert!(app.ai_busy());
        assert!(!app.ai_panel_take_close());
        app.ext.ai.close_requested = true;
        assert!(app.ai_panel_take_close() && !app.ai_panel_take_close());
    }
}
