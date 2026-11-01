//! The AI edit panel's conversation history, per document, in the user's
//! settings folder (`<settings>/ai-history/`), never in the document.
//!
//! Each conversation is a JSON Lines file of its turns, one line appended per
//! question, and `index.json` lists them with the document they belong to: its
//! path and a fingerprint of its text. A document is matched by path first; a
//! moved or renamed copy is offered by fingerprint. Files are written for the
//! current user only (Unix 0600; on Windows the per-user profile's ACL), hold no
//! token or credential, and the history can be switched off or deleted per
//! document or all at once, CLI-side copies included.
use super::changes::Change;
use super::*;
use serde::{Deserialize, Serialize};

/// One tool call of a turn, as the CLI made it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ToolRecord {
    pub tool: String,
    pub args: serde_json::Value,
    pub error: Option<String>,
}

/// One question and its answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct TurnRecord {
    /// Unix seconds.
    pub at: u64,
    pub engine: String,
    /// The CLI's conversation id after this turn.
    pub session: Option<String>,
    pub question: String,
    /// The context chip it was sent with.
    pub context: Option<String>,
    pub answer: Option<String>,
    pub tools: Vec<ToolRecord>,
    pub changed: bool,
    pub stopped: bool,
    pub error: Option<String>,
    /// Text fingerprints of the document before and after.
    pub before: String,
    pub after: String,
    pub changes: Vec<Change>,
    /// Every call only read or typed text (it can be taken back as text).
    pub text_only: bool,
    /// Recorded in the document as tracked changes (author).
    pub tracked: Option<String>,
    /// Taken back from the panel since.
    #[serde(default)]
    pub reverted: bool,
}

/// One conversation in the index.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Conversation {
    pub id: String,
    pub title: String,
    pub engine: String,
    pub session: Option<String>,
    pub created: u64,
    pub updated: u64,
    pub turns: usize,
    /// The document's path when last used (none while it was never saved).
    pub path: Option<String>,
    /// The document's text fingerprint after the last turn.
    pub fingerprint: String,
}

/// How a stored conversation relates to the open document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Relation {
    /// Same file, unchanged since the last turn.
    Same,
    /// Same file, changed since (by hand or in another program).
    Changed,
    /// Another path but the same text: the file was moved, renamed or copied.
    Moved,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Settings {
    /// History off: nothing is written.
    off: bool,
    /// Record text-only questions in the document as tracked changes.
    #[serde(default)]
    track: bool,
}

pub(crate) struct Store {
    root: PathBuf,
}

pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `path` as the index keeps it.
pub(crate) fn key(path: &Path) -> String {
    let p = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    let s = p.to_string_lossy().trim_start_matches(r"\\?\").to_owned();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// The store's folder, made on its first write (user only on Unix).
fn private_dir(path: &Path) -> std::io::Result<()> {
    let Some(dir) = path.parent() else {
        return Ok(());
    };
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir)
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    private_dir(path)?;
    let tmp = path.with_extension("hwpcraft-tmp");
    {
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        let mut f = o.open(&tmp)?;
        std::io::Write::write_all(&mut f, bytes)?;
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn append_private(path: &Path, line: &str) -> std::io::Result<()> {
    private_dir(path)?;
    let mut o = std::fs::OpenOptions::new();
    o.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    std::io::Write::write_all(&mut o.open(path)?, format!("{line}\n").as_bytes())
}

/// The run's document id and token written as these, wherever they appear in text.
fn hide(text: &str, secrets: &[String]) -> String {
    secrets
        .iter()
        .filter(|s| s.len() >= 8)
        .fold(text.to_owned(), |t, s| t.replace(s.as_str(), "<run-id>"))
}

fn hide_in(v: serde_json::Value, secrets: &[String]) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) => serde_json::Value::String(hide(&s, secrets)),
        serde_json::Value::Array(a) => a.into_iter().map(|x| hide_in(x, secrets)).collect(),
        serde_json::Value::Object(o) => o
            .into_iter()
            .map(|(k, x)| (k, hide_in(x, secrets)))
            .collect::<serde_json::Map<_, _>>()
            .into(),
        other => other,
    }
}

/// Strip what a turn must not keep: the run's document id (and any token-like key).
fn scrub(mut v: serde_json::Value) -> serde_json::Value {
    if let Some(o) = v.as_object_mut() {
        o.retain(|k, _| {
            let k = k.to_ascii_lowercase();
            k != "document_id"
                && !k.contains("token")
                && !k.contains("authorization")
                && !k.contains("password")
        });
        for x in o.values_mut() {
            *x = scrub(std::mem::take(x));
        }
    } else if let Some(a) = v.as_array_mut() {
        for x in a.iter_mut() {
            *x = scrub(std::mem::take(x));
        }
    }
    v
}

impl Store {
    /// The store in the settings folder (tests: their scratch settings folder).
    pub fn open() -> Option<Self> {
        Self::at(ext_shell::config_dir()?.join("ai-history"))
    }
    /// Nothing is made until something is written (a look, or the history
    /// switched off, leaves no folder behind).
    pub fn at(root: PathBuf) -> Option<Self> {
        Some(Self { root })
    }
    fn settings(&self) -> Settings {
        std::fs::read(self.root.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    fn set_settings(&self, s: &Settings) -> Result<()> {
        write_private(
            &self.root.join("settings.json"),
            &serde_json::to_vec_pretty(s)?,
        )?;
        Ok(())
    }
    pub fn enabled(&self) -> bool {
        !self.settings().off
    }
    pub fn set_enabled(&self, on: bool) -> Result<()> {
        self.set_settings(&Settings {
            off: !on,
            ..self.settings()
        })
    }
    pub fn track(&self) -> bool {
        self.settings().track
    }
    pub fn set_track(&self, on: bool) -> Result<()> {
        self.set_settings(&Settings {
            track: on,
            ..self.settings()
        })
    }

    pub fn list(&self) -> Vec<Conversation> {
        std::fs::read(self.root.join("index.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    fn save_list(&self, list: &[Conversation]) -> Result<()> {
        write_private(
            &self.root.join("index.json"),
            &serde_json::to_vec_pretty(list)?,
        )?;
        Ok(())
    }
    fn file(&self, id: &str) -> PathBuf {
        // Ids are made here (hex); never a path from elsewhere.
        let safe: String = id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        self.root.join(format!("{safe}.jsonl"))
    }

    /// Conversations of the document at `path` (or of unsaved text with this
    /// fingerprint), newest first, with how each relates to it.
    pub fn for_document(
        &self,
        path: Option<&Path>,
        fingerprint: &str,
    ) -> Vec<(Conversation, Relation)> {
        let path = path.map(key);
        let mut out: Vec<(Conversation, Relation)> = self
            .list()
            .into_iter()
            .filter_map(|c| {
                // An unsaved document is its text: never saved and the same text.
                let same_path = c.path == path;
                let same_text = c.fingerprint == fingerprint;
                match (same_path, same_text) {
                    (true, true) => Some((c, Relation::Same)),
                    (true, false) if path.is_some() => Some((c, Relation::Changed)),
                    (false, true) => Some((c, Relation::Moved)),
                    _ => None,
                }
            })
            .collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.0.updated));
        out
    }

    /// Start a conversation with its first turn.
    pub fn create(&self, path: Option<&Path>, turn: &TurnRecord) -> Result<String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let id = format!("{nanos:x}-{:x}", std::process::id());
        let title: String = turn
            .question
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(40)
            .collect();
        let mut list = self.list();
        list.push(Conversation {
            id: id.clone(),
            title,
            engine: turn.engine.clone(),
            session: turn.session.clone(),
            created: turn.at,
            updated: turn.at,
            turns: 0,
            path: path.map(key),
            fingerprint: turn.after.clone(),
        });
        self.save_list(&list)?;
        self.append(&id, path, turn)?;
        Ok(id)
    }

    /// Add a turn to a conversation; the document's path and fingerprint follow it.
    pub fn append(&self, id: &str, path: Option<&Path>, turn: &TurnRecord) -> Result<()> {
        let mut t = turn.clone();
        for tool in &mut t.tools {
            tool.args = scrub(std::mem::take(&mut tool.args));
        }
        append_private(&self.file(id), &serde_json::to_string(&t)?)?;
        let mut list = self.list();
        if let Some(c) = list.iter_mut().find(|c| c.id == id) {
            c.turns += 1;
            c.updated = t.at;
            c.engine = t.engine.clone();
            if t.session.is_some() {
                c.session = t.session.clone();
            }
            if let Some(p) = path {
                c.path = Some(key(p));
            }
            c.fingerprint = t.after.clone();
        }
        self.save_list(&list)
    }

    /// The document was saved somewhere (first save, 다른 이름으로 저장): the
    /// conversation follows it.
    pub fn relink(&self, id: &str, path: &Path, fingerprint: &str) -> Result<()> {
        let mut list = self.list();
        if let Some(c) = list.iter_mut().find(|c| c.id == id) {
            c.path = Some(key(path));
            c.fingerprint = fingerprint.to_owned();
        }
        self.save_list(&list)
    }

    pub fn turns(&self, id: &str) -> Vec<TurnRecord> {
        std::fs::read_to_string(self.file(id))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    /// Mark turn `n` of a conversation as taken back.
    pub fn mark_reverted(&self, id: &str, n: usize) -> Result<()> {
        let mut turns = self.turns(id);
        if let Some(t) = turns.get_mut(n) {
            t.reverted = true;
        }
        let body: String = turns
            .iter()
            .filter_map(|t| serde_json::to_string(t).ok())
            .map(|l| l + "\n")
            .collect();
        write_private(&self.file(id), body.as_bytes())?;
        Ok(())
    }

    /// Delete a conversation; with `cli_copies` also the CLIs' own records of
    /// every conversation id its turns used. Returns the files removed.
    pub fn delete(&self, id: &str, cli_copies: bool) -> Result<usize> {
        let mut sessions = turn_sessions(&self.turns(id));
        let mut list = self.list();
        for c in list.iter().filter(|c| c.id == id) {
            if let Some(s) = &c.session
                && !sessions.iter().any(|(e, x)| e == &c.engine && x == s)
            {
                sessions.push((c.engine.clone(), s.clone()));
            }
        }
        list.retain(|c| c.id != id);
        self.save_list(&list)?;
        let mut removed = usize::from(std::fs::remove_file(self.file(id)).is_ok());
        if cli_copies {
            for (engine, sid) in sessions {
                for f in cli_files(&engine, &sid) {
                    if std::fs::remove_file(&f).is_ok() || std::fs::remove_dir_all(&f).is_ok() {
                        removed += 1;
                    }
                }
            }
        }
        Ok(removed)
    }

    /// Delete these conversations.
    pub fn delete_many(&self, ids: &[String], cli_copies: bool) -> Result<usize> {
        let mut n = 0;
        for id in ids {
            n += self.delete(id, cli_copies)?;
        }
        Ok(n)
    }

    /// Delete the whole history (the switch and track settings stay).
    pub fn delete_all(&self, cli_copies: bool) -> Result<usize> {
        let mut n = 0;
        for c in self.list() {
            n += self.delete(&c.id, cli_copies)?;
        }
        let _ = std::fs::remove_file(self.root.join("index.json"));
        Ok(n)
    }
}

fn turn_sessions(turns: &[TurnRecord]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for t in turns {
        if let Some(s) = &t.session
            && !out.iter().any(|(e, x)| e == &t.engine && x == s)
        {
            out.push((t.engine.clone(), s.clone()));
        }
    }
    out
}

/// The CLI's own files of conversation `id`: Claude Code keeps
/// `~/.claude/projects/<panel work folder>/<id>.jsonl` (and a folder of the same
/// name), Codex the panel's own home `ai-work/codex-home/sessions/**/…<id>.jsonl`.
pub(crate) fn cli_files(engine: &str, id: &str) -> Vec<PathBuf> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Vec::new();
    }
    let mut out = Vec::new();
    if engine == Engine::Claude.name() {
        let work = work_root().join("run");
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
        let projects = home
            .map(PathBuf::from)
            .map(|h| h.join(".claude").join("projects"));
        for dir in projects
            .and_then(|p| std::fs::read_dir(p).ok())
            .into_iter()
            .flatten()
            .flatten()
        {
            // Only the panel's work folder, never the user's projects.
            if !claude_project(&dir.file_name().to_string_lossy(), &work) {
                continue;
            }
            for f in [dir.path().join(format!("{id}.jsonl")), dir.path().join(id)] {
                if f.exists() {
                    out.push(f);
                }
            }
        }
    } else {
        fn walk(dir: &Path, id: &str, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, id, out);
                } else if p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().contains(id))
                {
                    out.push(p);
                }
            }
        }
        walk(
            &work_root().join("codex-home").join("sessions"),
            id,
            &mut out,
        );
    }
    out
}

/// Claude Code's `~/.claude/projects` folder name for a working folder: every
/// character but ASCII letters and digits becomes `-`. A very long name is cut
/// by Claude Code (and given a suffix), so only its first 200 characters count.
fn claude_project(name: &str, work: &Path) -> bool {
    let want: String = work
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let (name, want) = if cfg!(windows) {
        (name.to_lowercase(), want.to_lowercase())
    } else {
        (name.to_owned(), want)
    };
    if want.len() > 200 {
        name.starts_with(&want[..200])
    } else {
        name == want
    }
}

/// The CLI can still continue conversation `id`.
pub(crate) fn resumable(engine: &str, id: &str) -> bool {
    !cli_files(engine, id).is_empty()
}

/// What earlier turns said, for a conversation the CLI can no longer continue.
pub(crate) fn summary(turns: &[TurnRecord]) -> String {
    let mut s = String::from(
        "[앞 대화 요약]\n이 문서에서 앞서 나눈 대화다. 문서는 그 뒤 바뀌었을 수 있으니 지금 문서를 다시 읽고 판단한다.\n",
    );
    for (i, t) in turns.iter().enumerate().rev().take(8).rev() {
        let answer: String = t
            .answer
            .clone()
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        s.push_str(&format!(
            "{}. 요청: {}\n   답: {}{}\n",
            i + 1,
            t.question.chars().take(200).collect::<String>(),
            answer,
            if t.reverted {
                " (그 뒤 되돌림)"
            } else {
                ""
            }
        ));
    }
    s
}

/// The conversation as Markdown (내보내기).
pub(crate) fn markdown(c: &Conversation, turns: &[TurnRecord]) -> String {
    let mut s = format!(
        "# {}\n\n- 엔진: {}\n- 질문 {}개\n\n",
        c.title,
        c.engine,
        turns.len()
    );
    for (i, t) in turns.iter().enumerate() {
        s.push_str(&format!("## 질문 {}\n\n{}\n\n", i + 1, t.question));
        if let Some(ctx) = &t.context {
            s.push_str(&format!("> 맥락: {ctx}\n\n"));
        }
        if let Some(a) = &t.answer {
            s.push_str(&format!("**답**\n\n{a}\n\n"));
        }
        if !t.changes.is_empty() {
            s.push_str("**바뀐 곳**\n\n");
            for ch in &t.changes {
                let line = match ch.kind {
                    changes::Kind::Modify => format!(
                        "- 고침: {} → {}",
                        ch.before.as_deref().unwrap_or(""),
                        ch.after.as_deref().unwrap_or("")
                    ),
                    changes::Kind::Insert => {
                        format!("- 넣음: {}", ch.after.as_deref().unwrap_or(""))
                    }
                    changes::Kind::Delete => {
                        format!("- 지움: {}", ch.before.as_deref().unwrap_or(""))
                    }
                };
                s.push_str(&line);
                s.push('\n');
            }
            s.push('\n');
        }
        if let Some(e) = &t.error {
            s.push_str(&format!("_오류: {e}_\n\n"));
        }
    }
    s
}

// ── The panel's side ────────────────────────────────────────────────────

/// A question's place in the history and what it changed.
#[derive(Clone, Debug, Default)]
pub(crate) struct Link {
    /// Conversation and turn number (none while the history is off).
    pub conv: Option<(String, usize)>,
    pub changes: Vec<Change>,
    pub text_only: bool,
    /// Recorded in the document as tracked changes by this author.
    pub tracked: Option<String>,
    /// Why it was not recorded as tracked changes (with that switch on).
    pub track_note: Option<String>,
    pub reverted: bool,
    /// `Session::generation` right after the question: while it is unchanged,
    /// one undo takes the question back exactly.
    pub generation: Option<u64>,
}

/// What the second click of 지우기 deletes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Doomed {
    One(String),
    Document,
    All,
}

/// The history as the panel shows it.
#[derive(Default)]
pub(crate) struct Hist {
    /// The conversation the panel adds to, and its number of turns.
    pub current: Option<(String, usize)>,
    /// Summary sent with the next question of a loaded conversation the CLI
    /// cannot continue.
    pub seed: Option<String>,
    /// The document the panel's conversation belongs to.
    serial: Option<u64>,
    path: Option<PathBuf>,
    /// This document's conversations.
    pub found: Vec<(Conversation, Relation)>,
    /// The switches as last read (history on, tracked replay on).
    pub on: bool,
    pub track: bool,
    pub open: bool,
    pub notice: Option<String>,
    pub confirm: Option<Doomed>,
    /// Deleting leaves the CLIs' own copies of the conversation.
    pub keep_cli_copies: bool,
    /// Tests keep their history apart.
    #[cfg(test)]
    pub root: Option<PathBuf>,
}

impl HwpApp {
    fn ai_store(&self) -> Option<Store> {
        if !self.persists_settings() {
            return None;
        }
        #[cfg(test)]
        if let Some(root) = &self.ext.ai.hist.root {
            return Store::at(root.clone());
        }
        Store::open()
    }

    /// Start an empty conversation (새 대화, another document).
    pub(super) fn ai_new_conversation(&mut self) {
        self.ext.ai.messages.clear();
        self.ext.ai.conversation = None;
        let h = &mut self.ext.ai.hist;
        h.current = None;
        h.seed = None;
        h.confirm = None;
        h.notice = None;
    }

    /// Each frame the panel shows, between questions: a newly opened document
    /// gets its own conversation and list, with its latest conversation loaded
    /// when the file is unchanged since; a document saved under a new name takes
    /// its conversation along.
    pub(super) fn ai_history_follow(&mut self) {
        if self.ai_busy() {
            return;
        }
        let serial = self.session.document_serial;
        let path = self.session.path.clone();
        let h = &self.ext.ai.hist;
        if h.serial != Some(serial) {
            let first = h.serial.is_none();
            self.ext.ai.hist.serial = Some(serial);
            self.ext.ai.hist.path = path;
            if !first {
                self.ai_new_conversation();
            }
            self.ai_history_scan();
            // Only a saved file: every new empty document has the same text.
            let latest = self.ext.ai.hist.found.first();
            if self.session.path.is_some()
                && self.ext.ai.messages.is_empty()
                && let Some((c, Relation::Same)) = latest
            {
                let id = c.id.clone();
                self.ai_history_open(&id);
            }
        } else if h.path != path {
            self.ext.ai.hist.path = path.clone();
            let now = changes::snapshot(&self.session);
            if let (Some((id, _)), Some(p), Some(store)) =
                (self.ext.ai.hist.current.clone(), path, self.ai_store())
                && let Err(e) = store.relink(&id, &p, &changes::fingerprint(&now))
            {
                self.ext.ai.hist.notice = Some(format!("대화 기록을 옮기지 못했습니다: {e}"));
            }
            self.ai_history_scan_with(&now);
        }
    }

    /// Read this document's conversations and the switches again.
    pub(super) fn ai_history_scan(&mut self) {
        let now = changes::snapshot(&self.session);
        self.ai_history_scan_with(&now);
    }

    /// The same, with the document's paragraphs already read.
    fn ai_history_scan_with(&mut self, now: &[changes::Para]) {
        let Some(store) = self.ai_store() else {
            self.ext.ai.hist.found.clear();
            return;
        };
        // Every empty new document has the same text: only its own path counts.
        let has_text = now.iter().any(|p| !p.text.trim().is_empty());
        let path = self.session.path.clone();
        let h = &mut self.ext.ai.hist;
        h.found = store
            .for_document(path.as_deref(), &changes::fingerprint(now))
            .into_iter()
            .filter(|(_, r)| has_text || *r != Relation::Moved)
            .collect();
        h.on = store.enabled();
        h.track = store.track();
    }

    /// After a question: its change set, recorded with the conversation and,
    /// with that switch on, in the document as tracked changes.
    pub(super) fn ai_record(
        &mut self,
        message: usize,
        engine: Engine,
        before: &[changes::Para],
        secrets: &[String],
    ) {
        let Some(m) = self.ext.ai.messages.get(message) else {
            return;
        };
        let turn = m.turn.clone().unwrap_or_default();
        let (changed, stopped, error) = (m.changed, m.stopped, m.error.clone());
        let (question, context) = message
            .checked_sub(1)
            .and_then(|i| self.ext.ai.messages.get(i))
            .filter(|q| q.user)
            .map(|q| (q.text.clone(), q.context.clone()))
            .unwrap_or_default();
        let after = changes::snapshot(&self.session);
        let calls: Vec<(String, serde_json::Value)> = turn
            .calls
            .iter()
            .map(|(_, t, a)| (t.clone(), a.clone()))
            .collect();
        let mut link = Link {
            changes: if changed {
                changes::diff(before, &after)
            } else {
                Vec::new()
            },
            text_only: changed && changes::text_only(&calls),
            ..Link::default()
        };
        let store = self.ai_store();
        if link.text_only && !stopped && error.is_none() && store.as_ref().is_some_and(Store::track)
        {
            let author = changes::track_author(engine.name());
            match changes::track_replay(&mut self.session, before, &after, &link.changes, &author) {
                Ok(()) => link.tracked = Some(author),
                Err(why) => link.track_note = Some(why),
            }
        }
        link.generation = Some(self.session.generation);
        // Recording as tracked changes changed the text (deleted text stays, marked).
        let shown = if link.tracked.is_some() {
            changes::snapshot(&self.session)
        } else {
            after
        };
        if let Some(store) = store.filter(Store::enabled) {
            let record = TurnRecord {
                at: now(),
                engine: engine.name().into(),
                session: turn.session.clone(),
                question: hide(&question, secrets),
                context,
                answer: turn.answer.as_deref().map(|a| hide(a, secrets)),
                tools: turn
                    .calls
                    .iter()
                    .map(|(id, tool, args)| ToolRecord {
                        tool: tool.clone(),
                        args: hide_in(args.clone(), secrets),
                        error: turn
                            .tools
                            .iter()
                            .find(|t| &t.0 == id)
                            .and_then(|t| t.3.as_deref())
                            .map(|e| hide(e, secrets)),
                    })
                    .collect(),
                changed,
                stopped,
                error: error.as_deref().map(|e| hide(e, secrets)),
                before: changes::fingerprint(before),
                after: changes::fingerprint(&shown),
                changes: link.changes.clone(),
                text_only: link.text_only,
                tracked: link.tracked.clone(),
                reverted: false,
            };
            let path = self.session.path.clone();
            let saved = match self.ext.ai.hist.current.clone() {
                Some((id, n)) => store
                    .append(&id, path.as_deref(), &record)
                    .map(|()| (id, n)),
                None => store.create(path.as_deref(), &record).map(|id| (id, 0)),
            };
            match saved {
                Ok((id, n)) => {
                    link.conv = Some((id.clone(), n));
                    self.ext.ai.hist.current = Some((id, n + 1));
                }
                Err(e) => {
                    self.ext.ai.hist.notice = Some(format!("대화 기록을 남기지 못했습니다: {e}"));
                }
            }
            self.ai_history_scan_with(&shown);
        }
        if let Some(m) = self.ext.ai.messages.get_mut(message) {
            m.link = Some(link);
        }
    }

    /// Show a stored conversation and continue it: through the CLI's own
    /// conversation while it still has it, else with a summary sent along.
    pub(super) fn ai_history_open(&mut self, id: &str) {
        let Some(store) = self.ai_store() else {
            return;
        };
        let Some(c) = store.list().into_iter().find(|c| c.id == id) else {
            return;
        };
        let turns = store.turns(id);
        let mut messages = Vec::new();
        for (i, t) in turns.iter().enumerate() {
            messages.push(Message::user(t.question.clone(), t.context.clone()));
            let tools = t
                .tools
                .iter()
                .enumerate()
                .map(|(k, r)| {
                    (
                        k.to_string(),
                        stream::tool_label(&r.tool, &r.args),
                        true,
                        r.error.clone(),
                    )
                })
                .collect();
            messages.push(Message {
                turn: Some(Turn {
                    session: t.session.clone(),
                    tools,
                    done: true,
                    answer: t.answer.clone(),
                    ..Turn::default()
                }),
                changed: t.changed,
                stopped: t.stopped,
                error: t.error.clone(),
                link: Some(Link {
                    conv: Some((id.to_owned(), i)),
                    changes: t.changes.clone(),
                    text_only: t.text_only,
                    tracked: t.tracked.clone(),
                    reverted: t.reverted,
                    ..Link::default()
                }),
                ..Message::ai()
            });
        }
        let engine = ENGINES
            .into_iter()
            .find(|e| e.name() == c.engine)
            .unwrap_or_default();
        let resume = c.session.clone().filter(|s| resumable(&c.engine, s));
        let ai = &mut self.ext.ai;
        ai.engine = engine;
        ai.conversation = resume.clone().map(|s| (engine, s));
        ai.messages = messages;
        let h = &mut ai.hist;
        h.current = Some((id.to_owned(), turns.len()));
        h.seed = (resume.is_none() && !turns.is_empty()).then(|| summary(&turns));
        h.confirm = None;
        h.open = false;
        h.notice = Some(if resume.is_some() {
            format!(
                "'{}' 대화를 불러왔습니다. 이어서 물으면 {}가 앞 대화를 기억합니다.",
                c.title,
                engine.name()
            )
        } else {
            format!(
                "'{}' 대화를 불러왔습니다. {}에 그 대화가 남아 있지 않아 다음 질문에 앞 대화의 요약을 함께 보냅니다.",
                c.title,
                engine.name()
            )
        });
    }

    /// Put the caret on a change of a question, found by its text.
    pub(super) fn ai_history_jump(&mut self, message: usize, k: usize) {
        let Some(c) = self
            .ext
            .ai
            .messages
            .get(message)
            .and_then(|m| m.link.as_ref())
            .and_then(|l| l.changes.get(k))
            .cloned()
        else {
            return;
        };
        let found = changes::find(&changes::snapshot(&self.session), &c);
        self.ext.ai.hist.notice = match found {
            Ok(at) => match self.session.select(at.cursor(0), false) {
                Ok(_) => {
                    self.scroll_to_caret = true;
                    None
                }
                Err(e) => Some(e.to_string()),
            },
            Err(why) => Some(why),
        };
    }

    /// Take a question back: exactly by one undo while nothing happened since,
    /// else, for a question that only changed text, by its text.
    pub(super) fn ai_history_revert(&mut self, message: usize) {
        if self.ai_busy() {
            return;
        }
        let Some(link) = self
            .ext
            .ai
            .messages
            .get(message)
            .and_then(|m| m.link.clone())
        else {
            return;
        };
        let result = if link.generation == Some(self.session.generation) {
            self.session
                .execute(Command::Undo)
                .map(|_| "이 질문의 편집을 되돌렸습니다.".to_owned())
                .map_err(|e| e.to_string())
        } else if link.tracked.is_some() {
            Err("변경 추적으로 남긴 질문입니다. 검토 탭에서 그 변경을 거부하세요.".into())
        } else if !link.text_only {
            Err(crate::ribbon::platform_keys(
                "서식·표·개체를 바꾼 질문은 그 뒤 문서가 바뀌면 여기서 되돌릴 수 없습니다. Ctrl+Z로 되돌리세요.",
            )
            .into_owned())
        } else {
            changes::revert(&mut self.session, &link.changes)
                .map(|n| format!("바뀐 곳 {n}곳을 되돌렸습니다. Ctrl+Z로 다시 살릴 수 있습니다."))
        };
        match result {
            Ok(note) => {
                self.scroll_to_caret = true;
                if let Some(l) = self
                    .ext
                    .ai
                    .messages
                    .get_mut(message)
                    .and_then(|m| m.link.as_mut())
                {
                    l.reverted = true;
                    l.generation = None;
                }
                if let (Some((id, n)), Some(store)) = (link.conv, self.ai_store())
                    && let Err(e) = store.mark_reverted(&id, n)
                {
                    self.ext.ai.hist.notice = Some(format!("{note} (기록에 남기지 못함: {e})"));
                    return;
                }
                self.ext.ai.hist.notice = Some(note);
            }
            Err(why) => self.ext.ai.hist.notice = Some(why),
        }
    }

    /// Write a conversation out as Markdown, where the user picks.
    pub(super) fn ai_history_export(&mut self, id: &str) {
        let Some(store) = self.ai_store() else {
            return;
        };
        let Some(c) = store.list().into_iter().find(|c| c.id == id) else {
            return;
        };
        let name: String = c
            .title
            .chars()
            .map(|ch| if r#"\/:*?"<>|"#.contains(ch) { '_' } else { ch })
            .collect();
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("{}.md", name.trim()))
            .add_filter("Markdown", &["md"])
            .save_file()
        else {
            return;
        };
        self.ext.ai.hist.notice = Some(
            match std::fs::write(&path, markdown(&c, &store.turns(id))) {
                Ok(()) => format!("대화를 내보냈습니다: {}", path.display()),
                Err(e) => format!("대화를 내보내지 못했습니다: {e}"),
            },
        );
    }

    /// Delete conversations (the CLIs' copies too, unless kept).
    pub(super) fn ai_history_delete(&mut self, what: &Doomed) {
        let Some(store) = self.ai_store() else {
            return;
        };
        let copies = !self.ext.ai.hist.keep_cli_copies;
        let result = match what {
            Doomed::One(id) => store.delete(id, copies),
            // What the list shows for this document, no more.
            Doomed::Document => {
                self.ai_history_scan();
                let ids: Vec<String> = self
                    .ext
                    .ai
                    .hist
                    .found
                    .iter()
                    .map(|(c, _)| c.id.clone())
                    .collect();
                store.delete_many(&ids, copies)
            }
            Doomed::All => store.delete_all(copies),
        };
        let kept = store.list();
        // The panel's own conversation was among them: it goes from the panel too.
        if let Some((id, _)) = &self.ext.ai.hist.current
            && !kept.iter().any(|c| &c.id == id)
        {
            self.ai_new_conversation();
        }
        self.ext.ai.hist.confirm = None;
        self.ext.ai.hist.notice = Some(match result {
            Ok(n) => format!("대화 기록을 지웠습니다 (파일 {n}개)."),
            Err(e) => format!("대화 기록을 지우지 못했습니다: {e}"),
        });
        self.ai_history_scan();
    }

    /// Turn the history or the tracked replay on or off.
    pub(super) fn ai_history_set(&mut self, on: Option<bool>, track: Option<bool>) {
        let Some(store) = self.ai_store() else {
            return;
        };
        let r = on
            .map_or(Ok(()), |v| store.set_enabled(v))
            .and_then(|()| track.map_or(Ok(()), |v| store.set_track(v)));
        if let Err(e) = r {
            self.ext.ai.hist.notice = Some(format!("설정을 저장하지 못했습니다: {e}"));
        }
        self.ai_history_scan();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(q: &str, after: &str) -> TurnRecord {
        TurnRecord {
            at: now(),
            engine: "Claude Code".into(),
            session: Some("abc-123".into()),
            question: q.into(),
            context: None,
            answer: Some("고쳤습니다".into()),
            tools: vec![ToolRecord {
                tool: "type_text".into(),
                args: json!({"document_id": "doc-1", "text": "가", "auth": {"token": "secret"}}),
                error: None,
            }],
            changed: true,
            stopped: false,
            error: None,
            before: "0".into(),
            after: after.into(),
            changes: vec![],
            text_only: true,
            tracked: None,
            reverted: false,
        }
    }

    #[test]
    fn conversations_follow_their_document_by_path_then_by_text() {
        let dir = std::env::temp_dir().join(format!("hwpcraft-ai-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::at(dir.clone()).unwrap();
        // Looking makes no folder.
        assert!(store.list().is_empty() && store.enabled() && !dir.exists());
        std::fs::create_dir_all(&dir).unwrap();
        let doc = dir.join("보고서.hwpx");
        std::fs::write(&doc, b"x").unwrap();
        let id = store.create(Some(&doc), &turn("표 넣어 줘", "f1")).unwrap();
        store
            .append(&id, Some(&doc), &turn("한 줄 더", "f2"))
            .unwrap();
        assert_eq!(store.turns(&id).len(), 2);
        assert_eq!(store.for_document(Some(&doc), "f2")[0].1, Relation::Same);
        assert_eq!(store.for_document(Some(&doc), "f9")[0].1, Relation::Changed);
        let moved = dir.join("옮긴 보고서.hwpx");
        assert_eq!(store.for_document(Some(&moved), "f2")[0].1, Relation::Moved);
        assert!(store.for_document(Some(&moved), "f9").is_empty());
        // No document id, token or credential is written.
        let raw = std::fs::read_to_string(store.file(&id)).unwrap();
        assert!(!raw.contains("doc-1") && !raw.contains("secret"), "{raw}");
        // The switch, then deleting.
        assert!(store.enabled());
        store.set_enabled(false).unwrap();
        assert!(!store.enabled());
        store.mark_reverted(&id, 1).unwrap();
        assert!(store.turns(&id)[1].reverted);
        let md = markdown(&store.list()[0], &store.turns(&id));
        assert!(md.contains("## 질문 2") && md.contains("한 줄 더"), "{md}");
        assert!(summary(&store.turns(&id)).contains("(그 뒤 되돌림)"));
        let ids: Vec<String> = store
            .for_document(Some(&doc), "f2")
            .into_iter()
            .map(|(c, _)| c.id)
            .collect();
        store.delete_many(&ids, false).unwrap();
        assert!(store.list().is_empty() && !store.file(&id).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One finished question on the panel, as `ai_finish` leaves it: the AI's
    /// edits one undo step, its calls in the turn.
    fn ask(
        app: &mut HwpApp,
        question: &str,
        tool: &str,
        args: serde_json::Value,
        edit: impl FnOnce(&mut Session),
    ) -> usize {
        use stream::Event;
        let before = changes::snapshot(&app.session);
        app.session.begin_undo_group().unwrap();
        edit(&mut app.session);
        let changed = app.session.end_undo_group().unwrap();
        app.ext
            .ai
            .messages
            .push(Message::user(question.into(), Some("커서 문단".into())));
        let mut turn = Turn::default();
        for e in [
            Event::Session("4f1c-panel-test".into()),
            Event::ToolStarted {
                id: "1".into(),
                label: tool.into(),
            },
            Event::Call {
                id: "1".into(),
                tool: tool.into(),
                args,
            },
            Event::ToolFinished {
                id: "1".into(),
                error: None,
            },
            Event::Done {
                answer: Some("고쳤습니다.".into()),
                error: None,
            },
        ] {
            turn.apply(e);
        }
        app.ext.ai.messages.push(Message {
            turn: Some(turn),
            changed,
            ..Message::ai()
        });
        let i = app.ext.ai.messages.len() - 1;
        app.ai_record(i, Engine::Claude, &before, &["doc-run-secret-1".to_owned()]);
        i
    }
    fn at(p: usize, offset: usize) -> Cursor {
        Cursor {
            section: 0,
            paragraph: p,
            offset,
            cell: None,
        }
    }
    fn texts(app: &HwpApp) -> Vec<String> {
        changes::snapshot(&app.session)
            .into_iter()
            .map(|p| p.text)
            .collect()
    }

    #[test]
    fn the_panel_records_reloads_finds_and_takes_back_a_question() {
        let dir =
            std::env::temp_dir().join(format!("hwpcraft-ai-panel-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.ext.ai.hist.root = Some(dir.clone());
        app.session
            .execute(Command::Insert(
                "제목\n본 사업은 6개월간 수행한다\n끝".into(),
            ))
            .unwrap();
        app.ai_history_follow();
        let i = ask(
            &mut app,
            "기간을 고쳐 줘",
            "type_text",
            json!({"text": "계약일로부터 ", "document_id": "doc-run-1", "note": "id doc-run-secret-1 쓰기"}),
            |s| {
                s.select(at(1, 6), false).unwrap();
                s.execute(Command::Insert("계약일로부터 ".into())).unwrap();
            },
        );
        let link = app.ext.ai.messages[i].link.clone().unwrap();
        assert_eq!(link.changes.len(), 1, "{link:?}");
        assert!(
            link.text_only && link.tracked.is_none() && link.conv.is_some(),
            "{link:?}"
        );
        let store = Store::at(dir.clone()).unwrap();
        let list = store.list();
        assert_eq!((list.len(), list[0].title.as_str()), (1, "기간을 고쳐 줘"));
        let id = list[0].id.clone();
        let raw = std::fs::read_to_string(store.file(&id)).unwrap();
        assert!(
            !raw.contains("doc-run-1")
                && !raw.contains("doc-run-secret-1")
                && raw.contains("<run-id>")
                && raw.contains("계약일로부터"),
            "{raw}"
        );

        // 새 대화, then the stored one is listed for this (unsaved) document and opened.
        app.ai_new_conversation();
        assert!(app.ext.ai.messages.is_empty() && app.ext.ai.hist.current.is_none());
        app.ai_history_scan();
        assert_eq!(app.ext.ai.hist.found.len(), 1);
        assert_eq!(app.ext.ai.hist.found[0].1, Relation::Same);
        app.ai_history_open(&id);
        let m = &app.ext.ai.messages;
        assert_eq!(m.len(), 2);
        assert!(m[0].user && m[0].text == "기간을 고쳐 줘");
        let t = m[1].turn.as_ref().unwrap();
        assert_eq!(t.answer.as_deref(), Some("고쳤습니다."));
        assert_eq!(t.tools[0].1, "type_text");
        // The CLI has no copy of this test's conversation: a summary goes along.
        assert!(app.ext.ai.conversation.is_none());
        assert!(
            app.ext
                .ai
                .hist
                .seed
                .as_deref()
                .unwrap()
                .contains("기간을 고쳐 줘")
        );
        assert_eq!(app.ext.ai.hist.current, Some((id.clone(), 1)));

        // The user typed above it since: found and taken back by its text.
        app.session.select(at(0, 0), false).unwrap();
        app.session
            .execute(Command::Insert("머리 ".into()))
            .unwrap();
        app.ai_history_jump(1, 0);
        assert_eq!(app.ext.ai.hist.notice, None);
        assert_eq!(app.session.cursor.paragraph, 1);
        app.ai_history_revert(1);
        assert_eq!(
            texts(&app),
            ["머리 제목", "본 사업은 6개월간 수행한다", "끝"],
            "{:?}",
            app.ext.ai.hist.notice
        );
        assert!(app.ext.ai.messages[1].link.as_ref().unwrap().reverted);
        assert!(store.turns(&id)[0].reverted);

        // The next question joins the conversation; right after it one undo takes it back.
        let j = ask(
            &mut app,
            "끝에 한 줄",
            "type_text",
            json!({"text": "추가"}),
            |s| {
                s.select(at(2, 1), false).unwrap();
                s.execute(Command::Split).unwrap();
                s.execute(Command::Insert("추가".into())).unwrap();
            },
        );
        assert_eq!(store.turns(&id).len(), 2);
        assert_eq!(
            app.ext.ai.messages[j].link.as_ref().unwrap().conv,
            Some((id.clone(), 1))
        );
        app.ai_history_revert(j);
        assert_eq!(
            texts(&app),
            ["머리 제목", "본 사업은 6개월간 수행한다", "끝"]
        );

        // With the switch on, a text-only question is left as tracked changes.
        app.ai_history_set(None, Some(true));
        assert!(app.ext.ai.hist.track);
        let k = ask(
            &mut app,
            "제목을 바꿔 줘",
            "execute",
            json!({"command": "edit.replace_all", "params": {}}),
            |s| {
                s.select(at(0, 3), false).unwrap();
                s.select(at(0, 5), true).unwrap();
                s.execute(Command::Insert("표제".into())).unwrap();
            },
        );
        let link = app.ext.ai.messages[k].link.clone().unwrap();
        assert_eq!(
            link.tracked.as_deref(),
            Some("AI · Claude Code"),
            "{link:?}"
        );
        assert_eq!(app.session.track_changes()["tracked"], true);
        assert_eq!(
            store.turns(&id)[2].tracked.as_deref(),
            Some("AI · Claude Code")
        );
        // A formatting question is recorded but not replayed, and is not taken
        // back by text once the document moved on.
        let f = ask(
            &mut app,
            "굵게",
            "execute",
            json!({"command": "format.character"}),
            |s| {
                s.select(at(2, 0), false).unwrap();
                s.execute(Command::Insert("!".into())).unwrap();
            },
        );
        let link = app.ext.ai.messages[f].link.clone().unwrap();
        assert!(!link.text_only && link.tracked.is_none(), "{link:?}");
        app.session.select(at(2, 0), false).unwrap();
        app.session.execute(Command::Insert("손".into())).unwrap();
        let now = texts(&app);
        app.ai_history_revert(f);
        assert_eq!(texts(&app), now);
        assert!(
            app.ext
                .ai
                .hist
                .notice
                .as_deref()
                .unwrap()
                .contains("되돌릴 수 없습니다")
        );

        // Another document: an empty conversation and its own (empty) list.
        let kept = app.session.document_serial;
        app.session = Session::new().unwrap();
        assert_ne!(app.session.document_serial, kept);
        app.ai_history_follow();
        assert!(app.ext.ai.messages.is_empty() && app.ext.ai.hist.found.is_empty());

        // Saved under a name: the conversation follows the file.
        app.session
            .execute(Command::Insert("새 문서 글".into()))
            .unwrap();
        let q = ask(&mut app, "다듬어 줘", "type_text", json!({}), |s| {
            s.execute(Command::Insert(" 더".into())).unwrap();
        });
        let new_id = app.ext.ai.messages[q]
            .link
            .as_ref()
            .unwrap()
            .conv
            .clone()
            .unwrap()
            .0;
        let file = dir.join("저장한 문서.hwpx");
        app.session.save(&file).unwrap();
        app.ai_history_follow();
        let c = store.list().into_iter().find(|c| c.id == new_id).unwrap();
        assert_eq!(c.path, Some(key(&file)));
        assert_eq!(app.ext.ai.hist.found.len(), 1);
        // Opened again later: its latest conversation is back in the panel.
        let mut again = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        again.ext.ai.hist.root = Some(dir.clone());
        again.session.open(&file).unwrap();
        again.ai_history_follow();
        assert_eq!(
            again.ext.ai.hist.current.as_ref().map(|c| c.0.as_str()),
            Some(new_id.as_str())
        );
        assert_eq!(again.ext.ai.messages.len(), 2);
        assert_eq!(again.ext.ai.messages[0].text, "다듬어 줘");
        drop(again);

        // Deleting this document's history; the panel's conversation goes with it.
        app.ai_history_delete(&Doomed::Document);
        assert!(app.ext.ai.messages.is_empty());
        assert_eq!(
            store.list().len(),
            1,
            "only the first document's conversation is left"
        );
        app.ai_history_delete(&Doomed::All);
        assert!(store.list().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tracked_question_saved_and_reopened_comes_back() {
        let dir =
            std::env::temp_dir().join(format!("hwpcraft-ai-tracked-reopen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.ext.ai.hist.root = Some(dir.join("history"));
        app.session
            .execute(Command::Insert("가고, 나\n둘째 문단".into()))
            .unwrap();
        let file = dir.join("추적.hwpx");
        app.session.save(&file).unwrap();
        app.ai_history_follow();
        app.ai_history_set(None, Some(true));
        // One deletion and one insertion, left as tracked changes.
        let i = ask(&mut app, "고쳐 줘", "type_text", json!({}), |s| {
            s.select(at(0, 1), false).unwrap();
            s.select(at(0, 3), true).unwrap();
            s.execute(Command::Delete).unwrap();
            s.select(at(1, 5), false).unwrap();
            s.execute(Command::Insert("다.".into())).unwrap();
        });
        let link = app.ext.ai.messages[i].link.clone().unwrap();
        assert!(link.tracked.is_some(), "{link:?}");
        let (id, _) = link.conv.clone().unwrap();
        app.session.save(&file).unwrap();
        app.ai_history_follow();
        let shown = changes::fingerprint(&changes::snapshot(&app.session));
        let mut again = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        again.ext.ai.hist.root = Some(dir.join("history"));
        again.session.open(&file).unwrap();
        let reopened = changes::fingerprint(&changes::snapshot(&again.session));
        again.ai_history_follow();
        assert_eq!(
            again.ext.ai.hist.found.first().map(|f| f.1),
            Some(Relation::Same),
            "fingerprint before save {shown}, after reopening {reopened}: {:?} vs {:?}",
            texts(&app),
            texts(&again)
        );
        assert_eq!(
            again.ext.ai.hist.current.as_ref().map(|c| c.0.as_str()),
            Some(id.as_str())
        );
        drop((app, again));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_screen_capture_keeps_no_history() {
        let dir =
            std::env::temp_dir().join(format!("hwpcraft-ai-shot-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.ext.ai.hist.root = Some(dir.clone());
        app.screenshot = Some(dir.join("shot.png"));
        app.ai_history_follow();
        app.ai_history_set(Some(true), Some(true));
        ask(&mut app, "넣어 줘", "type_text", json!({}), |s| {
            s.execute(Command::Insert("가".into())).unwrap();
        });
        let link = app.ext.ai.messages[1].link.clone().unwrap();
        assert!(link.conv.is_none() && link.tracked.is_none(), "{link:?}");
        assert!(!dir.exists(), "a capture wrote the history");
    }

    #[test]
    fn only_the_panels_own_claude_folder_is_looked_at() {
        let work = Path::new(r"C:\Users\me\AppData\Roaming\hwpcraft\ai-work\run");
        assert!(claude_project(
            "C--Users-me-AppData-Roaming-hwpcraft-ai-work-run",
            work
        ));
        // Another project with "ai-work" in its name is the user's own.
        assert!(!claude_project("C--Users-me-src-ai-work-run-notes", work));
        assert!(!claude_project("C--Users-me-src-my-ai-work", work));
        let long = PathBuf::from(format!("C:\\{}\\ai-work\\run", "긴폴더".repeat(80)));
        let name: String = long
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .take(200)
            .collect();
        assert!(claude_project(&format!("{name}-1a2b3c"), &long));
    }
}
