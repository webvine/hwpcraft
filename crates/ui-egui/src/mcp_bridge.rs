//! Owner: core area. 도구 ▸ MCP 연결 (C58): lets an MCP client (an AI assistant)
//! edit the document open in this window while it stays on screen.
//!
//! Turning it on starts `hwpcraft_mcp::desktop_bridge` on 127.0.0.1:7083 with the
//! bearer token in the settings folder; requests run between frames on this
//! window's session, so every edit shows at once and takes part in undo. Every
//! HwpCraft window is its own process, so a second window connected at the same time
//! takes the next free port (7084, 7085 … up to 7099) and shows it in 연결 정보.
use super::*;
use crate::ext_shell::{dialog, group, notice};
use anyhow::Context as _;
use hwpcraft_mcp::desktop_bridge::{AiRun, DEFAULT_LISTEN, DesktopBridge};

#[derive(Default)]
pub(crate) struct State {
    bridge: Option<DesktopBridge>,
    show_info: bool,
    /// The AI edit panel turned the connection on (it turns it off again).
    panel_started: bool,
    /// Whether the document changed in an AI run the connection ended when it
    /// went off, for the panel's `ai_run_end`.
    ended_run: Option<bool>,
}

/// Time per frame the window gives MCP requests before drawing again.
const SERVE_BUDGET: Duration = Duration::from_millis(30);
/// Ports tried after the default one, for other windows connected at the same time.
const MORE_PORTS: u16 = 16;

/// The first address from `first` on (the same host, `count` ports) nothing listens on.
fn free_listen(first: std::net::SocketAddr, count: u16) -> Option<std::net::SocketAddr> {
    (0..count)
        .filter_map(|n| first.port().checked_add(n))
        .map(|port| std::net::SocketAddr::new(first.ip(), port))
        .find(|addr| std::net::TcpListener::bind(addr).is_ok())
}

impl HwpApp {
    pub(crate) fn mcp_ext_enabled(&self, id: &str) -> Option<bool> {
        matches!(id, "tools.mcp_connect" | "tools.mcp_info").then_some(true)
    }

    pub(crate) fn mcp_ext_dispatch(&mut self, id: &str, ctx: &egui::Context) -> bool {
        match id {
            "tools.mcp_connect" => {
                if self.ext.mcp.bridge.is_some() {
                    self.stop_mcp_bridge("MCP 연결을 껐습니다.");
                } else {
                    self.start_mcp_bridge(ctx);
                }
            }
            "tools.mcp_info" => self.ext.mcp.show_info = true,
            _ => return false,
        }
        true
    }

    /// `--view mcp` starts the connection and shows its details.
    pub(crate) fn mcp_ext_view(&mut self, name: &str) -> bool {
        if name != "mcp" {
            return false;
        }
        let ctx = self.ctx.clone();
        self.start_mcp_bridge(&ctx);
        self.ext.mcp.show_info = true;
        true
    }

    /// Status bar text while connected.
    pub(crate) fn mcp_status(&self) -> Option<&'static str> {
        self.ext.mcp.bridge.as_ref().map(|_| "MCP 연결됨")
    }

    fn start_mcp_bridge(&mut self, ctx: &egui::Context) {
        let wake = {
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        };
        let listen = DEFAULT_LISTEN
            .parse()
            .map_err(anyhow::Error::from)
            .and_then(|first| {
                free_listen(first, MORE_PORTS).ok_or_else(|| {
                    anyhow::anyhow!(
                        "{first}부터 {MORE_PORTS}개 포트를 모두 다른 프로그램이나 HwpCraft 창이 쓰고 있습니다. 다른 창의 MCP 연결을 끄고 다시 켜세요."
                    )
                })
            });
        let started =
            listen.and_then(|listen| DesktopBridge::start(listen, None, &self.session, wake));
        if let Some(bridge) = self.result(started) {
            self.message = format!("MCP 연결을 켰습니다: {}", bridge.url());
            self.ext.mcp.bridge = Some(bridge);
            self.ext.mcp.show_info = true;
        }
    }

    fn stop_mcp_bridge(&mut self, message: &str) {
        // An open AI run ends first, so its edits close into one undo step.
        if let Some(mut bridge) = self.ext.mcp.bridge.take()
            && bridge.ai_run_active()
        {
            let ended = bridge.end_ai_run(&mut self.session);
            // Unknown after a failure: say it may have changed (one Ctrl+Z
            // takes the run back either way).
            self.ext.mcp.ended_run = Some(*ended.as_ref().unwrap_or(&true));
            self.result(ended);
        }
        self.ext.mcp.show_info = false;
        self.ext.mcp.panel_started = false;
        self.message = message.into();
    }

    /// Start an AI run on this window's document for the AI edit panel, turning
    /// the connection on first if it is off. Its edits become one undo step at
    /// [`HwpApp::ai_run_end`].
    pub(crate) fn ai_run_begin(&mut self, ctx: &egui::Context) -> anyhow::Result<AiRun> {
        self.ext.mcp.ended_run = None;
        if self.ext.mcp.bridge.is_none() {
            self.start_mcp_bridge(ctx);
            self.ext.mcp.show_info = false;
            self.ext.mcp.panel_started = self.ext.mcp.bridge.is_some();
        }
        let bridge = self
            .ext
            .mcp
            .bridge
            .as_mut()
            .context("MCP 연결을 켜지 못했습니다.")?;
        bridge.begin_ai_run(&mut self.session)
    }

    /// End the AI run; returns whether the document changed.
    pub(crate) fn ai_run_end(&mut self) -> anyhow::Result<bool> {
        if let Some(changed) = self.ext.mcp.ended_run.take() {
            return Ok(changed);
        }
        match self.ext.mcp.bridge.as_mut() {
            Some(bridge) => bridge.end_ai_run(&mut self.session),
            // The connection went away without ending the run.
            None => DesktopBridge::end_orphaned_ai_run(&mut self.session),
        }
    }

    /// Turn the connection off if the AI edit panel turned it on (the panel
    /// closes); a connection the user turned on stays.
    pub(crate) fn ai_panel_release_connection(&mut self) {
        if self.ext.mcp.panel_started {
            self.stop_mcp_bridge("AI 편집 패널의 MCP 연결을 껐습니다.");
        }
    }

    /// Run queued MCP requests on this window's document; called every frame.
    pub(crate) fn serve_mcp_bridge(&mut self, ctx: &egui::Context) {
        let Some(bridge) = self.ext.mcp.bridge.as_mut() else {
            return;
        };
        if let Some(failure) = bridge.failure() {
            self.error = Some(format!("MCP 연결이 끊겼습니다: {failure}"));
            self.stop_mcp_bridge("MCP 연결이 끊겼습니다.");
            return;
        }
        let before = self.session.generation;
        match bridge.serve(&mut self.session, SERVE_BUDGET) {
            Ok(0) => bridge.update(&self.session),
            Ok(served) => {
                if self.session.generation != before {
                    self.scroll_to_caret = true;
                }
                self.message = format!("MCP 요청 {served}개를 처리했습니다.");
                // More may be queued; come back next frame.
                ctx.request_repaint();
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    pub(crate) fn mcp_ext_dialogs(&mut self, ctx: &egui::Context) {
        if !self.ext.mcp.show_info {
            return;
        }
        let Some(bridge) = self.ext.mcp.bridge.as_ref() else {
            self.ext.mcp.show_info = false;
            return;
        };
        let url = bridge.url().to_owned();
        let token = bridge.token_file().display().to_string();
        let id = bridge.document_id().to_owned();
        // A window on another port gets its own server name, so two can be registered.
        let name = match url
            .rsplit_once(':')
            .and_then(|(_, rest)| rest.split('/').next())
        {
            Some(port) if port != "7083" => format!("hwpcraft-desktop-{port}"),
            _ => "hwpcraft-desktop".to_owned(),
        };
        let config = format!(
            "{{\n  \"mcpServers\": {{\n    \"{name}\": {{\n      \"type\": \"http\",\n      \"url\": \"{url}\",\n      \"headers\": {{ \"Authorization\": \"Bearer <토큰 파일의 내용>\" }}\n    }}\n  }}\n}}"
        );
        let mut copy = None;
        let mut stop = false;
        let clicked = dialog(ctx, "MCP 연결", 560.0, &["닫기"], |ui| {
            group(ui, "연결 정보");
            egui::Grid::new("mcp-info")
                .num_columns(3)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    for (label, value) in [
                        ("주소", url.as_str()),
                        ("토큰 파일", token.as_str()),
                        ("이 창의 문서 ID", id.as_str()),
                    ] {
                        ui.label(label);
                        ui.scope(|ui| {
                            ui.set_width(300.0);
                            ui.add(egui::Label::new(RichText::new(value).monospace()).truncate())
                                .on_hover_text(value);
                        });
                        if ui.small_button("복사").clicked() {
                            copy = Some(value.to_owned());
                        }
                        ui.end_row();
                    }
                });
            group(ui, "클라이언트 설정 예");
            ui.label(RichText::new(&config).monospace().size(11.0));
            if ui.small_button("설정 예 복사").clicked() {
                copy = Some(config.clone());
            }
            notice(
                ui,
                "이 컴퓨터(127.0.0.1)에서 토큰을 가진 클라이언트만 연결할 수 있습니다. \
                 클라이언트는 list_documents로 이 창의 문서를 찾고, 그 document_id로 편집합니다. \
                 고친 내용은 바로 화면에 보이고 실행 취소로 되돌릴 수 있습니다. \
                 MCP로 새로 만든 문서는 이 창과 따로 열립니다. 창을 닫으면 연결도 끝납니다. \
                 HwpCraft 창마다 따로 연결할 수 있고, 먼저 켠 창부터 7083, 7084 … 포트를 씁니다. \
                 클라이언트에는 창마다 다른 이름(예: hwpcraft-desktop-7084)으로 그 주소를 등록하세요.",
            );
            ui.add_space(6.0);
            stop = ui.button("연결 끄기").clicked();
        });
        if let Some(text) = copy {
            ctx.copy_text(text);
            self.message = "복사했습니다.".into();
        }
        if stop {
            self.stop_mcp_bridge("MCP 연결을 껐습니다.");
        } else if clicked.is_some() {
            self.ext.mcp.show_info = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second window connected at the same time takes the next free port.
    #[test]
    fn a_busy_port_moves_the_connection_to_the_next_one() -> anyhow::Result<()> {
        // A port the OS just handed out, held as another window would hold 7083.
        let held = std::net::TcpListener::bind("127.0.0.1:0")?;
        let first = held.local_addr()?;
        let next = free_listen(first, 4).expect("a free port after the held one");
        assert!(next.port() > first.port(), "{next} after {first}");
        assert_eq!(next.ip(), first.ip());
        assert_eq!(free_listen(first, 1), None, "only the held port to try");
        Ok(())
    }
}
