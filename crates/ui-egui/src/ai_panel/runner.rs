//! Finds the local Claude Code / Codex CLI and runs one question as a child
//! process: prompt on stdin, JSON lines read on a thread and handed to the UI
//! through a channel, stop = kill the whole process tree.
//!
//! Isolation: Claude runs without user settings, hooks,
//! plugins or built-in tools and sees only the hwpcraft MCP server. Codex runs
//! with its own CODEX_HOME that holds a minimal config and, for the length of
//! one question, a copy of the user's login (`CodexLogin`).
use super::stream::{Engine, Event, parse_line};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Instant, SystemTime};

/// Name of the MCP server the CLIs see; Claude's tool names start with
/// `mcp__hwpcraft__`.
pub const SERVER: &str = "hwpcraft";
/// Environment variable carrying the bearer token to Codex.
pub const TOKEN_ENV: &str = "HWPCRAFT_MCP_TOKEN";

impl Engine {
    fn program(self) -> &'static str {
        match self {
            Engine::Claude => "claude",
            Engine::Codex => "codex",
        }
    }
    /// Places the installers put the CLI when it is not on PATH.
    fn known_dirs(self) -> Vec<PathBuf> {
        let home =
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        let mut dirs = Vec::new();
        if let Some(h) = &home {
            dirs.push(h.join(".local").join("bin"));
            if self == Engine::Claude {
                dirs.push(h.join(".claude").join("local"));
            }
            dirs.push(h.join(".npm-global").join("bin"));
        }
        if cfg!(windows) {
            if let Some(a) = std::env::var_os("APPDATA") {
                dirs.push(PathBuf::from(a).join("npm"));
            }
            if self == Engine::Codex
                && let Some(l) = std::env::var_os("LOCALAPPDATA")
            {
                dirs.push(PathBuf::from(l).join("Programs/OpenAI/Codex/bin"));
            }
        } else {
            dirs.push(PathBuf::from("/opt/homebrew/bin"));
            dirs.push(PathBuf::from("/usr/local/bin"));
        }
        dirs
    }
}

/// The CLI's executable: PATH first, then the installers' usual places.
pub fn find_cli(engine: Engine) -> Option<PathBuf> {
    let path_dirs = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    find_cli_in(
        engine,
        path_dirs.into_iter().chain(engine.known_dirs()).collect(),
    )
}

/// The CLI's executable in the first of `dirs` that has it.
pub fn find_cli_in(engine: Engine, dirs: Vec<PathBuf>) -> Option<PathBuf> {
    let exts: Vec<String> = if cfg!(windows) {
        // A bare name with no extension is not runnable on Windows.
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|e| e.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    dirs.into_iter()
        .flat_map(|d| {
            exts.iter()
                .map(move |e| d.join(format!("{}{e}", engine.program())))
        })
        .find(|p| p.is_file())
}

/// One question to run.
#[derive(Clone, Debug)]
pub struct Request {
    pub engine: Engine,
    pub program: PathBuf,
    /// The user's question with the context block in front.
    pub prompt: String,
    /// Standing instructions (PLAN §8.5).
    pub system: String,
    /// Conversation to continue.
    pub resume: Option<String>,
    /// MCP endpoint of this window, e.g. `http://127.0.0.1:7083/mcp`.
    pub url: String,
    pub token: String,
    /// Working folder for the run (never the document's folder).
    pub work_dir: PathBuf,
    /// Codex only: the panel's own CODEX_HOME.
    pub codex_home: PathBuf,
    /// Model (Claude alias or Codex slug); `None` leaves the CLI's default.
    pub model: Option<String>,
    /// Reasoning effort; `None` leaves the model's default.
    pub effort: Option<String>,
}

/// The program arguments, without the prompt (sent on stdin). Claude reads
/// its MCP config and the standing instructions from private files, so neither
/// the token nor the document id is on the command line.
pub fn arguments(r: &Request, mcp_config: &Path, system_file: &Path) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    let mut push = |s: &str| a.push(s.to_owned());
    match r.engine {
        Engine::Claude => {
            for s in ["-p", "--output-format", "stream-json", "--verbose"] {
                push(s);
            }
            push("--mcp-config");
            push(&mcp_config.display().to_string());
            push("--strict-mcp-config");
            // No user/project settings: no hooks, plugins, CLAUDE.md.
            push("--setting-sources");
            push("");
            // No built-in tools (shell, file edits, web): only MCP.
            push("--tools");
            push("");
            push(&format!("--allowedTools=mcp__{SERVER}"));
            if let Some(m) = &r.model {
                push("--model");
                push(m);
            }
            if let Some(e) = &r.effort {
                push("--effort");
                push(e);
            }
            push("--append-system-prompt-file");
            push(&system_file.display().to_string());
            if let Some(id) = &r.resume {
                push("--resume");
                push(id);
            }
        }
        Engine::Codex => {
            push("exec");
            if let Some(id) = &r.resume {
                push("resume");
                push(id);
            }
            for s in ["--json", "--skip-git-repo-check"] {
                push(s);
            }
            if r.resume.is_none() {
                push("-s");
                push("read-only");
                push("-C");
                push(&r.work_dir.display().to_string());
            }
            if let Some(m) = &r.model {
                push("-m");
                push(m);
            }
            if let Some(e) = &r.effort {
                push("-c");
                push(&format!("model_reasoning_effort={e:?}"));
            }
            // Prompt from stdin.
            push("-");
        }
    }
    a
}

/// Claude's `--mcp-config` file content. It holds the token, so it is written
/// to the run folder and removed when the run ends.
pub fn claude_mcp_config(url: &str, token: &str) -> String {
    serde_json::json!({"mcpServers": {SERVER: {
        "type": "http", "url": url,
        "headers": {"Authorization": format!("Bearer {token}")}}}})
    .to_string()
}

/// The panel's CODEX_HOME `config.toml`: only this window's MCP server, its
/// tools approved (Codex refuses MCP writes under its default `never` policy).
pub fn codex_config(url: &str) -> String {
    let features = CODEX_FEATURES_OFF
        .iter()
        .map(|f| format!("{f} = false\n"))
        .collect::<String>();
    format!(
        "web_search = \"disabled\"\n\n[features]\n{features}\n[mcp_servers.{SERVER}]\nurl = {url:?}\nbearer_token_env_var = {TOKEN_ENV:?}\ndefault_tools_approval_mode = \"approve\"\n"
    )
}

/// Codex features the panel turns off (codex-cli 0.166, checked 2026-11-01).
/// With `shell_tool` off Codex has no shell; its code mode stays on because MCP
/// calls run through it, and code mode cannot read files (no `require`, `Deno`
/// or `import`). `tool_search_always_defer_mcp_tools` off loads the hwpcraft
/// tools up front. The rest are routes out of the document: account apps,
/// browsers, computer use, plugins, hooks, image generation, sub-agents.
pub const CODEX_FEATURES_OFF: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "unified_exec_tty",
    "apps",
    "browser_use",
    "browser_use_external",
    "in_app_browser",
    "computer_use",
    "plugins",
    "remote_plugin",
    "plugin_discovery_prefetch",
    "hooks",
    "image_generation",
    "multi_agent",
    "collaboration_modes",
    "skill_mcp_dependency_install",
    "tool_search_always_defer_mcp_tools",
];

/// Codex may be used only when the panel's settings really turn its shell off:
/// a newer Codex could ignore the keys. Writes the panel's config (the URL is
/// replaced at each run) and reads `codex features list` (no login needed).
pub fn codex_shell_off(program: &Path, codex_home: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(codex_home)?;
    let config = codex_home.join("config.toml");
    if !config.exists() {
        write_private(
            &config,
            codex_config("http://127.0.0.1:7083/mcp").as_bytes(),
        )?;
    }
    let mut cmd = Command::new(program);
    cmd.args(["features", "list"])
        .env("CODEX_HOME", codex_home)
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.stdout(Stdio::piped()).spawn()?;
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout, &mut text);
        text
    });
    // A Codex waiting on an update or the network must not hold the panel.
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    while child.try_wait()?.is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "Codex가 10초 안에 응답하지 않아 쓰지 않습니다. 터미널에서 codex를 한 번 실행해 보세요."
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    shell_off_in(&reader.join().unwrap_or_default())
}

/// `shell_tool` must be listed and `false` in `codex features list`.
fn shell_off_in(features: &str) -> anyhow::Result<()> {
    let shell = features
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .find(|w| w.first() == Some(&"shell_tool"))
        .and_then(|w| w.last().map(|v| *v == "false"));
    match shell {
        Some(true) => Ok(()),
        _ => anyhow::bail!(
            "이 Codex 판에서는 셸을 끌 수 없어 AI 편집에 쓰지 않습니다. Claude Code를 쓰세요."
        ),
    }
}

/// What Codex is told to do, since `exec` has no system-prompt flag.
fn codex_prompt(r: &Request) -> String {
    if r.resume.is_some() {
        r.prompt.clone()
    } else {
        format!("{}\n\n{}", r.system, r.prompt)
    }
}

/// A copy of the user's Codex login that lives only while one question runs.
/// A token Codex refreshed meanwhile is written back to the user's file only
/// if that file did not change in between (approved by the user 2026-11-01).
pub struct CodexLogin {
    original: PathBuf,
    copy: PathBuf,
    original_bytes: Vec<u8>,
    original_time: Option<SystemTime>,
    copied_bytes: Vec<u8>,
    done: bool,
}

fn modified(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl CodexLogin {
    /// The user's Codex home: CODEX_HOME, else `~/.codex`.
    pub fn user_home() -> Option<PathBuf> {
        std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .map(|h| PathBuf::from(h).join(".codex"))
            })
    }
    /// Copy `<user_home>/auth.json` into `panel_home`.
    pub fn copy(user_home: &Path, panel_home: &Path) -> anyhow::Result<Self> {
        let original = user_home.join("auth.json");
        let original_bytes = std::fs::read(&original).map_err(|e| {
            anyhow::anyhow!(
                "Codex 로그인 정보를 찾지 못했습니다({e}). 터미널에서 `codex login`을 먼저 하세요."
            )
        })?;
        std::fs::create_dir_all(panel_home)?;
        let copy = panel_home.join("auth.json");
        write_private(&copy, &original_bytes)?;
        Ok(Self {
            original_time: modified(&original),
            original,
            copied_bytes: original_bytes.clone(),
            original_bytes,
            copy,
            done: false,
        })
    }
    /// Write a refreshed login back when safe, then delete the copy.
    /// Returns whether the user's file was updated.
    pub fn finish(mut self) -> anyhow::Result<bool> {
        self.done = true;
        let result = self.write_back();
        let _ = std::fs::remove_file(&self.copy);
        result
    }
    fn write_back(&self) -> anyhow::Result<bool> {
        let Ok(now) = std::fs::read(&self.copy) else {
            return Ok(false);
        };
        // A copy killed while Codex wrote it may be cut short: only a whole
        // JSON login goes back.
        if now == self.copied_bytes || serde_json::from_slice::<serde_json::Value>(&now).is_err() {
            return Ok(false);
        }
        let untouched = std::fs::read(&self.original).ok().as_ref() == Some(&self.original_bytes)
            && modified(&self.original) == self.original_time;
        if !untouched {
            // Both changed: keep the user's file.
            return Ok(false);
        }
        let tmp = self.original.with_extension("json.hwpcraft-tmp");
        write_private(&tmp, &now)?;
        std::fs::rename(&tmp, &self.original).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })?;
        Ok(true)
    }
}

impl Drop for CodexLogin {
    fn drop(&mut self) {
        if !self.done {
            let _ = std::fs::remove_file(&self.copy);
        }
    }
}

/// Write a file only the current user can read (Unix 0600; on Windows the
/// per-user profile folder's ACL already limits it to the user).
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)?.write_all(bytes)
}

/// Lines the run sends to the UI.
#[derive(Debug)]
pub enum Output {
    Event(Event),
    /// The process ended; `stderr` keeps its last lines for error messages.
    Exited {
        stderr: String,
    },
}

/// A running question.
pub struct Running {
    child: Child,
    pub events: Receiver<Output>,
    pub started: Instant,
    /// Files of this run only (MCP config, instructions, Codex lock); removed
    /// when the run ends.
    files: Vec<PathBuf>,
    login: Option<CodexLogin>,
    waiter: Option<std::thread::JoinHandle<()>>,
}

/// A name part unique to one run of this process, so two HwpCraft windows
/// (separate processes) never share a run's files.
fn run_tag() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{}-{n}", std::process::id())
}

/// A lock older than this belongs to a run that did not clean up.
const STALE_LOCK: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// The panel's Codex home holds one login copy and one config at a time: a
/// second window waits for the first one's question to end.
fn lock_codex_home(home: &Path) -> anyhow::Result<PathBuf> {
    let lock = home.join("run.lock");
    let stale = std::fs::metadata(&lock)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age > STALE_LOCK);
    if stale {
        let _ = std::fs::remove_file(&lock);
    }
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
    {
        Ok(mut f) => {
            let _ = write!(f, "{}", std::process::id());
            Ok(lock)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => anyhow::bail!(
            "다른 HwpCraft 창에서 Codex가 작업 중입니다. 그 질문이 끝난 뒤 다시 보내세요."
        ),
        Err(e) => Err(e.into()),
    }
}

/// Files removed when dropped (a run that fails to start).
struct TempFiles(Vec<PathBuf>);

impl Drop for TempFiles {
    fn drop(&mut self) {
        for f in &self.0 {
            let _ = std::fs::remove_file(f);
        }
    }
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Start a question. The prompt goes in on stdin, which is then closed.
pub fn start(r: &Request) -> anyhow::Result<Running> {
    std::fs::create_dir_all(&r.work_dir)?;
    // Removed when this function fails, or by `Running` when the run ends.
    let mut files = TempFiles(Vec::new());
    let mut login = None;
    let mut cmd = Command::new(&r.program);
    match r.engine {
        Engine::Claude => {
            let tag = run_tag();
            let config = r.work_dir.join(format!("hwpcraft-mcp-{tag}.json"));
            let system = r.work_dir.join(format!("instructions-{tag}.txt"));
            files.0.push(config.clone());
            files.0.push(system.clone());
            write_private(&config, claude_mcp_config(&r.url, &r.token).as_bytes())?;
            write_private(&system, r.system.as_bytes())?;
            cmd.args(arguments(r, &config, &system));
        }
        Engine::Codex => {
            std::fs::create_dir_all(&r.codex_home)?;
            files.0.push(lock_codex_home(&r.codex_home)?);
            write_private(
                &r.codex_home.join("config.toml"),
                codex_config(&r.url).as_bytes(),
            )?;
            let user = CodexLogin::user_home()
                .ok_or_else(|| anyhow::anyhow!("Codex 설정 폴더를 찾지 못했습니다."))?;
            login = Some(CodexLogin::copy(&user, &r.codex_home)?);
            cmd.args(arguments(r, Path::new(""), Path::new("")))
                .env("CODEX_HOME", &r.codex_home)
                .env(TOKEN_ENV, &r.token);
        }
    }
    cmd.current_dir(&r.work_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    // Its own process group, so stopping it also stops what it started.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("{}을(를) 실행하지 못했습니다: {e}", r.program.display()))?;
    let prompt = match r.engine {
        Engine::Claude => r.prompt.clone(),
        Engine::Codex => codex_prompt(r),
    };
    let stdin = child.stdin.take();
    let running = Running::attach(child, r.engine, std::mem::take(&mut files.0), login);
    // Written on its own thread: a long question must not wait on the CLI
    // reading it. Dropping stdin afterwards closes it.
    if let Some(mut stdin) = stdin {
        std::thread::spawn(move || {
            let _ = stdin.write_all(prompt.as_bytes());
        });
    }
    Ok(running)
}

impl Running {
    /// Read a started CLI's output on threads; `child` has piped stdout/stderr.
    fn attach(
        mut child: Child,
        engine: Engine,
        files: Vec<PathBuf>,
        login: Option<CodexLogin>,
    ) -> Self {
        let (tx, rx) = channel();
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let err_lines = std::thread::spawn(move || {
            let mut tail: std::collections::VecDeque<String> = Default::default();
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if tail.len() == 20 {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
            tail.into_iter().collect::<Vec<_>>().join("\n")
        });
        let out_tx: Sender<Output> = tx;
        let waiter = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                for e in parse_line(engine, &line) {
                    if out_tx.send(Output::Event(e)).is_err() {
                        return;
                    }
                }
            }
            let stderr = err_lines.join().unwrap_or_default();
            let _ = out_tx.send(Output::Exited { stderr });
        });
        Running {
            child,
            events: rx,
            started: Instant::now(),
            files,
            login,
            waiter: Some(waiter),
        }
    }

    /// A stand-in CLI for tests: a process that prints nothing and waits.
    #[cfg(test)]
    pub fn idle_for_test() -> Self {
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 60"]);
            c
        } else {
            let mut c = Command::new("sleep");
            c.arg("60");
            c
        };
        let child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("a sleeping process");
        Self::attach(child, Engine::Claude, Vec::new(), None)
    }

    /// Kill the CLI and everything it started (MCP helpers, shells).
    pub fn stop(&mut self) {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill")
                .args(["/T", "/F", "/PID", &self.child.id().to_string()])
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(unix)]
        {
            // The whole process group (`process_group(0)` at start).
            let _ = Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", self.child.id())])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = self.child.kill();
    }
    /// The CLI process itself has ended (its helpers may still hold stdout).
    pub fn has_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
    /// Clean up after the process ended (or was stopped): remove the token
    /// file, hand back a refreshed Codex login, delete the login copy.
    /// Returns whether the user's Codex login file was updated.
    pub fn finish(mut self) -> anyhow::Result<bool> {
        let _ = self.child.wait();
        // The reader ends when stdout closes. A helper that outlived the CLI
        // may keep it open: wait briefly, then leave the reader to end alone
        // rather than hold the window.
        if let Some(w) = self.waiter.take() {
            let until = Instant::now() + std::time::Duration::from_secs(1);
            while !w.is_finished() && Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if w.is_finished() {
                let _ = w.join();
            }
        }
        for f in std::mem::take(&mut self.files) {
            let _ = std::fs::remove_file(f);
        }
        match self.login.take() {
            Some(l) => l.finish(),
            None => Ok(false),
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if self.waiter.is_some() {
            self.stop();
        }
        for f in &self.files {
            let _ = std::fs::remove_file(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(engine: Engine, resume: Option<&str>) -> Request {
        Request {
            engine,
            program: PathBuf::from("x"),
            prompt: "질문".into(),
            system: "안내".into(),
            resume: resume.map(Into::into),
            url: "http://127.0.0.1:7083/mcp".into(),
            token: "secret".into(),
            work_dir: PathBuf::from("w"),
            codex_home: PathBuf::from("h"),
            model: None,
            effort: None,
        }
    }

    #[test]
    fn claude_runs_with_only_the_hwpcraft_server_and_no_built_in_tools() {
        let a = arguments(
            &request(Engine::Claude, Some("s1")),
            Path::new("m.json"),
            Path::new("i.txt"),
        );
        let at = |flag: &str| a.iter().position(|x| x == flag).map(|i| a[i + 1].as_str());
        assert_eq!(at("--setting-sources"), Some(""));
        assert_eq!(at("--tools"), Some(""));
        assert_eq!(at("--mcp-config"), Some("m.json"));
        assert_eq!(at("--resume"), Some("s1"));
        // The instructions (with the document id) come from a private file.
        assert_eq!(at("--append-system-prompt-file"), Some("i.txt"));
        assert!(!a.iter().any(|x| x.contains("안내")));
        assert!(a.contains(&"--strict-mcp-config".to_owned()));
        assert!(a.contains(&"--allowedTools=mcp__hwpcraft".to_owned()));
        assert!(
            !a.iter().any(|x| x.contains("secret")),
            "token must not be on the command line"
        );
        assert!(
            !a.contains(&"--bare".to_owned()),
            "--bare skips the user's login"
        );
    }

    #[test]
    fn codex_resumes_by_thread_and_reads_the_prompt_from_stdin() {
        let first = arguments(&request(Engine::Codex, None), Path::new(""), Path::new(""));
        assert_eq!(first[0], "exec");
        assert!(first.windows(2).any(|w| w == ["-s", "read-only"]));
        assert_eq!(first.last().map(String::as_str), Some("-"));
        let next = arguments(
            &request(Engine::Codex, Some("t1")),
            Path::new(""),
            Path::new(""),
        );
        assert_eq!(&next[..3], ["exec", "resume", "t1"]);
        assert!(!next.iter().any(|x| x.contains("secret")));
        // The standing instructions go in front of the first question only.
        assert!(codex_prompt(&request(Engine::Codex, None)).starts_with("안내"));
        assert_eq!(codex_prompt(&request(Engine::Codex, Some("t1"))), "질문");
    }

    #[test]
    fn the_chosen_model_and_effort_go_on_the_command_line() {
        let mut r = request(Engine::Claude, None);
        let none = arguments(&r, Path::new("m.json"), Path::new("i.txt"));
        assert!(!none.iter().any(|x| x == "--model" || x == "--effort"));
        r.model = Some("fable".into());
        r.effort = Some("high".into());
        let a = arguments(&r, Path::new("m.json"), Path::new("i.txt"));
        assert!(a.windows(2).any(|w| w == ["--model", "fable"]));
        assert!(a.windows(2).any(|w| w == ["--effort", "high"]));
        let mut r = request(Engine::Codex, Some("t1"));
        r.model = Some("gpt-6.1-sol".into());
        r.effort = Some("xhigh".into());
        let a = arguments(&r, Path::new(""), Path::new(""));
        assert!(a.windows(2).any(|w| w == ["-m", "gpt-6.1-sol"]));
        assert!(
            a.windows(2)
                .any(|w| w == ["-c", "model_reasoning_effort=\"xhigh\""])
        );
        // The prompt still comes last, from stdin.
        assert_eq!(a.last().map(String::as_str), Some("-"));
    }

    #[test]
    fn codex_config_names_only_this_server_and_approves_its_tools() {
        let c = codex_config("http://127.0.0.1:7084/mcp");
        assert!(c.contains("[mcp_servers.hwpcraft]"));
        assert!(c.contains("url = \"http://127.0.0.1:7084/mcp\""));
        assert!(c.contains("bearer_token_env_var = \"HWPCRAFT_MCP_TOKEN\""));
        assert!(c.contains("default_tools_approval_mode = \"approve\""));
        assert_eq!(c.matches("[mcp_servers").count(), 1);
        // No shell, no apps/browser/computer use; MCP tools loaded up front.
        for f in ["shell_tool", "computer_use", "apps", "plugins", "hooks"] {
            assert!(c.contains(&format!("{f} = false\n")), "{f}");
        }
        assert!(c.contains("tool_search_always_defer_mcp_tools = false"));
        assert!(!c.contains("code_mode"), "MCP calls need code mode");
        assert!(c.contains("web_search = \"disabled\""));
        let j: serde_json::Value = serde_json::from_str(&claude_mcp_config("u", "t")).unwrap();
        assert_eq!(
            j["mcpServers"]["hwpcraft"]["headers"]["Authorization"],
            "Bearer t"
        );
    }

    #[test]
    fn codex_is_used_only_with_its_shell_listed_off() {
        let listed = "apps                stable  false
shell_tool          stable  false
";
        assert!(shell_off_in(listed).is_ok());
        assert!(
            shell_off_in(
                "shell_tool stable true
"
            )
            .is_err()
        );
        // Not listed at all (renamed in a newer Codex): not trusted.
        assert!(
            shell_off_in(
                "apps stable false
"
            )
            .is_err()
        );
    }

    fn dirs() -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "ai-login-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let (user, panel) = (root.join("user"), root.join("panel"));
        std::fs::create_dir_all(&user).unwrap();
        (root, user, panel)
    }

    #[test]
    fn a_refreshed_login_goes_back_only_when_the_users_file_is_unchanged() {
        let (root, user, panel) = dirs();
        std::fs::write(user.join("auth.json"), br#"{"t":"old"}"#).unwrap();
        // Unchanged copy: nothing written back, copy deleted.
        let l = CodexLogin::copy(&user, &panel).unwrap();
        assert_eq!(
            std::fs::read(panel.join("auth.json")).unwrap(),
            br#"{"t":"old"}"#
        );
        assert!(!l.finish().unwrap());
        assert!(!panel.join("auth.json").exists());
        // Codex refreshed the token: it goes back to the user's file.
        let l = CodexLogin::copy(&user, &panel).unwrap();
        std::fs::write(panel.join("auth.json"), br#"{"t":"new"}"#).unwrap();
        assert!(l.finish().unwrap());
        assert_eq!(
            std::fs::read(user.join("auth.json")).unwrap(),
            br#"{"t":"new"}"#
        );
        // A copy cut short while Codex wrote it does not go back.
        let l = CodexLogin::copy(&user, &panel).unwrap();
        std::fs::write(panel.join("auth.json"), br#"{"t":"ne"#).unwrap();
        assert!(!l.finish().unwrap());
        assert_eq!(
            std::fs::read(user.join("auth.json")).unwrap(),
            br#"{"t":"new"}"#
        );
        assert!(!panel.join("auth.json").exists());
        // Both changed: the user's file wins.
        let l = CodexLogin::copy(&user, &panel).unwrap();
        std::fs::write(panel.join("auth.json"), br#"{"t":"mine"}"#).unwrap();
        std::fs::write(user.join("auth.json"), br#"{"t":"theirs"}"#).unwrap();
        assert!(!l.finish().unwrap());
        assert_eq!(
            std::fs::read(user.join("auth.json")).unwrap(),
            br#"{"t":"theirs"}"#
        );
        // Dropped without finish (crash path): the copy is still removed.
        drop(CodexLogin::copy(&user, &panel).unwrap());
        assert!(!panel.join("auth.json").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_second_window_waits_for_the_codex_home() {
        let (root, _, panel) = dirs();
        std::fs::create_dir_all(&panel).unwrap();
        let lock = lock_codex_home(&panel).unwrap();
        let e = lock_codex_home(&panel).err().unwrap().to_string();
        assert!(e.contains("다른 HwpCraft 창"), "{e}");
        std::fs::remove_file(&lock).unwrap();
        assert!(lock_codex_home(&panel).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    /// Unix: 중지 stops the CLI's whole process group, so a helper it started
    /// (here a background `sleep` that also holds stdout open) goes too, and
    /// cleanup does not wait for it.
    #[cfg(unix)]
    #[test]
    fn stopping_kills_what_the_cli_started_too() {
        use std::os::unix::process::CommandExt;
        use std::time::Duration;
        let dir = std::env::temp_dir().join(format!("ai-pgid-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid_file = dir.join("child.pid");
        let script = format!("sleep 60 & echo $! > '{}'; sleep 60", pid_file.display());
        let child = Command::new("sh")
            .args(["-c", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        let mut run = Running::attach(child, Engine::Claude, Vec::new(), None);
        let started = Instant::now();
        let helper = loop {
            if let Ok(t) = std::fs::read_to_string(&pid_file)
                && !t.trim().is_empty()
            {
                break t.trim().to_owned();
            }
            assert!(started.elapsed() < Duration::from_secs(10), "no helper pid");
            std::thread::sleep(Duration::from_millis(20));
        };
        let alive = |pid: &str| {
            Command::new("kill")
                .args(["-0", pid])
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        assert!(alive(&helper));
        run.stop();
        let stopping = Instant::now();
        run.finish().unwrap();
        assert!(
            stopping.elapsed() < Duration::from_secs(3),
            "cleanup waited on the helper"
        );
        let gone = Instant::now();
        while alive(&helper) && gone.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!alive(&helper), "the helper outlived 중지");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn run_files_are_unique_per_run() {
        assert_ne!(run_tag(), run_tag());
    }

    #[test]
    fn no_login_gives_a_codex_login_hint() {
        let (root, user, panel) = dirs();
        let e = CodexLogin::copy(&user, &panel).err().unwrap().to_string();
        assert!(e.contains("codex login"), "{e}");
        let _ = std::fs::remove_dir_all(root);
    }
}
