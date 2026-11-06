use ely_gpui_component::{
    buttons::{Button, ButtonVariant, SegmentedControl},
    i18n::text,
    layout::Collapsible,
    primitives::IconName,
    theme::ControlSize,
};
use gpui::{Context, Focusable, ParentElement, Styled, Window, div};

use super::Workspace;
use crate::settings::{PanelTab, SettingsStore};

impl Workspace {
    pub(super) fn panel_tabs(&self, cx: &Context<Self>) -> SegmentedControl {
        let chosen = match self.tab {
            PanelTab::Git => "git",
            PanelTab::Files => "files",
            PanelTab::Usage => "usage",
        };
        let (workspace, shown) = (cx.entity(), cx.entity());
        SegmentedControl::new("panel-tab", chosen)
            .segment("git", text(cx, "et.panel.git", &[]), None)
            .segment("files", text(cx, "et.panel.files", &[]), None)
            .segment("usage", text(cx, "et.panel.usage", &[]), None)
            .size(ControlSize::Sm)
            .on_change(move |value, window, cx| {
                shown.update(cx, |this, cx| this.show_tab(value, window, cx))
            })
            .on_context(move |value, _, window, cx| {
                if value == "files" && SettingsStore::get(cx).files_menu {
                    workspace.update(cx, |this, cx| this.flip_folder_actions(window, cx));
                }
            })
    }

    /// Shows Files, then shows or hides its folder row.
    fn flip_folder_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_tab(&"files".into(), window, cx);
        if self.tab != PanelTab::Files {
            return;
        }
        self.folder_actions = !self.folder_actions;
        let shown = if self.folder_actions {
            "shown"
        } else {
            "hidden"
        };
        log::info!("files: folder actions {shown}");
        cx.notify();
    }

    pub(super) fn folder_actions(&self, cx: &Context<Self>) -> Collapsible {
        let settings = SettingsStore::get(cx);
        let shown = self.folder_actions && settings.files_menu && self.tab == PanelTab::Files;
        let workspace = cx.entity();
        let choose = Button::new(
            "choose-session-folder",
            text(cx, "et.files.choose_folder", &[]),
        )
        .variant(ButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .icon(IconName::Folder)
        .full_width()
        .disabled(self.choosing_folder)
        .on_click(move |_, window, cx| {
            workspace.update(cx, |this, cx| {
                this.folder_actions = false;
                log::info!("files: folder actions hidden for chooser");
                if let Some(session) = this.current().and_then(|tab| tab.term()) {
                    window.focus(&session.terminal.focus_handle(cx), cx);
                }
                cx.notify();
                this.choose_folder(window, cx);
            });
        });
        Collapsible::new("files-folder-actions", shown).child(div().px_2().pt_2().child(choose))
    }
}
