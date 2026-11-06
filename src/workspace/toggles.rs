use ely_gpui_component::{
    buttons::{ButtonVariant, IconButton},
    i18n::text,
    primitives::IconName,
    theme::ControlSize,
};
use gpui::{App, Context, Entity, SharedString, Window};

use super::Workspace;
use crate::settings::{PanelTab, Settings, SettingsStore};

impl Workspace {
    /// Shows or hides a side column, as the settings keep it.
    pub(super) fn toggle(&mut self, flip: fn(&mut Settings), cx: &mut Context<Self>) {
        match SettingsStore::update(cx, flip) {
            Ok(()) => {
                let settings = SettingsStore::get(cx);
                let shown = |on: bool| if on { "shown" } else { "hidden" };
                log::info!(
                    "app: sessions {}, right column {}",
                    shown(settings.show_sessions),
                    shown(settings.show_panel)
                );
            }
            Err(error) => log::error!("app: the columns stay: {error:#}"),
        }
    }
}

impl Workspace {
    /// The user shows a tab: this home shows it, and the settings keep it for homes with none of their own.
    pub(super) fn show_tab(
        &mut self,
        value: &SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel = match value.as_ref() {
            "git" => PanelTab::Git,
            "files" => PanelTab::Files,
            "usage" => PanelTab::Usage,
            other => unreachable!("app: no {other} tab"),
        };
        self.turn_tab(panel, window, cx);
        if SettingsStore::get(cx).panel == panel {
            return;
        }
        match SettingsStore::update(cx, |settings| settings.panel = panel) {
            Ok(()) => self.seen_panel = panel,
            Err(error) => log::error!("app: the default tab stays: {error:#}"),
        }
    }

    /// The right column shows `panel`, in this window alone, unsaved but in `workspace.json`.
    pub(super) fn turn_tab(
        &mut self,
        panel: PanelTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tab != panel {
            log::info!("app: the right column shows {panel:?}");
            self.tab = panel;
            self.dirty(cx);
        }
        self.retab(window, cx);
    }

    /// What hangs on the tab shown: the preview's and Files' polls, and Files' folder row.
    pub(super) fn retab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = SettingsStore::get(cx);
        let files = settings.show_panel && self.tab == PanelTab::Files;
        if !(files && settings.files_menu) {
            self.folder_actions = false;
        }
        self.files.update(cx, |it, _| it.set_showing(files));
        self.sync_preview(window, cx);
        cx.notify();
    }
}

/// A header button that hides or shows a column, saved.
pub(super) fn column_toggle(
    workspace: &Entity<Workspace>,
    (id, icon): (&'static str, IconName),
    (shown, [hide, show]): (bool, [&str; 2]),
    flip: fn(&mut Settings),
    cx: &App,
) -> IconButton {
    let workspace = workspace.clone();
    IconButton::new(id, icon)
        .variant(ButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .tooltip(text(cx, if shown { hide } else { show }, &[]))
        .on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.toggle(flip, cx)))
}
