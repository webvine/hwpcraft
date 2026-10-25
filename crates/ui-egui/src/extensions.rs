//! Owner: core area. Routes ribbon/menu command ids to per-owner UI modules.
//! A command id whose handler is missing stays visible but disabled.
use super::*;

#[derive(Default)]
pub(crate) struct ExtState {
    pub core: ext_core::State,
    pub format: ext_format::State,
    pub table: ext_table::State,
    pub pen: table_drawing::State,
    pub object: ext_object::State,
    pub shell: ext_shell::State,
    pub tools: review_dialogs::State,
    pub comments: comments_editor::State,
    pub contexts: context_editor::State,
    pub context_regions: context_editor_v2::State,
}

impl HwpApp {
    pub(crate) fn ext_enabled(&self, id: &str) -> bool {
        if matches!(id, "edit.regions" | "tools.autotext.body") {
            return true;
        }
        self.pen_enabled(id)
            .or_else(|| self.core_ext_enabled(id))
            .or_else(|| self.format_ext_enabled(id))
            .or_else(|| self.table_ext_enabled(id))
            .or_else(|| self.comments_ext_enabled(id))
            .or_else(|| self.contexts_ext_enabled(id))
            .or_else(|| self.object_ext_enabled(id))
            .or_else(|| self.shell_ext_enabled(id))
            .or_else(|| self.tools_ext_enabled(id))
            .unwrap_or(false)
    }
    pub(crate) fn ext_dispatch(&mut self, id: &str, ctx: &egui::Context) {
        if !matches!(id, "table.draw" | "table.eraser") {
            self.ext.pen = Default::default();
        }
        if id == "tools.autotext.body" {
            self.open_body_autotext();
            return;
        }
        if id == "edit.regions" {
            self.ext.context_regions.open_editor();
            return;
        }
        let _handled = self.pen_dispatch(id)
            || self.core_ext_dispatch(id)
            || self.format_ext_dispatch(id, ctx)
            || self.table_ext_dispatch(id, ctx)
            || self.comments_ext_dispatch(id, ctx)
            || self.contexts_ext_dispatch(id)
            || self.object_ext_dispatch(id, ctx)
            || self.shell_ext_dispatch(id, ctx)
            || self.tools_ext_dispatch(id, ctx);
    }
    pub(crate) fn ext_dialogs(&mut self, ctx: &egui::Context) {
        self.pen_dialog(ctx);
        self.core_ext_dialogs(ctx);
        self.format_ext_dialogs(ctx);
        self.table_ext_dialogs(ctx);
        self.comments_ext_dialogs(ctx);
        self.contexts_ext_dialogs(ctx);
        let mut regions = std::mem::take(&mut self.ext.context_regions);
        regions.show(ctx, &mut self.session, &mut self.message);
        self.ext.context_regions = regions;
        self.object_ext_dialogs(ctx);
        self.shell_ext_dialogs(ctx);
        self.tools_ext_dialogs(ctx);
    }
    /// Module keyboard shortcuts, consumed before the built-in ones. True when handled.
    pub(crate) fn ext_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        self.format_ext_shortcuts(ctx)
            || self.shell_ext_shortcuts(ctx)
            || self.tools_ext_shortcuts(ctx)
    }
    /// Start-up `--view` names owned by modules.
    pub(crate) fn ext_view(&mut self, name: &str) -> bool {
        if name == "regions" {
            self.ext.context_regions.open_editor();
            return true;
        }
        if name == "table-select"
            && let Some((c, _)) = self
                .session
                .entries()
                .into_iter()
                .find(|(c, _)| c.cell.is_some())
        {
            let _ = self.session.select(c, false);
            self.pen_dispatch("table.select");
            return true;
        }
        if matches!(name, "table-draw" | "table-eraser") {
            self.pen_dispatch(if name == "table-draw" {
                "table.draw"
            } else {
                "table.eraser"
            });
            return true;
        }
        let ctx = self.ctx.clone();
        self.core_ext_view(name)
            || self.format_ext_view(name)
            || self.comments_ext_view(name)
            || self.contexts_ext_view(name)
            || self.table_ext_view(name, &ctx)
            || self.shell_ext_view(name)
            || self.tools_ext_view(name)
    }
    /// A module dialog that must block document typing and shortcuts.
    pub(crate) fn ext_modal_open(&self) -> bool {
        self.core_ext_modal_open()
            || self.format_ext_modal_open()
            || self.table_ext_modal_open()
            || self.comments_ext_modal_open()
            || self.contexts_ext_modal_open()
            || self.ext.context_regions.open
            || self.shell_ext_modal_open()
            || self.tools_ext_modal_open()
    }
    /// Pointer input on a page; true when a module consumed it (skip default selection).
    pub(crate) fn ext_canvas(&mut self, page: u32, response: &egui::Response, rect: Rect) -> bool {
        self.pen_canvas(page, response, rect)
            || self.contexts_canvas(page, response, rect)
            || self.table_ext_canvas(page, response, rect)
    }
    /// Mode keys (e.g. F5 cell block) consumed before document shortcuts and typing.
    pub(crate) fn ext_keys(&mut self, ctx: &egui::Context) -> bool {
        self.pen_keys(ctx) || self.table_ext_keys(ctx)
    }
}
