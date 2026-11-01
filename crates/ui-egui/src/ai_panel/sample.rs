//! Capture helper: `--view ai-sample` fills the panel with a finished sample
//! conversation (folded commands, a Markdown answer, changed places, the
//! history list) so `--screenshot` can show the panel's conversation screen
//! without running a CLI. Nothing is sent and nothing is recorded.
use super::*;

fn turn(tools: &[(&str, Option<&str>)], answer: &str) -> Turn {
    let mut t = Turn::default();
    for (i, (label, error)) in tools.iter().enumerate() {
        t.apply(stream::Event::ToolStarted {
            id: i.to_string(),
            label: (*label).to_owned(),
        });
        t.apply(stream::Event::ToolFinished {
            id: i.to_string(),
            error: error.map(str::to_owned),
        });
    }
    t.apply(stream::Event::Done {
        answer: Some(answer.to_owned()),
        error: None,
    });
    t
}

fn change(kind: changes::Kind, before: Option<&str>, after: Option<&str>) -> changes::Change {
    changes::Change {
        kind,
        before: before.map(str::to_owned),
        after: after.map(str::to_owned),
        at_before: None,
        at_after: None,
        follows: None,
        prev: None,
        next: None,
        controls: false,
    }
}

impl HwpApp {
    /// `--view ai-sample` / `ai-sample-history`: the panel with a sample
    /// conversation (and its history list open).
    pub(crate) fn ai_panel_view(&mut self, name: &str) -> bool {
        // `ai-sample-history` also opens the history list; `ai-sample-busy`
        // shows a third question still running (for captures of the lock).
        if !matches!(name, "ai-sample" | "ai-sample-history" | "ai-sample-busy") {
            return false;
        }
        // Only a blank document gets the sample text.
        if self.session.entries().iter().all(|(_, t)| t.is_empty()) {
            let _ = self.session.execute(Command::Insert(
                "2. 사업 추진 일정\n본 사업은 계약일로부터 6개월간 수행한다.".into(),
            ));
        }
        let first = Message {
            turn: Some(turn(
                &[
                    ("list_documents", None),
                    ("get_text", None),
                    ("format.char", None),
                    ("table.create", None),
                    ("table.cell_text", None),
                ],
                "2장 제목을 **굵게** 하고 바로 아래에 5행 3열 표를 넣었습니다.\n\
                 - 머리글: 단계 · 기간 · 산출물\n\
                 - 나머지 칸은 비워 두었습니다.",
            )),
            changed: true,
            link: Some(history::Link {
                changes: vec![
                    change(
                        changes::Kind::Modify,
                        Some("2. 사업 추진 일정"),
                        Some("2. 사업 추진 일정"),
                    ),
                    change(changes::Kind::Insert, None, Some("단계 기간 산출물")),
                ],
                ..history::Link::default()
            }),
            ..Message::ai()
        };
        let second = Message {
            turn: Some(turn(
                &[("get_selection", None), ("type_text", None)],
                "선택한 문장을 \"본 사업은 계약일로부터 6개월 동안 수행합니다.\"로 다듬었습니다.",
            )),
            changed: true,
            link: Some(history::Link {
                changes: vec![change(
                    changes::Kind::Modify,
                    Some("본 사업은 계약일로부터 6개월간 수행한다."),
                    Some("본 사업은 계약일로부터 6개월 동안 수행합니다."),
                )],
                tracked: Some("AI · Claude Code".into()),
                ..history::Link::default()
            }),
            ..Message::ai()
        };
        self.ext.ai.messages = vec![
            Message::user(
                "2장 제목을 굵게 하고 아래에 일정 표(3열 5행)를 넣어 줘".into(),
                Some("커서 문단: 구역 1 문단 1 · '2. 사업 추진 일정'".into()),
            ),
            first,
            Message::user(
                "이 문장을 더 공손하게 다듬어 줘".into(),
                Some("선택 영역: 구역 1 문단 2 · '본 사업은 계약일로부터 6개월간…'".into()),
            ),
            second,
        ];
        if name == "ai-sample-busy" {
            let mut running = Turn::default();
            for (id, label, done) in [("0", "get_text", true), ("1", "table.create", false)] {
                running.apply(stream::Event::ToolStarted {
                    id: id.into(),
                    label: label.into(),
                });
                if done {
                    running.apply(stream::Event::ToolFinished {
                        id: id.into(),
                        error: None,
                    });
                }
            }
            self.ext.ai.messages.push(Message::user(
                "문서 끝에 추진 일정 요약 표를 넣어 줘".into(),
                Some("커서 문단: 구역 1 문단 2 · '본 사업은 계약일로부터…'".into()),
            ));
            self.ext.ai.messages.push(Message {
                turn: Some(running),
                ..Message::ai()
            });
            self.ext.ai.sample_busy = Some(self.ext.ai.messages.len() - 1);
        }
        // The history list, opened, with this conversation and an older one.
        self.ai_history_follow();
        let conversation = |id: &str, title: &str, turns, updated| history::Conversation {
            id: id.into(),
            title: title.into(),
            engine: "Claude Code".into(),
            session: None,
            created: updated,
            updated,
            turns,
            path: None,
            fingerprint: String::new(),
        };
        let now = history::now();
        let h = &mut self.ext.ai.hist;
        h.open = name == "ai-sample-history";
        h.on = true;
        h.current = Some(("a".into(), 2));
        h.found = vec![
            (
                conversation("a", "2장 제목을 굵게 하고 아래에 일정 표를 넣어 줘", 2, now),
                history::Relation::Same,
            ),
            (
                conversation(
                    "b",
                    "문서 끝에 전체 내용을 세 줄로 요약해 줘",
                    3,
                    now - 7200,
                ),
                history::Relation::Changed,
            ),
        ];
        true
    }
}
