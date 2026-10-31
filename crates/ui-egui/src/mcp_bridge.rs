//! Owner: core area. 도구 ▸ MCP 연결 (C58): lets an MCP client (an AI assistant)
//! edit the document open in this window while it stays on screen.
//!
//! Turning it on starts `hwpcraft_mcp::desktop_bridge` on 127.0.0.1:7083 with the
//! bearer token in the settings folder; requests run between frames on this
//! window's session, so every edit shows at once and takes part in undo.
use super::*;
use crate::ext_shell::{dialog, group, notice};
use hwpcraft_mcp::desktop_bridge::{DEFAULT_LISTEN, DesktopBridge};

#[derive(Default)]
pub(crate) struct State {
    bridge: Option<DesktopBridge>,
    show_info: bool,
}

/// Time per frame the window gives MCP requests before drawing again.
const SERVE_BUDGET: Duration = Duration::from_millis(30);

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
        let listen = DEFAULT_LISTEN.parse().map_err(anyhow::Error::from);
        let started =
            listen.and_then(|listen| DesktopBridge::start(listen, None, &self.session, wake));
        if let Some(bridge) = self.result(started) {
            self.message = format!("MCP 연결을 켰습니다: {}", bridge.url());
            self.ext.mcp.bridge = Some(bridge);
            self.ext.mcp.show_info = true;
        }
    }

    fn stop_mcp_bridge(&mut self, message: &str) {
        self.ext.mcp.bridge = None;
        self.ext.mcp.show_info = false;
        self.message = message.into();
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
        let config = format!(
            "{{\n  \"mcpServers\": {{\n    \"hwpcraft-desktop\": {{\n      \"type\": \"http\",\n      \"url\": \"{url}\",\n      \"headers\": {{ \"Authorization\": \"Bearer <토큰 파일의 내용>\" }}\n    }}\n  }}\n}}"
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
                 MCP로 새로 만든 문서는 이 창과 따로 열립니다. 창을 닫으면 연결도 끝납니다.",
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
