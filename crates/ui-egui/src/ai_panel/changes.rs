//! What one AI question changed in the document, kept with the conversation so
//! a change can be found again, the question taken back after the undo history
//! is gone, and a text-only question recorded as tracked changes.
//!
//! A question's change set is a paragraph diff of the body and first-level table
//! cell paragraphs before and after it. Each change carries its text and the text
//! of the paragraphs around it, so it is found by what it says, not by an index
//! that later edits move. Formatting and objects are not diffed: a question that
//! used commands other than text ones is marked so, and only text-only questions
//! can be taken back or replayed as tracked changes.
use super::*;
use anyhow::Context as _;
use serde::{Deserialize, Serialize};

/// Text kept for a neighbour or a long paragraph (characters).
const KEEP: usize = 400;
/// Paragraph edits a question may make and still be diffed paragraph by
/// paragraph; more is recorded as one replaced block.
const MAX_EDITS: usize = 1000;

/// A paragraph's place: section, paragraph, and the table cell it sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct At {
    pub s: usize,
    pub p: usize,
    /// (control, cell, paragraph in the cell)
    pub cell: Option<(usize, usize, usize)>,
}

impl At {
    pub fn of(c: Cursor) -> Self {
        Self {
            s: c.section,
            p: c.paragraph,
            cell: c.cell.map(|t| (t.control, t.index, t.paragraph)),
        }
    }
    pub fn cursor(self, offset: usize) -> Cursor {
        Cursor {
            section: self.s,
            paragraph: self.p,
            offset,
            cell: self
                .cell
                .map(|(control, index, paragraph)| hwpcraft_doc::Cell {
                    control,
                    index,
                    paragraph,
                }),
        }
    }
    /// The body, or one table cell: paragraphs join and split only inside one.
    fn container(self) -> (usize, Option<(usize, usize, usize)>) {
        match self.cell {
            None => (self.s, None),
            Some((control, index, _)) => (self.s, Some((self.p, control, index))),
        }
    }
    fn same_container(self, other: At) -> bool {
        self.container() == other.container()
    }
    fn with_p(self, p: usize) -> Self {
        match self.cell {
            None => Self { p, ..self },
            Some((control, index, _)) => Self {
                cell: Some((control, index, p)),
                ..self
            },
        }
    }
    /// The paragraph number inside its container.
    fn index(self) -> usize {
        self.cell.map_or(self.p, |c| c.2)
    }
}

/// One paragraph of a snapshot.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Para {
    pub at: At,
    pub text: String,
    /// Holds a control (table, picture, field…): not edited by text replay.
    pub controls: bool,
}

/// A control that is an object in the text (a table, picture, note, field…),
/// not a setting of the section or page the first paragraph carries.
fn object(c: &hwpcraft_doc::model::control::Control) -> bool {
    use hwpcraft_doc::model::control::Control as C;
    !matches!(
        c,
        C::SectionDef(_)
            | C::ColumnDef(_)
            | C::Header(_)
            | C::Footer(_)
            | C::PageNumberPos(_)
            | C::PageNumCtrl(_)
            | C::NewNumber(_)
    )
}

/// Body and first-level cell paragraphs, in document order.
pub(crate) fn snapshot(session: &Session) -> Vec<Para> {
    session
        .cursors()
        .into_iter()
        .filter_map(|c| {
            let p = session.paragraph(c).ok()?;
            Some(Para {
                at: At::of(c),
                text: p.text.clone(),
                controls: p.controls.iter().any(object),
            })
        })
        .collect()
}

/// A stable fingerprint of a snapshot's text (FNV-1a over every paragraph).
pub(crate) fn fingerprint(paras: &[Para]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in paras {
        for b in p.text.as_bytes().iter().chain(b"\n") {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Kind {
    Insert,
    Delete,
    Modify,
}

/// One changed paragraph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Change {
    pub kind: Kind,
    pub before: Option<String>,
    pub after: Option<String>,
    /// Where it was before the question (Delete, Modify).
    pub at_before: Option<At>,
    /// Where it is after (Insert, Modify).
    pub at_after: Option<At>,
    /// For an insert: the paragraph it follows, before the question (same
    /// container), or none at the container's start.
    pub follows: Option<At>,
    /// Text of the paragraph before and after it in the document after the question.
    pub prev: Option<String>,
    pub next: Option<String>,
    /// The paragraph holds a control (table, picture…).
    pub controls: bool,
}

fn keep(s: &str) -> String {
    s.chars().take(KEEP).collect()
}

/// Shortest edit script from `a` to `b` (Myers), as (index in `a`, index in
/// `b`) pairs: both for a kept item, one for a deleted or inserted one. None
/// when it takes more than `limit` edits.
fn edit_script(
    a: &[&str],
    b: &[&str],
    limit: usize,
) -> Option<Vec<(Option<usize>, Option<usize>)>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = (n + m) as usize;
    let off = max as isize + 1;
    let mut v = vec![0isize; 2 * max + 3];
    // The furthest x per diagonal before each round, for diagonals -d..=d.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut end = None;
    'rounds: for d in 0..=max.min(limit) as isize {
        trace.push(v[(off - d) as usize..=(off + d) as usize].to_vec());
        for k in (-d..=d).step_by(2) {
            let down = k == -d || (k != d && v[(off + k - 1) as usize] < v[(off + k + 1) as usize]);
            let mut x = if down {
                v[(off + k + 1) as usize]
            } else {
                v[(off + k - 1) as usize] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[(off + k) as usize] = x;
            if x >= n && y >= m {
                end = Some(d);
                break 'rounds;
            }
        }
    }
    let end = end?;
    let (mut x, mut y) = (n, m);
    let mut ops = Vec::new();
    for d in (1..=end).rev() {
        let w = &trace[d as usize];
        let at = |k: isize| w[(k + d) as usize];
        let k = x - y;
        let prev_k = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = at(prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            ops.push((Some(x as usize), Some(y as usize)));
        }
        if x == prev_x {
            ops.push((None, Some(prev_y as usize)));
        } else {
            ops.push((Some(prev_x as usize), None));
        }
        (x, y) = (prev_x, prev_y);
    }
    while x > 0 {
        x -= 1;
        y -= 1;
        ops.push((Some(x as usize), Some(y as usize)));
    }
    ops.reverse();
    Some(ops)
}

/// The paragraphs that changed from `before` to `after`.
pub(crate) fn diff(before: &[Para], after: &[Para]) -> Vec<Change> {
    let (a, b) = (before, after);
    // Unchanged ends first: questions touch a few paragraphs of long documents.
    let mut head = 0;
    while head < a.len() && head < b.len() && a[head].text == b[head].text {
        head += 1;
    }
    let mut tail = 0;
    while tail < a.len() - head
        && tail < b.len() - head
        && a[a.len() - 1 - tail].text == b[b.len() - 1 - tail].text
    {
        tail += 1;
    }
    let (ma, mb) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    // Edit script over the middle (Myers: time grows with the edits, not the
    // document), else one block replaced.
    let ta: Vec<&str> = ma.iter().map(|p| p.text.as_str()).collect();
    let tb: Vec<&str> = mb.iter().map(|p| p.text.as_str()).collect();
    // (before idx, after idx)
    let ops: Vec<(Option<usize>, Option<usize>)> = match edit_script(&ta, &tb, MAX_EDITS) {
        Some(script) => script
            .into_iter()
            .map(|(i, j)| (i.map(|i| head + i), j.map(|j| head + j)))
            .collect(),
        None => (0..ma.len())
            .map(|i| (Some(head + i), None))
            .chain((0..mb.len()).map(|j| (None, Some(head + j))))
            .collect(),
    };
    // A delete next to an insert in one container is one modified paragraph.
    let mut out = Vec::new();
    let mut k = 0;
    // The last before-paragraph seen, for an insert's place.
    let mut last_before: Option<usize> = head.checked_sub(1);
    let mut last_after: Option<usize> = head.checked_sub(1);
    while k < ops.len() {
        match ops[k] {
            (Some(i), Some(j)) => {
                last_before = Some(i);
                last_after = Some(j);
                k += 1;
            }
            _ => {
                // A run of deletes and inserts.
                let start = k;
                while k < ops.len() && !matches!(ops[k], (Some(_), Some(_))) {
                    k += 1;
                }
                let dels: Vec<usize> = ops[start..k]
                    .iter()
                    .filter_map(|o| o.0.filter(|_| o.1.is_none()))
                    .collect();
                let ins: Vec<usize> = ops[start..k]
                    .iter()
                    .filter_map(|o| o.1.filter(|_| o.0.is_none()))
                    .collect();
                let pairs = dels.len().min(ins.len());
                // A deleted paragraph's place: the gap after the run's last
                // paragraph in `after` (every delete of a run shares it).
                let gap = ins.last().copied().or(last_after);
                let gap_prev = gap.map(|i| keep(&b[i].text));
                let gap_next = gap
                    .map_or(Some(0), |i| Some(i + 1))
                    .and_then(|i| b.get(i))
                    .map(|p| keep(&p.text));
                for x in 0..dels.len().max(ins.len()) {
                    let d = dels.get(x).copied();
                    let n = ins.get(x).copied();
                    let paired = x < pairs && a[d.unwrap()].at.same_container(b[n.unwrap()].at);
                    if paired {
                        let (d, n) = (d.unwrap(), n.unwrap());
                        out.push(change(Kind::Modify, Some(d), Some(n), None, a, b));
                    } else {
                        if let Some(d) = d {
                            let mut c = change(Kind::Delete, Some(d), None, None, a, b);
                            c.prev = gap_prev.clone();
                            c.next = gap_next.clone();
                            out.push(c);
                        }
                        if let Some(n) = n {
                            let follows = last_before
                                .filter(|&i| a[i].at.same_container(b[n].at))
                                .map(|i| a[i].at);
                            out.push(change(Kind::Insert, None, Some(n), follows, a, b));
                        }
                    }
                }
                if let Some(&d) = dels.last() {
                    last_before = Some(d);
                }
                if let Some(&j) = ins.last() {
                    last_after = Some(j);
                }
            }
        }
    }
    out
}

fn change(
    kind: Kind,
    d: Option<usize>,
    n: Option<usize>,
    follows: Option<At>,
    a: &[Para],
    b: &[Para],
) -> Change {
    // Neighbours in the document after the question (for a delete: around its place).
    let (prev, next) = match n {
        Some(n) => (
            n.checked_sub(1).map(|i| keep(&b[i].text)),
            b.get(n + 1).map(|p| keep(&p.text)),
        ),
        None => {
            let d = d.unwrap();
            (
                d.checked_sub(1).map(|i| keep(&a[i].text)),
                a.get(d + 1).map(|p| keep(&p.text)),
            )
        }
    };
    Change {
        kind,
        before: d.map(|i| a[i].text.clone()),
        after: n.map(|i| b[i].text.clone()),
        at_before: d.map(|i| a[i].at),
        at_after: n.map(|i| b[i].at),
        follows,
        prev,
        next,
        controls: d.is_some_and(|i| a[i].controls) || n.is_some_and(|i| b[i].controls),
    }
}

/// MCP tools and engine commands a text-only question may use.
fn text_command(name: &str) -> bool {
    const READS: [&str; 10] = [
        "document.text",
        "document.inspect",
        "object.list",
        "caret.get",
        "caret.move",
        "select.get",
        "context.v2.list",
        "context.v2.get",
        "commands.search",
        "page.layout.get",
    ];
    const EDITS: [&str; 6] = [
        "text.insert",
        "text.delete",
        "edit.replace",
        "edit.replace_all",
        "edit.find",
        "select.text",
    ];
    READS.contains(&name)
        || EDITS.contains(&name)
        || name.starts_with("select.")
        || name.starts_with("selection.")
        || (name.starts_with("format.") && name.ends_with(".get"))
}

/// True when every call of a question only read, moved, selected or typed text.
pub(crate) fn text_only(tools: &[(String, serde_json::Value)]) -> bool {
    tools.iter().all(|(tool, args)| {
        let tool = tool.strip_prefix("mcp__hwpcraft__").unwrap_or(tool);
        match tool {
            "list_documents" | "list_commands" | "get_text" | "get_selection"
            | "inspect_document" | "render_page" | "select_text" | "type_text" => true,
            "execute" => args["command"].as_str().is_some_and(text_command),
            "batch" => args["commands"].as_array().is_some_and(|cs| {
                cs.iter()
                    .all(|c| c["command"].as_str().is_some_and(text_command))
            }),
            "hwp_action" => args["id"].as_str().is_some_and(|id| {
                id.starts_with("Move") || id.starts_with("Select") || id == "Cancel"
            }),
            _ => false,
        }
    })
}

/// Why a change set cannot be replayed or taken back as text, if it cannot.
pub(crate) fn text_blocker(changes: &[Change]) -> Option<&'static str> {
    if changes.is_empty() {
        return Some("글이 바뀐 곳이 없습니다(서식·개체만 바뀐 질문).");
    }
    if changes.iter().any(|c| c.controls) {
        return Some("표·그림 같은 개체가 든 문단이 바뀌었습니다.");
    }
    if changes
        .iter()
        .any(|c| c.kind == Kind::Insert && c.follows.is_none() && c.next.is_none())
    {
        return Some("새로 생긴 표나 셀 안의 글이라 자리를 정할 수 없습니다.");
    }
    None
}

// ── Editing with plain text steps ───────────────────────────────────────

/// One step on the document, with places in the document it applies to.
#[derive(Clone, Debug)]
enum Step {
    /// Replace a paragraph's text.
    Set(At, String),
    /// Remove a paragraph.
    Remove(At),
    /// Add a paragraph with this text after one (or at the container's start
    /// when `after` is none, before `before`).
    Add {
        after: Option<At>,
        before: Option<At>,
        text: String,
    },
}

impl Step {
    /// Order key: later places first (a step never moves the places of earlier ones).
    fn key(&self) -> (At, u8) {
        match self {
            Step::Set(at, _) => (*at, 1),
            Step::Remove(at) => (*at, 0),
            Step::Add {
                after: Some(at), ..
            } => (*at, 2),
            Step::Add {
                before: Some(at), ..
            } => (at.with_p(at.index()), 0),
            Step::Add { .. } => (
                At {
                    s: 0,
                    p: 0,
                    cell: None,
                },
                0,
            ),
        }
    }
}

fn len(session: &Session, at: At) -> Result<usize> {
    Ok(session.paragraph(at.cursor(0))?.text.chars().count())
}

/// Paragraph count of `at`'s container.
fn count(session: &Session, at: At) -> usize {
    let mut n = 0;
    while session.paragraph(at.with_p(n).cursor(0)).is_ok() {
        n += 1;
    }
    n
}

fn apply_step(session: &mut Session, step: &Step) -> Result<()> {
    match step {
        Step::Set(at, text) => {
            // Only the part that differs: tracked changes then mark just it.
            let old: Vec<char> = session.paragraph(at.cursor(0))?.text.chars().collect();
            let new: Vec<char> = text.chars().collect();
            let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
            let tail = old[head..]
                .iter()
                .rev()
                .zip(new[head..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            let (end, middle) = (old.len() - tail, &new[head..new.len() - tail]);
            if head < end {
                session.select(at.cursor(head), false)?;
                session.select(at.cursor(end), true)?;
                if middle.is_empty() {
                    session.execute(Command::Delete)?;
                } else {
                    session.execute(Command::Insert(middle.iter().collect()))?;
                }
            } else if !middle.is_empty() {
                session.select(at.cursor(head), false)?;
                session.execute(Command::Insert(middle.iter().collect()))?;
            }
        }
        Step::Remove(at) => {
            let i = at.index();
            if i > 0 {
                // From the end of the paragraph before to the end of this one.
                let prev = at.with_p(i - 1);
                let pl = len(session, prev)?;
                let n = len(session, *at)?;
                session.select(prev.cursor(pl), false)?;
                session.select(at.cursor(n), true)?;
                session.execute(Command::Delete)?;
            } else if count(session, *at) > 1 {
                // The first paragraph: up to the start of the next one.
                let next = at.with_p(1);
                session.select(at.cursor(0), false)?;
                session.select(next.cursor(0), true)?;
                session.execute(Command::Delete)?;
            } else {
                // The only paragraph: its text goes, the paragraph stays.
                apply_step(session, &Step::Set(*at, String::new()))?;
            }
        }
        Step::Add {
            after: Some(at),
            text,
            ..
        } => {
            let n = len(session, *at)?;
            session.select(at.cursor(n), false)?;
            session.execute(Command::Split)?;
            if !text.is_empty() {
                session.execute(Command::Insert(text.clone()))?;
            }
        }
        Step::Add {
            before: Some(at),
            text,
            ..
        } => {
            session.select(at.cursor(0), false)?;
            if !text.is_empty() {
                session.execute(Command::Insert(text.clone()))?;
            }
            session.execute(Command::Split)?;
        }
        Step::Add { .. } => anyhow::bail!("넣을 자리가 없습니다."),
    }
    Ok(())
}

fn apply(session: &mut Session, mut steps: Vec<Step>) -> Result<()> {
    steps.sort_by_key(|x| std::cmp::Reverse(x.key()));
    for step in &steps {
        apply_step(session, step)?;
    }
    Ok(())
}

/// Make the question's change set again on the document as it was before it
/// (places are those before the question).
pub(crate) fn replay(session: &mut Session, changes: &[Change]) -> Result<()> {
    // Inserts after one paragraph go in reverse so they end up in order.
    let mut steps = Vec::new();
    let mut adds: Vec<Step> = Vec::new();
    for c in changes {
        match c.kind {
            Kind::Modify => steps.push(Step::Set(
                c.at_before.context("자리 없음")?,
                c.after.clone().unwrap_or_default(),
            )),
            Kind::Delete => steps.push(Step::Remove(c.at_before.context("자리 없음")?)),
            Kind::Insert => adds.push(Step::Add {
                after: c.follows,
                before: None,
                text: c.after.clone().unwrap_or_default(),
            }),
        }
    }
    // Adds after the same paragraph: the last one first (each goes right after it).
    adds.reverse();
    let mut ordered = steps;
    for add in adds {
        ordered.push(add);
    }
    // A stable sort keeps the reversed add order for equal keys.
    ordered.sort_by_key(|x| std::cmp::Reverse(x.key()));
    for step in &ordered {
        apply_step(session, step)?;
    }
    Ok(())
}

/// The paragraphs of a snapshot by their text, to find recorded changes.
struct Index<'a> {
    now: &'a [Para],
    by_text: std::collections::HashMap<String, Vec<usize>>,
}

impl<'a> Index<'a> {
    fn new(now: &'a [Para]) -> Self {
        let mut by_text: std::collections::HashMap<String, Vec<usize>> = Default::default();
        for (i, p) in now.iter().enumerate() {
            by_text.entry(keep(&p.text)).or_default().push(i);
        }
        Self { now, by_text }
    }

    /// Where a recorded change is now, found by its text and its neighbours.
    fn locate(
        &self,
        text: &str,
        prev: Option<&str>,
        next: Option<&str>,
    ) -> std::result::Result<usize, String> {
        let now = self.now;
        // A change keeps its own text whole and its neighbours' cut at KEEP.
        let same = |p: &Para, t: &str| p.text == t || keep(&p.text) == t;
        let near = |p: Option<&Para>, t: Option<&str>| match (p, t) {
            (Some(p), Some(t)) => same(p, t),
            (None, None) => true,
            _ => false,
        };
        let candidates = self.by_text.get(&keep(text)).map_or(&[][..], Vec::as_slice);
        let fits = |i: usize, with_prev: bool, with_next: bool| {
            same(&now[i], text)
                && (!with_prev || near(i.checked_sub(1).map(|j| &now[j]), prev))
                && (!with_next || near(now.get(i + 1), next))
        };
        for (wp, wn) in [(true, true), (true, false), (false, true)] {
            let hits: Vec<usize> = candidates
                .iter()
                .copied()
                .filter(|&i| fits(i, wp, wn))
                .collect();
            match hits.len() {
                1 => return Ok(hits[0]),
                0 => continue,
                _ if wp && wn => {
                    return Err("같은 글이 여러 곳에 있어 자리를 정할 수 없습니다.".into());
                }
                _ => continue,
            }
        }
        Err("그 뒤 문서가 바뀌어 바뀐 곳을 찾을 수 없습니다.".into())
    }
}

/// Where each change of a question is in the document now (for 이동).
pub(crate) fn find(now: &[Para], c: &Change) -> std::result::Result<At, String> {
    let index = Index::new(now);
    match c.kind {
        Kind::Insert | Kind::Modify => index
            .locate(
                c.after.as_deref().unwrap_or_default(),
                c.prev.as_deref(),
                c.next.as_deref(),
            )
            .map(|i| now[i].at),
        Kind::Delete => match (&c.prev, &c.next) {
            (Some(prev), _) => index
                .locate(prev, None, c.next.as_deref())
                .map(|i| now[i].at),
            (None, Some(next)) => index.locate(next, None, None).map(|i| now[i].at),
            _ => Err("지운 곳의 앞뒤를 찾을 수 없습니다.".into()),
        },
    }
}

/// Take a text-only question back on the document as it is now: every change is
/// found by its text first, and nothing is edited unless all are found.
pub(crate) fn revert(
    session: &mut Session,
    changes: &[Change],
) -> std::result::Result<usize, String> {
    if let Some(why) = text_blocker(changes) {
        return Err(why.into());
    }
    let now = snapshot(session);
    let index = Index::new(&now);
    let mut steps = Vec::new();
    let mut adds = Vec::new();
    for c in changes {
        match c.kind {
            Kind::Modify => {
                let i = index.locate(
                    c.after.as_deref().unwrap_or_default(),
                    c.prev.as_deref(),
                    c.next.as_deref(),
                )?;
                steps.push(Step::Set(now[i].at, c.before.clone().unwrap_or_default()));
            }
            Kind::Insert => {
                let i = index.locate(
                    c.after.as_deref().unwrap_or_default(),
                    c.prev.as_deref(),
                    c.next.as_deref(),
                )?;
                steps.push(Step::Remove(now[i].at));
            }
            Kind::Delete => {
                let text = c.before.clone().unwrap_or_default();
                let step = match (&c.prev, &c.next) {
                    (Some(prev), _) => {
                        let i = index.locate(prev, None, c.next.as_deref())?;
                        Step::Add {
                            after: Some(now[i].at),
                            before: None,
                            text,
                        }
                    }
                    (None, Some(next)) => {
                        let i = index.locate(next, None, None)?;
                        Step::Add {
                            after: None,
                            before: Some(now[i].at),
                            text,
                        }
                    }
                    _ => return Err("지운 곳의 앞뒤를 찾을 수 없습니다.".into()),
                };
                adds.push(step);
            }
        }
    }
    // Paragraphs put back after one place: the last first, so they end in order.
    adds.reverse();
    steps.extend(adds);
    apply_all_or_nothing(session, steps)
}

/// Apply steps as one undo step; a step that fails takes the earlier ones back.
fn apply_all_or_nothing(
    session: &mut Session,
    steps: Vec<Step>,
) -> std::result::Result<usize, String> {
    let n = steps.len();
    session.begin_undo_group().map_err(|e| e.to_string())?;
    let cp = match session.full_checkpoint() {
        Ok(cp) => cp,
        Err(e) => {
            let _ = session.end_undo_group();
            return Err(e.to_string());
        }
    };
    match apply(session, steps) {
        Ok(()) => {
            let _ = session.release_full(cp);
            let _ = session.end_undo_group();
            Ok(n)
        }
        Err(e) => {
            let undone = session.rollback_full(cp);
            let _ = session.end_undo_group();
            Err(match undone {
                Ok(_) => format!("되돌리는 중 멈춰 아무것도 바꾸지 않았습니다: {e}"),
                Err(r) => {
                    format!("되돌리는 중 멈췄고 그 앞까지의 편집도 되돌리지 못했습니다: {e} ({r})")
                }
            })
        }
    }
}

/// Author of the tracked changes an engine's questions leave (one fixed name per
/// engine: each new name is a permanent author record in the document).
pub(crate) fn track_author(engine: &str) -> String {
    format!("AI · {engine}")
}

/// Record a finished text-only question in the document as tracked changes.
///
/// The AI edited without recording (recording refuses formatting, objects and
/// table structure). Its edits are one undo step on top; this takes that step
/// back, then makes the same text changes again with recording on, from the end
/// of the document backwards (02). A rehearsal under a full checkpoint first
/// checks that accepting every change gives exactly the AI's text; on any
/// failure the document is left with the AI's own result. Returns the reason
/// when it did not record.
pub(crate) fn track_replay(
    session: &mut Session,
    before: &[Para],
    after: &[Para],
    changes: &[Change],
    author: &str,
) -> std::result::Result<(), String> {
    if let Some(why) = text_blocker(changes) {
        return Err(why.into());
    }
    let tracked = session.track_changes();
    if tracked["tracked"] == true || tracked["marks"].as_u64().unwrap_or(0) > 0 {
        return Err("문서에 이미 변경 추적 기록이 있어 AI 변경을 섞지 않습니다.".into());
    }
    if session.track_recording() {
        return Err("변경 추적 기록이 켜져 있습니다.".into());
    }
    let texts = |s: &Session| snapshot(s).into_iter().map(|p| p.text).collect::<Vec<_>>();
    let want: Vec<String> = after.iter().map(|p| p.text.clone()).collect();
    // ② Back to the document before the question; the AI's step waits in redo.
    session.execute(Command::Undo).map_err(|e| e.to_string())?;
    if texts(session) != before.iter().map(|p| p.text.clone()).collect::<Vec<_>>() {
        let _ = session.execute(Command::Redo);
        return Err("질문 전 문서로 정확히 돌아가지 않아 기록하지 않았습니다.".into());
    }
    // Back to the AI's own result: close the group unchanged (redo kept), redo.
    let restore = |session: &mut Session, why: String| {
        let _ = session.end_undo_group();
        let _ = session.execute(Command::Redo);
        Err(why)
    };
    if let Err(e) = session.begin_undo_group() {
        let _ = session.execute(Command::Redo);
        return Err(e.to_string());
    }
    let pass = |session: &mut Session| -> Result<()> {
        session.record_track_changes(true, Some(author.to_owned()))?;
        let made = replay(session, changes);
        // Always off again: left on, every later edit of the user is refused.
        session.record_track_changes(false, None)?;
        made
    };
    // ③–⑤ Rehearsal: accepting every change must give the AI's text.
    let rehearsal = match session.full_checkpoint() {
        Ok(cp) => cp,
        Err(e) => return restore(session, e.to_string()),
    };
    let checked = pass(session).and_then(|()| {
        session.run("document.track_changes.accept", json!({"all": true}))?;
        Ok(texts(session) == want)
    });
    if let Err(e) = session.rollback_full(rehearsal) {
        return restore(session, format!("시험 적용을 되돌리지 못했습니다: {e}"));
    }
    match checked {
        Ok(true) => {}
        Ok(false) => {
            return restore(
                session,
                "변경 추적으로 다시 적용한 글이 AI 결과와 달라 기록하지 않았습니다.".into(),
            );
        }
        Err(e) => {
            return restore(
                session,
                format!("변경 추적으로 기록할 수 없는 편집입니다: {e}"),
            );
        }
    }
    // ⑥ The real pass, under its own checkpoint.
    let cp = match session.full_checkpoint() {
        Ok(cp) => cp,
        Err(e) => return restore(session, e.to_string()),
    };
    match pass(session) {
        Ok(()) => {
            let _ = session.release_full(cp);
            // The real pass must match the rehearsal: accepting every tracked
            // change gives the AI's text (checked and rolled back; debug only).
            #[cfg(debug_assertions)]
            if let Ok(probe) = session.full_checkpoint() {
                let same = session
                    .run("document.track_changes.accept", json!({"all": true}))
                    .is_ok()
                    && texts(session) == want;
                let _ = session.rollback_full(probe);
                debug_assert!(
                    same,
                    "accepting the tracked changes must give the AI's text"
                );
            }
            session.end_undo_group().map_err(|e| e.to_string())?;
            Ok(())
        }
        Err(e) => {
            let _ = session.rollback_full(cp);
            restore(session, format!("변경 추적으로 기록하지 못했습니다: {e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(lines: &[&str]) -> Session {
        let mut s = Session::new().unwrap();
        s.execute(Command::Insert(lines.join("\n"))).unwrap();
        s
    }
    fn texts(s: &Session) -> Vec<String> {
        snapshot(s).into_iter().map(|p| p.text).collect()
    }

    #[test]
    fn a_diff_names_the_changed_paragraphs_with_their_neighbours() {
        let a = doc(&["제목", "첫째", "둘째", "셋째"]);
        let b = doc(&["제목", "첫째 고침", "둘째", "새 문단", "셋째"]);
        let c = diff(&snapshot(&a), &snapshot(&b));
        assert_eq!(c.len(), 2, "{c:?}");
        assert_eq!(c[0].kind, Kind::Modify);
        assert_eq!(c[0].before.as_deref(), Some("첫째"));
        assert_eq!(c[0].after.as_deref(), Some("첫째 고침"));
        assert_eq!(c[0].prev.as_deref(), Some("제목"));
        assert_eq!(c[1].kind, Kind::Insert);
        assert_eq!(c[1].after.as_deref(), Some("새 문단"));
        assert_eq!(c[1].follows.map(|a| a.p), Some(2));
        let d = diff(&snapshot(&b), &snapshot(&a));
        assert!(
            d.iter()
                .any(|x| x.kind == Kind::Delete && x.before.as_deref() == Some("새 문단")),
            "{d:?}"
        );
        assert_eq!(
            fingerprint(&snapshot(&a)),
            fingerprint(&snapshot(&doc(&["제목", "첫째", "둘째", "셋째"])))
        );
        assert_ne!(fingerprint(&snapshot(&a)), fingerprint(&snapshot(&b)));
    }

    #[test]
    fn a_change_set_replays_onto_the_document_before_it() {
        for (before, after) in [
            (vec!["가", "나", "다"], vec!["가", "나 고침", "다"]),
            (vec!["가", "나", "다"], vec!["가", "다"]),
            (vec!["가", "나", "다"], vec!["가", "나", "하나", "둘", "다"]),
            (vec!["가", "나", "다"], vec!["가 고침", "새", "다", "끝"]),
            (vec!["가", "나", "다"], vec!["나", "다"]),
        ] {
            let mut s = doc(&before);
            let c = diff(&snapshot(&s), &snapshot(&doc(&after)));
            replay(&mut s, &c).unwrap();
            assert_eq!(texts(&s), after, "{before:?} → {after:?}: {c:?}");
            // The first paragraph keeps the section's settings.
            let first = s
                .paragraph(
                    At {
                        s: 0,
                        p: 0,
                        cell: None,
                    }
                    .cursor(0),
                )
                .unwrap();
            assert!(
                first
                    .controls
                    .iter()
                    .any(|c| matches!(c, hwpcraft_doc::model::control::Control::SectionDef(_))),
                "{before:?} → {after:?}"
            );
        }
    }

    #[test]
    fn a_question_is_taken_back_by_its_text_even_after_other_edits() {
        let before = ["제목", "본 사업은 6개월간 수행한다", "끝"];
        let after = [
            "제목",
            "본 사업은 계약일로부터 6개월간 수행한다",
            "추가 문단",
            "끝",
        ];
        let c = diff(&snapshot(&doc(&before)), &snapshot(&doc(&after)));
        // The user typed above it since: indexes moved, texts did not.
        let mut s = doc(&[
            "머리 글",
            "제목",
            "본 사업은 계약일로부터 6개월간 수행한다",
            "추가 문단",
            "끝",
        ]);
        assert_eq!(revert(&mut s, &c), Ok(2));
        assert_eq!(
            texts(&s),
            ["머리 글", "제목", "본 사업은 6개월간 수행한다", "끝"]
        );
        // One undo step.
        s.execute(Command::Undo).unwrap();
        assert_eq!(texts(&s)[2], "본 사업은 계약일로부터 6개월간 수행한다");
        // Paragraphs deleted next to a changed one come back in order.
        let before = ["가", "나", "다", "라"];
        let after = ["가 고침", "라"];
        let c2 = diff(&snapshot(&doc(&before)), &snapshot(&doc(&after)));
        let mut u = doc(&after);
        assert!(revert(&mut u, &c2).is_ok(), "{c2:?}");
        assert_eq!(texts(&u), before, "{c2:?}");
        // Changed again since: refused, nothing edited.
        let mut t = doc(&["제목", "다른 글", "추가 문단", "끝"]);
        assert!(revert(&mut t, &c).is_err());
        assert_eq!(texts(&t), ["제목", "다른 글", "추가 문단", "끝"]);
    }

    #[test]
    fn a_text_only_question_is_recorded_as_tracked_changes() {
        let mut s = doc(&["제목", "본 사업은 6개월간 수행한다", "끝"]);
        let before = snapshot(&s);
        // The AI's edit: one undo step, as an AI run leaves it.
        s.begin_undo_group().unwrap();
        s.select(
            At::of(Cursor {
                section: 0,
                paragraph: 1,
                offset: 0,
                cell: None,
            })
            .cursor(5),
            false,
        )
        .unwrap();
        s.execute(Command::Insert("계약일로부터 ".into())).unwrap();
        s.select(
            Cursor {
                section: 0,
                paragraph: 2,
                offset: 1,
                cell: None,
            },
            false,
        )
        .unwrap();
        s.execute(Command::Split).unwrap();
        s.execute(Command::Insert("추가".into())).unwrap();
        assert!(s.end_undo_group().unwrap());
        let after = snapshot(&s);
        let c = diff(&before, &after);
        track_replay(&mut s, &before, &after, &c, &track_author("Claude Code")).unwrap();
        let t = s.track_changes();
        assert_eq!(t["tracked"], true, "{t}");
        assert!(t["list"].as_array().is_some_and(|l| !l.is_empty()), "{t}");
        assert!(t.to_string().contains("AI · Claude Code"), "{t}");
        // Accepting gives the AI's text; one Ctrl+Z gives the document before.
        let mut accepted = doc(&[]);
        std::mem::swap(&mut accepted, &mut s);
        accepted
            .run("document.track_changes.accept", json!({"all": true}))
            .unwrap();
        assert_eq!(
            texts(&accepted),
            after.iter().map(|p| p.text.clone()).collect::<Vec<_>>()
        );
        accepted.execute(Command::Undo).unwrap();
        // (Undo of the acceptance, then of the tracked step.)
        accepted.execute(Command::Undo).unwrap();
        assert_eq!(
            texts(&accepted),
            before.iter().map(|p| p.text.clone()).collect::<Vec<_>>()
        );
        // A document that already tracks changes is left alone, with the AI's result.
        let mut u = doc(&["가", "나"]);
        u.record_track_changes(true, Some("사람".into())).unwrap();
        u.execute(Command::Insert("사람 글".into())).unwrap();
        u.record_track_changes(false, None).unwrap();
        let b = snapshot(&u);
        u.begin_undo_group().unwrap();
        u.select(
            Cursor {
                section: 0,
                paragraph: 1,
                offset: 1,
                cell: None,
            },
            false,
        )
        .unwrap();
        u.execute(Command::Insert("다".into())).unwrap();
        u.end_undo_group().unwrap();
        let a = snapshot(&u);
        assert!(track_replay(&mut u, &b, &a, &diff(&b, &a), "AI · Codex").is_err());
        assert_eq!(
            texts(&u),
            a.iter().map(|p| p.text.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_long_document_edited_at_both_ends_names_just_those_two() {
        let mut lines: Vec<String> = (0..2000).map(|i| format!("문단 {i}")).collect();
        let before = doc(&lines.iter().map(String::as_str).collect::<Vec<_>>());
        lines[0] = "문단 0 고침".into();
        lines[1999] = "문단 1999 고침".into();
        lines.insert(1000, "가운데 새 문단".into());
        let after = doc(&lines.iter().map(String::as_str).collect::<Vec<_>>());
        let started = std::time::Instant::now();
        let c = diff(&snapshot(&before), &snapshot(&after));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(
            c.len(),
            3,
            "{:?}",
            c.iter().map(|x| &x.kind).collect::<Vec<_>>()
        );
        assert_eq!(
            c.iter().map(|x| x.kind).collect::<Vec<_>>(),
            [Kind::Modify, Kind::Insert, Kind::Modify]
        );
        // The edit script itself, both ways and empty.
        let script = edit_script(&["a", "b", "c"], &["a", "x", "c", "d"], 10).unwrap();
        assert_eq!(
            script,
            [
                (Some(0), Some(0)),
                (Some(1), None),
                (None, Some(1)),
                (Some(2), Some(2)),
                (None, Some(3))
            ]
        );
        assert_eq!(edit_script(&[], &[], 0).unwrap(), []);
        assert!(edit_script(&["a", "b"], &["c", "d"], 1).is_none());
    }

    #[test]
    fn a_step_that_fails_takes_the_earlier_ones_back() {
        let mut s = doc(&["가", "나"]);
        let first = At {
            s: 0,
            p: 0,
            cell: None,
        };
        // A text change, then a step with no place (applied after it).
        let steps = vec![
            Step::Set(first, "바뀜".into()),
            Step::Add {
                after: None,
                before: None,
                text: "x".into(),
            },
        ];
        let e = apply_all_or_nothing(&mut s, steps).unwrap_err();
        assert!(e.contains("아무것도 바꾸지 않았습니다"), "{e}");
        assert_eq!(texts(&s), ["가", "나"]);
        // A long neighbour is kept cut; a paragraph deleted next to it is still found.
        let long = "긴 문단 ".repeat(80);
        let a = doc(&[long.as_str(), "지울 문단", "끝"]);
        let b = doc(&[long.as_str(), "끝"]);
        let c = diff(&snapshot(&a), &snapshot(&b));
        let mut u = doc(&[long.as_str(), "끝"]);
        assert_eq!(revert(&mut u, &c), Ok(1), "{c:?}");
        assert_eq!(texts(&u), [long.as_str(), "지울 문단", "끝"]);
    }

    #[test]
    fn only_text_commands_make_a_text_only_question() {
        let t = |name: &str, args: serde_json::Value| (name.to_owned(), args);
        assert!(text_only(&[
            t("mcp__hwpcraft__get_text", json!({})),
            t("type_text", json!({"text": "가"})),
            t(
                "execute",
                json!({"command": "edit.replace_all", "params": {}})
            ),
            t("hwp_action", json!({"id": "MoveDocEnd"})),
            t(
                "batch",
                json!({"commands": [{"command": "selection.range"}, {"command": "text.insert"}]})
            ),
        ]));
        assert!(!text_only(&[t(
            "execute",
            json!({"command": "format.character"})
        )]));
        assert!(!text_only(&[t(
            "batch",
            json!({"commands": [{"command": "table.create"}]})
        )]));
        assert!(!text_only(&[t(
            "hwp_action",
            json!({"id": "CharShapeBold"})
        )]));
        assert!(text_blocker(&[]).is_some());
    }
}
